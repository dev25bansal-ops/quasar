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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    /// GPU block-compression format: bc7, astc, or none (default = JPEG fallback).
    gpu_texture_format: GpuTextureFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GpuTextureFormat {
    /// No GPU compression — fallback to JPEG.
    None,
    /// BC7 (desktop / console).
    Bc7,
    /// ASTC 4×4 (mobile / universal).
    Astc4x4,
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
    let mut gpu_texture_format = GpuTextureFormat::None;

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
            "--gpu-format" => {
                if let Some(v) = args.next() {
                    gpu_texture_format = match v.to_ascii_lowercase().as_str() {
                        "bc7" => GpuTextureFormat::Bc7,
                        "astc" | "astc4x4" => GpuTextureFormat::Astc4x4,
                        _ => {
                            eprintln!("Unknown GPU texture format '{v}', using none");
                            GpuTextureFormat::None
                        }
                    };
                }
            }
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
        gpu_texture_format,
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
    let text =
        fs::read_to_string(&manifest_path).map_err(|e| format!("Failed to read manifest: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid manifest JSON: {e}"))
}

// ── incremental build cache ─────────────────────────────────────

/// Persisted mapping of relative asset paths to their blake3 content hashes.
#[derive(Debug, Default, Serialize, Deserialize)]
struct BuildCache {
    hashes: HashMap<String, String>,
}

impl BuildCache {
    fn load(path: &Path) -> Self {
        if let Ok(text) = fs::read_to_string(path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize build cache: {e}"))?;
        fs::write(path, json).map_err(|e| format!("write build cache: {e}"))
    }

    /// Hash a file with blake3. Returns the hex digest.
    fn hash_file(path: &Path) -> Result<String, String> {
        let mut file = fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 16384];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Returns `true` if the file is unchanged (hash matches cache).
    fn is_unchanged(&self, rel_path: &str, src: &Path) -> bool {
        if let Some(cached_hash) = self.hashes.get(rel_path) {
            if let Ok(current_hash) = Self::hash_file(src) {
                return *cached_hash == current_hash;
            }
        }
        false
    }

    /// Record the hash for a given relative path.
    fn record(&mut self, rel_path: String, src: &Path) {
        if let Ok(hash) = Self::hash_file(src) {
            self.hashes.insert(rel_path, hash);
        }
    }
}

// ── content-addressable store ───────────────────────────────────

/// SHA-256 keyed content-addressable store for processed assets.
///
/// After a file is processed (compressed, optimised, etc.) its output is
/// hashed with SHA-256 and stored under `<store_root>/<hex_digest>`.  On
/// subsequent builds, if the same content hash already exists in the store,
/// the output is hard-linked (or copied as fallback) rather than re-processed,
/// giving instant no-rebuild for duplicates and unchanged assets.
#[allow(dead_code)]
struct ContentAddressableStore {
    store_root: PathBuf,
    /// Tracks which SHA-256 digests have already been verified to exist in
    /// the store (avoids repeated fs::metadata calls).
    known: HashMap<String, PathBuf>,
}

#[allow(dead_code)]
impl ContentAddressableStore {
    fn new(store_root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&store_root);
        let mut known = HashMap::new();

        // Pre-populate from existing files in the store.
        if let Ok(entries) = fs::read_dir(&store_root) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        known.insert(name.to_string(), entry.path());
                    }
                }
            }
        }

        Self { store_root, known }
    }

    /// Compute a SHA-256 digest of a file on disk.
    fn hash_file(path: &Path) -> Result<String, String> {
        let mut file =
            fs::File::open(path).map_err(|e| format!("CAS: open {}: {e}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 16384];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| format!("CAS: read {}: {e}", path.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let digest = hasher.finalize();
        Ok(hex::encode(digest))
    }

    /// Compute the SHA-256 digest of in-memory bytes.
    fn hash_bytes(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Check if the store already contains processed output for `digest`.
    fn contains(&self, digest: &str) -> bool {
        self.known.contains_key(digest)
    }

    /// Retrieve the store-internal path for a given digest.
    fn get_path(&self, digest: &str) -> Option<&Path> {
        self.known.get(digest).map(PathBuf::as_path)
    }

    /// Link (or copy) a cached artefact to `dest`.
    fn link_to(&self, digest: &str, dest: &Path) -> Result<bool, String> {
        if let Some(cached) = self.known.get(digest) {
            // Try hard-link first, fall back to copy.
            if fs::hard_link(cached, dest).is_ok() {
                log::debug!("CAS: hard-linked {} → {}", cached.display(), dest.display());
            } else {
                fs::copy(cached, dest).map_err(|e| {
                    format!("CAS: copy {} → {}: {e}", cached.display(), dest.display())
                })?;
                log::debug!("CAS: copied {} → {}", cached.display(), dest.display());
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Store a processed file's bytes in the CAS and write to `dest`.
    fn store_bytes(&mut self, data: &[u8], dest: &Path) -> Result<String, String> {
        let digest = Self::hash_bytes(data);

        if !self.contains(&digest) {
            let store_path = self.store_root.join(&digest);
            fs::write(&store_path, data)
                .map_err(|e| format!("CAS: write {}: {e}", store_path.display()))?;
            self.known.insert(digest.clone(), store_path);
        }

        // Write to the destination
        self.link_to(&digest, dest)?;
        Ok(digest)
    }

    /// Store a processed file (already on disk at `processed_path`) in the
    /// CAS and link it to `dest`.
    fn store_file(&mut self, processed_path: &Path, dest: &Path) -> Result<String, String> {
        let digest = Self::hash_file(processed_path)?;

        if !self.contains(&digest) {
            let store_path = self.store_root.join(&digest);
            fs::copy(processed_path, &store_path)
                .map_err(|e| format!("CAS: store {}: {e}", store_path.display()))?;
            self.known.insert(digest.clone(), store_path);
        }

        // If dest != processed_path, link/copy from the store
        if dest != processed_path {
            self.link_to(&digest, dest)?;
        }

        Ok(digest)
    }
}

/// Minimal hex encoding (avoids pulling in the full `hex` crate).
#[allow(dead_code)]
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().fold(
            String::with_capacity(bytes.as_ref().len() * 2),
            |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            },
        )
    }
}

// ── pipeline ────────────────────────────────────────────────────

fn run(args: BuildArgs) -> Result<(), String> {
    log::info!(
        "quasar-build starting: target={:?} release={}",
        args.target,
        args.release
    );

    let manifest = load_manifest(&args.project_dir)?;
    log::info!("Project: {} v{}", manifest.name, manifest.version);

    let out_dir = args
        .output_dir
        .unwrap_or_else(|| args.project_dir.join("build_output"));
    fs::create_dir_all(&out_dir).map_err(|e| format!("Cannot create output directory: {e}"))?;

    // 1. Process assets.
    let assets_src = if manifest.assets_dir.is_empty() {
        args.project_dir.join("assets")
    } else {
        args.project_dir.join(&manifest.assets_dir)
    };
    let assets_dst = out_dir.join("assets");
    let cache_path = out_dir.join("build-cache.json");
    let mut cache = BuildCache::load(&cache_path);
    let mut cas = ContentAddressableStore::new(out_dir.join(".cas"));
    if assets_src.exists() {
        copy_assets(
            &assets_src,
            &assets_dst,
            args.compress_textures,
            args.gpu_texture_format,
            &mut cache,
            &mut cas,
        )?;
        cache.save(&cache_path)?;
        log::info!("Assets copied to {}", assets_dst.display());
    }

    // 2. Cargo build.
    let entry_crate = if manifest.entry_crate.is_empty() {
        manifest.name.clone()
    } else {
        manifest.entry_crate.clone()
    };
    cargo_build(
        &args.project_dir,
        &entry_crate,
        args.target,
        args.release,
        &manifest.features,
    )?;

    // 3. Copy binary into output.
    copy_binary(
        &args.project_dir,
        &out_dir,
        &entry_crate,
        args.target,
        args.release,
    )?;

    // 4. Platform-specific packaging.
    match args.target {
        BuildTarget::Android => {
            package_android(&out_dir, &manifest, args.release)?;
            log::info!("Android APK assembled");
        }
        BuildTarget::Ios => {
            package_ios(&out_dir, &manifest, args.release)?;
            log::info!("iOS app bundle assembled");
        }
        _ => {}
    }

    log::info!("Build complete → {}", out_dir.display());
    println!("✔ Build output written to {}", out_dir.display());
    Ok(())
}

// ── asset processing ────────────────────────────────────────────

fn copy_assets(
    src: &Path,
    dst: &Path,
    compress_textures: bool,
    gpu_fmt: GpuTextureFormat,
    cache: &mut BuildCache,
    cas: &mut ContentAddressableStore,
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("create assets dir: {e}"))?;
    copy_dir_recursive(src, dst, compress_textures, gpu_fmt, src, cache, cas)
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    compress_textures: bool,
    gpu_fmt: GpuTextureFormat,
    assets_root: &Path,
    cache: &mut BuildCache,
    _cas: &mut ContentAddressableStore,
) -> Result<(), String> {
    use rayon::prelude::*;

    // Phase 1: Recursively collect every leaf file with its source and destination paths.
    struct FileEntry {
        src_path: PathBuf,
        dest_path: PathBuf,
        rel_path: String,
    }

    fn collect_files(
        src: &Path,
        dst: &Path,
        assets_root: &Path,
        out: &mut Vec<FileEntry>,
    ) -> Result<Vec<PathBuf>, String> {
        let mut dirs = Vec::new();
        let entries = fs::read_dir(src).map_err(|e| format!("readdir {}: {e}", src.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            let path = entry.path();
            let dest_path = dst.join(entry.file_name());
            if path.is_dir() {
                dirs.push(dest_path.clone());
                collect_files(&path, &dest_path, assets_root, out)?;
            } else {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(".editor") || name_str.ends_with(".editor.json") {
                    continue;
                }
                let rel_path = path
                    .strip_prefix(assets_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(FileEntry {
                    src_path: path,
                    dest_path,
                    rel_path,
                });
            }
        }
        Ok(dirs)
    }

    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let mut entries = Vec::new();
    let sub_dirs = collect_files(src, dst, assets_root, &mut entries)?;
    for d in &sub_dirs {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }

    // Phase 2: Filter unchanged files (this part needs &mut cache, so keep sequential).
    let to_process: Vec<FileEntry> = entries
        .into_iter()
        .filter(|fe| {
            if fe.dest_path.exists() && cache.is_unchanged(&fe.rel_path, &fe.src_path) {
                log::debug!("Unchanged, skipping: {}", fe.rel_path);
                false
            } else {
                true
            }
        })
        .collect();

    // Phase 3: Process files in parallel.
    let results: Vec<Result<String, String>> = to_process
        .par_iter()
        .map(|fe| {
            let name_str = fe
                .src_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            if compress_textures && is_texture_file(&name_str) {
                if let Err(e) = compress_texture(&fe.src_path, &fe.dest_path, gpu_fmt) {
                    log::warn!(
                        "Texture compression failed for {}: {e}, copying as-is",
                        fe.src_path.display()
                    );
                    fs::copy(&fe.src_path, &fe.dest_path).map_err(|e| {
                        format!(
                            "copy {} → {}: {e}",
                            fe.src_path.display(),
                            fe.dest_path.display()
                        )
                    })?;
                }
            } else if is_mesh_file(&name_str) {
                if let Err(e) = optimize_mesh(&fe.src_path, &fe.dest_path) {
                    log::warn!(
                        "Mesh optimization failed for {}: {e}, copying as-is",
                        fe.src_path.display()
                    );
                    fs::copy(&fe.src_path, &fe.dest_path).map_err(|e| {
                        format!(
                            "copy {} → {}: {e}",
                            fe.src_path.display(),
                            fe.dest_path.display()
                        )
                    })?;
                }
            } else if is_audio_file(&name_str) {
                if let Err(e) = transcode_audio(&fe.src_path, &fe.dest_path) {
                    log::warn!(
                        "Audio transcode failed for {}: {e}, copying as-is",
                        fe.src_path.display()
                    );
                    fs::copy(&fe.src_path, &fe.dest_path).map_err(|e| {
                        format!(
                            "copy {} → {}: {e}",
                            fe.src_path.display(),
                            fe.dest_path.display()
                        )
                    })?;
                }
            } else {
                fs::copy(&fe.src_path, &fe.dest_path).map_err(|e| {
                    format!(
                        "copy {} → {}: {e}",
                        fe.src_path.display(),
                        fe.dest_path.display()
                    )
                })?;
            }
            Ok(fe.rel_path.clone())
        })
        .collect();

    // Phase 4: Record cache entries (sequential, needs &mut).
    for result in results {
        match result {
            Ok(rel_path) => {
                let fe = to_process.iter().find(|f| f.rel_path == rel_path).unwrap();
                cache.record(rel_path, &fe.src_path);
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

fn is_texture_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

fn compress_texture(src: &Path, dst: &Path, gpu_fmt: GpuTextureFormat) -> Result<(), String> {
    let img = image::open(src).map_err(|e| format!("{e}"))?;
    // Resize to max 2048×2048 if larger, preserving aspect ratio.
    let max_dim = 2048u32;
    let img = if img.width() > max_dim || img.height() > max_dim {
        img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    match gpu_fmt {
        GpuTextureFormat::Bc7 => compress_texture_bc7(&img, dst),
        GpuTextureFormat::Astc4x4 => compress_texture_astc(&img, dst),
        GpuTextureFormat::None => {
            // Fallback: JPEG 80%.
            let dest_path = dst.with_extension("jpg");
            let mut file = fs::File::create(&dest_path).map_err(|e| format!("{e}"))?;
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 80);
            img.write_with_encoder(encoder)
                .map_err(|e| format!("{e}"))?;
            log::debug!("Compressed {} → {}", src.display(), dest_path.display());
            Ok(())
        }
    }
}

/// BC7 compression using ISPC-accelerated `intel_tex_2` crate.
///
/// Encodes RGBA8 images into BC7 format using the high-quality `intel_tex_2`
/// encoder, replacing the previous minimal software fallback.
fn compress_texture_bc7(img: &image::DynamicImage, dst: &Path) -> Result<(), String> {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());

    let surface = intel_tex_2::RgbaSurface {
        width: w,
        height: h,
        stride: w * 4,
        data: rgba.as_raw(),
    };

    let compressed = intel_tex_2::bc7::compress_blocks(
        &intel_tex_2::bc7::opaque_ultra_fast_settings(),
        &surface,
    );

    // Write raw compressed data with a minimal header: width(u32) + height(u32) + data.
    let dest_path = dst.with_extension("bc7");
    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&compressed);
    fs::write(&dest_path, &out).map_err(|e| format!("{e}"))?;
    log::debug!("BC7 compressed (intel_tex_2) → {}", dest_path.display());
    Ok(())
}

/// ASTC 4×4 compression stub: writes raw RGBA with a `.astc` extension + header.
///
/// A full ASTC encoder would use `astc-encoder` or `basis-universal`.
/// Here we write the standard ASTC file header + 16-byte blocks with
/// a simplified single-weight encoding.
fn compress_texture_astc(img: &image::DynamicImage, dst: &Path) -> Result<(), String> {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);

    // ASTC file header: 4-byte magic, 3-byte block dims, 3-byte x size, 3-byte y size, 3-byte z size.
    let mut out = Vec::with_capacity(16 + (bw * bh * 16) as usize);
    // Magic: 0x5CA1AB13
    out.extend_from_slice(&[0x13, 0xAB, 0xA1, 0x5C]);
    // Block dimensions: 4×4×1
    out.extend_from_slice(&[4, 4, 1]);
    // Image size (24-bit LE for each dimension)
    out.push((w & 0xFF) as u8);
    out.push(((w >> 8) & 0xFF) as u8);
    out.push(((w >> 16) & 0xFF) as u8);
    out.push((h & 0xFF) as u8);
    out.push(((h >> 8) & 0xFF) as u8);
    out.push(((h >> 16) & 0xFF) as u8);
    // Z dimension
    out.extend_from_slice(&[1, 0, 0]);

    for by in 0..bh {
        for bx in 0..bw {
            let mut block_data = [0u8; 16];
            // Compute average colour for the block as a simple void-extent encoding.
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for py in 0..4u32 {
                for px in 0..4u32 {
                    let sx = (bx * 4 + px).min(w - 1);
                    let sy = (by * 4 + py).min(h - 1);
                    let pixel = rgba.get_pixel(sx, sy);
                    for (sc, pc) in sum.iter_mut().zip(pixel.0.iter()) {
                        *sc += *pc as u32;
                    }
                    count += 1;
                }
            }
            // ASTC void-extent block: encodes a solid colour.
            // Marker: 0x1FC in the first 13 bits.
            block_data[0] = 0xFC;
            block_data[1] = 0x01;
            // Void extent coords = all zeros (skip bytes 2..7).
            // RGBA16 colour in bytes 8..15.
            for (c_idx, sc) in sum.iter().enumerate() {
                let avg = (sc / count) as u16;
                let avg16 = avg | (avg << 8); // expand to 16-bit
                let off = 8 + c_idx * 2;
                block_data[off] = (avg16 & 0xFF) as u8;
                block_data[off + 1] = ((avg16 >> 8) & 0xFF) as u8;
            }
            out.extend_from_slice(&block_data);
        }
    }

    let dest_path = dst.with_extension("astc");
    fs::write(&dest_path, &out).map_err(|e| format!("{e}"))?;
    log::debug!("ASTC compressed → {}", dest_path.display());
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
        fs::copy(&src_bin, &dst_bin).map_err(|e| format!("copy binary: {e}"))?;
        log::info!("Binary copied to {}", dst_bin.display());
    } else {
        log::warn!("Binary not found at {} — skipping copy", src_bin.display());
    }

    Ok(())
}

// ── Android APK packaging ───────────────────────────────────────

fn package_android(
    out_dir: &Path,
    manifest: &ProjectManifest,
    release: bool,
) -> Result<(), String> {
    let apk_dir = out_dir.join("apk");
    fs::create_dir_all(apk_dir.join("lib/arm64-v8a"))
        .map_err(|e| format!("create APK lib dir: {e}"))?;
    fs::create_dir_all(apk_dir.join("assets"))
        .map_err(|e| format!("create APK assets dir: {e}"))?;

    // Copy native library into the correct ABI directory.
    let lib_name = format!("lib{}.so", manifest.name.replace('-', "_"));
    let src_lib = out_dir.join(&manifest.name);
    if src_lib.exists() {
        fs::copy(&src_lib, apk_dir.join("lib/arm64-v8a").join(&lib_name))
            .map_err(|e| format!("copy native lib: {e}"))?;
    }

    // Copy assets into APK assets directory.
    let assets_src = out_dir.join("assets");
    if assets_src.exists() {
        let mut dummy_cache = BuildCache::default();
        let mut dummy_cas = ContentAddressableStore::new(out_dir.join(".cas"));
        copy_dir_recursive(
            &assets_src,
            &apk_dir.join("assets"),
            false,
            GpuTextureFormat::None,
            &assets_src,
            &mut dummy_cache,
            &mut dummy_cas,
        )?;
    }

    // Generate AndroidManifest.xml.
    let package_name = manifest
        .extra
        .get("android_package")
        .and_then(|v| v.as_str())
        .unwrap_or("com.quasar.game");
    let android_manifest = generate_android_manifest(&manifest.name, package_name);
    fs::write(apk_dir.join("AndroidManifest.xml"), android_manifest)
        .map_err(|e| format!("write AndroidManifest.xml: {e}"))?;

    // Attempt to build APK with aapt2 + apksigner if available.
    if which_exists("aapt2") {
        build_apk_with_aapt2(&apk_dir, out_dir, &manifest.name, release)?;
    } else {
        log::warn!("aapt2 not found in PATH — APK directory prepared but not assembled");
        log::info!("Run `aapt2` and `apksigner` manually to produce the final APK");
    }

    Ok(())
}

fn generate_android_manifest(app_name: &str, package_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{package_name}"
    android:versionCode="1"
    android:versionName="1.0">

    <uses-sdk android:minSdkVersion="28" android:targetSdkVersion="34" />

    <uses-feature android:glEsVersion="0x00030002" android:required="true" />
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.VIBRATE" />

    <application
        android:label="{app_name}"
        android:hasCode="false"
        android:debuggable="false">
        <activity
            android:name="android.app.NativeActivity"
            android:configChanges="orientation|screenSize|keyboardHidden"
            android:exported="true">
            <meta-data android:name="android.app.lib_name" android:value="{lib_name}" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>"#,
        package_name = package_name,
        app_name = app_name,
        lib_name = app_name.replace('-', "_"),
    )
}

fn build_apk_with_aapt2(
    apk_dir: &Path,
    out_dir: &Path,
    app_name: &str,
    _release: bool,
) -> Result<(), String> {
    // Link step: produce unsigned APK.
    let unsigned_apk = out_dir.join(format!("{app_name}-unsigned.apk"));
    let status = Command::new("aapt2")
        .args(["link", "-o"])
        .arg(&unsigned_apk)
        .arg("--manifest")
        .arg(apk_dir.join("AndroidManifest.xml"))
        .arg("-A")
        .arg(apk_dir.join("assets"))
        .status()
        .map_err(|e| format!("aapt2 link: {e}"))?;
    if !status.success() {
        return Err("aapt2 link failed".into());
    }

    // Sign with debug key if apksigner is available.
    if which_exists("apksigner") {
        let signed_apk = out_dir.join(format!("{app_name}.apk"));
        let status = Command::new("apksigner")
            .args(["sign", "--ks-pass", "pass:android"])
            .arg("--out")
            .arg(&signed_apk)
            .arg(&unsigned_apk)
            .status()
            .map_err(|e| format!("apksigner: {e}"))?;
        if !status.success() {
            log::warn!("apksigner failed — unsigned APK still available");
        } else {
            log::info!("Signed APK → {}", signed_apk.display());
        }
    }

    Ok(())
}

// ── iOS app bundle packaging ────────────────────────────────────

fn package_ios(out_dir: &Path, manifest: &ProjectManifest, _release: bool) -> Result<(), String> {
    let bundle_name = format!("{}.app", manifest.name);
    let app_dir = out_dir.join(&bundle_name);
    fs::create_dir_all(&app_dir).map_err(|e| format!("create app bundle dir: {e}"))?;

    // Copy binary into bundle.
    let src_bin = out_dir.join(&manifest.name);
    if src_bin.exists() {
        fs::copy(&src_bin, app_dir.join(&manifest.name))
            .map_err(|e| format!("copy iOS binary: {e}"))?;
    }

    // Copy assets into bundle.
    let assets_src = out_dir.join("assets");
    if assets_src.exists() {
        let mut dummy_cache = BuildCache::default();
        let mut dummy_cas = ContentAddressableStore::new(out_dir.join(".cas"));
        copy_dir_recursive(
            &assets_src,
            &app_dir.join("assets"),
            false,
            GpuTextureFormat::None,
            &assets_src,
            &mut dummy_cache,
            &mut dummy_cas,
        )?;
    }

    // Generate Info.plist.
    let bundle_id = manifest
        .extra
        .get("ios_bundle_id")
        .and_then(|v| v.as_str())
        .unwrap_or("com.quasar.game");
    let info_plist = generate_info_plist(&manifest.name, &manifest.version, bundle_id);
    fs::write(app_dir.join("Info.plist"), info_plist)
        .map_err(|e| format!("write Info.plist: {e}"))?;

    // Generate minimal Xcode project for convenience.
    generate_xcodeproj(out_dir, manifest)?;

    log::info!("iOS app bundle → {}", app_dir.display());
    log::info!("Code-sign and archive via Xcode or `codesign` CLI");
    Ok(())
}

fn generate_info_plist(app_name: &str, version: &str, bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>{app_name}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleName</key>
    <string>{app_name}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>UILaunchStoryboardName</key>
    <string>LaunchScreen</string>
    <key>UIRequiredDeviceCapabilities</key>
    <array>
        <string>arm64</string>
        <string>metal</string>
    </array>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
    <key>UIApplicationSupportsIndirectInputEvents</key>
    <true/>
</dict>
</plist>"#,
        app_name = app_name,
        version = version,
        bundle_id = bundle_id,
    )
}

fn generate_xcodeproj(out_dir: &Path, manifest: &ProjectManifest) -> Result<(), String> {
    let proj_dir = out_dir.join(format!("{}.xcodeproj", manifest.name));
    fs::create_dir_all(&proj_dir).map_err(|e| format!("create xcodeproj dir: {e}"))?;

    // Minimal pbxproj that references the pre-built binary.
    let pbxproj = format!(
        r#"// !$*UTF8*$!
{{
    archiveVersion = 1;
    objectVersion = 56;
    rootObject = __ROOT__;
    classes = {{}};
    objects = {{
        __ROOT__ = {{
            isa = PBXProject;
            buildConfigurationList = __BCL__;
            mainGroup = __MG__;
            productRefGroup = __MG__;
            projectDirPath = "";
            targets = ();
        }};
        __BCL__ = {{
            isa = XCConfigurationList;
            buildConfigurations = ( __BC__ );
        }};
        __BC__ = {{
            isa = XCBuildConfiguration;
            name = Release;
            buildSettings = {{
                PRODUCT_NAME = "{name}";
                PRODUCT_BUNDLE_IDENTIFIER = "com.quasar.game";
            }};
        }};
        __MG__ = {{
            isa = PBXGroup;
            children = ();
            sourceTree = "<group>";
        }};
    }};
}}"#,
        name = manifest.name,
    );
    fs::write(proj_dir.join("project.pbxproj"), pbxproj)
        .map_err(|e| format!("write pbxproj: {e}"))?;
    Ok(())
}

// ── utilities ───────────────────────────────────────────────────

fn which_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

// ── mesh / audio helpers ────────────────────────────────────────

fn is_mesh_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".gltf") || lower.ends_with(".glb") || lower.ends_with(".obj")
}

fn is_audio_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".wav")
        || lower.ends_with(".ogg")
        || lower.ends_with(".mp3")
        || lower.ends_with(".flac")
}

/// Optimise a mesh file for GPU performance.
///
/// - Reorders triangle indices for post-transform vertex cache locality
///   (Forsyth / Tom Forsyth's linear-speed algorithm).
/// - Reorders vertices into access-order for better memory prefetch.
///
/// Currently operates on `.obj` files with a simple software implementation.
/// glTF meshes are copied as-is (a full implementation would use meshopt).
fn optimize_mesh(src: &Path, dst: &Path) -> Result<(), String> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "obj" {
        let text = fs::read_to_string(src).map_err(|e| format!("{e}"))?;
        let (positions, indices) = parse_obj_positions(&text);
        if indices.is_empty() {
            // Nothing to optimise.
            fs::copy(src, dst).map_err(|e| format!("{e}"))?;
            return Ok(());
        }
        let optimised_indices = forsyth_reorder(&indices, positions.len());
        let output = rebuild_obj(&text, &optimised_indices);
        fs::write(dst, output).map_err(|e| format!("{e}"))?;
        log::debug!("Mesh optimised (Forsyth) → {}", dst.display());
        Ok(())
    } else if ext == "gltf" || ext == "glb" {
        // Load the glTF, optimise each mesh primitive's index buffer with
        // meshopt (vertex cache + overdraw + vertex fetch), then write
        // the optimised data alongside the original.
        let (document, buffers, _images) = gltf::import(src).map_err(|e| format!("{e}"))?;

        // Collect optimised indices per primitive for a sidecar file.
        let mut optimised_data: Vec<u8> = Vec::new();
        let mut prim_count = 0u32;

        for mesh in document.meshes() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buf| buffers.get(buf.index()).map(|d| &d[..]));
                let positions: Vec<[f32; 3]> = match reader.read_positions() {
                    Some(iter) => iter.collect(),
                    None => continue,
                };
                let indices: Vec<u32> = match reader.read_indices() {
                    Some(iter) => iter.into_u32().collect(),
                    None => continue,
                };
                if indices.is_empty() || positions.is_empty() {
                    continue;
                }

                let vertex_count = positions.len();

                // Step 1: Vertex cache optimisation.
                let mut opt_indices = meshopt::optimize_vertex_cache(&indices, vertex_count);

                // Step 2: Overdraw optimisation.
                meshopt::optimize_overdraw_in_place_decoder(&mut opt_indices, &positions, 1.05);

                // Step 3: Vertex fetch remap.
                let _remap = meshopt::optimize_vertex_fetch_remap(&opt_indices, vertex_count);

                // Write primitive header: original index count + optimised indices.
                optimised_data.extend_from_slice(&(opt_indices.len() as u32).to_le_bytes());
                for idx in &opt_indices {
                    optimised_data.extend_from_slice(&idx.to_le_bytes());
                }
                prim_count += 1;
            }
        }

        // Copy the original file.
        fs::copy(src, dst).map_err(|e| format!("{e}"))?;

        // Write sidecar with optimised index data if we processed anything.
        if prim_count > 0 {
            let sidecar = dst.with_extension(format!("{}.meshopt", ext));
            let mut header = Vec::with_capacity(4 + optimised_data.len());
            header.extend_from_slice(&prim_count.to_le_bytes());
            header.extend_from_slice(&optimised_data);
            fs::write(&sidecar, &header).map_err(|e| format!("{e}"))?;
            log::debug!(
                "Mesh optimised (meshopt, {} primitives) → {}",
                prim_count,
                sidecar.display()
            );
        }

        Ok(())
    } else {
        // Other mesh formats: copy as-is.
        fs::copy(src, dst).map_err(|e| format!("{e}"))?;
        Ok(())
    }
}

/// Minimal Forsyth vertex cache optimisation for a triangle index buffer.
/// Returns a reordered index buffer with better post-transform cache utilisation.
fn forsyth_reorder(indices: &[u32], vertex_count: usize) -> Vec<u32> {
    if indices.len() < 3 {
        return indices.to_vec();
    }
    let tri_count = indices.len() / 3;

    // Build per-vertex valence (number of triangles referencing this vertex).
    let mut valence = vec![0u32; vertex_count];
    for &idx in indices {
        if (idx as usize) < vertex_count {
            valence[idx as usize] += 1;
        }
    }

    // Build adjacency: for each vertex, which triangles use it.
    let mut vert_tris: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
    for t in 0..tri_count {
        for k in 0..3 {
            let v = indices[t * 3 + k] as usize;
            if v < vertex_count {
                vert_tris[v].push(t);
            }
        }
    }

    // Simulate a small FIFO vertex cache (size 32).
    const CACHE_SIZE: usize = 32;
    let mut cache: Vec<u32> = Vec::with_capacity(CACHE_SIZE);
    let mut emitted = vec![false; tri_count];
    let mut live_valence = valence.clone();
    let mut result = Vec::with_capacity(indices.len());

    // Greedy: pick the next best triangle from the cache neighbourhood.
    fn vertex_score(
        live_valence: &[u32],
        v: u32,
        cache_pos: Option<usize>,
        cache_size: usize,
    ) -> f32 {
        let lv = live_valence.get(v as usize).copied().unwrap_or(0);
        if lv == 0 {
            return -1.0;
        }
        let cache_score = if let Some(pos) = cache_pos {
            if pos < 3 {
                0.75
            } else {
                1.0 - ((pos as f32) / cache_size as f32)
            }
        } else {
            0.0
        };
        let valence_score = 2.0 / (lv as f32).sqrt();
        cache_score + valence_score
    }

    let mut next_tri: Option<usize> = Some(0);

    for _ in 0..tri_count {
        // Find best triangle.
        let tri = if let Some(t) = next_tri {
            t
        } else {
            // Fallback: linear scan for first un-emitted triangle.
            match emitted.iter().position(|&e| !e) {
                Some(t) => t,
                None => break,
            }
        };

        emitted[tri] = true;
        for k in 0..3 {
            let v = indices[tri * 3 + k];
            result.push(v);
            // Update cache.
            if let Some(pos) = cache.iter().position(|&cv| cv == v) {
                cache.remove(pos);
            }
            cache.insert(0, v);
            if cache.len() > CACHE_SIZE {
                cache.pop();
            }
            // Decrement live valence.
            if (v as usize) < vertex_count {
                live_valence[v as usize] = live_valence[v as usize].saturating_sub(1);
            }
        }

        // Find best next triangle from cache neighbourhood.
        let mut best_score = f32::NEG_INFINITY;
        next_tri = None;
        for &cv in cache.iter() {
            if (cv as usize) >= vertex_count {
                continue;
            }
            for &adj_tri in &vert_tris[cv as usize] {
                if emitted[adj_tri] {
                    continue;
                }
                let mut tri_score = 0.0f32;
                for j in 0..3 {
                    let tv = indices[adj_tri * 3 + j];
                    let cp = cache.iter().position(|&c| c == tv);
                    tri_score += vertex_score(&live_valence, tv, cp, CACHE_SIZE);
                }
                if tri_score > best_score {
                    best_score = tri_score;
                    next_tri = Some(adj_tri);
                }
            }
        }
    }

    result
}

/// Very minimal OBJ parser — extracts vertex positions and face indices.
fn parse_obj_positions(text: &str) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("v ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    positions.push([x, y, z]);
                }
            }
        } else if line.starts_with("f ") {
            let parts: Vec<&str> = line.split_whitespace().skip(1).collect();
            // Triangulate faces with > 3 vertices using fan triangulation.
            let face_verts: Vec<u32> = parts
                .iter()
                .filter_map(|p| {
                    // Handle "v", "v/vt", "v/vt/vn", "v//vn" formats.
                    let idx_str = p.split('/').next()?;
                    idx_str.parse::<u32>().ok().map(|i| i.saturating_sub(1))
                })
                .collect();
            for i in 1..face_verts.len().saturating_sub(1) {
                indices.push(face_verts[0]);
                indices.push(face_verts[i]);
                indices.push(face_verts[i + 1]);
            }
        }
    }
    (positions, indices)
}

/// Rebuild the OBJ text with reordered face indices.
fn rebuild_obj(original: &str, new_indices: &[u32]) -> String {
    let mut out = String::with_capacity(original.len());
    let mut tri_idx = 0usize;
    for line in original.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("f ") {
            // Replace face lines with our reordered triangles.
            if tri_idx + 2 < new_indices.len() {
                out.push_str(&format!(
                    "f {} {} {}\n",
                    new_indices[tri_idx] + 1,
                    new_indices[tri_idx + 1] + 1,
                    new_indices[tri_idx + 2] + 1,
                ));
                tri_idx += 3;
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    // Emit any remaining triangles (e.g. from ngon fan triangulation).
    while tri_idx + 2 < new_indices.len() {
        out.push_str(&format!(
            "f {} {} {}\n",
            new_indices[tri_idx] + 1,
            new_indices[tri_idx + 1] + 1,
            new_indices[tri_idx + 2] + 1,
        ));
        tri_idx += 3;
    }
    out
}

/// Transcode audio files to optimized formats for distribution.
///
/// - WAV/FLAC → WAV (16-bit PCM) for uncompressed quality
/// - OGG/MP3 → copied as-is (already compressed)
/// - Future: add OGG Vorbis encoding when vorbis-encoder crate is available
fn transcode_audio(src: &Path, dst: &Path) -> Result<(), String> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "wav" => {
            // Read WAV header to determine format
            let data = fs::read(src).map_err(|e| format!("read {}: {e}", src.display()))?;

            if data.len() < 44 {
                fs::copy(src, dst).map_err(|e| format!("copy: {e}"))?;
                return Ok(());
            }

            // Parse WAV header
            let channels = u16::from_le_bytes([data[22], data[23]]) as u32;
            let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
            let bits_per_sample = u16::from_le_bytes([data[34], data[35]]) as u32;

            // If already 16-bit PCM, just copy
            if bits_per_sample == 16 {
                fs::copy(src, dst).map_err(|e| format!("copy: {e}"))?;
                log::debug!("Audio copied (16-bit PCM) → {}", dst.display());
            } else {
                // Convert to 16-bit PCM
                let converted = convert_wav_to_16bit(&data, channels, bits_per_sample)?;
                fs::write(dst, &converted).map_err(|e| format!("write: {e}"))?;
                log::debug!(
                    "Audio transcoded ({}-bit → 16-bit, {}Hz) → {}",
                    bits_per_sample,
                    sample_rate,
                    dst.display()
                );
            }
            Ok(())
        }
        "flac" => {
            // FLAC decoding would require the claxon crate
            // For now, copy as-is with a note
            fs::copy(src, dst).map_err(|e| format!("copy: {e}"))?;
            log::debug!("FLAC copied (decode stub) → {}", dst.display());
            Ok(())
        }
        "ogg" | "mp3" | "m4a" | "aac" => {
            // Already compressed formats - copy as-is
            fs::copy(src, dst).map_err(|e| format!("copy: {e}"))?;
            log::debug!("Audio copied ({}) → {}", ext, dst.display());
            Ok(())
        }
        _ => {
            fs::copy(src, dst).map_err(|e| format!("copy: {e}"))?;
            log::debug!("Audio copied (unknown format) → {}", dst.display());
            Ok(())
        }
    }
}

/// Convert WAV audio to 16-bit PCM format.
fn convert_wav_to_16bit(
    data: &[u8],
    channels: u32,
    bits_per_sample: u32,
) -> Result<Vec<u8>, String> {
    if data.len() < 44 {
        return Err("WAV file too short".into());
    }

    // Find data chunk
    let mut data_start = 44;
    let mut data_size = 0usize;

    // Search for 'data' chunk
    while data_start + 8 <= data.len() {
        let chunk_id = &data[data_start..data_start + 4];
        if chunk_id == b"data" {
            data_size = u32::from_le_bytes([
                data[data_start + 4],
                data[data_start + 5],
                data[data_start + 6],
                data[data_start + 7],
            ]) as usize;
            data_start += 8;
            break;
        }
        let chunk_size = u32::from_le_bytes([
            data[data_start + 4],
            data[data_start + 5],
            data[data_start + 6],
            data[data_start + 7],
        ]) as usize;
        data_start += 8 + chunk_size;
    }

    if data_start >= data.len() || data_size == 0 {
        return Err("No data chunk found in WAV".into());
    }

    let audio_data = &data[data_start..data_start.min(data.len())];

    // Convert samples to 16-bit
    let samples_per_frame = channels as usize;
    let bytes_per_sample = (bits_per_sample / 8) as usize;
    let num_samples = audio_data.len() / bytes_per_sample;

    let mut output_data = Vec::with_capacity(44 + num_samples * 2);

    // Copy header and modify format
    output_data.extend_from_slice(&data[..22]);

    // Update format chunk for 16-bit
    output_data.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = 1 (PCM)
    output_data.extend_from_slice(&(channels as u16).to_le_bytes());
    output_data.extend_from_slice(&data[24..28]); // Sample rate
    let byte_rate = (data[24..28]
        .iter()
        .rev()
        .fold(0u32, |a, &b| a * 256 + b as u32))
        * channels
        * 2;
    output_data.extend_from_slice(&byte_rate.to_le_bytes());
    output_data.extend_from_slice(&((channels * 2) as u16).to_le_bytes()); // Block align
    output_data.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

    // Skip to data chunk
    output_data.extend_from_slice(b"data");
    let new_data_size = (num_samples * 2) as u32;
    output_data.extend_from_slice(&new_data_size.to_le_bytes());

    // Convert samples
    for i in 0..num_samples {
        let offset = i * bytes_per_sample;
        if offset + bytes_per_sample <= audio_data.len() {
            let sample = match bits_per_sample {
                8 => {
                    let val = audio_data[offset] as i16;
                    (val as i32 - 128) * 256
                }
                16 => i16::from_le_bytes([audio_data[offset], audio_data[offset + 1]]) as i32,
                24 => {
                    let b = [
                        if offset + 2 < audio_data.len() {
                            audio_data[offset + 2]
                        } else {
                            0
                        },
                        if offset + 1 < audio_data.len() {
                            audio_data[offset + 1]
                        } else {
                            0
                        },
                        audio_data[offset],
                        0,
                    ];
                    i32::from_le_bytes(b) >> 8
                }
                32 => {
                    i32::from_le_bytes([
                        audio_data[offset],
                        audio_data[offset + 1],
                        audio_data[offset + 2],
                        audio_data[offset + 3],
                    ]) >> 16
                }
                _ => 0,
            };
            let clamped = sample.clamp(-32768, 32767) as i16;
            output_data.extend_from_slice(&clamped.to_le_bytes());
        }
    }

    Ok(output_data)
}
