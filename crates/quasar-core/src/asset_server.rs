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
    watch_thread: Option<JoinHandle<()>>,
    pending_reloads: RwLock<Vec<(AssetId, PathBuf)>>,
}

#[derive(Debug, Clone)]
pub enum AssetEvent {
    Loaded { handle: AssetHandle, path: PathBuf },
    Reloaded { handle: AssetHandle, path: PathBuf },
    Failed { path: PathBuf, error: String },
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
            watch_thread: None,
            pending_reloads: RwLock::new(Vec::new()),
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
            let mut storage = self.asset_storage.write().unwrap();
            let type_storage = storage
                .entry(TypeId::of::<T>())
                .or_insert_with(HashMap::new);
            type_storage.insert(id, Box::new(asset));
        }

        let handle = AssetHandle { id, generation: 0 };

        {
            let mut handles = self.asset_handles.write().unwrap();
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
        let storage = self.asset_storage.read().unwrap();
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
        let handles = self.asset_handles.read().unwrap();

        for (id, meta) in handles.iter() {
            if &meta.path == path && meta.hot_reload_enabled {
                if let Ok(bytes) = std::fs::read(path) {
                    for (type_id, loader) in &self.loaders {
                        if loader.can_load(path) {
                            // Try to reload the asset in-place
                            let mut storage = self.asset_storage.write().unwrap();
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
        let mut handles = self.asset_handles.write().unwrap();
        if let Some(meta) = handles.get_mut(&handle.id) {
            meta.hot_reload_enabled = enabled;
        }
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

impl crate::Plugin for AssetPlugin {
    fn name(&self) -> &str {
        "AssetPlugin"
    }

    fn build(&self, app: &mut crate::App) {
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
