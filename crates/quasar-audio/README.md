# quasar-audio

Spatial audio system for the Quasar Engine.

## Features

- **Playback**: One-shot, looped, and streaming audio
- **6-Bus Mixer**: Master, Music, SFX, Voice, Ambient, Custom
- **DSP Chain**: EQ, Compressor, Limiter, Reverb
- **Spatial Audio**: Inverse-distance attenuation
- **Doppler Effect**: Velocity-based pitch shift
- **Reverb Zones**: AABB-based reverb areas
- **Ambisonics**: Orders 1-3, ACN/SN3D format
- **GPU Reverb**: Compute shader convolution (optional)

## Usage

```rust
use quasar_audio::{AudioPlugin, AudioResource};

app.add_plugin(AudioPlugin);

let audio = world.resource::<AudioResource>();
audio.play("sound.ogg");
```

## Feature Flags

- `gpu-reverb` - GPU-accelerated reverb
