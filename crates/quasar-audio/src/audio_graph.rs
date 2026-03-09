//! Audio DSP graph — ordered effect chain per bus.
//!
//! Each [`AudioGraph`] holds a sequence of [`DspNode`] effects that are
//! processed in order.  Supported node types: parametric EQ, compressor,
//! reverb send, sidechain ducker, and limiter.

// ---------------------------------------------------------------------------
// DSP Nodes
// ---------------------------------------------------------------------------

/// Trait for a single processing node in the audio graph.
pub trait DspNode: Send + Sync {
    fn name(&self) -> &str;
    /// Process `buffer` in-place.  `sample_rate` is provided for
    /// time-dependent effects.  `sidechain` is an optional external signal
    /// for nodes that support keying (e.g., ducking).
    fn process(&mut self, buffer: &mut [f32], sample_rate: u32, sidechain: Option<&[f32]>);
    /// Reset internal state (e.g., on seek / scene change).
    fn reset(&mut self);
}

// ---------------------------------------------------------------------------
// Parametric EQ (peaking bell)
// ---------------------------------------------------------------------------

/// Simple second-order peaking-bell EQ.
pub struct ParametricEq {
    pub center_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    // Biquad state
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    needs_recalc: bool,
}

impl ParametricEq {
    pub fn new(center_hz: f32, gain_db: f32, q: f32) -> Self {
        Self {
            center_hz,
            gain_db,
            q,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            needs_recalc: true,
        }
    }

    fn recalc(&mut self, sample_rate: u32) {
        let a = 10.0f32.powf(self.gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * self.center_hz / sample_rate as f32;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * self.q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
        self.needs_recalc = false;
    }
}

impl DspNode for ParametricEq {
    fn name(&self) -> &str {
        "eq"
    }

    fn process(&mut self, buffer: &mut [f32], sample_rate: u32, _sidechain: Option<&[f32]>) {
        if self.needs_recalc {
            self.recalc(sample_rate);
        }
        for s in buffer.iter_mut() {
            let x0 = *s;
            let y0 = self.b0 * x0 + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1
                - self.a2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x0;
            self.y2 = self.y1;
            self.y1 = y0;
            *s = y0;
        }
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
        self.needs_recalc = true;
    }
}

// ---------------------------------------------------------------------------
// Compressor
// ---------------------------------------------------------------------------

/// Basic feed-forward compressor.
pub struct Compressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    env: f32,
}

impl Compressor {
    pub fn new(threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32) -> Self {
        Self {
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            env: 0.0,
        }
    }
}

impl DspNode for Compressor {
    fn name(&self) -> &str {
        "compressor"
    }

    fn process(&mut self, buffer: &mut [f32], sample_rate: u32, _sidechain: Option<&[f32]>) {
        let attack_coeff = (-1.0 / (self.attack_ms * 0.001 * sample_rate as f32)).exp();
        let release_coeff = (-1.0 / (self.release_ms * 0.001 * sample_rate as f32)).exp();

        for s in buffer.iter_mut() {
            let abs_val = s.abs().max(1e-12);
            let input_db = 20.0 * abs_val.log10();

            let coeff = if input_db > self.env {
                attack_coeff
            } else {
                release_coeff
            };
            self.env = coeff * self.env + (1.0 - coeff) * input_db;

            let over = (self.env - self.threshold_db).max(0.0);
            let gain_reduction_db = over * (1.0 - 1.0 / self.ratio);
            let gain = 10.0f32.powf(-gain_reduction_db / 20.0);
            *s *= gain;
        }
    }

    fn reset(&mut self) {
        self.env = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Reverb send (simple delay + feedback)
// ---------------------------------------------------------------------------

/// Mono reverb send effect (comb-filter approximation).
pub struct ReverbSend {
    pub delay_ms: f32,
    pub feedback: f32,
    pub wet: f32,
    buffer: Vec<f32>,
    write_pos: usize,
}

impl ReverbSend {
    pub fn new(delay_ms: f32, feedback: f32, wet: f32) -> Self {
        Self {
            delay_ms,
            feedback: feedback.clamp(0.0, 0.99),
            wet,
            buffer: Vec::new(),
            write_pos: 0,
        }
    }
}

impl DspNode for ReverbSend {
    fn name(&self) -> &str {
        "reverb_send"
    }

    fn process(&mut self, buf: &mut [f32], sample_rate: u32, _sidechain: Option<&[f32]>) {
        let delay_samples = (self.delay_ms * 0.001 * sample_rate as f32) as usize;
        let delay_samples = delay_samples.max(1);

        if self.buffer.len() != delay_samples {
            self.buffer = vec![0.0; delay_samples];
            self.write_pos = 0;
        }

        for s in buf.iter_mut() {
            let read_pos = self.write_pos;
            let delayed = self.buffer[read_pos];
            let input = *s + delayed * self.feedback;
            self.buffer[self.write_pos] = input;
            self.write_pos = (self.write_pos + 1) % delay_samples;
            *s = *s * (1.0 - self.wet) + delayed * self.wet;
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

// ---------------------------------------------------------------------------
// Sidechain ducker
// ---------------------------------------------------------------------------

/// Ducks signal when a sidechain signal exceeds a threshold.
pub struct SidechainDucker {
    pub threshold_db: f32,
    pub duck_amount_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    env: f32,
}

impl SidechainDucker {
    pub fn new(threshold_db: f32, duck_amount_db: f32, attack_ms: f32, release_ms: f32) -> Self {
        Self {
            threshold_db,
            duck_amount_db,
            attack_ms,
            release_ms,
            env: 0.0,
        }
    }
}

impl DspNode for SidechainDucker {
    fn name(&self) -> &str {
        "sidechain_ducker"
    }

    fn process(&mut self, buffer: &mut [f32], sample_rate: u32, sidechain: Option<&[f32]>) {
        let sc = match sidechain {
            Some(s) => s,
            None => return,
        };
        let attack_coeff = (-1.0 / (self.attack_ms * 0.001 * sample_rate as f32)).exp();
        let release_coeff = (-1.0 / (self.release_ms * 0.001 * sample_rate as f32)).exp();

        for (i, s) in buffer.iter_mut().enumerate() {
            let sc_val = sc.get(i).copied().unwrap_or(0.0).abs().max(1e-12);
            let sc_db = 20.0 * sc_val.log10();

            let coeff = if sc_db > self.env {
                attack_coeff
            } else {
                release_coeff
            };
            self.env = coeff * self.env + (1.0 - coeff) * sc_db;

            let over = (self.env - self.threshold_db).max(0.0);
            let duck_frac = (over / (-self.threshold_db).max(1.0)).min(1.0);
            let gain = 10.0f32.powf(-self.duck_amount_db * duck_frac / 20.0);
            *s *= gain;
        }
    }

    fn reset(&mut self) {
        self.env = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Limiter (brick-wall)
// ---------------------------------------------------------------------------

/// Simple brick-wall limiter.
pub struct Limiter {
    pub ceiling_db: f32,
    pub release_ms: f32,
    gain_reduction: f32,
}

impl Limiter {
    pub fn new(ceiling_db: f32, release_ms: f32) -> Self {
        Self {
            ceiling_db,
            release_ms,
            gain_reduction: 1.0,
        }
    }
}

impl DspNode for Limiter {
    fn name(&self) -> &str {
        "limiter"
    }

    fn process(&mut self, buffer: &mut [f32], sample_rate: u32, _sidechain: Option<&[f32]>) {
        let ceiling = 10.0f32.powf(self.ceiling_db / 20.0);
        let release_coeff = (-1.0 / (self.release_ms * 0.001 * sample_rate as f32)).exp();

        for s in buffer.iter_mut() {
            let abs_val = s.abs();
            if abs_val > ceiling {
                let needed = ceiling / abs_val;
                if needed < self.gain_reduction {
                    self.gain_reduction = needed;
                }
            } else {
                self.gain_reduction =
                    release_coeff * self.gain_reduction + (1.0 - release_coeff) * 1.0;
            }
            *s *= self.gain_reduction;
        }
    }

    fn reset(&mut self) {
        self.gain_reduction = 1.0;
    }
}

// ---------------------------------------------------------------------------
// Audio Graph
// ---------------------------------------------------------------------------

/// An ordered chain of DSP nodes forming an audio bus effect chain.
pub struct AudioGraph {
    nodes: Vec<Box<dyn DspNode>>,
    pub sample_rate: u32,
}

impl AudioGraph {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            nodes: Vec::new(),
            sample_rate,
        }
    }

    /// Append a DSP node to the end of the chain.
    pub fn add_node(&mut self, node: Box<dyn DspNode>) {
        self.nodes.push(node);
    }

    /// Insert a DSP node at a specific position.
    pub fn insert_node(&mut self, index: usize, node: Box<dyn DspNode>) {
        self.nodes.insert(index.min(self.nodes.len()), node);
    }

    /// Remove the node at `index`. Returns it if valid.
    pub fn remove_node(&mut self, index: usize) -> Option<Box<dyn DspNode>> {
        if index < self.nodes.len() {
            Some(self.nodes.remove(index))
        } else {
            None
        }
    }

    /// Number of nodes in the chain.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Process `buffer` through all nodes in order.
    pub fn process(&mut self, buffer: &mut [f32], sidechain: Option<&[f32]>) {
        let sr = self.sample_rate;
        for node in &mut self.nodes {
            node.process(buffer, sr, sidechain);
        }
    }

    /// Reset all nodes.
    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.reset();
        }
    }
}
