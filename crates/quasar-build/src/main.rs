//! # quasar-build
//!
//! CLI tool that packages a Quasar project for distribution.
//!
//! Stages:
//! 1. Validate the project manifest (`quasar-project.json`).
//! 2. Copy / process assets (compress textures, strip editor-only data).
//! 3. Invoke `cargo build` for the chosen target.
//! 4. Bundle the final artefact.
//!
//! Usage:
//! ```text
//! quasar-build --project ./my_game --target windows --release
//! quasar-build --project ./my_game --target web
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

fn main() {
    env_logger::init();
    let args = parse_args();
    if let Err(e) = run(args) {
        eprintln!("quasar-build: error: {e}");
        std::process::exit(1);
    }
}

// ── CLI args ────────────────────────────────────────────────────

#[derive(Debug)]
struct BuildArgs {
    project_dir: PathBuf,
    target: BuildTarget,
    release: bool,
    output_dir: Option<PathBuf>,
    compress_textures: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildTarget {
    Windows,
    Linux,
    MacOs,
    Web,
    Android,
    Ios,
}

impl BuildTarget {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "windows" | "win" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            "macos" | "mac" => Some(Self::MacOs),
            "web" | "wasm" => Some(Self::Web),
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            _ => None,
        }
    }

    fn cargo_target_triple(&self) -> Option<&'static str> {
        match self {
            Self::Web => Some("wasm32-unknown-unknown"),
            Self::Android => Some("aarch64-linux-android"),
            Self::Ios => Some("aarch64-apple-ios"),
            // Native host — no explicit triple needed.
            _ => None,
        }
    }
}

fn parse_args() -> BuildArgs {
    let mut args = std::env::args().skip(1);
    let mut project_dir = PathBuf::from(".");
    let mut target = BuildTarget::Windows;
    let mut release = false;
    let mut output_dir: Option<PathBuf> = None;
    let mut compress_textures = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" | "-p" => {
                if let Some(v) = args.next() {
                    project_dir = PathBuf::from(v);
                }
            }
            "--target" | "-t" => {
                if let Some(v) = args.next() {
                    target = BuildTarget::from_str(&v).unwrap_or_else(|| {
                        eprintln!("Unknown target '{v}', defaulting to windows");
                        BuildTarget::Windows
                    });
                }
            }
            "--release" | "-r" => release = true,
            "--compress-textures" => compress_textures = true,
            "--output" | "-o" => {
                if let Some(v) = args.next() {
                    output_dir = Some(PathBuf::from(v));
                }
            }
            other => {
                eprintln!("Unknown flag '{other}'");
            }
        }
    }
    BuildArgs {
        project_dir,
        target,
        release,
        output_dir,
        compress_textures,
    }
}

// ── project manifest ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct ProjectManifest {
    name: String,
    version: String,
    #[serde(default)]
    entry_crate: String,
    #[serde(default)]
    assets_dir: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    extra: HashMap<String, serde_json::Value>,
}

fn load_manifest(project_dir: &Path) -> Result<ProjectManifest, String> {
    let manifest_path = project_dir.join("quasar-project.json");
    if !manifest_path.exists() {
        return Err(format!(
            "No quasar-project.json found in {}",
            project_dir.display()
        ));
    }
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid manifest JSON: {e}"))
}

// ── pipeline ────────────────────────────────────────────────────

fn run(args: BuildArgs) -> Result<(), String> {
    log::info!("quasar-build starting: target={:?} release={}", args.target, args.release);

    let manifest = load_manifest(&args.project_dir)?;
    log::info!("Project: {} v{}", manifest.name, manifest.version);

    let out_dir = args
        .output_dir
        .unwrap_or_else(|| args.project_dir.join("build_output"));
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Cannot create output directory: {e}"))?;

    // 1. Process assets.
    let assets_src = if manifest.assets_dir.is_empty() {
        args.project_dir.join("assets")
    } else {
        args.project_dir.join(&manifest.assets_dir)
    };
    let assets_dst = out_dir.join("assets");
    if assets_src.exists() {
        copy_assets(&assets_src, &assets_dst, args.compress_textures)?;
        log::info!("Assets copied to {}", assets_dst.display());
    }

    // 2. Cargo build.
    let entry_crate = if manifest.entry_crate.is_empty() {
        manifest.name.clone()
    } else {
        manifest.entry_crate.clone()
    };
    cargo_build(&args.project_dir, &entry_crate, args.target, args.release, &manifest.features)?;

    // 3. Copy binary into output.
    copy_binary(&args.project_dir, &out_dir, &entry_crate, args.target, args.release)?;

    log::info!("Build complete → {}", out_dir.display());
    println!("✔ Build output written to {}", out_dir.display());
    Ok(())
}

// ── asset processing ────────────────────────────────────────────

fn copy_assets(src: &Path, dst: &Path, compress_textures: bool) -> Result<(), String> {
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| format!("clean assets dir: {e}"))?;
    }
    copy_dir_recursive(src, dst, compress_textures)
}

fn copy_dir_recursive(src: &Path, dst: &Path, compress_textures: bool) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("readdir {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path, compress_textures)?;
        } else {
            // Skip editor-only files.
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(".editor") || name_str.ends_with(".editor.json") {
                continue;
            }
            // Compress textures if flag is set.
            if compress_textures && is_texture_file(&name_str) {
                if let Err(e) = compress_texture(&path, &dest_path) {
                    log::warn!("Texture compression failed for {}: {e}, copying as-is", path.display());
                    fs::copy(&path, &dest_path)
                        .map_err(|e| format!("copy {} → {}: {e}", path.display(), dest_path.display()))?;
                }
            } else {
                fs::copy(&path, &dest_path)
                    .map_err(|e| format!("copy {} → {}: {e}", path.display(), dest_path.display()))?;
            }
        }
    }
    Ok(())
}

fn is_texture_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

fn compress_texture(src: &Path, dst: &Path) -> Result<(), String> {
    let img = image::open(src).map_err(|e| format!("{e}"))?;
    // Resize to max 2048×2048 if larger, preserving aspect ratio.
    let max_dim = 2048u32;
    let img = if img.width() > max_dim || img.height() > max_dim {
        img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    // Save as JPEG with 80% quality for lossy compression.
    let dest_path = dst.with_extension("jpg");
    let mut file = fs::File::create(&dest_path).map_err(|e| format!("{e}"))?;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 80);
    img.write_with_encoder(encoder).map_err(|e| format!("{e}"))?;
    log::debug!("Compressed {} → {}", src.display(), dest_path.display());
    Ok(())
}

// ── cargo build ─────────────────────────────────────────────────

fn cargo_build(
    project_dir: &Path,
    crate_name: &str,
    target: BuildTarget,
    release: bool,
    features: &[String],
) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("-p").arg(crate_name);
    if release {
        cmd.arg("--release");
    }
    if let Some(triple) = target.cargo_target_triple() {
        cmd.arg("--target").arg(triple);
    }
    if !features.is_empty() {
        cmd.arg("--features").arg(features.join(","));
    }
    cmd.current_dir(project_dir);

    log::info!("Running: {:?}", cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build failed (exit {})", status));
    }
    Ok(())
}

// ── copy binary ─────────────────────────────────────────────────

fn copy_binary(
    project_dir: &Path,
    out_dir: &Path,
    crate_name: &str,
    target: BuildTarget,
    release: bool,
) -> Result<(), String> {
    let profile_dir = if release { "release" } else { "debug" };

    let target_base = if let Some(triple) = target.cargo_target_triple() {
        project_dir.join("target").join(triple).join(profile_dir)
    } else {
        project_dir.join("target").join(profile_dir)
    };

    let bin_name = if cfg!(windows) && target == BuildTarget::Windows {
        format!("{crate_name}.exe")
    } else {
        crate_name.to_string()
    };

    let src_bin = target_base.join(&bin_name);
    if src_bin.exists() {
        let dst_bin = out_dir.join(&bin_name);
        fs::copy(&src_bin, &dst_bin)
            .map_err(|e| format!("copy binary: {e}"))?;
        log::info!("Binary copied to {}", dst_bin.display());
    } else {
        log::warn!("Binary not found at {} — skipping copy", src_bin.display());
    }

    Ok(())
}
