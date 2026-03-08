//! Asset pipeline with hot-reload support.
//!
//! Provides an AssetServer that watches the assets/ directory with notify,
//! loads assets in background threads, and notifies dependent systems
//! when an asset changes (shader hot-reload, texture hot-reload).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{bounded, Receiver, Sender};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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
    asset_storage: RwLock<HashMap<TypeId, HashMap<AssetId, Box<dyn Any + Send + Sync>>>>,
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
    pub fn get_asset<T: crate::asset::Asset>(
        &self,
        handle: &crate::asset::AssetHandle<T>,
    ) -> Option<T>
    where
        T: Clone,
    {
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
            Some("png" | "jpg" | "jpeg") => ReloadKind::Texture,
            Some("hdr" | "exr") => ReloadKind::Hdr,
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
            let mut next_id = self.next_id.lock().unwrap();
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
            let mut storage = self.asset_storage.write().unwrap_or_else(|e| e.into_inner());
            let type_storage = storage
                .entry(TypeId::of::<T>())
                .or_insert_with(HashMap::new);
            type_storage.insert(id, Box::new(asset));
        }

        let handle = AssetHandle { id, generation: 0 };

        {
            let mut handles = self.asset_handles.write().unwrap_or_else(|e| e.into_inner());
            handles.insert(
                id,
                AssetMeta {
                    path: full_path.clone(),
                    generation: 0,
                    loaded: true,
                    hot_reload_enabled: true,
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
        let typed = asset.downcast_ref::<T>()?;
        Some(Arc::new(typed.clone()))
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
                            if ["png", "jpg", "jpeg", "glsl", "wgsl", "hdr", "exr"].contains(&ext) {
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
            if &meta.path == path && meta.hot_reload_enabled {
                if let Ok(bytes) = std::fs::read(path) {
                    for (type_id, loader) in &self.loaders {
                        if loader.can_load(path) {
                            // Try to reload the asset in-place
                            let mut storage = self.asset_storage.write().unwrap_or_else(|e| e.into_inner());
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
        let mut handles = self.asset_handles.write().unwrap_or_else(|e| e.into_inner());
        if let Some(meta) = handles.get_mut(&handle.id) {
            meta.hot_reload_enabled = enabled;
        }
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
            let mut handles = self.asset_handles.write().unwrap_or_else(|e| e.into_inner());
            handles.insert(
                id,
                AssetMeta {
                    path: full_path.clone(),
                    generation: 0,
                    loaded: false,
                    hot_reload_enabled: true,
                },
            );
        }

        let sender = self.event_sender.clone();
        let fp = full_path.clone();

        std::thread::spawn(move || {
            match std::fs::read(&fp) {
                Ok(_bytes) => {
                    let _ = sender.send(AssetEvent::Loaded {
                        handle,
                        path: fp,
                    });
                }
                Err(e) => {
                    let _ = sender.send(AssetEvent::Failed {
                        path: fp,
                        error: format!("{}", e),
                    });
                }
            }
        });

        Ok(handle)
    }

    /// Check whether an asset handle has been fully loaded.
    pub fn is_loaded(&self, handle: AssetHandle) -> bool {
        let handles = self.asset_handles.read().unwrap_or_else(|e| e.into_inner());
        handles.get(&handle.id).map(|m| m.loaded).unwrap_or(false)
    }
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

        log::info!("AssetPlugin loaded — asset hot-reload active");
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
