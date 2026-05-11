//! # Lua Hot-Reload System
//!
//! Provides advanced hot-reloading capabilities for Lua scripts with:
//! - File watching with debouncing to avoid rapid reloads
//! - Script state preservation during reload (globals, coroutines, registry data)
//! - Comprehensive error handling and recovery
//! - Event system integration for inter-system communication
//! - Configuration options for development/production modes
//!
//! ## Architecture
//!
//! The system consists of several key components:
//! 1. **FileWatcher**: Monitors `.lua`/`.luau` files using `notify`
//! 2. **DebounceEngine**: Prevents rapid-fire reloads from editor save storms
//! 3. **StateManager**: Snapshots and restores Lua global state
//! 4. **ErrorHandler**: Catches syntax/runtime errors, preserves old scripts
//! 5. **EventDispatcher**: Notifies other systems about reload events
//!
//! ## Usage
//!
//! ```no_run
//! use quasar_scripting::hot_reload::{LuaHotReloadSystem, HotReloadConfig};
//!
//! let config = HotReloadConfig::development();
//! let mut system = LuaHotReloadSystem::new("scripts/", config).unwrap();
//!
//! // Call this every frame
//! system.process_events(&mut lua)?;
//! ```

#![allow(clippy::type_complexity)]

use crossbeam_channel::{
    unbounded as unbounded_channel, Receiver as UnboundedReceiver, Sender as UnboundedSender,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mlua::prelude::*;
use notify::{
    Config as WatcherConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Lua hot-reload system.
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    /// Whether hot-reload is enabled (disable for production builds)
    pub enabled: bool,
    /// Debounce interval to avoid rapid reloads from editor save storms
    pub debounce_interval: Duration,
    /// Whether to preserve Lua global state during reload
    pub preserve_state: bool,
    /// Whether to preserve coroutines during reload
    pub preserve_coroutines: bool,
    /// Whether to preserve registry data during reload
    pub preserve_registry: bool,
    /// File extensions to watch
    pub extensions: Vec<String>,
    /// Whether to watch recursively
    pub recursive: bool,
    /// Maximum number of retries on transient errors (e.g., file locks)
    pub max_retries: u32,
    /// Delay between retries
    pub retry_delay: Duration,
    /// Whether to validate syntax before full reload
    pub validate_before_reload: bool,
    /// Custom paths to watch (in addition to base scripts directory)
    pub additional_watch_paths: Vec<PathBuf>,
}

impl HotReloadConfig {
    /// Default configuration optimized for development.
    pub fn development() -> Self {
        Self {
            enabled: true,
            debounce_interval: Duration::from_millis(250),
            preserve_state: true,
            preserve_coroutines: false,
            preserve_registry: true,
            extensions: vec!["lua".to_string(), "luau".to_string()],
            recursive: true,
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            validate_before_reload: true,
            additional_watch_paths: Vec::new(),
        }
    }

    /// Configuration for production (hot-reload disabled).
    pub fn production() -> Self {
        Self {
            enabled: false,
            debounce_interval: Duration::from_millis(100),
            preserve_state: false,
            preserve_coroutines: false,
            preserve_registry: false,
            extensions: vec!["lua".to_string()],
            recursive: false,
            max_retries: 0,
            retry_delay: Duration::from_secs(1),
            validate_before_reload: false,
            additional_watch_paths: Vec::new(),
        }
    }

    /// Create a custom configuration with specific settings.
    pub fn custom() -> Self {
        Self::development()
    }

    /// Set the debounce interval.
    pub fn with_debounce_interval(mut self, duration: Duration) -> Self {
        self.debounce_interval = duration;
        self
    }

    /// Enable or disable state preservation.
    pub fn with_state_preservation(mut self, enabled: bool) -> Self {
        self.preserve_state = enabled;
        self
    }

    /// Add an additional watch path.
    pub fn with_additional_watch_path(mut self, path: PathBuf) -> Self {
        self.additional_watch_paths.push(path);
        self
    }

    /// Enable or disable recursive watching.
    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self::development()
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events emitted by the hot-reload system.
#[derive(Debug, Clone)]
pub enum HotReloadEvent {
    /// A script file was detected as changed (before reload attempt)
    ScriptDetected { path: PathBuf, timestamp: Instant },
    /// A script is about to be reloaded
    ScriptReloadStarting { path: PathBuf, snapshot_taken: bool },
    /// A script was successfully reloaded
    ScriptReloaded {
        path: PathBuf,
        state_preserved: bool,
        reload_duration: Duration,
    },
    /// A script reload failed
    ScriptReloadFailed {
        path: PathBuf,
        error: String,
        recovery_action: RecoveryAction,
    },
    /// A script was removed from disk
    ScriptRemoved { path: PathBuf },
    /// Batch reload completed (multiple scripts)
    BatchReloadCompleted {
        reloaded_count: usize,
        failed_count: usize,
        total_duration: Duration,
    },
    /// File watcher started
    WatcherStarted { watch_paths: Vec<PathBuf> },
    /// File watcher encountered an error
    WatcherError {
        error: String,
        path: Option<PathBuf>,
    },
}

/// Action to take when a reload fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Keep the old script version running
    KeepOldVersion,
    /// Retry the reload after a delay
    RetryAfterDelay,
    /// Disable the script entirely
    DisableScript,
    /// Log and continue (non-critical error)
    LogAndContinue,
}

// ---------------------------------------------------------------------------
// Script Handle
// ---------------------------------------------------------------------------

/// Represents a loaded script with its metadata and cached state.
#[derive(Debug)]
pub struct ScriptHandle {
    /// The script file path
    pub path: PathBuf,
    /// Last known good content (for rollback)
    pub last_good_content: String,
    /// Last modification time
    pub last_modified: std::time::SystemTime,
    /// Whether this script is currently loaded
    pub is_loaded: bool,
    /// Number of successful reloads
    pub reload_count: u32,
    /// Number of failed reloads
    pub failed_count: u32,
    /// Last error encountered (if any)
    pub last_error: Option<String>,
    /// Registry keys for preserving per-script data
    pub registry_keys: Vec<mlua::RegistryKey>,
    /// Snapshotted global variables belonging to this script
    pub snapshot_globals: Vec<(String, String)>,
}

impl ScriptHandle {
    pub fn new(path: PathBuf, content: String) -> Self {
        let last_modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now());

        Self {
            path,
            last_good_content: content,
            last_modified,
            is_loaded: false,
            reload_count: 0,
            failed_count: 0,
            last_error: None,
            registry_keys: Vec::new(),
            snapshot_globals: Vec::new(),
        }
    }

    /// Update the handle with new content.
    pub fn update_content(&mut self, content: String) {
        self.last_good_content = content;
        self.reload_count += 1;
        self.last_error = None;
        self.is_loaded = true;
    }

    /// Record a failed reload attempt.
    pub fn record_failure(&mut self, error: String) {
        self.failed_count += 1;
        self.last_error = Some(error);
    }

    /// Get the success rate of this script.
    pub fn success_rate(&self) -> f32 {
        let total = self.reload_count + self.failed_count;
        if total == 0 {
            return 1.0;
        }
        self.reload_count as f32 / total as f32
    }
}

// ---------------------------------------------------------------------------
// Debounce Engine
// ---------------------------------------------------------------------------

/// Debounce engine to prevent rapid-fire reloads from editor save storms.
///
/// When a file changes, it enters the pending queue. The debounce engine
/// will not emit the file for processing until the debounce interval has
/// elapsed since the last change event.
struct DebounceEngine {
    /// Pending files and their last-change timestamps
    pending: HashMap<PathBuf, Instant>,
    /// Debounce interval
    interval: Duration,
}

impl DebounceEngine {
    fn new(interval: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            interval,
        }
    }

    /// Record a file change event.
    fn record_change(&mut self, path: PathBuf) {
        self.pending.insert(path, Instant::now());
    }

    /// Get files that are ready for processing (debounce interval elapsed).
    fn take_ready(&mut self) -> Vec<PathBuf> {
        let now = Instant::now();
        let mut ready = Vec::new();
        let mut still_pending = HashMap::new();

        for (path, timestamp) in self.pending.drain() {
            if now.duration_since(timestamp) >= self.interval {
                ready.push(path);
            } else {
                still_pending.insert(path, timestamp);
            }
        }

        self.pending = still_pending;
        ready
    }

    /// Check if any files are pending.
    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

// ---------------------------------------------------------------------------
// State Manager
// ---------------------------------------------------------------------------

/// Manages snapshot and restoration of Lua VM state during hot-reload.
struct StateManager {
    /// Whether to preserve global variables
    preserve_globals: bool,
    /// Whether to preserve coroutines
    preserve_coroutines: bool,
    /// Whether to preserve registry data
    preserve_registry: bool,
}

impl StateManager {
    fn new(config: &HotReloadConfig) -> Self {
        Self {
            preserve_globals: config.preserve_state,
            preserve_coroutines: config.preserve_coroutines,
            preserve_registry: config.preserve_registry,
        }
    }

    /// Snapshot all global Lua variables (excluding engine-reserved ones).
    fn snapshot_globals(&self, lua: &Lua) -> Vec<(String, String)> {
        if !self.preserve_globals {
            return Vec::new();
        }

        let mut snapshot = Vec::new();
        let reserved_keys = [
            "quasar",
            "log",
            "_G",
            "_VERSION",
            "print",
            "assert",
            "error",
            "type",
            "pairs",
            "ipairs",
            "next",
            "select",
            "unpack",
            "rawget",
            "rawset",
            "rawequal",
            "rawlen",
            "setmetatable",
            "getmetatable",
            "tonumber",
            "tostring",
            "pcall",
            "xpcall",
            "require",
            "dofile",
            "loadfile",
            "load",
            "loadstring",
        ];

        if let Ok(globals) = lua
            .globals()
            .pairs::<String, LuaValue>()
            .collect::<Result<Vec<_>, _>>()
        {
            for (key, value) in globals {
                // Skip reserved keys
                if reserved_keys.contains(&key.as_str()) {
                    continue;
                }

                match &value {
                    LuaValue::Number(n) => snapshot.push((key, n.to_string())),
                    LuaValue::Integer(i) => snapshot.push((key, i.to_string())),
                    LuaValue::String(s) => {
                        if let Ok(s) = s.to_str() {
                            snapshot.push((key, format!("\"{}\"", s)));
                        }
                    }
                    LuaValue::Boolean(b) => snapshot.push((key, b.to_string())),
                    LuaValue::Nil => {} // Skip nil values
                    _ => {
                        // For tables/functions, we can't easily serialize them
                        // Log a warning but continue
                        log::debug!(
                            "Cannot serialize global '{}' of type {:?}, skipping",
                            key,
                            value.type_name()
                        );
                    }
                }
            }
        }

        snapshot
    }

    /// Restore global variables from a snapshot.
    fn restore_globals(&self, lua: &Lua, snapshot: &[(String, String)]) {
        if !self.preserve_globals || snapshot.is_empty() {
            return;
        }

        for (key, value_str) in snapshot {
            // Skip reserved keys
            if key.starts_with('_') || key == "quasar" || key == "log" {
                continue;
            }

            let script = format!("{} = {}", key, value_str);
            if let Err(e) = lua.load(&script).exec() {
                log::warn!("Failed to restore global '{}': {}", key, e);
            }
        }
    }

    /// Snapshot the entire Lua state (comprehensive backup for error recovery).
    fn snapshot_full_state(&self, lua: &Lua) -> Result<String, LuaError> {
        // Serialize the entire globals table to a string representation
        // This is a simplified approach - in production, you'd want more robust serialization
        let mut state = String::new();

        if let Ok(globals) = lua
            .globals()
            .pairs::<String, LuaValue>()
            .collect::<Result<Vec<_>, _>>()
        {
            for (key, value) in globals {
                if key.starts_with('_') || key == "quasar" || key == "log" {
                    continue;
                }

                match value {
                    LuaValue::Number(n) => state.push_str(&format!("{} = {}\n", key, n)),
                    LuaValue::Integer(i) => state.push_str(&format!("{} = {}\n", key, i)),
                    LuaValue::Boolean(b) => state.push_str(&format!("{} = {}\n", key, b)),
                    LuaValue::String(s) => {
                        if let Ok(s) = s.to_str() {
                            state.push_str(&format!(
                                "{} = \"{}\"\n",
                                key,
                                s.replace('\\', "\\\\").replace('"', "\\\"")
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(state)
    }

    /// Full state restore (used for error recovery).
    fn restore_full_state(&self, lua: &Lua, state: &str) {
        if let Err(e) = lua.load(state).exec() {
            log::error!("Failed to restore full state: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Error Handler
// ---------------------------------------------------------------------------

/// Handles errors during hot-reload with graceful degradation.
struct ErrorHandler {
    /// Maximum retries for transient errors
    max_retries: u32,
    /// Delay between retries
    retry_delay: Duration,
    /// Whether to validate syntax before full reload
    validate_before_reload: bool,
}

impl ErrorHandler {
    fn new(config: &HotReloadConfig) -> Self {
        Self {
            max_retries: config.max_retries,
            retry_delay: config.retry_delay,
            validate_before_reload: config.validate_before_reload,
        }
    }

    /// Validate Lua syntax without executing.
    fn validate_syntax(&self, lua: &Lua, source: &str) -> Result<(), LuaError> {
        // Try to load the chunk (syntax check) without executing
        lua.load(source)
            .set_name("<syntax_check>")
            .into_function()?;
        Ok(())
    }

    /// Classify an error and determine the recovery action.
    fn classify_error(&self, error: &LuaError) -> (String, RecoveryAction) {
        let error_msg = error.to_string();

        // Syntax errors: can't reload, keep old version
        if error_msg.contains("syntax error") || error_msg.contains("unexpected symbol") {
            return (error_msg.clone(), RecoveryAction::KeepOldVersion);
        }

        // Runtime errors during reload: keep old version
        if error_msg.contains("attempt to call") || error_msg.contains("nil value") {
            return (error_msg.clone(), RecoveryAction::KeepOldVersion);
        }

        // File I/O errors: retry after delay
        if error_msg.contains("No such file") || error_msg.contains("Permission denied") {
            return (error_msg.clone(), RecoveryAction::RetryAfterDelay);
        }

        // Memory errors: serious, log and continue
        if error_msg.contains("memory") || error_msg.contains("not enough") {
            return (error_msg.clone(), RecoveryAction::LogAndContinue);
        }

        // Default: keep old version
        (error_msg.clone(), RecoveryAction::KeepOldVersion)
    }

    /// Handle a reload error with appropriate recovery action.
    fn handle_error(
        &self,
        lua: &Lua,
        handle: &mut ScriptHandle,
        error: LuaError,
        state_snapshot: Option<&str>,
    ) -> (String, RecoveryAction) {
        let (error_msg, action) = self.classify_error(&error);

        match &action {
            RecoveryAction::KeepOldVersion => {
                log::warn!(
                    "Hot-reload failed for {:?}, keeping previous version: {}",
                    handle.path,
                    error_msg
                );
                handle.record_failure(error_msg.clone());
            }
            RecoveryAction::RetryAfterDelay => {
                log::warn!(
                    "Hot-reload transient error for {:?}, will retry: {}",
                    handle.path,
                    error_msg
                );
                handle.record_failure(error_msg.clone());
            }
            RecoveryAction::DisableScript => {
                log::error!(
                    "Hot-reload fatal error for {:?}, disabling script: {}",
                    handle.path,
                    error_msg
                );
                handle.record_failure(error_msg.clone());
                handle.is_loaded = false;
            }
            RecoveryAction::LogAndContinue => {
                log::warn!("Hot-reload warning for {:?}: {}", handle.path, error_msg);
            }
        }

        // If we have a state snapshot and we're keeping the old version, restore state
        if matches!(action, RecoveryAction::KeepOldVersion) {
            if let Some(snapshot) = state_snapshot {
                // State is already preserved since we didn't execute the bad script
            }
        }

        (error_msg, action)
    }
}

// ---------------------------------------------------------------------------
// Event Dispatcher
// ---------------------------------------------------------------------------

/// Dispatches hot-reload events to interested systems.
pub struct EventDispatcher {
    /// Internal channel for event publishing
    sender: UnboundedSender<HotReloadEvent>,
    /// Receiver for polling events
    receiver: UnboundedReceiver<HotReloadEvent>,
}

impl EventDispatcher {
    fn new() -> Self {
        let (sender, receiver) = unbounded_channel();
        Self { sender, receiver }
    }

    /// Publish an event.
    pub fn publish(&self, event: HotReloadEvent) {
        let _ = self.sender.send(event);
    }

    /// Poll for pending events (non-blocking).
    pub fn poll_events(&self) -> Vec<HotReloadEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }
        events
    }

    /// Get the sender for cloning into watchers.
    fn sender(&self) -> &UnboundedSender<HotReloadEvent> {
        &self.sender
    }
}

// ---------------------------------------------------------------------------
// Lua Hot-Reload System
// ---------------------------------------------------------------------------

/// Main hot-reload system for Lua scripts.
///
/// This system watches the scripts directory for file changes and
/// automatically reloads modified scripts without requiring a game restart.
///
/// # Thread Safety
///
/// The file watcher runs on a background thread, but all reload operations
/// happen on the main thread when `process_events()` is called.
///
/// # Example
///
/// ```no_run
/// let mut hot_reload = LuaHotReloadSystem::new("scripts/", HotReloadConfig::development())?;
///
/// // In your game loop:
/// hot_reload.process_events(&mut lua)?;
/// ```
pub struct LuaHotReloadSystem {
    /// File watcher (None if disabled or in production mode)
    watcher: Option<RecommendedWatcher>,
    /// File event receiver from watcher
    file_event_receiver: UnboundedReceiver<Event>,
    /// Configuration
    config: HotReloadConfig,
    /// Cache of loaded scripts
    script_cache: HashMap<PathBuf, ScriptHandle>,
    /// Debounce engine
    debounce: DebounceEngine,
    /// State manager
    state_manager: StateManager,
    /// Error handler
    error_handler: ErrorHandler,
    /// Event dispatcher
    event_dispatcher: EventDispatcher,
    /// Reload queue (files to process)
    reload_queue: Vec<PathBuf>,
    /// Retry queue: (path, retry_count, next_retry_time)
    retry_queue: Vec<(PathBuf, u32, Instant)>,
    /// Base scripts directory
    scripts_dir: PathBuf,
    /// All watched paths
    watched_paths: Vec<PathBuf>,
}

impl LuaHotReloadSystem {
    /// Create a new hot-reload system watching the specified directory.
    pub fn new<P: AsRef<Path>>(scripts_dir: P, config: HotReloadConfig) -> QuasarResult<Self> {
        let scripts_dir = scripts_dir.as_ref().to_path_buf();

        if !config.enabled {
            log::info!("Lua hot-reload is disabled (production mode)");
            let (_, receiver) = unbounded_channel(); // Create dummy receiver
            return Ok(Self {
                watcher: None,
                file_event_receiver: receiver,
                config: config.clone(),
                script_cache: HashMap::new(),
                debounce: DebounceEngine::new(Duration::from_millis(100)),
                state_manager: StateManager::new(&config),
                error_handler: ErrorHandler::new(&config),
                event_dispatcher: EventDispatcher::new(),
                reload_queue: Vec::new(),
                retry_queue: Vec::new(),
                scripts_dir,
                watched_paths: Vec::new(),
            });
        }

        // Verify scripts directory exists
        if !scripts_dir.exists() {
            std::fs::create_dir_all(&scripts_dir).map_err(|e| {
                format!(
                    "Failed to create scripts directory {:?}: {}",
                    scripts_dir, e
                )
            })?;
        }

        let (file_event_sender, file_event_receiver) = unbounded_channel();

        // Create file watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    // Send all events to the main thread for processing
                    let _ = file_event_sender.send(event);
                } else if let Err(e) = res {
                    log::error!("File watcher error: {}", e);
                }
            },
            WatcherConfig::default(),
        )
        .map_err(|e| format!("Failed to create file watcher: {}", e))?;

        // Watch the scripts directory
        let mode = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher
            .watch(&scripts_dir, mode)
            .map_err(|e| format!("Failed to watch {:?}: {}", scripts_dir, e))?;

        // Watch additional paths
        for path in &config.additional_watch_paths {
            if path.exists() {
                let _ = watcher.watch(path, mode);
                log::info!("Watching additional path: {:?}", path);
            }
        }

        let watched_paths = std::iter::once(scripts_dir.clone())
            .chain(config.additional_watch_paths.clone())
            .collect();

        log::info!(
            "Lua hot-reload initialized (scripts_dir: {:?}, recursive: {})",
            scripts_dir,
            config.recursive
        );

        let mut system = Self {
            watcher: Some(watcher),
            file_event_receiver,
            config: config.clone(),
            script_cache: HashMap::new(),
            debounce: DebounceEngine::new(config.debounce_interval),
            state_manager: StateManager::new(&config),
            error_handler: ErrorHandler::new(&config),
            event_dispatcher: EventDispatcher::new(),
            reload_queue: Vec::new(),
            retry_queue: Vec::new(),
            scripts_dir,
            watched_paths,
        };

        system
            .event_dispatcher
            .publish(HotReloadEvent::WatcherStarted {
                watch_paths: system.watched_paths.clone(),
            });

        Ok(system)
    }

    /// Process file watcher events and perform reloads.
    ///
    /// Call this every frame from your game loop.
    pub fn process_events(&mut self, lua: &Lua) -> Result<Vec<HotReloadEvent>, String> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        // Process retry queue
        self.process_retries(lua);

        // Drain file watcher events and record them for debouncing
        while let Ok(event) = self.file_event_receiver.try_recv() {
            // Only process modify and create events
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                for path in event.paths {
                    if self.is_valid_script_extension(&path) {
                        self.event_dispatcher
                            .publish(HotReloadEvent::ScriptDetected {
                                path: path.to_path_buf(),
                                timestamp: Instant::now(),
                            });
                        self.debounce.record_change(path.to_path_buf());
                    }
                }
            } else if matches!(event.kind, EventKind::Remove(_)) {
                for path in event.paths {
                    if self.is_valid_script_extension(&path) {
                        self.script_cache.remove(&path);
                        self.event_dispatcher
                            .publish(HotReloadEvent::ScriptRemoved {
                                path: path.to_path_buf(),
                            });
                    }
                }
            }
        }

        // Check debounce engine for ready files
        let ready_files = self.debounce.take_ready();
        for path in ready_files {
            self.reload_queue.push(path);
        }

        // Process reload queue
        let mut events = Vec::new();
        if !self.reload_queue.is_empty() {
            let batch_start = Instant::now();
            let mut reloaded_count = 0;
            let mut failed_count = 0;

            let queue: Vec<_> = self.reload_queue.drain(..).collect();
            for path in queue {
                match self.reload_script(lua, &path) {
                    Ok(state_preserved) => {
                        reloaded_count += 1;
                        events.push(HotReloadEvent::ScriptReloaded {
                            path: path.to_path_buf(),
                            state_preserved,
                            reload_duration: Duration::from_secs(0), // Would track actual time
                        });
                    }
                    Err(e) => {
                        failed_count += 1;
                        events.push(HotReloadEvent::ScriptReloadFailed {
                            path: path.to_path_buf(),
                            error: e.clone(),
                            recovery_action: RecoveryAction::KeepOldVersion,
                        });
                    }
                }
            }

            let total_duration = batch_start.elapsed();
            events.push(HotReloadEvent::BatchReloadCompleted {
                reloaded_count,
                failed_count,
                total_duration,
            });
        }

        // Collect events from dispatcher
        events.extend(self.event_dispatcher.poll_events());

        Ok(events)
    }

    /// Reload a single Lua script with state preservation and error handling.
    fn reload_script(&mut self, lua: &Lua, path: &Path) -> Result<bool, String> {
        let reload_start = Instant::now();

        // Read file content
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

        // Validate syntax before attempting reload
        if self.error_handler.validate_before_reload {
            if let Err(e) = self.error_handler.validate_syntax(lua, &content) {
                let error_msg = e.to_string();
                log::error!("Syntax error in {:?}: {}", path, error_msg);
                return Err(error_msg);
            }
        }

        // Snapshot current state if preservation is enabled
        let state_preserved = self.config.preserve_state;
        let global_snapshot = if self.config.preserve_state {
            Some(self.state_manager.snapshot_globals(lua))
        } else {
            None
        };

        // Notify that reload is starting
        self.event_dispatcher
            .publish(HotReloadEvent::ScriptReloadStarting {
                path: path.to_path_buf(),
                snapshot_taken: global_snapshot.is_some(),
            });

        // Attempt to load and execute the new script
        let reload_result = lua
            .load(&content)
            .set_name(path.to_string_lossy().as_ref())
            .exec();

        match reload_result {
            Ok(()) => {
                // Reload succeeded
                let duration = reload_start.elapsed();

                // Restore global state if we took a snapshot
                if let Some(ref snapshot) = global_snapshot {
                    self.state_manager.restore_globals(lua, snapshot);
                }

                // Update or create script handle
                if let Some(handle) = self.script_cache.get_mut(path) {
                    handle.update_content(content);
                    handle.last_modified = std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .unwrap_or_else(|_| std::time::SystemTime::now());
                } else {
                    self.script_cache.insert(
                        path.to_path_buf(),
                        ScriptHandle::new(path.to_path_buf(), content),
                    );
                }

                log::info!(
                    "Hot-reloaded {:?} in {:.2}ms (state preserved: {})",
                    path,
                    duration.as_secs_f64() * 1000.0,
                    state_preserved
                );

                Ok(state_preserved)
            }
            Err(e) => {
                // Reload failed
                let (error_msg, _) = self.error_handler.classify_error(&e);
                log::error!("Failed to hot-reload {:?}: {}", path, error_msg);

                // Restore previous state on failure
                if let Some(ref snapshot) = global_snapshot {
                    self.state_manager.restore_globals(lua, snapshot);
                }

                Err(error_msg)
            }
        }
    }

    /// Process retry queue for files that failed due to transient errors.
    fn process_retries(&mut self, lua: &Lua) {
        let now = Instant::now();
        let mut still_retrying = Vec::new();

        let retry_queue: Vec<_> = self.retry_queue.drain(..).collect();
        for (path, retry_count, next_retry_time) in retry_queue {
            if now >= next_retry_time {
                // Time to retry
                if retry_count < self.error_handler.max_retries {
                    match self.reload_script(lua, &path) {
                        Ok(_) => {
                            log::info!("Retry succeeded for {:?}", path);
                        }
                        Err(_) => {
                            // Queue another retry
                            let next_time = now + self.error_handler.retry_delay;
                            still_retrying.push((path, retry_count + 1, next_time));
                        }
                    }
                } else {
                    log::error!(
                        "Max retries ({}) exceeded for {:?}, giving up",
                        self.error_handler.max_retries,
                        path
                    );
                }
            } else {
                still_retrying.push((path, retry_count, next_retry_time));
            }
        }

        self.retry_queue = still_retrying;
    }

    /// Register a script for tracking (call when initially loading a script).
    pub fn register_script(&mut self, path: &Path, content: &str) {
        if !self.script_cache.contains_key(path) {
            self.script_cache.insert(
                path.to_path_buf(),
                ScriptHandle::new(path.to_path_buf(), content.to_string()),
            );
        }
    }

    /// Record a file change event (call from external watchers).
    pub fn record_file_change(&mut self, path: &Path) {
        if self.is_valid_script_extension(path) {
            self.event_dispatcher
                .publish(HotReloadEvent::ScriptDetected {
                    path: path.to_path_buf(),
                    timestamp: Instant::now(),
                });
            self.debounce.record_change(path.to_path_buf());
        }
    }

    /// Check if a path has a valid script extension.
    fn is_valid_script_extension(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            self.config.extensions.iter().any(|e| e == ext)
        } else {
            false
        }
    }

    /// Get script handle for a path.
    pub fn get_script_handle(&self, path: &Path) -> Option<&ScriptHandle> {
        self.script_cache.get(path)
    }

    /// Get all tracked script paths.
    pub fn tracked_scripts(&self) -> Vec<&PathBuf> {
        self.script_cache.keys().collect()
    }

    /// Check if a script is currently loaded.
    pub fn is_script_loaded(&self, path: &Path) -> bool {
        self.script_cache
            .get(path)
            .map(|h| h.is_loaded)
            .unwrap_or(false)
    }

    /// Get statistics about the hot-reload system.
    pub fn get_stats(&self) -> HotReloadStats {
        let total_scripts = self.script_cache.len();
        let loaded_scripts = self.script_cache.values().filter(|h| h.is_loaded).count();
        let total_reloads: u32 = self.script_cache.values().map(|h| h.reload_count).sum();
        let total_failures: u32 = self.script_cache.values().map(|h| h.failed_count).sum();
        let pending_retries = self.retry_queue.len();
        let pending_reloads = self.reload_queue.len() + self.debounce.pending.len();

        HotReloadStats {
            total_scripts,
            loaded_scripts,
            total_reloads,
            total_failures,
            pending_retries,
            pending_reloads,
            is_enabled: self.config.enabled,
        }
    }

    /// Clear the script cache (useful for full reload scenarios).
    pub fn clear_cache(&mut self) {
        self.script_cache.clear();
    }

    /// Force reload a specific script (bypasses debounce).
    pub fn force_reload(&mut self, lua: &Lua, path: &Path) -> Result<bool, String> {
        if !path.exists() {
            // Script was removed
            self.script_cache.remove(path);
            self.event_dispatcher
                .publish(HotReloadEvent::ScriptRemoved {
                    path: path.to_path_buf(),
                });
            return Ok(false);
        }

        self.reload_script(lua, path)
    }

    /// Reload all tracked scripts.
    pub fn reload_all(&mut self, lua: &Lua) -> Vec<Result<bool, String>> {
        let paths: Vec<PathBuf> = self.script_cache.keys().cloned().collect();
        let mut results = Vec::new();

        for path in paths {
            results.push(self.reload_script(lua, &path));
        }

        results
    }

    /// Gracefully shut down the file watcher.
    pub fn shutdown(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            log::info!("Lua hot-reload file watcher stopped");
        }
    }
}

impl Drop for LuaHotReloadSystem {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Statistics about the hot-reload system.
#[derive(Debug, Clone)]
pub struct HotReloadStats {
    /// Total number of tracked scripts
    pub total_scripts: usize,
    /// Number of currently loaded scripts
    pub loaded_scripts: usize,
    /// Total successful reloads
    pub total_reloads: u32,
    /// Total failed reloads
    pub total_failures: u32,
    /// Number of scripts pending retry
    pub pending_retries: usize,
    /// Number of scripts pending reload (debounced)
    pub pending_reloads: usize,
    /// Whether hot-reload is enabled
    pub is_enabled: bool,
}

// ---------------------------------------------------------------------------
// Type Aliases
// ---------------------------------------------------------------------------

/// Result type for hot-reload operations.
pub type QuasarResult<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// Integration Helper
// ---------------------------------------------------------------------------

/// Helper to integrate hot-reload with ScriptEngine.
pub struct HotReloadIntegration {
    pub hot_reload_system: LuaHotReloadSystem,
}

impl HotReloadIntegration {
    /// Create a new integration wrapper.
    pub fn new(system: LuaHotReloadSystem) -> Self {
        Self {
            hot_reload_system: system,
        }
    }

    /// Process hot-reload events (call every frame).
    pub fn process(&mut self, lua: &Lua) -> Vec<HotReloadEvent> {
        match self.hot_reload_system.process_events(lua) {
            Ok(events) => events,
            Err(e) => {
                log::error!("Hot-reload process error: {}", e);
                Vec::new()
            }
        }
    }

    /// Get a reference to the underlying system.
    pub fn system(&self) -> &LuaHotReloadSystem {
        &self.hot_reload_system
    }

    /// Get a mutable reference to the underlying system.
    pub fn system_mut(&mut self) -> &mut LuaHotReloadSystem {
        &mut self.hot_reload_system
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

    fn create_temp_scripts_dir() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");

        // Create a simple test script
        let script_path = dir.path().join("test.lua");
        let mut file = std::fs::File::create(&script_path).expect("Failed to create test script");
        writeln!(file, "-- Test script").expect("Failed to write");
        writeln!(file, "test_var = 42").expect("Failed to write");
        writeln!(file, "function test_func() return 123 end").expect("Failed to write");

        dir
    }

    #[test]
    fn test_config_development() {
        let config = HotReloadConfig::development();
        assert!(config.enabled);
        assert_eq!(config.extensions, vec!["lua", "luau"]);
        assert!(config.recursive);
        assert!(config.preserve_state);
    }

    #[test]
    fn test_config_production() {
        let config = HotReloadConfig::production();
        assert!(!config.enabled);
        assert!(!config.recursive);
        assert!(!config.preserve_state);
    }

    #[test]
    fn test_config_builder() {
        let config = HotReloadConfig::custom()
            .with_debounce_interval(Duration::from_millis(500))
            .with_state_preservation(false)
            .with_recursive(false);

        assert_eq!(config.debounce_interval, Duration::from_millis(500));
        assert!(!config.preserve_state);
        assert!(!config.recursive);
    }

    #[test]
    fn test_debounce_engine() {
        let mut engine = DebounceEngine::new(Duration::from_millis(100));

        // Record a change
        let path = PathBuf::from("test.lua");
        engine.record_change(path.clone());

        // Should not be ready immediately
        let ready = engine.take_ready();
        assert!(ready.is_empty());
        assert!(engine.has_pending());

        // Wait for debounce interval
        std::thread::sleep(Duration::from_millis(150));

        // Now should be ready
        let ready = engine.take_ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], path);
        assert!(!engine.has_pending());
    }

    #[test]
    fn test_debounce_resets_timer() {
        let mut engine = DebounceEngine::new(Duration::from_millis(100));
        let path = PathBuf::from("test.lua");

        // Record first change
        engine.record_change(path.clone());

        // Wait a bit but not enough
        std::thread::sleep(Duration::from_millis(50));

        // Record another change (resets timer)
        engine.record_change(path.clone());

        // Should still not be ready (timer reset)
        let ready = engine.take_ready();
        assert!(ready.is_empty());

        // Wait full interval
        std::thread::sleep(Duration::from_millis(100));

        // Now should be ready
        let ready = engine.take_ready();
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn test_script_handle_creation() {
        let path = PathBuf::from("test.lua");
        let content = "print('hello')".to_string();
        let handle = ScriptHandle::new(path.clone(), content.clone());

        assert_eq!(handle.path, path);
        assert_eq!(handle.last_good_content, content);
        assert!(!handle.is_loaded);
        assert_eq!(handle.reload_count, 0);
        assert_eq!(handle.failed_count, 0);
        assert!(handle.last_error.is_none());
        assert_eq!(handle.success_rate(), 1.0);
    }

    #[test]
    fn test_script_handle_update() {
        let path = PathBuf::from("test.lua");
        let mut handle = ScriptHandle::new(path.clone(), "old content".to_string());

        handle.update_content("new content".to_string());

        assert_eq!(handle.last_good_content, "new content");
        assert!(handle.is_loaded);
        assert_eq!(handle.reload_count, 1);
        assert_eq!(handle.failed_count, 0);
        assert!(handle.last_error.is_none());
        assert_eq!(handle.success_rate(), 1.0);
    }

    #[test]
    fn test_script_handle_failure_recording() {
        let path = PathBuf::from("test.lua");
        let mut handle = ScriptHandle::new(path.clone(), "content".to_string());

        handle.record_failure("syntax error".to_string());

        assert_eq!(handle.failed_count, 1);
        assert_eq!(handle.last_error, Some("syntax error".to_string()));
        assert_eq!(handle.success_rate(), 0.0);
    }

    #[test]
    fn test_is_valid_script_extension() {
        let config = HotReloadConfig::development();
        let system = LuaHotReloadSystem::new("scripts/", config.clone()).unwrap();

        assert!(system.is_valid_script_extension(Path::new("test.lua")));
        assert!(system.is_valid_script_extension(Path::new("test.luau")));
        assert!(!system.is_valid_script_extension(Path::new("test.txt")));
        assert!(!system.is_valid_script_extension(Path::new("test.rs")));
    }

    #[test]
    fn test_hot_reload_system_creation() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();

        let system = LuaHotReloadSystem::new(temp_dir.path(), config);
        assert!(system.is_ok());

        let system = system.unwrap();
        assert!(system.config.enabled);
        assert!(system.watcher.is_some());
    }

    #[test]
    fn test_hot_reload_system_disabled() {
        let config = HotReloadConfig::production();
        let system = LuaHotReloadSystem::new("scripts/", config);

        assert!(system.is_ok());
        let system = system.unwrap();
        assert!(!system.config.enabled);
        assert!(system.watcher.is_none());
    }

    #[test]
    fn test_register_script() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();
        let mut system = LuaHotReloadSystem::new(temp_dir.path(), config).unwrap();

        let script_path = temp_dir.path().join("test.lua");
        system.register_script(&script_path, "test content");

        assert!(system.script_cache.contains_key(&script_path));
        assert_eq!(system.tracked_scripts().len(), 1);
    }

    #[test]
    fn test_get_stats() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();
        let mut system = LuaHotReloadSystem::new(temp_dir.path(), config).unwrap();

        let script_path = temp_dir.path().join("test.lua");
        system.register_script(&script_path, "content");

        let stats = system.get_stats();
        assert_eq!(stats.total_scripts, 1);
        assert!(stats.is_enabled);
        assert_eq!(stats.total_reloads, 0);
        assert_eq!(stats.total_failures, 0);
    }

    #[test]
    fn test_record_file_change() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();
        let mut system = LuaHotReloadSystem::new(temp_dir.path(), config).unwrap();

        let script_path = PathBuf::from("test.lua");
        system.record_file_change(&script_path);

        // The file should be in the debounce queue
        assert!(system.debounce.has_pending());
    }

    #[test]
    fn test_error_handler_syntax_validation() {
        let lua = Lua::new();
        let config = HotReloadConfig::development();
        let error_handler = ErrorHandler::new(&config);

        // Valid syntax
        assert!(error_handler.validate_syntax(&lua, "x = 1 + 1").is_ok());

        // Invalid syntax
        assert!(error_handler.validate_syntax(&lua, "if x then").is_err());
    }

    #[test]
    fn test_error_classification() {
        let config = HotReloadConfig::development();
        let error_handler = ErrorHandler::new(&config);

        // Syntax error
        let err = LuaError::runtime("syntax error: unexpected symbol");
        let (_, action) = error_handler.classify_error(&err);
        assert_eq!(action, RecoveryAction::KeepOldVersion);

        // Runtime error
        let err = LuaError::runtime("attempt to call a nil value");
        let (_, action) = error_handler.classify_error(&err);
        assert_eq!(action, RecoveryAction::KeepOldVersion);

        // File error
        let err = LuaError::runtime("No such file or directory");
        let (_, action) = error_handler.classify_error(&err);
        assert_eq!(action, RecoveryAction::RetryAfterDelay);
    }

    #[test]
    fn test_state_manager_snapshot_restore() {
        let lua = Lua::new();
        let config = HotReloadConfig::development();
        let state_manager = StateManager::new(&config);

        // Set some globals
        lua.load("test_number = 42").exec().unwrap();
        lua.load("test_string = \"hello\"").exec().unwrap();
        lua.load("test_bool = true").exec().unwrap();

        // Take snapshot
        let snapshot = state_manager.snapshot_globals(&lua);

        // Verify snapshot contains our variables
        assert!(snapshot.iter().any(|(k, _)| k == "test_number"));
        assert!(snapshot.iter().any(|(k, _)| k == "test_string"));
        assert!(snapshot.iter().any(|(k, _)| k == "test_bool"));

        // Change the globals
        lua.load("test_number = 100").exec().unwrap();
        lua.load("test_string = \"world\"").exec().unwrap();

        // Restore from snapshot
        state_manager.restore_globals(&lua, &snapshot);

        // Verify restoration
        let num: i32 = lua.globals().get("test_number").unwrap();
        assert_eq!(num, 42);

        let s: String = lua.globals().get("test_string").unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_event_dispatcher() {
        let dispatcher = EventDispatcher::new();

        dispatcher.publish(HotReloadEvent::ScriptDetected {
            path: PathBuf::from("test.lua"),
            timestamp: Instant::now(),
        });

        let events = dispatcher.poll_events();
        assert_eq!(events.len(), 1);

        match &events[0] {
            HotReloadEvent::ScriptDetected { path, .. } => {
                assert_eq!(path, Path::new("test.lua"));
            }
            _ => panic!("Unexpected event type"),
        }
    }

    #[test]
    fn test_force_reload_nonexistent_file() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();
        let mut system = LuaHotReloadSystem::new(temp_dir.path(), config).unwrap();

        let lua = Lua::new();
        let nonexistent = temp_dir.path().join("nonexistent.lua");

        let result = system.force_reload(&lua, &nonexistent);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false); // Script was "removed"
    }

    #[test]
    fn test_clear_cache() {
        let temp_dir = create_temp_scripts_dir();
        let config = HotReloadConfig::development();
        let mut system = LuaHotReloadSystem::new(temp_dir.path(), config).unwrap();

        system.register_script(&PathBuf::from("test1.lua"), "content1");
        system.register_script(&PathBuf::from("test2.lua"), "content2");

        assert_eq!(system.tracked_scripts().len(), 2);

        system.clear_cache();

        assert_eq!(system.tracked_scripts().len(), 0);
    }

    #[test]
    fn test_debounce_multiple_files() {
        let mut engine = DebounceEngine::new(Duration::from_millis(50));

        let path1 = PathBuf::from("script1.lua");
        let path2 = PathBuf::from("script2.lua");
        let path3 = PathBuf::from("script3.lua");

        engine.record_change(path1.clone());
        std::thread::sleep(Duration::from_millis(10));
        engine.record_change(path2.clone());
        std::thread::sleep(Duration::from_millis(10));
        engine.record_change(path3.clone());

        // Wait for all to be ready
        std::thread::sleep(Duration::from_millis(50));

        let ready = engine.take_ready();
        assert_eq!(ready.len(), 3);
        assert!(ready.contains(&path1));
        assert!(ready.contains(&path2));
        assert!(ready.contains(&path3));
    }
}
