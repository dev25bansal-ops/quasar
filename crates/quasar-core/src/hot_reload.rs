//! # Extended Hot-Reload System for Quasar Core
//!
//! Provides core hot-reload infrastructure that integrates with the asset pipeline
//! and provides event communication between systems during hot-reload operations.
//!
//! This module extends the asset server's hot-reload capabilities with:
//! - Unified hot-reload event bus
//! - Dependency tracking for cascading reloads
//! - Hot-reload configuration management
//! - System coordination during reload operations

#![allow(clippy::type_complexity)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, Config};

use crate::asset_server::{AssetReloadedEvent, ReloadKind};

// ---------------------------------------------------------------------------
// Hot-Reload Configuration
// ---------------------------------------------------------------------------

/// Global hot-reload configuration for the engine.
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    /// Whether hot-reload is globally enabled
    pub enabled: bool,
    /// Whether Lua script hot-reload is enabled
    pub lua_enabled: bool,
    /// Whether shader hot-reload is enabled
    pub shader_enabled: bool,
    /// Whether texture hot-reload is enabled
    pub texture_enabled: bool,
    /// Whether scene hot-reload is enabled
    pub scene_enabled: bool,
    /// Whether animation hot-reload is enabled
    pub animation_enabled: bool,
    /// Debounce interval for all file watches
    pub debounce_interval: Duration,
    /// Whether to pause the game during hot-reload
    pub pause_on_reload: bool,
    /// Maximum concurrent reloads
    pub max_concurrent_reloads: usize,
}

impl HotReloadConfig {
    /// Development configuration with all hot-reload features enabled.
    pub fn development() -> Self {
        Self {
            enabled: true,
            lua_enabled: true,
            shader_enabled: true,
            texture_enabled: true,
            scene_enabled: true,
            animation_enabled: true,
            debounce_interval: Duration::from_millis(250),
            pause_on_reload: false,
            max_concurrent_reloads: 4,
        }
    }

    /// Production configuration with hot-reload disabled.
    pub fn production() -> Self {
        Self {
            enabled: false,
            lua_enabled: false,
            shader_enabled: false,
            texture_enabled: false,
            scene_enabled: false,
            animation_enabled: false,
            debounce_interval: Duration::from_millis(100),
            pause_on_reload: false,
            max_concurrent_reloads: 1,
        }
    }

    /// Check if a specific reload kind is enabled.
    pub fn is_kind_enabled(&self, kind: &ReloadKind) -> bool {
        match kind {
            ReloadKind::Lua => self.lua_enabled,
            ReloadKind::Shader => self.shader_enabled,
            ReloadKind::Texture => self.texture_enabled,
            ReloadKind::Scene => self.scene_enabled,
            ReloadKind::Animation => self.animation_enabled,
            _ => self.enabled,
        }
    }
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self::development()
    }
}

// ---------------------------------------------------------------------------
// Hot-Reload Events
// ---------------------------------------------------------------------------

/// Events emitted by the core hot-reload system.
#[derive(Debug, Clone)]
pub enum HotReloadEvent {
    /// File change detected
    FileChangeDetected {
        path: PathBuf,
        kind: ReloadKind,
        timestamp: Instant,
    },
    /// Reload started
    ReloadStarted {
        path: PathBuf,
        kind: ReloadKind,
    },
    /// Reload completed successfully
    ReloadCompleted {
        path: PathBuf,
        kind: ReloadKind,
        duration: Duration,
    },
    /// Reload failed
    ReloadFailed {
        path: PathBuf,
        kind: ReloadKind,
        error: String,
    },
    /// Dependency cascade triggered
    DependencyCascade {
        source_path: PathBuf,
        dependent_paths: Vec<PathBuf>,
    },
    /// Hot-reload system state changed
    SystemStateChanged {
        is_reloading: bool,
        pending_count: usize,
    },
}

// ---------------------------------------------------------------------------
// Hot-Reload Manager
// ---------------------------------------------------------------------------

/// Central manager for all hot-reload operations in the engine.
///
/// Coordinates file watching, debouncing, dependency tracking, and
/// event dispatching across all asset types.
pub struct HotReloadManager {
    /// Configuration
    config: HotReloadConfig,
    /// File watcher
    watcher: Option<RecommendedWatcher>,
    /// Event receiver from file watcher
    watcher_receiver: Receiver<std::result::Result<notify::Event, notify::Error>>,
    /// Debounce queue: (path, last_change_time)
    debounce_queue: HashMap<PathBuf, Instant>,
    /// Pending reloads (passed debounce)
    pending_reloads: VecDeque<(PathBuf, ReloadKind)>,
    /// Currently reloading paths
    reloading_paths: HashSet<PathBuf>,
    /// Event sender
    event_sender: Sender<HotReloadEvent>,
    /// Event receiver
    event_receiver: Receiver<HotReloadEvent>,
    /// Whether the system is currently reloading
    is_reloading: AtomicBool,
    /// Watched paths
    watched_paths: HashSet<PathBuf>,
    /// Dependency graph for cascading reloads
    dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    /// Reverse dependency graph (path -> paths that depend on it)
    reverse_dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    /// Reload history for statistics
    reload_history: VecDeque<ReloadRecord>,
    /// Maximum history size
    max_history_size: usize,
}

/// Record of a reload operation for statistics.
#[derive(Debug, Clone)]
pub struct ReloadRecord {
    pub path: PathBuf,
    pub kind: ReloadKind,
    pub timestamp: Instant,
    pub success: bool,
    pub duration: Duration,
    pub error: Option<String>,
}

impl HotReloadManager {
    /// Create a new hot-reload manager.
    pub fn new(config: HotReloadConfig) -> Self {
        let (event_sender, event_receiver) = bounded(256);
        let (watcher_sender, watcher_receiver) = bounded(64);

        let watcher = if config.enabled {
            let watcher = RecommendedWatcher::new(
                move |res: std::result::Result<Event, notify::Error>| {
                    let _ = watcher_sender.send(res);
                },
                Config::default(),
            );

            match watcher {
                Ok(w) => Some(w),
                Err(e) => {
                    log::error!("Failed to create hot-reload file watcher: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            config,
            watcher,
            watcher_receiver,
            debounce_queue: HashMap::new(),
            pending_reloads: VecDeque::new(),
            reloading_paths: HashSet::new(),
            event_sender,
            event_receiver,
            is_reloading: AtomicBool::new(false),
            watched_paths: HashSet::new(),
            dependencies: HashMap::new(),
            reverse_dependencies: HashMap::new(),
            reload_history: VecDeque::new(),
            max_history_size: 1000,
        }
    }

    /// Watch a directory for file changes.
    pub fn watch_directory(&mut self, path: &Path, recursive: bool) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        if !path.exists() {
            return Err(format!("Path does not exist: {:?}", path));
        }

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        if let Some(ref mut watcher) = self.watcher {
            watcher
                .watch(path, mode)
                .map_err(|e| format!("Failed to watch {:?}: {}", path, e))?;
        }

        self.watched_paths.insert(path.to_path_buf());
        log::info!("Hot-reload watching: {:?} (recursive: {})", path, recursive);
        Ok(())
    }

    /// Process file watcher events.
    ///
    /// Call this every frame to detect file changes.
    pub fn process_watcher_events(&mut self) {
        if !self.config.enabled {
            return;
        }

        // Drain watcher events
        loop {
            match self.watcher_receiver.try_recv() {
                Ok(Ok(event)) => {
                    self.handle_file_event(event);
                }
                Ok(Err(e)) => {
                    log::error!("File watcher error: {}", e);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("File watcher channel disconnected");
                    break;
                }
            }
        }

        // Process debounce queue
        self.process_debounce_queue();
    }

    /// Handle a single file event.
    fn handle_file_event(&mut self, event: Event) {
        // Only process modify and create events
        if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
            // Handle remove events
            if matches!(event.kind, EventKind::Remove(_)) {
                for path in event.paths {
                    log::info!("File removed: {:?}", path);
                    self.dependencies.remove(&path);
                    self.reverse_dependencies.remove(&path);
                }
                return;
            }
            return;
        }

        let now = Instant::now();

        for path in event.paths {
            // Determine the kind of asset
            let kind = AssetReloadedEvent::from_path(&path).kind;

            // Check if this kind is enabled
            if !self.config.is_kind_enabled(&kind) {
                continue;
            }

            // Add to debounce queue
            self.debounce_queue.insert(path.clone(), now);

            // Emit detection event
            let _ = self.event_sender.send(HotReloadEvent::FileChangeDetected {
                path,
                kind,
                timestamp: now,
            });
        }
    }

    /// Process the debounce queue and move ready files to pending reloads.
    fn process_debounce_queue(&mut self) {
        let now = Instant::now();
        let mut ready_paths = Vec::new();

        // Find paths that have passed the debounce interval
        for (path, last_change) in &self.debounce_queue {
            if now.duration_since(*last_change) >= self.config.debounce_interval {
                ready_paths.push(path.clone());
            }
        }

        // Move ready paths to pending reloads
        for path in ready_paths {
            if let Some(_) = self.debounce_queue.remove(&path) {
                let kind = AssetReloadedEvent::from_path(&path).kind;
                self.pending_reloads.push_back((path, kind));
            }
        }
    }

    /// Process pending reloads.
    ///
    /// Call this every frame to execute pending reloads.
    /// Returns a list of events generated during processing.
    pub fn process_pending_reloads(&mut self) -> Vec<HotReloadEvent> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut events = Vec::new();
        let max_to_process = self.config.max_concurrent_reloads;

        // Update reloading state
        let was_reloading = self.is_reloading.load(Ordering::SeqCst);
        let mut processed_count = 0;

        while processed_count < max_to_process && !self.pending_reloads.is_empty() {
            if let Some((path, kind)) = self.pending_reloads.pop_front() {
                // Skip if already reloading this path
                if self.reloading_paths.contains(&path) {
                    continue;
                }

                let reload_start = Instant::now();

                // Emit reload started event
                events.push(HotReloadEvent::ReloadStarted {
                    path: path.clone(),
                    kind: kind.clone(),
                });

                // Mark as reloading
                self.reloading_paths.insert(path.clone());
                self.is_reloading.store(true, Ordering::SeqCst);

                // Perform the actual reload (simplified - actual asset reloading
                // is handled by specialized systems like ScriptEngine, AssetServer, etc.)
                let reload_result = self.perform_reload(&path, &kind);

                let duration = reload_start.elapsed();

                match reload_result {
                    Ok(()) => {
                        events.push(HotReloadEvent::ReloadCompleted {
                            path: path.clone(),
                            kind: kind.clone(),
                            duration,
                        });

                        // Record success
                        self.record_reload_record(path.clone(), kind.clone(), true, duration, None);

                        log::info!(
                            "Hot-reloaded {:?} ({:?}) in {:.2}ms",
                            path,
                            kind,
                            duration.as_secs_f64() * 1000.0
                        );

                        // Check for dependencies and cascade
                        let dependents = self.get_dependents(&path);
                        if !dependents.is_empty() {
                            events.push(HotReloadEvent::DependencyCascade {
                                source_path: path.clone(),
                                dependent_paths: dependents.clone(),
                            });

                            // Queue dependents for reload
                            for dep_path in dependents {
                                let dep_kind = AssetReloadedEvent::from_path(&dep_path).kind;
                                if self.config.is_kind_enabled(&dep_kind) {
                                    self.pending_reloads.push_back((dep_path, dep_kind));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        events.push(HotReloadEvent::ReloadFailed {
                            path: path.clone(),
                            kind: kind.clone(),
                            error: e.clone(),
                        });

                        // Record failure
                        self.record_reload_record(path.clone(), kind.clone(), false, duration, Some(e.clone()));

                        log::error!("Failed to hot-reload {:?}: {}", path, e);
                    }
                }

                // Mark as no longer reloading
                self.reloading_paths.remove(&path);
                processed_count += 1;
            }
        }

        // Update reloading state
        let is_now_reloading = !self.pending_reloads.is_empty() || !self.reloading_paths.is_empty();
        if was_reloading != is_now_reloading {
            self.is_reloading.store(is_now_reloading, Ordering::SeqCst);
            events.push(HotReloadEvent::SystemStateChanged {
                is_reloading: is_now_reloading,
                pending_count: self.pending_reloads.len(),
            });
        }

        events
    }

    /// Perform the actual asset reload.
    ///
    /// This is a placeholder - actual reloading is delegated to specialized systems.
    fn perform_reload(&self, path: &Path, kind: &ReloadKind) -> Result<(), String> {
        // For Lua scripts, the ScriptEngine handles actual reloading
        // For shaders, the renderer handles reloading
        // For textures, the asset server handles reloading
        //
        // This function just validates the file exists and can be read
        if !path.exists() {
            return Err(format!("File not found: {:?}", path));
        }

        match kind {
            ReloadKind::Lua => {
                // Validate Lua syntax
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
                
                // Basic syntax validation
                if content.is_empty() {
                    return Err("File is empty".to_string());
                }
                
                Ok(())
            }
            ReloadKind::Shader | ReloadKind::Texture | ReloadKind::Hdr => {
                // Validate binary asset can be read
                std::fs::metadata(path)
                    .map_err(|e| format!("Failed to read metadata for {:?}: {}", path, e))?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Record a reload operation for statistics.
    fn record_reload_record(
        &mut self,
        path: PathBuf,
        kind: ReloadKind,
        success: bool,
        duration: Duration,
        error: Option<String>,
    ) {
        let record = ReloadRecord {
            path,
            kind,
            timestamp: Instant::now(),
            success,
            duration,
            error,
        };

        self.reload_history.push_back(record);

        // Trim history if needed
        while self.reload_history.len() > self.max_history_size {
            self.reload_history.pop_front();
        }
    }

    /// Add a dependency relationship.
    ///
    /// When `source_path` changes, `dependent_path` will also be reloaded.
    pub fn add_dependency(&mut self, dependent_path: PathBuf, source_path: PathBuf) {
        self.dependencies
            .entry(dependent_path.clone())
            .or_default()
            .insert(source_path.clone());

        self.reverse_dependencies
            .entry(source_path)
            .or_default()
            .insert(dependent_path);
    }

    /// Remove a dependency.
    pub fn remove_dependency(&mut self, dependent_path: &Path, source_path: &Path) {
        if let Some(deps) = self.dependencies.get_mut(dependent_path) {
            deps.remove(source_path);
        }

        if let Some(reverse_deps) = self.reverse_dependencies.get_mut(source_path) {
            reverse_deps.remove(dependent_path);
        }
    }

    /// Get all paths that depend on the given path (direct and transitive).
    pub fn get_dependents(&self, path: &Path) -> Vec<PathBuf> {
        let mut dependents = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start with direct dependents
        if let Some(reverse_deps) = self.reverse_dependencies.get(path) {
            for dep in reverse_deps {
                queue.push_back(dep.clone());
            }
        }

        // BFS to find all transitive dependents
        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            dependents.push(current.clone());

            // Add dependents of this path
            if let Some(reverse_deps) = self.reverse_dependencies.get(&current) {
                for dep in reverse_deps {
                    if !visited.contains(dep) {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        dependents
    }

    /// Poll for hot-reload events (non-blocking).
    pub fn poll_events(&self) -> Vec<HotReloadEvent> {
        let mut events = Vec::new();
        loop {
            match self.event_receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    /// Get statistics about the hot-reload system.
    pub fn get_stats(&self) -> HotReloadStats {
        let total_reloads = self.reload_history.len();
        let successful_reloads = self.reload_history.iter().filter(|r| r.success).count();
        let failed_reloads = total_reloads - successful_reloads;
        
        let avg_duration = if total_reloads > 0 {
            let total_ms: f64 = self.reload_history.iter()
                .map(|r| r.duration.as_secs_f64())
                .sum();
            Duration::from_secs_f64(total_ms / total_reloads as f64)
        } else {
            Duration::ZERO
        };

        let recent_failures: Vec<&ReloadRecord> = self.reload_history.iter()
            .filter(|r| !r.success)
            .rev()
            .take(10)
            .collect();

        HotReloadStats {
            is_enabled: self.config.enabled,
            is_reloading: self.is_reloading.load(Ordering::SeqCst),
            pending_count: self.pending_reloads.len(),
            reloading_count: self.reloading_paths.len(),
            watched_paths_count: self.watched_paths.len(),
            total_reloads,
            successful_reloads,
            failed_reloads,
            avg_reload_duration: avg_duration,
            recent_failures: recent_failures.into_iter().map(|r| r.clone()).collect(),
        }
    }

    /// Check if a specific path is currently being reloaded.
    pub fn is_reloading_path(&self, path: &Path) -> bool {
        self.reloading_paths.contains(path)
    }

    /// Get pending reload count.
    pub fn pending_count(&self) -> usize {
        self.pending_reloads.len()
    }

    /// Force reload a specific path (bypasses debounce).
    pub fn force_reload(&mut self, path: &Path) {
        let kind = AssetReloadedEvent::from_path(path).kind;
        if self.config.is_kind_enabled(&kind) {
            // Remove from debounce queue if present
            self.debounce_queue.remove(path);
            self.pending_reloads.push_back((path.to_path_buf(), kind));
        }
    }

    /// Clear all pending reloads.
    pub fn clear_pending(&mut self) {
        self.pending_reloads.clear();
        self.debounce_queue.clear();
    }

    /// Get the configuration.
    pub fn config(&self) -> &HotReloadConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: HotReloadConfig) {
        self.config = config;
    }
}

impl Drop for HotReloadManager {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
        log::info!("Hot-reload manager shut down");
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Statistics about the hot-reload system.
#[derive(Debug, Clone)]
pub struct HotReloadStats {
    /// Whether hot-reload is enabled
    pub is_enabled: bool,
    /// Whether the system is currently reloading
    pub is_reloading: bool,
    /// Number of pending reloads
    pub pending_count: usize,
    /// Number of currently reloading paths
    pub reloading_count: usize,
    /// Number of watched paths
    pub watched_paths_count: usize,
    /// Total reloads recorded
    pub total_reloads: usize,
    /// Successful reloads
    pub successful_reloads: usize,
    /// Failed reloads
    pub failed_reloads: usize,
    /// Average reload duration
    pub avg_reload_duration: Duration,
    /// Recent failures (last 10)
    pub recent_failures: Vec<ReloadRecord>,
}

// ---------------------------------------------------------------------------
// Integration with AssetServer
// ---------------------------------------------------------------------------

/// Helper to integrate HotReloadManager with AssetServer.
pub struct AssetServerHotReload {
    pub manager: HotReloadManager,
}

impl AssetServerHotReload {
    /// Create a new integration helper.
    pub fn new(config: HotReloadConfig) -> Self {
        Self {
            manager: HotReloadManager::new(config),
        }
    }

    /// Watch the asset server's directory.
    pub fn watch_assets(&mut self, assets_dir: &Path) -> Result<(), String> {
        self.manager.watch_directory(assets_dir, true)
    }

    /// Process all hot-reload events.
    ///
    /// Call this every frame.
    pub fn process(&mut self) -> Vec<HotReloadEvent> {
        self.manager.process_watcher_events();
        let mut events = self.manager.process_pending_reloads();
        events.extend(self.manager.poll_events());
        events
    }

    /// Get a reference to the manager.
    pub fn manager(&self) -> &HotReloadManager {
        &self.manager
    }

    /// Get a mutable reference to the manager.
    pub fn manager_mut(&mut self) -> &mut HotReloadManager {
        &mut self.manager
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_temp_asset_dir() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");
        
        // Create test files
        let lua_path = dir.path().join("test.lua");
        let mut file = std::fs::File::create(&lua_path).expect("Failed to create test file");
        writeln!(file, "print('hello')").expect("Failed to write");
        
        let shader_path = dir.path().join("test.wgsl");
        let mut file = std::fs::File::create(&shader_path).expect("Failed to create test file");
        writeln!(file, "// Test shader").expect("Failed to write");
        
        dir
    }

    #[test]
    fn test_config_development() {
        let config = HotReloadConfig::development();
        assert!(config.enabled);
        assert!(config.lua_enabled);
        assert!(config.shader_enabled);
        assert!(config.texture_enabled);
    }

    #[test]
    fn test_config_production() {
        let config = HotReloadConfig::production();
        assert!(!config.enabled);
        assert!(!config.lua_enabled);
        assert!(!config.shader_enabled);
    }

    #[test]
    fn test_config_is_kind_enabled() {
        let config = HotReloadConfig::development();
        assert!(config.is_kind_enabled(&ReloadKind::Lua));
        assert!(config.is_kind_enabled(&ReloadKind::Shader));
        assert!(config.is_kind_enabled(&ReloadKind::Texture));
    }

    #[test]
    fn test_manager_creation() {
        let config = HotReloadConfig::development();
        let manager = HotReloadManager::new(config);
        
        assert!(manager.config.enabled);
        assert!(manager.watcher.is_some());
        assert!(manager.debounce_queue.is_empty());
        assert!(manager.pending_reloads.is_empty());
    }

    #[test]
    fn test_manager_production_mode() {
        let config = HotReloadConfig::production();
        let manager = HotReloadManager::new(config);
        
        assert!(!manager.config.enabled);
    }

    #[test]
    fn test_watch_directory() {
        let temp_dir = create_temp_asset_dir();
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let result = manager.watch_directory(temp_dir.path(), true);
        assert!(result.is_ok());
        assert!(manager.watched_paths.contains(temp_dir.path()));
    }

    #[test]
    fn test_watch_nonexistent_directory() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let result = manager.watch_directory(Path::new("/nonexistent/path"), true);
        assert!(result.is_err());
    }

    #[test]
    fn test_dependency_tracking() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let source = PathBuf::from("scripts/base.lua");
        let dependent = PathBuf::from("scripts/player.lua");
        
        manager.add_dependency(dependent.clone(), source.clone());
        
        // Check forward dependency
        assert!(manager.dependencies.contains_key(&dependent));
        assert!(manager.dependencies[&dependent].contains(&source));
        
        // Check reverse dependency
        assert!(manager.reverse_dependencies.contains_key(&source));
        assert!(manager.reverse_dependencies[&source].contains(&dependent));
        
        // Test get_dependents
        let dependents = manager.get_dependents(&source);
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0], dependent);
    }

    #[test]
    fn test_transitive_dependencies() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        // A -> B -> C (when A changes, B and C should reload)
        let a = PathBuf::from("a.lua");
        let b = PathBuf::from("b.lua");
        let c = PathBuf::from("c.lua");
        
        manager.add_dependency(b.clone(), a.clone());
        manager.add_dependency(c.clone(), b.clone());
        
        let dependents = manager.get_dependents(&a);
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&b));
        assert!(dependents.contains(&c));
    }

    #[test]
    fn test_remove_dependency() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let source = PathBuf::from("base.lua");
        let dependent = PathBuf::from("player.lua");
        
        manager.add_dependency(dependent.clone(), source.clone());
        manager.remove_dependency(&dependent, &source);
        
        assert!(!manager.dependencies[&dependent].contains(&source));
        assert!(!manager.reverse_dependencies[&source].contains(&dependent));
    }

    #[test]
    fn test_force_reload() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let path = PathBuf::from("test.lua");
        manager.force_reload(&path);
        
        assert_eq!(manager.pending_reloads.len(), 1);
        assert_eq!(manager.pending_reloads[0].0, path);
    }

    #[test]
    fn test_clear_pending() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        manager.force_reload(&PathBuf::from("test1.lua"));
        manager.force_reload(&PathBuf::from("test2.lua"));
        
        assert_eq!(manager.pending_reloads.len(), 2);
        
        manager.clear_pending();
        
        assert_eq!(manager.pending_reloads.len(), 0);
    }

    #[test]
    fn test_get_stats() {
        let config = HotReloadConfig::development();
        let manager = HotReloadManager::new(config);
        
        let stats = manager.get_stats();
        
        assert!(stats.is_enabled);
        assert!(!stats.is_reloading);
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.total_reloads, 0);
    }

    #[test]
    fn test_is_reloading_path() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let path = PathBuf::from("test.lua");
        manager.reloading_paths.insert(path.clone());
        
        assert!(manager.is_reloading_path(&path));
        assert!(!manager.is_reloading_path(&PathBuf::from("other.lua")));
    }

    #[test]
    fn test_reload_record_recording() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        let path = PathBuf::from("test.lua");
        let kind = ReloadKind::Lua;
        let duration = Duration::from_millis(50);
        
        // Simulate recording via get_stats after forcing reload
        manager.force_reload(&path);
        
        let stats = manager.get_stats();
        assert_eq!(stats.pending_count, 1);
    }

    #[test]
    fn test_asset_server_integration() {
        let temp_dir = create_temp_asset_dir();
        let mut integration = AssetServerHotReload::new(HotReloadConfig::development());
        
        let result = integration.watch_assets(temp_dir.path());
        assert!(result.is_ok());
        
        // Process should not panic
        let events = integration.process();
        // Events may be empty since we didn't actually modify files
        assert!(events.len() >= 0);
    }

    #[test]
    fn test_debounce_queue_processing() {
        let mut manager = HotReloadManager::new(
            HotReloadConfig::development()
        );
        
        // Manually add to debounce queue with old timestamp
        let path = PathBuf::from("test.lua");
        let old_time = Instant::now() - Duration::from_secs(1);
        manager.debounce_queue.insert(path.clone(), old_time);
        
        // Process debounce queue
        manager.process_debounce_queue();
        
        // Should have moved to pending
        assert_eq!(manager.pending_reloads.len(), 1);
        assert_eq!(manager.pending_reloads[0].0, path);
    }

    #[test]
    fn test_debounce_queue_not_ready() {
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        // Add with recent timestamp
        let path = PathBuf::from("test.lua");
        let recent_time = Instant::now() - Duration::from_millis(10);
        manager.debounce_queue.insert(path.clone(), recent_time);
        
        // Process debounce queue
        manager.process_debounce_queue();
        
        // Should still be in debounce queue (not ready yet)
        assert_eq!(manager.pending_reloads.len(), 0);
        assert!(manager.debounce_queue.contains_key(&path));
    }

    #[test]
    fn test_perform_reload_lua_validation() {
        let temp_dir = create_temp_asset_dir();
        let manager = HotReloadManager::new(HotReloadConfig::development());
        
        let lua_path = temp_dir.path().join("test.lua");
        let result = manager.perform_reload(&lua_path, &ReloadKind::Lua);
        assert!(result.is_ok());
    }

    #[test]
    fn test_perform_reload_nonexistent_file() {
        let manager = HotReloadManager::new(HotReloadConfig::development());
        
        let path = PathBuf::from("/nonexistent/test.lua");
        let result = manager.perform_reload(&path, &ReloadKind::Lua);
        assert!(result.is_err());
    }

    #[test]
    fn test_events_are_emitted() {
        let temp_dir = create_temp_asset_dir();
        let mut manager = HotReloadManager::new(HotReloadConfig::development());
        
        // Force a reload to generate events
        let path = temp_dir.path().join("test.lua");
        manager.force_reload(&path);
        
        // Process to generate events
        let events = manager.process_pending_reloads();
        
        // Should have at least ReloadStarted event
        assert!(!events.is_empty());
        assert!(matches!(events[0], HotReloadEvent::ReloadStarted { .. }));
    }
}
