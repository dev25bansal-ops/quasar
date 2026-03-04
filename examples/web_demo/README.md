# WASM/Web Target Support

This directory contains configuration for running Quasar Engine in web browsers using WebGPU.

## Prerequisites

1. Install the wasm32 target:
```sh
rustup target add wasm32-unknown-unknown
```

2. Install trunk (WASM bundler):
```sh
cargo install trunk
```

## Building for Web

From the repository root:

```sh
cd examples/web_demo
trunk build --release
```

For development with hot-reload:

```sh
trunk serve
```

Then open http://localhost:8080 in your browser.

## Browser Support

WebGPU is supported in:
- Chrome 113+
- Edge 113+
- Firefox Nightly (behind flag: `dom.webgpu.enabled`)
- Safari Technology Preview (behind flag)

## Known Limitations

- **No audio support** - Kira is not WASM-compatible
- **No physics** - Rapier3D needs special wasm setup (can be enabled with feature flag)
- **No Lua scripting** - mlua is not WASM-compatible

## Project Structure

```
web_demo/
├── Cargo.toml       # WASM-enabled dependencies
├── src/
│   └── lib.rs       # wasm-bindgen entry point
├── index.html       # HTML template with WebGPU canvas
└── README.md        # This file
```

## Implementation Notes

The web demo uses:
- `wasm-bindgen` for Rust/JavaScript interop
- `web-sys` for DOM and WebGPU access
- `console_log` for browser console logging
- `console_error_panic_hook` for panic stack traces

Full rendering requires:
1. WebGPU instance creation via `wgpu::Instance`
2. Canvas surface integration
3. `requestAnimationFrame` render loop
4. Resizing handling

The current implementation demonstrates the basic structure. Full WebGPU rendering
integration would require additional work on the engine's renderer to support
async surface creation and web-specific rendering pipelines.
