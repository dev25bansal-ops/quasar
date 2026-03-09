//! Ambisonics encoding and decoding (orders 1–3).
//!
//! - [`AmbisonicsEncoder`] converts a mono source at a given direction into
//!   B-format (ACN/SN3D channel ordering).
//! - [`AmbisonicsDecoder`] maps B-format channels to a given [`SpeakerLayout`].

/// Ambisonics order (1 through 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbisonicsOrder {
    First  = 1,
    Second = 2,
    Third  = 3,
}

impl AmbisonicsOrder {
    /// Number of B-format channels for this order: `(order+1)^2`.
    pub fn channel_count(self) -> usize {
        let o = self as usize;
        (o + 1) * (o + 1)
    }
}

/// Encodes a mono signal into Ambisonics B-format (ACN / SN3D).
pub struct AmbisonicsEncoder {
    pub order: AmbisonicsOrder,
}

impl AmbisonicsEncoder {
    pub fn new(order: AmbisonicsOrder) -> Self {
        Self { order }
    }

    /// Encode a mono sample arriving from `(azimuth, elevation)` in radians.
    ///
    /// Returns B-format channel coefficients (length = `order.channel_count()`).
    /// Azimuth: 0 = front, π/2 = left.  Elevation: 0 = horizon, π/2 = up.
    pub fn encode(&self, azimuth: f32, elevation: f32) -> Vec<f32> {
        let n = self.order.channel_count();
        let mut coeffs = vec![0.0f32; n];

        let (sin_az, cos_az) = azimuth.sin_cos();
        let (sin_el, cos_el) = elevation.sin_cos();

        // ACN 0: W (order 0)
        coeffs[0] = 1.0; // SN3D W = 1

        if n > 1 {
            // ACN 1: Y (order 1, degree -1)
            coeffs[1] = sin_az * cos_el;
            // ACN 2: Z (order 1, degree 0)
            coeffs[2] = sin_el;
            // ACN 3: X (order 1, degree 1)
            coeffs[3] = cos_az * cos_el;
        }

        if n > 4 {
            // Order 2 (5 channels, ACN 4–8)
            let cos2_el = cos_el * cos_el;
            let sin2_az = (2.0 * azimuth).sin();
            let cos2_az = (2.0 * azimuth).cos();

            // ACN 4: V
            coeffs[4] = SN3D_2N1 * sin2_az * cos2_el;
            // ACN 5: T
            coeffs[5] = SN3D_2N1 * sin_az * sin_el * cos_el;
            // ACN 6: R
            coeffs[6] = 0.5 * (3.0 * sin_el * sin_el - 1.0);
            // ACN 7: S
            coeffs[7] = SN3D_2N1 * cos_az * sin_el * cos_el;
            // ACN 8: U
            coeffs[8] = SN3D_2N1 * cos2_az * cos2_el;
        }

        if n > 9 {
            // Order 3 (7 channels, ACN 9–15)
            let sin3_az = (3.0 * azimuth).sin();
            let cos3_az = (3.0 * azimuth).cos();
            let sin2_az = (2.0 * azimuth).sin();
            let cos2_az = (2.0 * azimuth).cos();
            let cos2_el = cos_el * cos_el;

            // ACN 9: Q
            coeffs[9] = SN3D_3N1 * sin3_az * cos2_el * cos_el;
            // ACN 10: O
            coeffs[10] = SN3D_3N2 * sin2_az * sin_el * cos2_el;
            // ACN 11: M
            coeffs[11] = SN3D_3N3 * sin_az * cos_el * (5.0 * sin_el * sin_el - 1.0);
            // ACN 12: K
            coeffs[12] = 0.5 * sin_el * (5.0 * sin_el * sin_el - 3.0);
            // ACN 13: L
            coeffs[13] = SN3D_3N3 * cos_az * cos_el * (5.0 * sin_el * sin_el - 1.0);
            // ACN 14: N
            coeffs[14] = SN3D_3N2 * cos2_az * sin_el * cos2_el;
            // ACN 15: P
            coeffs[15] = SN3D_3N1 * cos3_az * cos2_el * cos_el;
        }

        coeffs
    }

    /// Encode a mono buffer in-place, writing to B-format output channels.
    ///
    /// `output` must have `order.channel_count()` slices, each of length `mono.len()`.
    pub fn encode_buffer(
        &self,
        mono: &[f32],
        azimuth: f32,
        elevation: f32,
        output: &mut [Vec<f32>],
    ) {
        let coeffs = self.encode(azimuth, elevation);
        for (ch, coeff) in coeffs.iter().enumerate() {
            if ch < output.len() {
                output[ch].resize(mono.len(), 0.0);
                for (i, &s) in mono.iter().enumerate() {
                    output[ch][i] += s * coeff;
                }
            }
        }
    }
}

// SN3D normalization constants for higher orders.
const SN3D_2N1: f32 = 0.866_025_4; // sqrt(3)/2
const SN3D_3N1: f32 = 0.790_569_4; // sqrt(5/8)
const SN3D_3N2: f32 = 1.936_491_7; // sqrt(15)/2
const SN3D_3N3: f32 = 0.612_372_4; // sqrt(3/8)

// ---------------------------------------------------------------------------
// Speaker layouts & decoder
// ---------------------------------------------------------------------------

/// Predefined speaker layouts for decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakerLayout {
    /// Stereo (2 speakers: L / R).
    Stereo,
    /// Quadraphonic (4 speakers: FL / FR / RL / RR).
    Quad,
    /// 5.1 surround (6 speakers: FL / FR / C / LFE / RL / RR).
    Surround51,
    /// 7.1 surround (8 speakers: FL / FR / C / LFE / RL / RR / SL / SR).
    Surround71,
}

impl SpeakerLayout {
    /// Number of output channels.
    pub fn channel_count(self) -> usize {
        match self {
            Self::Stereo => 2,
            Self::Quad => 4,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    /// Speaker directions as (azimuth, elevation) in radians.
    fn directions(self) -> Vec<(f32, f32)> {
        let deg = |d: f32| d.to_radians();
        match self {
            Self::Stereo => vec![
                (deg(-30.0), 0.0), // L
                (deg(30.0), 0.0),  // R
            ],
            Self::Quad => vec![
                (deg(-45.0), 0.0), // FL
                (deg(45.0), 0.0),  // FR
                (deg(-135.0), 0.0), // RL
                (deg(135.0), 0.0),  // RR
            ],
            Self::Surround51 => vec![
                (deg(-30.0), 0.0),  // FL
                (deg(30.0), 0.0),   // FR
                (0.0, 0.0),         // C
                (0.0, 0.0),         // LFE (omnidirectional)
                (deg(-110.0), 0.0), // RL
                (deg(110.0), 0.0),  // RR
            ],
            Self::Surround71 => vec![
                (deg(-30.0), 0.0),  // FL
                (deg(30.0), 0.0),   // FR
                (0.0, 0.0),         // C
                (0.0, 0.0),         // LFE
                (deg(-135.0), 0.0), // RL
                (deg(135.0), 0.0),  // RR
                (deg(-90.0), 0.0),  // SL
                (deg(90.0), 0.0),   // SR
            ],
        }
    }
}

/// Decodes Ambisonics B-format into speaker feeds for a given [`SpeakerLayout`].
pub struct AmbisonicsDecoder {
    pub order: AmbisonicsOrder,
    pub layout: SpeakerLayout,
    /// Decode matrix: `[speaker_count][bformat_channels]`.
    decode_matrix: Vec<Vec<f32>>,
}

impl AmbisonicsDecoder {
    /// Build a basic sampling decoder for the given order and layout.
    pub fn new(order: AmbisonicsOrder, layout: SpeakerLayout) -> Self {
        let encoder = AmbisonicsEncoder::new(order);
        let dirs = layout.directions();
        let _n_bf = order.channel_count();
        let n_spk = dirs.len();

        // Basic sampling decoder: each speaker row = encoder coefficients for
        // the speaker direction, scaled by 1/n_spk to normalize energy.
        let scale = 1.0 / n_spk as f32;
        let decode_matrix = dirs
            .iter()
            .map(|&(az, el)| {
                encoder
                    .encode(az, el)
                    .into_iter()
                    .map(|c| c * scale)
                    .collect()
            })
            .collect();

        Self {
            order,
            layout,
            decode_matrix,
        }
    }

    /// Decode B-format channels into speaker output channels.
    ///
    /// `bformat` has `order.channel_count()` slices of equal length.
    /// Returns `layout.channel_count()` speaker buffers of the same length.
    pub fn decode(&self, bformat: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let n_bf = self.order.channel_count();
        let buf_len = bformat.first().map_or(0, |v| v.len());
        let n_spk = self.layout.channel_count();

        let mut output = vec![vec![0.0f32; buf_len]; n_spk];

        for (spk, row) in self.decode_matrix.iter().enumerate() {
            for (ch, &coeff) in row.iter().enumerate().take(n_bf) {
                if ch < bformat.len() {
                    for i in 0..buf_len {
                        output[spk][i] += bformat[ch][i] * coeff;
                    }
                }
            }
        }

        output
    }
}
