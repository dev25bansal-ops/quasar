# GPU Reverb — Offline Bake Process

## Overview

The `quasar-audio` crate provides GPU-accelerated convolution reverb via compute shaders (`convolution_reverb.wgsl`). This document describes the offline bake process for creating high-quality impulse responses.

## Architecture

### Components

1. **Impulse Response Capture**: Record or synthesize reverberant spaces
2. **Partitioned Convolution**: Split impulse response into 1024-sample partitions
3. **GPU Processing**: Overlap-add algorithm via compute shader
4. **Runtime Mixing**: Apply baked reverb to audio streams

### Workflow

```
[IR Audio File] → [Partition] → [FFT] → [GPU Upload] → [Runtime Convolution]
                                                              ↓
                                           [Audio Input] → [Overlap-Add] → [Reverb Output]
```

## Offline Bake Steps

### Step 1: Capture Impulse Response

Record impulse responses using:

- **Sine sweep method**: Play exponential sine sweep, deconvolve to get IR
- **Balloon pop / starter pistol**: Natural IR capture
- **Ray-traced simulation**: Generate IR from 3D scene geometry

Recommended IR lengths:
| Space Type | IR Duration | Sample Count (48kHz) |
|------------|-------------|---------------------|
| Small room | 0.5-1.0s | 24,000-48,000 |
| Medium hall | 1.5-2.5s | 72,000-120,000 |
| Cathedral | 3.0-6.0s | 144,000-288,000 |

### Step 2: Partition the IR

The GPU convolution uses 1024-sample partitions for efficient overlap-add:

```rust
// Example partition code (run offline)
const PARTITION_SIZE: usize = 1024;

fn partition_impulse_response(ir: &[f32]) -> Vec<[f32; 1024]> {
    let num_partitions = (ir.len() + PARTITION_SIZE - 1) / PARTITION_SIZE;
    let mut partitions = Vec::with_capacity(num_partitions);

    for i in 0..num_partitions {
        let mut partition = [0.0f32; 1024];
        let start = i * PARTITION_SIZE;
        let end = (start + PARTITION_SIZE).min(ir.len());
        partition[..end - start].copy_from_slice(&ir[start..end]);
        partitions.push(partition);
    }

    partitions
}
```

### Step 3: FFT Each Partition

Transform each partition to frequency domain for GPU convolution:

```rust
use rustfft::{FftPlanner, Complex};

fn fft_partitions(partitions: &[[f32; 1024]]) -> Vec<[Complex<f32>; 1024]> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(1024);

    partitions.iter().map(|p| {
        let mut buffer: Vec<Complex<f32>> = p.iter()
            .map(|&x| Complex { re: x, im: 0.0 })
            .collect();
        fft.process(&mut buffer);
        buffer.try_into().unwrap()
    }).collect()
}
```

### Step 4: Save Baked IR

Save the FFT'd partitions to a binary format for runtime loading:

```
File format:
- Header (32 bytes):
  - Magic: "QSIR" (4 bytes)
  - Version: u32 (4 bytes)
  - Sample rate: u32 (4 bytes)
  - Original IR length: u32 (4 bytes)
  - Partition count: u32 (4 bytes)
  - Partition size: u32 (4 bytes) = 1024
  - Reserved: 8 bytes
- Data:
  - Partition 0: 1024 × complex<f32> (16384 bytes)
  - Partition 1: 1024 × complex<f32> (16384 bytes)
  - ...
```

## Runtime Integration

### Loading Baked IR

```rust
impl GpuConvolutionReverb {
    pub fn load_baked_ir(&mut self, device: &wgpu::Device, path: &Path) -> Result<()> {
        let data = fs::read(path)?;
        // Parse header and upload partitions to GPU...
        self.partition_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("IR Partitions"),
            contents: &data[32..], // Skip header
            usage: wgpu::BufferUsages::STORAGE,
        }));
        Ok(())
    }
}
```

### Compute Shader Dispatch

The convolution reverb shader (`convolution_reverb.wgsl`) processes audio:

```wgsl
@compute @workgroup_size(256)
fn cs_convolve(@builtin(global_invocation_id) gid: vec3<u32>) {
    let partition_idx = gid.x;
    let sample_idx = gid.y;

    // Load input sample
    let input_sample = input_audio[sample_idx];

    // Load IR partition (frequency domain)
    let ir_partition = ir_partitions[partition_idx];

    // Complex multiply and accumulate
    var result = vec2<f32>(0.0, 0.0);
    // ... overlap-add algorithm ...

    output_audio[sample_idx] = result.x;
}
```

## Performance Characteristics

| Metric            | Value                           |
| ----------------- | ------------------------------- |
| Partition size    | 1024 samples                    |
| Latency           | 21.3ms @ 48kHz                  |
| GPU memory per IR | ~16KB × partition_count         |
| Compute time      | ~0.5ms per frame (1024 samples) |

## Limitations

1. **Fixed partition size**: 1024 samples introduces ~21ms latency
2. **No real-time IR updates**: IR must be baked offline
3. **Single IR per reverb zone**: Blend between zones for transitions

## Future Improvements

- [ ] Variable partition sizes for low-latency mode
- [ ] Real-time IR generation from scene geometry
- [ ] Multiple IR blending for smooth zone transitions
- [ ] Ambisonic reverb support

## References

- [Gardner, 1995] "Efficient Convolution Without Input-Output Delay"
- [Wefers, 2015] "Partitioned Convolution Algorithms for Real-Time Auralization"
