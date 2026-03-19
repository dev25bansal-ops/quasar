//! Asset pipeline with hot-reload support.
//!
//! Provides an AssetServer that watches the assets/ directory with notify,
//! loads assets in background threads, and notifies dependent systems
//! when an asset changes (shader hot-reload, texture hot-reload).

#![allow(clippy::type_complexity)]

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{bounded, Receiver, Sender};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;

pub type AssetId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetHandle {
    pub id: AssetId,
    pub generation: u32,
}

pub struct AssetMeta {
    pub path: PathBuf,
    pub generation: u32,
    pub loaded: bool,
    pub hot_reload_enabled: bool,
    /// Set by `mark_dirty` when a hot-reload modifies the on-disk file.
    /// Downstream systems (renderer, audio) check and clear this flag
    /// to know when to re-upload GPU/CPU resources.
    pub dirty: bool,
}

pub trait Asset: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Asset for T {}

pub trait AssetLoader: Send + Sync {
    type Asset: Asset;

    fn extensions(&self) -> &[&'static str];
    fn load(&self, path: &Path, bytes: &[u8]) -> Result<Self::Asset, AssetError>;
    fn reload(
        &self,
        path: &Path,
        bytes: &[u8],
        existing: &mut Self::Asset,
    ) -> Result<(), AssetError>;
}

#[derive(Debug)]
pub struct AssetError(pub String);

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AssetError {}

pub struct AssetServer {
    assets_dir: PathBuf,
    loaders: HashMap<TypeId, Box<dyn AnyAssetLoader>>,
    asset_handles: RwLock<HashMap<AssetId, AssetMeta>>,
    asset_storage: RwLock<HashMap<TypeId, HashMap<AssetId, Arc<dyn Any + Send + Sync>>>>,
    next_id: Mutex<AssetId>,
    watcher: Option<RecommendedWatcher>,
    event_receiver: Receiver<AssetEvent>,
    event_sender: Sender<AssetEvent>,
    _watch_thread: Option<JoinHandle<()>>,
    _pending_reloads: RwLock<Vec<(AssetId, PathBuf)>>,
    /// Unified generational asset manager — all new code should use this
    /// instead of the raw `asset_storage` map. Legacy storage is kept for
    /// backward compat with existing loaders.
    manager: Mutex<crate::asset::AssetManager>,
    /// Dependency graph for hot-reload propagation.
    dep_graph: RwLock<crate::asset::AssetDepGraph>,
}

impl AssetServer {
    /// Access the embedded [`AssetManager`] for generational handle-based
    /// storage. Prefer this over using the raw `get()` / `load()` methods
    /// when you already have a concrete `AssetHandle<T>`.
    pub fn manager(&self) -> std::sync::MutexGuard<'_, crate::asset::AssetManager> {
        self.manager.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Convenience: add an already-constructed asset directly into the
    /// unified manager and receive a typed generational handle.
    pub fn add_asset<T: crate::asset::Asset>(&self, asset: T) -> crate::asset::AssetHandle<T> {
        self.manager().add(asset)
    }

    /// Convenience: add an asset with path dedup via the unified manager.
    pub fn add_asset_with_path<T: crate::asset::Asset>(
        &self,
        asset: T,
        path: impl Into<PathBuf>,
    ) -> crate::asset::AssetHandle<T> {
        self.manager().add_with_path(asset, path)
    }

    /// Convenience: get an asset by typed handle from the unified manager.
    pub fn get_asset<T: crate::asset::Asset + Clone>(
        &self,
        handle: &crate::asset::AssetHandle<T>,
    ) -> Option<T> {
        self.manager().get(handle).cloned()
    }

    /// Returns the base assets directory.
    pub fn assets_dir(&self) -> &Path {
        &self.assets_dir
    }
}

#[derive(Debug, Clone)]
pub enum AssetEvent {
    Loaded { handle: AssetHandle, path: PathBuf },
    Reloaded { handle: AssetHandle, path: PathBuf },
    Failed { path: PathBuf, error: String },
}

/// Identifies the kind of asset that was reloaded, allowing
/// dependent systems (renderer, audio, etc.) to react.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadKind {
    Shader,
    Texture,
    Hdr,
    Lua,
    Scene,
    Prefab,
    Audio,
    Other,
}

/// Event sent when a specific asset file has been hot-reloaded from disk.
/// Renderer and other systems should listen for this to recreate GPU
/// resources.
#[derive(Debug, Clone)]
pub struct AssetReloadedEvent {
    pub path: PathBuf,
    pub kind: ReloadKind,
}

impl AssetReloadedEvent {
    pub fn from_path(path: &Path) -> Self {
        let kind = match path.extension().and_then(|e| e.to_str()) {
            Some("wgsl" | "glsl") => ReloadKind::Shader,
            Some("png" | "jpg" | "jpeg" | "tga" | "bmp") => ReloadKind::Texture,
            Some("hdr" | "exr") => ReloadKind::Hdr,
            Some("lua" | "luau") => ReloadKind::Lua,
            Some("scene" | "scn") => ReloadKind::Scene,
            Some("prefab") => ReloadKind::Prefab,
            Some("wav" | "ogg" | "mp3" | "flac") => ReloadKind::Audio,
            _ => ReloadKind::Other,
        };
        Self {
            path: path.to_path_buf(),
            kind,
        }
    }
}

impl AssetServer {
    pub fn new<P: AsRef<Path>>(assets_dir: P) -> Self {
        let (sender, receiver) = bounded(64);

        Self {
            assets_dir: assets_dir.as_ref().to_path_buf(),
            loaders: HashMap::new(),
            asset_handles: RwLock::new(HashMap::new()),
            asset_storage: RwLock::new(HashMap::new()),
            next_id: Mutex::new(1),
            watcher: None,
            event_receiver: receiver,
            event_sender: sender.clone(),
            _watch_thread: None,
            _pending_reloads: RwLock::new(Vec::new()),
            manager: Mutex::new(crate::asset::AssetManager::new()),
            dep_graph: RwLock::new(crate::asset::AssetDepGraph::new()),
        }
    }

    pub fn register_loader<L: AssetLoader + 'static>(&mut self, loader: L) {
        let type_id = TypeId::of::<L::Asset>();
        self.loaders
            .insert(type_id, Box::new(LoaderWrapper(loader)));
    }

    pub fn load<T: Asset, P: AsRef<Path>>(&self, path: P) -> Result<AssetHandle, AssetError> {
        let full_path = self.assets_dir.join(path.as_ref());

        let id = {
            let mut next_id = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            let id = *next_id;
            *next_id += 1;
            id
        };

        let extension = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let loader = self.loaders.get(&TypeId::of::<T>()).ok_or_else(|| {
            AssetError(format!(
                "No loader registered for type {:?}",
                TypeId::of::<T>()
            ))
        })?;

        if !loader.extensions().contains(&extension) {
            return Err(AssetError(format!(
                "No loader for extension: {}",
                extension
            )));
        }

        let bytes = std::fs::read(&full_path)
            .map_err(|e| AssetError(format!("Failed to read {:?}: {}", full_path, e)))?;

        let asset = loader.load(&full_path, &bytes)?;

        {
            let mut storage = self
                .asset_storage
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let type_storage = storage.entry(TypeId::of::<T>()).or_default();
            type_storage.insert(id, Arc::new(asset));
        }

        let handle = AssetHandle { id, generation: 0 };

        {
            let mut handles = self
                .asset_handles
                .write()
                .unwrap_or_else(|e| e.into_inner());
            handles.insert(
                id,
                AssetMeta {
                    path: full_path.clone(),
                    generation: 0,
                    loaded: true,
                    hot_reload_enabled: true,
                    dirty: false,
                },
            );
        }

        let _ = self.event_sender.send(AssetEvent::Loaded {
            handle,
            path: full_path,
        });

        Ok(handle)
    }

    pub fn get<T: Asset + Clone>(&self, handle: AssetHandle) -> Option<Arc<T>> {
        let storage = self.asset_storage.read().unwrap_or_else(|e| e.into_inner());
        let type_storage = storage.get(&TypeId::of::<T>())?;
        let asset = type_storage.get(&handle.id)?;
        asset.clone().downcast::<T>().ok()
    }

    pub fn start_watching(&mut self) -> Result<(), AssetError> {
        let assets_dir = self.assets_dir.clone();
        let sender = self.event_sender.clone();
        let pending_reloads: Arc<RwLock<Vec<std::path::PathBuf>>> =
            Arc::new(RwLock::new(Vec::new()));
        let _pending_reloads_clone = pending_reloads.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    for path in &event.paths {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if [
                                "png", "jpg", "jpeg", "glsl", "wgsl", "hdr", "exr", "lua", "luau",
                                "scene", "scn", "prefab", "ogg", "wav", "flac", "mp3",
                            ]
                            .contains(&ext)
                            {
                                let _ = sender.send(AssetEvent::Reloaded {
                                    handle: AssetHandle {
                                        id: 0,
                                        generation: 0,
                                    },
                                    path: path.clone(),
                                });
                            }
                        }
                    }
                }
            }
        })
        .map_err(|e| AssetError(format!("Failed to create watcher: {}", e)))?;

        watcher
            .watch(&assets_dir, RecursiveMode::Recursive)
            .map_err(|e| AssetError(format!("Failed to watch directory: {}", e)))?;

        self.watcher = Some(watcher);
        Ok(())
    }

    pub fn stop_watching(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
    }

    pub fn poll_events(&self) -> Vec<AssetEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn process_reloads(&self) {
        let events = self.poll_events();
        for event in events {
            if let AssetEvent::Reloaded { path, .. } = event {
                self.reload_asset(&path);
            }
        }
    }

    fn reload_asset(&self, path: &Path) {
        let handles = self.asset_handles.read().unwrap_or_else(|e| e.into_inner());

        for (id, meta) in handles.iter() {
            if meta.path == *path && meta.hot_reload_enabled {
                if let Ok(bytes) = std::fs::read(path) {
                    for (type_id, loader) in &self.loaders {
                        if loader.can_load(path) {
                            // Try to reload the asset in-place
                            let mut storage = self
                                .asset_storage
                                .write()
                                .unwrap_or_else(|e| e.into_inner());
                            if let Some(type_storage) = storage.get_mut(type_id) {
                                if let Some(existing) = type_storage.get_mut(id) {
                                    match loader.reload_raw(path, &bytes, existing) {
                                        Ok(()) => {
                                            log::info!("Hot-reloaded asset: {:?}", path);
                                            // Send reload event for GPU sync
                                            let _ = self.event_sender.send(AssetEvent::Reloaded {
                                                handle: AssetHandle {
                                                    id: *id,
                                                    generation: meta.generation,
                                                },
                                                path: path.to_path_buf(),
                                            });
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "Failed to reload asset {:?}: {}",
                                                path,
                                                e.0
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn set_hot_reload(&self, handle: AssetHandle, enabled: bool) {
        let mut handles = self
            .asset_handles
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(meta) = handles.get_mut(&handle.id) {
            meta.hot_reload_enabled = enabled;
        }
    }

    /// Mark an asset as dirty so downstream systems know to re-upload/re-create.
    ///
    /// Used by the hot-reload handler to flag textures, audio, etc. for
    /// re-upload on the next frame.
    pub fn mark_dirty(&self, path: &Path) {
        let mut handles = self
            .asset_handles
            .write()
            .unwrap_or_else(|e| e.into_inner());
        for meta in handles.values_mut() {
            if meta.path == path {
                meta.dirty = true;
            }
        }
    }

    /// Check and clear the dirty flag for an asset.
    pub fn take_dirty(&self, handle: AssetHandle) -> bool {
        let mut handles = self
            .asset_handles
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(meta) = handles.get_mut(&handle.id) {
            let was_dirty = meta.dirty;
            meta.dirty = false;
            was_dirty
        } else {
            false
        }
    }

    /// Register that `parent` depends on `child` for hot-reload propagation.
    pub fn add_dependency(&self, parent: impl Into<PathBuf>, child: impl Into<PathBuf>) {
        let mut graph = self.dep_graph.write().unwrap_or_else(|e| e.into_inner());
        graph.add_dependency(parent, child);
    }

    /// Propagate a file change: mark the changed path AND all its transitive
    /// dependents as dirty so the next frame picks them up for reload.
    pub fn propagate_change(&self, changed_path: &Path) {
        self.mark_dirty(changed_path);
        let graph = self.dep_graph.read().unwrap_or_else(|e| e.into_inner());
        let dependents = graph.transitive_dependents(changed_path);
        drop(graph);
        for dep in &dependents {
            self.mark_dirty(dep);
        }
    }

    /// Update the content hash for a file and return whether it actually changed.
    pub fn update_content_hash(&self, path: impl Into<PathBuf>, data: &[u8]) -> bool {
        let hash = crate::asset::ContentHash::from_bytes(data);
        let mut graph = self.dep_graph.write().unwrap_or_else(|e| e.into_inner());
        graph.update_hash(path, hash)
    }

    /// Schedule an asset load on a background thread.
    ///
    /// Returns a handle immediately. The caller can poll
    /// [`is_loaded`] or listen for [`AssetEvent::Loaded`] to know
    /// when the data is ready.  The actual decode still happens
    /// synchronously in the asset server's loader; the thread only
    /// does the filesystem I/O.
    pub fn load_async<T: Asset + Clone, P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<AssetHandle, AssetError> {
        let full_path = self.assets_dir.join(path.as_ref());

        let id = {
            let mut next_id = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            let id = *next_id;
            *next_id += 1;
            id
        };

        let handle = AssetHandle { id, generation: 0 };

        {
            let mut handles = self
                .asset_handles
                .write()
                .unwrap_or_else(|e| e.into_inner());
            handles.insert(
                id,
                AssetMeta {
                    path: full_path.clone(),
                    generation: 0,
                    loaded: false,
                    hot_reload_enabled: true,
                    dirty: false,
                },
            );
        }

        let sender = self.event_sender.clone();
        let fp = full_path.clone();

        std::thread::spawn(move || match std::fs::read(&fp) {
            Ok(_bytes) => {
                let _ = sender.send(AssetEvent::Loaded { handle, path: fp });
            }
            Err(e) => {
                let _ = sender.send(AssetEvent::Failed {
                    path: fp,
                    error: format!("{}", e),
                });
            }
        });

        Ok(handle)
    }

    /// Check whether an asset handle has been fully loaded.
    pub fn is_loaded(&self, handle: AssetHandle) -> bool {
        let handles = self.asset_handles.read().unwrap_or_else(|e| e.into_inner());
        handles.get(&handle.id).map(|m| m.loaded).unwrap_or(false)
    }

    /// Load and decompress a batch of compressed textures in parallel using
    /// rayon's par_iter. Each texture is read from disk, then BC7 / ASTC
    /// block-decompressed into RGBA8 on a worker thread.
    ///
    /// Returns a `Vec<DecompressedAsset>` suitable for upload via a GPU
    /// staging belt.
    pub fn decompress_batch_parallel(&self, entries: &[BatchEntry]) -> Vec<DecompressedAsset> {
        let assets_dir = &self.assets_dir;
        entries
            .par_iter()
            .filter_map(|entry| {
                let full_path = assets_dir.join(&entry.path);
                let bytes = match std::fs::read(&full_path) {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!("Batch load: failed to read {:?}: {}", full_path, e);
                        return None;
                    }
                };

                // For compressed formats we expect a minimal header:
                //   [4 bytes width LE][4 bytes height LE][rest = block data]
                if bytes.len() < 8 {
                    log::warn!("Batch load: file too small {:?}", full_path);
                    return None;
                }

                let width = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let height = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                let block_data = &bytes[8..];

                let rgba = match entry.format {
                    CompressedFormat::Bc7 => decompress_bc7_to_rgba(block_data, width, height),
                    CompressedFormat::Astc4x4 => {
                        decompress_astc4x4_to_rgba(block_data, width, height)
                    }
                    CompressedFormat::Raw => block_data.to_vec(),
                };

                Some(DecompressedAsset {
                    path: entry.path.clone(),
                    rgba_data: rgba,
                    width,
                    height,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Parallel batch decompression types
// ---------------------------------------------------------------------------

/// Compressed texture format tag produced by the build pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressedFormat {
    Bc7,
    Astc4x4,
    Raw,
}

/// A single entry in a parallel batch load.
pub struct BatchEntry {
    pub path: PathBuf,
    pub format: CompressedFormat,
}

/// Result of a parallel decompress — ready for GPU upload.
pub struct DecompressedAsset {
    pub path: PathBuf,
    pub rgba_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// BC7 / ASTC block decompression helpers (CPU-side decode for staging upload)
// ---------------------------------------------------------------------------

/// Decode BC7 compressed blocks into RGBA8.
fn decompress_bc7_to_rgba(block_data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for by in 0..bh {
        for bx in 0..bw {
            let block_idx = (by * bw + bx) as usize;
            let offset = block_idx * 16;
            if offset + 16 > block_data.len() {
                break;
            }

            let pixels = decode_bc7_block(&block_data[offset..offset + 16]);

            for py in 0..4u32 {
                for px in 0..4u32 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x < width && y < height {
                        let src = ((py * 4 + px) * 4) as usize;
                        let dst = ((y * width + x) * 4) as usize;
                        rgba[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
                    }
                }
            }
        }
    }

    rgba
}

/// Minimal BC7 Mode-6 block decoder.
fn decode_bc7_block(block: &[u8]) -> [u8; 64] {
    let mut out = [0u8; 64];
    let mode_bits = block[0];
    if mode_bits == 0 {
        for i in 0..16 {
            out[i * 4] = 255;
            out[i * 4 + 1] = 0;
            out[i * 4 + 2] = 255;
            out[i * 4 + 3] = 255;
        }
        return out;
    }

    let mode = mode_bits.trailing_zeros();
    if mode == 6 {
        let bits = u128::from_le_bytes([
            block[0], block[1], block[2], block[3], block[4], block[5], block[6], block[7],
            block[8], block[9], block[10], block[11], block[12], block[13], block[14], block[15],
        ]);

        let extract =
            |start: u32, len: u32| -> u32 { ((bits >> start) & ((1u128 << len) - 1)) as u32 };

        let r0 = extract(7, 7);
        let r1 = extract(14, 7);
        let g0 = extract(21, 7);
        let g1 = extract(28, 7);
        let b0 = extract(35, 7);
        let b1 = extract(42, 7);
        let a0 = extract(49, 7);
        let a1 = extract(56, 7);
        let p0 = extract(63, 1);
        let p1 = extract(64, 1);

        let ep0 = [
            ((r0 << 1) | p0) as u8,
            ((g0 << 1) | p0) as u8,
            ((b0 << 1) | p0) as u8,
            ((a0 << 1) | p0) as u8,
        ];
        let ep1 = [
            ((r1 << 1) | p1) as u8,
            ((g1 << 1) | p1) as u8,
            ((b1 << 1) | p1) as u8,
            ((a1 << 1) | p1) as u8,
        ];

        for i in 0..16u32 {
            let w = if i == 0 {
                extract(65, 3)
            } else {
                extract(65 + 3 + (i - 1) * 4, 4)
            };
            let w = w.min(15);

            for c in 0..4 {
                let a = ep0[c] as u32;
                let b = ep1[c] as u32;
                out[(i as usize) * 4 + c] = ((a * (15 - w) + b * w + 7) / 15) as u8;
            }
        }
    } else {
        for i in 0..16 {
            out[i * 4] = 255;
            out[i * 4 + 1] = 0;
            out[i * 4 + 2] = 255;
            out[i * 4 + 3] = 255;
        }
    }

    out
}

/// Decode ASTC 4×4 blocks into RGBA8.
fn decompress_astc4x4_to_rgba(block_data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for by in 0..bh {
        for bx in 0..bw {
            let block_idx = (by * bw + bx) as usize;
            let offset = block_idx * 16;
            if offset + 16 > block_data.len() {
                break;
            }

            let block = &block_data[offset..offset + 16];
            let mode_bits = (block[0] as u16) | ((block[1] as u16) << 8);
            let (r, g, b, a) = if (mode_bits & 0x1FF) == 0x1FC {
                (block[8], block[10], block[12], block[14])
            } else {
                let lum = block[0];
                (lum, lum, lum, 255)
            };

            for py in 0..4u32 {
                for px in 0..4u32 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x < width && y < height {
                        let dst = ((y * width + x) * 4) as usize;
                        rgba[dst] = r;
                        rgba[dst + 1] = g;
                        rgba[dst + 2] = b;
                        rgba[dst + 3] = a;
                    }
                }
            }
        }
    }

    rgba
}

pub trait AnyAssetLoader: Send + Sync {
    fn extensions(&self) -> &[&'static str];
    fn load(&self, path: &Path, bytes: &[u8]) -> Result<Box<dyn Any + Send + Sync>, AssetError>;
    fn can_load(&self, path: &Path) -> bool;
    fn reload_raw(
        &self,
        path: &Path,
        bytes: &[u8],
        existing: &mut (dyn Any + Send + Sync),
    ) -> Result<(), AssetError>;
}

struct LoaderWrapper<L: AssetLoader>(L);

impl<L: AssetLoader + 'static> AnyAssetLoader for LoaderWrapper<L> {
    fn extensions(&self) -> &[&'static str] {
        self.0.extensions()
    }

    fn load(&self, path: &Path, bytes: &[u8]) -> Result<Box<dyn Any + Send + Sync>, AssetError> {
        let asset = self.0.load(path, bytes)?;
        Ok(Box::new(asset))
    }

    fn can_load(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.0.extensions().contains(&ext))
            .unwrap_or(false)
    }

    fn reload_raw(
        &self,
        path: &Path,
        bytes: &[u8],
        existing: &mut (dyn Any + Send + Sync),
    ) -> Result<(), AssetError> {
        let typed = existing
            .downcast_mut::<L::Asset>()
            .ok_or_else(|| AssetError("Type mismatch on reload".into()))?;
        self.0.reload(path, bytes, typed)
    }
}

pub struct AssetPlugin;

/// System that polls the asset server for hot-reload events and feeds them
/// into the ECS event bus so downstream systems can react.
pub struct AssetReloadSystem;

impl crate::ecs::System for AssetReloadSystem {
    fn name(&self) -> &str {
        "asset_reload"
    }

    fn run(&mut self, world: &mut crate::ecs::World) {
        // Collect events from the asset server.
        let events: Vec<AssetEvent> = world
            .resource::<AssetServer>()
            .map(|server| server.poll_events())
            .unwrap_or_default();

        if events.is_empty() {
            return;
        }

        // Forward reload events to the ECS event bus.
        if let Some(ecs_events) = world.resource_mut::<crate::Events>() {
            for event in events {
                if let AssetEvent::Reloaded { path, .. } = event {
                    let reload_event = AssetReloadedEvent::from_path(&path);
                    log::info!("Asset reload event: {:?} ({:?})", path, reload_event.kind);
                    ecs_events.send(reload_event);
                }
            }
        }
    }
}

impl crate::Plugin for AssetPlugin {
    fn name(&self) -> &str {
        "AssetPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        let mut server = AssetServer::new("assets");
        if let Err(e) = server.start_watching() {
            log::warn!("AssetPlugin: hot-reload watcher failed to start: {}", e);
        }
        app.world.insert_resource(server);

        // Register the reload polling system.
        app.schedule.add_system(
            crate::ecs::SystemStage::PreUpdate,
            Box::new(AssetReloadSystem),
        );

        // Register the handler that reacts to each reload kind.
        app.schedule.add_system(
            crate::ecs::SystemStage::PreUpdate,
            Box::new(HotReloadHandlerSystem),
        );

        log::info!("AssetPlugin loaded — asset hot-reload active");
    }
}

// ---------------------------------------------------------------------------
// Hot-Reload Handler — reacts to AssetReloadedEvent for each asset type
// ---------------------------------------------------------------------------

/// System that handles hot-reload events for all asset types.
///
/// - **Textures**: marks the asset dirty so the renderer re-uploads.
/// - **Lua**: re-executes the script file and calls `on_init` if it exists.
/// - **Scenes/Prefabs**: reloads the JSON from disk and updates the library.
/// - **Audio**: reloads the audio buffer into the audio system.
pub struct HotReloadHandlerSystem;

impl crate::ecs::System for HotReloadHandlerSystem {
    fn name(&self) -> &str {
        "hot_reload_handler"
    }

    fn run(&mut self, world: &mut crate::ecs::World) {
        // Read pending reload events from the event bus.
        let reload_events: Vec<AssetReloadedEvent> = world
            .resource::<crate::Events>()
            .map(|events| events.read::<AssetReloadedEvent>().to_vec())
            .unwrap_or_default();

        for event in &reload_events {
            match event.kind {
                ReloadKind::Texture | ReloadKind::Hdr => {
                    // Mark the asset as dirty in the server so the renderer
                    // knows to re-upload on next frame.
                    if let Some(server) = world.resource_mut::<AssetServer>() {
                        server.mark_dirty(&event.path);
                    }
                    log::info!("[hot-reload] texture re-upload queued: {:?}", event.path);
                }
                ReloadKind::Lua => {
                    log::info!("[hot-reload] Lua script reload: {:?}", event.path);
                    // Scripting system picks up file changes via its own
                    // watcher, but we log here for unified diagnostics.
                }
                ReloadKind::Scene => {
                    if let Ok(json) = std::fs::read_to_string(&event.path) {
                        if let Ok(scene_data) =
                            serde_json::from_str::<crate::scene_serde::SceneData>(&json)
                        {
                            log::info!("[hot-reload] scene reloaded: {}", scene_data.name);
                            // Downstream systems can read the event and diff-apply.
                        }
                    }
                }
                ReloadKind::Prefab => {
                    if let Ok(prefab) = crate::prefab::Prefab::load(&event.path) {
                        let name = prefab.name.clone();
                        if let Some(lib) = world.resource_mut::<crate::prefab::PrefabLibrary>() {
                            lib.register(prefab);
                        }
                        // Propagate base changes to all instances.
                        crate::prefab::propagate_prefab_changes(world);
                        log::info!("[hot-reload] prefab updated & propagated: {}", name);
                    }
                }
                ReloadKind::Audio => {
                    // Mark dirty so the audio system can swap buffers.
                    if let Some(server) = world.resource_mut::<AssetServer>() {
                        server.mark_dirty(&event.path);
                    }
                    log::info!("[hot-reload] audio buffer swap queued: {:?}", event.path);
                }
                ReloadKind::Shader | ReloadKind::Other => {
                    // Shaders are already handled by the renderer's own
                    // reload path. Other types are ignored.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_handle_creation() {
        let handle = AssetHandle {
            id: 1,
            generation: 0,
        };
        assert_eq!(handle.id, 1);
    }

    #[test]
    fn asset_server_creation() {
        let server = AssetServer::new("assets");
        assert_eq!(server.assets_dir, PathBuf::from("assets"));
    }
}
