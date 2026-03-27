# quasar-build

Build utilities and asset processing for the Quasar Engine.

## Features

- **CLI Tool**: Build, pack, and deploy
- **Asset Compression**: Mesh optimization, texture compression
- **Content-Addressable Storage**: Deduplication
- **Multi-Platform Builds**: Windows, Linux, macOS, WASM, mobile

## CLI Usage

```bash
# Build game
quasar-build build --release

# Pack assets
quasar-build pack assets/ output.pak

# Deploy
quasar-build deploy --target wasm32-unknown-unknown
```

## Asset Processing

- Mesh optimization via meshopt
- Texture compression (ASTC, BC7, ETC2)
- Audio transcoding
- Font rasterization
