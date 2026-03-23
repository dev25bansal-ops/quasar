//! Asset processor that watches the assets/ directory and triggers reimports.
//!
//! Tracks content changes, manages .meta sidecar files, and coordinates with the
//! asset browser for drag-and-drop operations.

use super::asset_metadata::{AssetMeta, ImportSettings, ImportStatus};
use crate::asset_browser::AssetKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Unique handle for a loaded asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetHandle(pub u64);

/// Information about a loaded asset.
#[derive(Debug, Clone)]
pub struct LoadedAsset {
    pub source_path: PathBuf,
    pub kind: AssetKind,
    pub handle: AssetHandle,
    pub content_hash: String,
}

/// Asset import pipeline state.
pub struct AssetImporter {
    /// Watched assets directory.
    pub assets_dir: PathBuf,
    /// Cached metadata for all assets (source_path → metadata).
    pub metadata: HashMap<PathBuf, AssetMeta>,
    /// Pending assets waiting to be processed.
    pub pending_imports: Vec<PathBuf>,
    /// Channel to receive file change events.
    change_receiver: mpsc::Receiver<PathBuf>,
    /// Successfully loaded assets (source_path → loaded asset info).
    pub loaded_assets: HashMap<PathBuf, LoadedAsset>,
    /// Next asset handle ID.
    next_handle_id: u64,
}

impl AssetImporter {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        let assets_dir = assets_dir.into();
        let (tx, rx) = mpsc::channel();

        // Spawn watcher thread
        let watch_dir = assets_dir.clone();
        std::thread::spawn(move || {
            Self::run_watcher(&watch_dir, tx);
        });

        Self {
            metadata: HashMap::new(),
            pending_imports: Vec::new(),
            assets_dir,
            change_receiver: rx,
            loaded_assets: HashMap::new(),
            next_handle_id: 1,
        }
    }

    /// Scan assets directory and load existing `.meta` files.
    pub fn scan(&mut self) -> Result<Vec<PathBuf>, String> {
        self.metadata.clear();
        self.pending_imports.clear();

        let mut new_files = Vec::new();

        fn scan_recursive(dir: &Path, new_files: &mut Vec<PathBuf>) -> Result<(), String> {
            let read_dir =
                std::fs::read_dir(dir).map_err(|e| format!("Cannot read {:?}: {}", dir, e))?;

            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_recursive(&path, new_files)?;
                } else {
                    new_files.push(path.clone());
                    // Load existing metadata if available
                    let meta_path = AssetMeta::meta_path(&path);
                    if meta_path.exists() {
                        if let Ok(_meta) = AssetMeta::load(&meta_path) {
                            log::debug!("Loaded metadata for {:?}", path);
                        } else {
                            log::warn!("Failed to load metadata for {:?}", meta_path);
                        }
                    }
                }
            }
            Ok(())
        }

        scan_recursive(&self.assets_dir, &mut new_files)?;

        Ok(new_files)
    }

    /// Process a single asset file.
    pub fn process_asset(&mut self, asset_path: &PathBuf) -> Result<(), String> {
        let ext = asset_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let kind = AssetKind::from_extension(ext);

        log::info!("Processing asset {:?} ({:?})", asset_path, kind);

        // Load or create metadata
        let mut meta = if let Some(existing) = self.metadata.get(asset_path).cloned() {
            existing
        } else {
            AssetMeta::from_source_file(asset_path)?
        };

        // Perform asset-specific processing
        match kind {
            AssetKind::Texture => self.process_texture(asset_path, &mut meta),
            AssetKind::Model => self.process_model(asset_path, &mut meta),
            AssetKind::Script => self.process_script(asset_path, &self.assets_dir.join("lua")),
            AssetKind::Shader => self.process_shader(asset_path, &self.assets_dir.join("shaders")),
            AssetKind::Audio => self.process_audio(asset_path, &mut meta),
            AssetKind::Scene => self.process_scene(asset_path, &mut meta),
            AssetKind::Unknown => {
                log::warn!("Skipping unknown asset type: {:?}", asset_path);
                meta.status = ImportStatus::Skipped;
                Ok(())
            }
        }?;

        // Save updated metadata
        meta.save(asset_path)?;
        self.metadata.insert(asset_path.clone(), meta);

        Ok(())
    }

    fn process_texture(&mut self, path: &Path, meta: &mut AssetMeta) -> Result<(), String> {
        log::info!("Processing texture: {:?}", path);

        // Generate a handle for this texture
        let handle = AssetHandle(self.next_handle_id);
        self.next_handle_id += 1;

        // Track the loaded asset
        let loaded = LoadedAsset {
            source_path: path.to_path_buf(),
            kind: AssetKind::Texture,
            handle,
            content_hash: meta.content_hash.clone(),
        };
        self.loaded_assets.insert(path.to_path_buf(), loaded);

        // Note: Actual GPU texture loading should be done by the renderer
        // The renderer should call get_loaded_textures() to get paths that need GPU upload
        log::info!("Texture {:?} imported with handle {:?}", path, handle);

        meta.status = ImportStatus::Success;
        Ok(())
    }

    fn process_model(&mut self, path: &Path, meta: &mut AssetMeta) -> Result<(), String> {
        log::info!("Processing model: {:?}", path);

        // Generate a handle for this model
        let handle = AssetHandle(self.next_handle_id);
        self.next_handle_id += 1;

        // Track the loaded asset
        let loaded = LoadedAsset {
            source_path: path.to_path_buf(),
            kind: AssetKind::Model,
            handle,
            content_hash: meta.content_hash.clone(),
        };
        self.loaded_assets.insert(path.to_path_buf(), loaded);

        // Note: Actual mesh loading should be done by the renderer
        log::info!("Model {:?} imported with handle {:?}", path, handle);

        meta.status = ImportStatus::Success;
        Ok(())
    }

    fn process_script(&self, path: &Path, _target_dir: &Path) -> Result<(), String> {
        log::info!("Processing script: {:?}", path);
        // Scripts are just Lua files, no special processing needed
        Ok(())
    }

    fn process_shader(&self, path: &Path, _target_dir: &Path) -> Result<(), String> {
        log::info!("Processing shader: {:?}", path);
        // Shaders are just WGSL files, no special processing needed
        Ok(())
    }

    fn process_audio(&mut self, path: &Path, meta: &mut AssetMeta) -> Result<(), String> {
        log::info!("Processing audio: {:?}", path);

        let handle = AssetHandle(self.next_handle_id);
        self.next_handle_id += 1;

        let loaded = LoadedAsset {
            source_path: path.to_path_buf(),
            kind: AssetKind::Audio,
            handle,
            content_hash: meta.content_hash.clone(),
        };
        self.loaded_assets.insert(path.to_path_buf(), loaded);

        log::info!("Audio {:?} imported with handle {:?}", path, handle);
        meta.status = ImportStatus::Success;
        Ok(())
    }

    fn process_scene(&mut self, path: &Path, meta: &mut AssetMeta) -> Result<(), String> {
        log::info!("Processing scene: {:?}", path);

        let handle = AssetHandle(self.next_handle_id);
        self.next_handle_id += 1;

        let loaded = LoadedAsset {
            source_path: path.to_path_buf(),
            kind: AssetKind::Scene,
            handle,
            content_hash: meta.content_hash.clone(),
        };
        self.loaded_assets.insert(path.to_path_buf(), loaded);

        log::info!("Scene {:?} imported with handle {:?}", path, handle);
        meta.status = ImportStatus::Success;
        Ok(())
    }

    /// Check all tracked assets for content changes and mark pending.
    pub fn check_for_changes(&mut self) -> Result<Vec<PathBuf>, String> {
        let mut changed = Vec::new();

        let paths: Vec<_> = self.metadata.keys().cloned().collect();
        for path in paths {
            if let Some(meta) = self.metadata.remove(&path) {
                if meta.is_outdated(&path)? {
                    let mut updated = meta;
                    updated.mark_outdated(&path)?;
                    log::info!("Asset changed: {:?}", path);
                    changed.push(path.clone());
                    self.metadata.insert(path.clone(), updated);
                } else {
                    self.metadata.insert(path, meta);
                }
            }
        }

        Ok(changed)
    }

    /// Get import settings for an asset.
    pub fn get_settings(&self, asset_path: &Path) -> Option<&ImportSettings> {
        self.metadata.get(asset_path).map(|m| &m.settings)
    }

    /// Update import settings for an asset and mark for reimport.
    pub fn update_settings(
        &mut self,
        asset_path: &Path,
        settings: ImportSettings,
    ) -> Result<(), String> {
        if let Some(meta) = self.metadata.get_mut(asset_path) {
            meta.settings = settings;
            meta.status = ImportStatus::Pending;
            meta.save(asset_path)?;
        }
        Ok(())
    }

    /// Run filesystem watcher in a separate thread.
    fn run_watcher(dir: &Path, tx: mpsc::Sender<PathBuf>) {
        let mut watcher: RecommendedWatcher = match Watcher::new(
            move |res: std::result::Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    log::debug!("Filesystem event: {:?}", event);

                    for path in event.paths {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => {
                                let _ = tx.send(path);
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => log::error!("Watcher error: {:?}", e),
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create watcher: {:?}", e);
                return;
            }
        };

        match watcher.watch(dir, RecursiveMode::Recursive) {
            Ok(_) => log::info!("Watching directory: {:?}", dir),
            Err(e) => log::error!("Failed to watch {:?}: {}", dir, e),
        }

        // Keep the watcher alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    /// Poll for file change events from the watcher thread.
    pub fn poll_changes(&mut self) -> Vec<PathBuf> {
        let mut changes = Vec::new();
        while let Ok(path) = self.change_receiver.try_recv() {
            changes.push(path);
        }
        changes
    }

    /// Get all loaded textures that need GPU upload.
    pub fn get_loaded_textures(&self) -> Vec<&LoadedAsset> {
        self.loaded_assets
            .values()
            .filter(|a| a.kind == AssetKind::Texture)
            .collect()
    }

    /// Get all loaded models that need GPU upload.
    pub fn get_loaded_models(&self) -> Vec<&LoadedAsset> {
        self.loaded_assets
            .values()
            .filter(|a| a.kind == AssetKind::Model)
            .collect()
    }

    /// Get a loaded asset by path.
    pub fn get_asset(&self, path: &Path) -> Option<&LoadedAsset> {
        self.loaded_assets.get(path)
    }

    /// Get a loaded asset by handle.
    pub fn get_asset_by_handle(&self, handle: AssetHandle) -> Option<&LoadedAsset> {
        self.loaded_assets.values().find(|a| a.handle == handle)
    }

    /// Remove a loaded asset (call when GPU resource is freed).
    pub fn unload_asset(&mut self, path: &Path) -> Option<LoadedAsset> {
        self.loaded_assets.remove(path)
    }
}
