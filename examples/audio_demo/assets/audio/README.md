# Audio Demo Assets

This directory contains audio files for the audio_demo example.

## Required Audio Files

Place audio files here for testing:

- `ambient.ogg` - Ambient background sound
- `music.ogg` - Background music track
- `sfx.wav` - Sound effect for testing
- `spatial_test.ogg` - Audio file for 3D spatial audio testing

## Supported Formats

The Quasar audio system supports:

- OGG Vorbis (.ogg)
- WAV (.wav)
- MP3 (.mp3)
- FLAC (.flac)

## Testing Without Assets

The audio demo can run without external files - it will use synthesized test tones:

```rust
// The audio system can generate procedural sounds
let sound = audio_resource.create_tone(440.0, Duration::from_secs(1));
```

## Free Audio Resources

For testing, you can download free audio from:

- [Freesound.org](https://freesound.org)
- [OpenGameArt.org](https://opengameart.org/art-search?keys=audio)
