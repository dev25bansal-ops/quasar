# Contributing to Quasar Engine

Thank you for your interest in contributing to **Quasar** — a modular 3D game
engine written in Rust!

## Getting Started

1. **Fork & clone** the repository.
2. Make sure you have the latest stable Rust toolchain:
   ```bash
   rustup update stable
   rustup component add clippy rustfmt
   ```
3. Verify everything builds and passes:
   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --all -- --check
   ```

## Project Structure

| Crate | Description |
|---|---|
| `quasar-core` | ECS, app lifecycle, events, time, plugins, asset manager, scene graph |
| `quasar-math` | Transform, Color, glam re-exports |
| `quasar-render` | wgpu renderer, camera, meshes, textures, materials, camera controllers |
| `quasar-window` | Window creation & input (winit) |
| `quasar-physics` | Rapier3D integration |
| `quasar-audio` | Kira audio playback |
| `quasar-scripting` | Lua scripting via mlua |
| `quasar-editor` | egui-based scene editor |
| `quasar-engine` | Meta-crate with prelude |

## How to Contribute

### Reporting Issues

- Search existing issues before filing a new one.
- Include: Rust version, OS, GPU info, minimal reproduction steps.

### Pull Requests

1. **Create a branch** off `master` for your feature or fix.
2. **Write tests** — we aim for every module to have unit tests.
3. **Run the full CI check locally** before pushing:
   ```bash
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --all -- --check
   ```
4. **Keep commits atomic** — one logical change per commit.
5. **Write clear commit messages** using conventional prefixes:
   - `feat:` new feature
   - `fix:` bug fix
   - `refactor:` code restructuring
   - `style:` formatting / clippy
   - `docs:` documentation
   - `test:` tests only
   - `chore:` build / CI changes

### Code Style

- Follow `rustfmt` defaults (run `cargo fmt --all`).
- Zero clippy warnings (`cargo clippy --workspace -- -D warnings`).
- Public items need doc comments (`///`).
- Prefer `snake_case` for functions/variables, `PascalCase` for types.
- Keep modules focused — one responsibility per file.

### Testing

- Place unit tests in the same file under `#[cfg(test)] mod tests { ... }`.
- Integration tests go in `tests/` directories per crate.
- GPU-dependent tests should be marked `#[ignore]` so CI works headless.

## Architecture Guidelines

- **ECS-first**: Game state lives in the World as components.
- **Handle-based assets**: Use `AssetManager` for loading/caching.
- **Plugin system**: New subsystems should implement the `Plugin` trait.
- **No unwrap in library code**: Use `Result` or `Option` propagation.
- **Minimal dependencies**: Justify new crates in your PR.

## License

By contributing, you agree that your contributions will be licensed under
the MIT license.
