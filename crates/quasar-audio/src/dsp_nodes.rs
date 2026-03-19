//! Audio DSP nodes - biquad filters, compressor, HRTF, occlusion.

use std::f32::consts::PI;

/// Biquad filter coefficients for EQ/filtering.
#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    pub x1: f32,
    pub x2: f32,
    pub y1: f32,
    pub y2: f32,
}

impl BiquadState {
    pub fn process(&mut self, coeffs: &BiquadCoeffs, input: f32) -> f32 {
        let output = coeffs.b0 * input + coeffs.b1 * self.x1 + coeffs.b2 * self.x2
            - coeffs.a1 * self.y1
            - coeffs.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }
}

impl BiquadCoeffs {
    pub fn low_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let omega = 2.0 * PI * cutoff / sample_rate;
        let cos_omega = omega.cos();
        let sin_omega = omega.sin();
        let alpha = sin_omega / (2.0 * q);
        let b0 = (1.0 - cos_omega) / 2.0;
        let b1 = 1.0 - cos_omega;
        let b2 = (1.0 - cos_omega) / 2.0;
        let a0 = 1.0 + alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_omega / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    pub fn high_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let omega = 2.0 * PI * cutoff / sample_rate;
        let cos_omega = omega.cos();
        let sin_omega = omega.sin();
        let alpha = sin_omega / (2.0 * q);
        let b0 = (1.0 + cos_omega) / 2.0;
        let b1 = -(1.0 + cos_omega);
        let b2 = (1.0 + cos_omega) / 2.0;
        let a0 = 1.0 + alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_omega / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    pub fn peaking(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let omega = 2.0 * PI * freq / sample_rate;
        let a = 10.0_f32.powf(gain_db / 40.0);
        let alpha = omega.sin() / (2.0 * q);
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * omega.cos();
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: b1 / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

/// 6-band parametric EQ.
#[derive(Debug, Clone)]
pub struct ParametricEQ {
    pub bands: [EQBand; 6],
    pub states: [BiquadState; 6],
}

#[derive(Debug, Clone, Copy)]
pub struct EQBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub enabled: bool,
}

impl Default for ParametricEQ {
    fn default() -> Self {
        Self {
            bands: [
                EQBand {
                    freq: 60.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
                EQBand {
                    freq: 250.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
                EQBand {
                    freq: 1000.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
                EQBand {
                    freq: 4000.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
                EQBand {
                    freq: 8000.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
                EQBand {
                    freq: 12000.0,
                    gain_db: 0.0,
                    q: 1.0,
                    enabled: true,
                },
            ],
            states: Default::default(),
        }
    }
}

impl ParametricEQ {
    pub fn process(&mut self, sample_rate: f32, input: f32) -> f32 {
        let mut output = input;
        for (band, state) in self.bands.iter().zip(self.states.iter_mut()) {
            if band.enabled && band.gain_db.abs() > 0.01 {
                let coeffs = BiquadCoeffs::peaking(sample_rate, band.freq, band.gain_db, band.q);
                output = state.process(&coeffs, output);
            }
        }
        output
    }
}

/// RMS-based dynamic range compressor.
#[derive(Debug, Clone)]
pub struct Compressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub knee_db: f32,
    pub makeup_gain_db: f32,
    envelope: f32,
    sample_rate: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 6.0,
            makeup_gain_db: 0.0,
            envelope: 0.0,
            sample_rate: 44100.0,
        }
    }
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            ..Default::default()
        }
    }

    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32, f32) {
        let input_level = left.abs().max(right.abs()).max(1e-10);
        let input_db = 20.0 * input_level.log10();

        let target_gain_db = if input_db < self.threshold_db {
            0.0
        } else {
            (input_db - self.threshold_db) * (1.0 - 1.0 / self.ratio)
        };

        let attack_coeff = (-1.0 / (self.attack_ms * 0.001 * self.sample_rate)).exp();
        let release_coeff = (-1.0 / (self.release_ms * 0.001 * self.sample_rate)).exp();

        let coeff = if target_gain_db > self.envelope {
            attack_coeff
        } else {
            release_coeff
        };
        self.envelope = target_gain_db + coeff * (self.envelope - target_gain_db);

        let gain_linear = 10.0_f32.powf((self.makeup_gain_db - self.envelope) / 20.0);
        (left * gain_linear, right * gain_linear, self.envelope)
    }
}

/// HRTF processor for binaural rendering.
#[derive(Debug, Clone, Default)]
pub struct HRTFProcessor {
    prev_azimuth: f32,
    prev_elevation: f32,
}

impl HRTFProcessor {
    pub fn update(&mut self, _sample_rate: f32, azimuth: f32, elevation: f32) {
        self.prev_azimuth = azimuth;
        self.prev_elevation = elevation;
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let azimuth_rad = self.prev_azimuth.to_radians();
        let left_gain = (1.0 - azimuth_rad.sin().abs()).max(0.5);
        let right_gain = (azimuth_rad.sin().abs()).max(0.5);
        (input * left_gain, input * right_gain)
    }
}

/// Audio occlusion filter.
#[derive(Debug, Clone)]
pub struct OcclusionFilter {
    state: BiquadState,
    coeffs: BiquadCoeffs,
    current_occlusion: f32,
}

impl OcclusionFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            state: Default::default(),
            coeffs: BiquadCoeffs::low_pass(sample_rate, sample_rate / 4.0, 0.707),
            current_occlusion: 0.0,
        }
    }

    pub fn set_occlusion(&mut self, sample_rate: f32, factor: f32) {
        self.current_occlusion = factor;
        let cutoff = 20000.0 - factor * 19000.0;
        self.coeffs = BiquadCoeffs::low_pass(sample_rate, cutoff, 0.707);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.state.process(&self.coeffs, input)
    }
}
