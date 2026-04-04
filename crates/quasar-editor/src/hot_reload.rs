//! Hot-reload system for assets - file watching and automatic reload.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// Unique identifier for a reload callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId(u64);

/// Kind of file system change detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadKind {
    Created,
    Modified,
    Removed,
}

/// An asset that has been modified and needs reloading.
#[derive(Debug, Clone)]
pub struct AssetReloadEvent {
    pub path: PathBuf,
    pub timestamp: Instant,
    pub kind: ReloadKind,
}

/// Configuration for the hot-reload system.
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    pub watch_dirs: Vec<PathBuf>,
    pub extensions: HashSet<String>,
    pub debounce_ms: u64,
    pub enabled: bool,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        let mut extensions = HashSet::new();
        for ext in [
            "png", "jpg", "jpeg", "hdr", "gltf", "glb", "wgsl", "glsl", "lua", "json", "ron",
        ] {
            extensions.insert(ext.to_string());
        }
        Self {
            watch_dirs: vec![PathBuf::from("assets")],
            extensions,
            debounce_ms: 100,
            enabled: true,
        }
    }
}

/// File system event for hot-reload.
#[derive(Debug, Clone)]
pub struct FsEvent {
    pub path: PathBuf,
    pub kind: ReloadKind,
}

/// The hot-reload manager coordinates file watching and reload callbacks.
pub struct HotReloadManager {
    receiver: Option<Receiver<FsEvent>>,
    pending: HashMap<PathBuf, AssetReloadEvent>,
    config: HotReloadConfig,
    last_process: Instant,
    callbacks: HashMap<String, Vec<Box<dyn FnMut(&Path) + Send>>>,
}

impl HotReloadManager {
    pub fn new(config: HotReloadConfig) -> Self {
        Self {
            receiver: None,
            pending: HashMap::new(),
            config,
            last_process: Instant::now(),
            callbacks: HashMap::new(),
        }
    }

    pub fn register_callback<F>(&mut self, extension: &str, callback: F)
    where
        F: FnMut(&Path) + Send + 'static,
    {
        let ext = extension.trim_start_matches('.').to_lowercase();
        self.callbacks
            .entry(ext)
            .or_default()
            .push(Box::new(callback));
    }

    pub fn add_watch_dir(&mut self, path: PathBuf) {
        self.config.watch_dirs.push(path);
    }

    pub fn set_receiver(&mut self, rx: Receiver<FsEvent>) {
        self.receiver = Some(rx);
    }

    pub fn update(&mut self) -> Vec<AssetReloadEvent> {
        if !self.config.enabled {
            return Vec::new();
        }

        let debounce = Duration::from_millis(self.config.debounce_ms);
        let now = Instant::now();

        if let Some(ref rx) = self.receiver {
            while let Ok(event) = rx.try_recv() {
                if let Some(ext) = event.path.extension().and_then(|e| e.to_str()) {
                    if self.config.extensions.contains(ext) {
                        self.pending
                            .entry(event.path.clone())
                            .or_insert(AssetReloadEvent {
                                path: event.path,
                                timestamp: now,
                                kind: event.kind,
                            });
                    }
                }
            }
        }

        let mut ready = Vec::new();
        let mut to_remove = Vec::new();

        for (path, event) in &self.pending {
            if now.duration_since(event.timestamp) >= debounce {
                ready.push(event.clone());
                to_remove.push(path.clone());
            }
        }

        for path in to_remove {
            self.pending.remove(&path);
        }

        for event in &ready {
            if let Some(ext) = event.path.extension().and_then(|e| e.to_str()) {
                if let Some(callbacks) = self.callbacks.get_mut(ext) {
                    for cb in callbacks {
                        cb(&event.path);
                    }
                }
            }
        }

        self.last_process = now;
        ready
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new(HotReloadConfig::default())
    }
}

/// Spawn a file watcher thread.
pub fn spawn_file_watcher(
    watch_dirs: Vec<PathBuf>,
    sender: Sender<FsEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use std::fs;

        let mut last_modified: HashMap<PathBuf, Instant> = HashMap::new();
        let check_interval = Duration::from_millis(100);

        loop {
            for watch_dir in &watch_dirs {
                if let Ok(entries) = fs::read_dir(watch_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Ok(meta) = fs::metadata(&path) {
                                if let Ok(modified) = meta.modified() {
                                    let _modified_instant =
                                        Instant::now() - Instant::now().elapsed();
                                    let modified_duration =
                                        modified.elapsed().unwrap_or(Duration::ZERO);

                                    let last = last_modified.get(&path);
                                    let should_notify = last.map_or(true, |&_prev| {
                                        modified_duration > Duration::from_millis(100)
                                    });

                                    if should_notify {
                                        let _ = sender.send(FsEvent {
                                            path: path.clone(),
                                            kind: ReloadKind::Modified,
                                        });
                                        last_modified.insert(path, Instant::now());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(check_interval);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = HotReloadConfig::default();
        assert!(config.enabled);
        assert!(config.extensions.contains("png"));
        assert!(config.extensions.contains("wgsl"));
    }

    #[test]
    fn manager_default() {
        let manager = HotReloadManager::default();
        assert!(manager.is_enabled());
    }

    #[test]
    fn manager_enable_disable() {
        let mut manager = HotReloadManager::default();
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        manager.set_enabled(true);
        assert!(manager.is_enabled());
    }
}
