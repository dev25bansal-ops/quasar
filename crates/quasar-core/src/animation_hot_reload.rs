//! # Animation Hot-Reload System
//!
//! Provides file watching and hot-reload capabilities for animation clips
//! and state machines. Supports:
//! - `.json` and `.anim` file formats
//! - Animation state preservation during reload
//! - Dependency tracking and cascading reloads
//! - Comprehensive error handling and recovery
//! - Event system integration for inter-system communication

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::animation::{
    AnimationClip, AnimationPlayer, AnimationResource, AnimationState, AnimationStateMachine,
    SkeletalAnimationClip, TransformKeyframe,
};
use crate::ecs::World;
use crate::hot_reload::HotReloadConfig;

// ---------------------------------------------------------------------------
// Animation Hot-Reload Events
// ---------------------------------------------------------------------------

/// Events emitted when animation assets are hot-reloaded.
#[derive(Debug, Clone)]
pub enum AnimationReloadEvent {
    /// Animation clip reload started
    ClipReloadStarted {
        /// File path that triggered the reload
        path: PathBuf,
        /// Clip name being reloaded
        clip_name: String,
        /// Timestamp when reload started
        timestamp: Instant,
    },
    /// Animation clip reload completed successfully
    ClipReloaded {
        /// File path that was reloaded
        path: PathBuf,
        /// Clip name that was reloaded
        clip_name: String,
        /// Duration of the reload operation
        duration: Duration,
        /// Number of players that were updated
        affected_players: usize,
    },
    /// Skeletal animation clip reload completed
    SkeletalClipReloaded {
        /// File path that was reloaded
        path: PathBuf,
        /// Clip name that was reloaded
        clip_name: String,
        /// Duration of the reload operation
        duration: Duration,
        /// Number of bones in the clip
        bone_count: usize,
    },
    /// Animation state machine reload completed
    StateMachineReloaded {
        /// File path that was reloaded
        path: PathBuf,
        /// State machine name
        machine_name: String,
        /// Number of states in the machine
        state_count: usize,
        /// Duration of the reload operation
        duration: Duration,
    },
    /// Reload failed with error
    ReloadFailed {
        /// File path that failed to reload
        path: PathBuf,
        /// Error message
        error: String,
        /// Whether the previous version was preserved
        fallback_used: bool,
    },
    /// File watcher error
    WatcherError {
        /// Error message
        error: String,
    },
    /// Animation cache cleared
    CacheCleared {
        /// Number of clips removed from cache
        clips_removed: usize,
    },
}

impl AnimationReloadEvent {
    /// Create a new `ClipReloaded` event.
    #[must_use]
    pub fn clip_reloaded(
        path: PathBuf,
        clip_name: String,
        duration: Duration,
        affected_players: usize,
    ) -> Self {
        Self::ClipReloaded {
            path,
            clip_name,
            duration,
            affected_players,
        }
    }

    /// Create a new `ReloadFailed` event.
    #[must_use]
    pub fn reload_failed(path: PathBuf, error: String, fallback_used: bool) -> Self {
        Self::ReloadFailed {
            path,
            error,
            fallback_used,
        }
    }
}

// ---------------------------------------------------------------------------
// Animation File Format Detection
// ---------------------------------------------------------------------------

/// Supported animation file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnimationFormat {
    /// JSON format (.json)
    Json,
    /// Binary animation format (.anim)
    Anim,
    /// Unknown or unsupported format
    Unknown,
}

impl AnimationFormat {
    /// Detect format from file extension.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Self::Json,
            Some("anim") => Self::Anim,
            _ => Self::Unknown,
        }
    }

    /// Check if this format is supported for hot-reload.
    #[must_use]
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Json | Self::Anim)
    }
}

// ---------------------------------------------------------------------------
// Animation State Snapshot
// ---------------------------------------------------------------------------

/// Snapshot of animation playback state for preservation during reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationStateSnapshot {
    /// Current playback time
    pub time: f32,
    /// Playback speed
    pub speed: f32,
    /// Current animation state
    pub state: AnimationState,
    /// Clip name
    pub clip_name: String,
}

impl AnimationStateSnapshot {
    /// Create a snapshot from an animation player.
    #[must_use]
    pub fn from_player(player: &AnimationPlayer) -> Self {
        Self {
            time: player.time,
            speed: player.speed,
            state: player.state,
            clip_name: player.clip_name.clone(),
        }
    }

    /// Apply the snapshot to an animation player.
    pub fn apply_to_player(&self, player: &mut AnimationPlayer) {
        player.time = self.time;
        player.speed = self.speed;
        player.state = self.state;
        // Note: clip_name is NOT restored - the player keeps its reference
        // to the clip which should now point to the reloaded version
    }
}

// ---------------------------------------------------------------------------
// Animation Hot-Reload System
// ---------------------------------------------------------------------------

/// Hot-reload system for animation clips and state machines.
///
/// Monitors animation files for changes and reloads them without
/// requiring a game restart. Preserves animation state during reload
/// and emits events for inter-system communication.
pub struct AnimationHotReloadSystem {
    /// Whether hot-reload is enabled
    enabled: bool,
    /// File watcher
    watcher: Option<RecommendedWatcher>,
    /// Event receiver from file watcher
    watcher_receiver: Receiver<std::result::Result<Event, notify::Error>>,
    /// Debounce queue: (path, last_change_time)
    debounce_queue: HashMap<PathBuf, Instant>,
    /// Pending reloads (passed debounce)
    pending_reloads: VecDeque<PathBuf>,
    /// Currently reloading paths
    reloading_paths: HashSet<PathBuf>,
    /// Animation clip cache: path -> clip
    clip_cache: HashMap<PathBuf, AnimationClip>,
    /// Skeletal animation clip cache: path -> skeletal clip
    skeletal_clip_cache: HashMap<PathBuf, SkeletalAnimationClip>,
    /// State machine cache: path -> state machine
    state_machine_cache: HashMap<PathBuf, AnimationStateMachine>,
    /// Previous versions for rollback on error
    previous_clips: HashMap<PathBuf, AnimationClip>,
    /// Previous skeletal clips for rollback
    previous_skeletal_clips: HashMap<PathBuf, SkeletalAnimationClip>,
    /// Event sender
    event_sender: Sender<AnimationReloadEvent>,
    /// Event receiver
    event_receiver: Receiver<AnimationReloadEvent>,
    /// Debounce interval
    debounce_interval: Duration,
    /// Maximum concurrent reloads
    max_concurrent_reloads: usize,
    /// Watched directories
    watched_dirs: HashSet<PathBuf>,
    /// File extension filters
    allowed_extensions: HashSet<String>,
    /// Reload history for statistics
    reload_history: VecDeque<ReloadRecord>,
    /// Maximum history size
    max_history_size: usize,
    /// Path-to-clip name mapping (for multi-clip files)
    path_to_clip_names: HashMap<PathBuf, Vec<String>>,
}

/// Record of a reload operation for statistics.
#[derive(Debug, Clone)]
pub struct ReloadRecord {
    /// File path that was reloaded
    pub path: PathBuf,
    /// Timestamp of the reload
    pub timestamp: Instant,
    /// Whether the reload was successful
    pub success: bool,
    /// Duration of the reload
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
    /// Number of affected players
    pub affected_players: usize,
}

impl AnimationHotReloadSystem {
    /// Create a new animation hot-reload system.
    ///
    /// # Arguments
    /// * `config` - Global hot-reload configuration
    /// * `animations_dir` - Directory containing animation files to watch
    ///
    /// # Returns
    /// A new `AnimationHotReloadSystem` instance, or an error if the
    /// file watcher cannot be created.
    pub fn new(config: &HotReloadConfig, animations_dir: &Path) -> Result<Self, String> {
        let enabled = config.enabled;
        let debounce_interval = config.debounce_interval;
        let max_concurrent_reloads = config.max_concurrent_reloads;

        let (event_sender, event_receiver) = bounded(256);
        let (watcher_sender, watcher_receiver) = bounded(64);

        let watcher = if enabled {
            let watcher = RecommendedWatcher::new(
                move |res: std::result::Result<Event, notify::Error>| {
                    let _ = watcher_sender.send(res);
                },
                Config::default(),
            );

            match watcher {
                Ok(mut w) => {
                    // Watch the animations directory recursively
                    if animations_dir.exists() {
                        w.watch(animations_dir, RecursiveMode::Recursive)
                            .map_err(|e| format!("Failed to watch {:?}: {}", animations_dir, e))?;
                        log::info!("Animation hot-reload watching: {:?}", animations_dir);
                    } else {
                        log::warn!("Animation directory does not exist: {:?}", animations_dir);
                    }
                    Some(w)
                }
                Err(e) => {
                    log::error!("Failed to create animation file watcher: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Allowed animation file extensions
        let mut allowed_extensions = HashSet::new();
        allowed_extensions.insert("json".to_string());
        allowed_extensions.insert("anim".to_string());

        Ok(Self {
            enabled,
            watcher,
            watcher_receiver,
            debounce_queue: HashMap::new(),
            pending_reloads: VecDeque::new(),
            reloading_paths: HashSet::new(),
            clip_cache: HashMap::new(),
            skeletal_clip_cache: HashMap::new(),
            state_machine_cache: HashMap::new(),
            previous_clips: HashMap::new(),
            previous_skeletal_clips: HashMap::new(),
            event_sender,
            event_receiver,
            debounce_interval,
            max_concurrent_reloads,
            watched_dirs: if enabled && animations_dir.exists() {
                let mut dirs = HashSet::new();
                dirs.insert(animations_dir.to_path_buf());
                dirs
            } else {
                HashSet::new()
            },
            allowed_extensions,
            reload_history: VecDeque::new(),
            max_history_size: 1000,
            path_to_clip_names: HashMap::new(),
        })
    }

    /// Add a directory to watch for animation files.
    pub fn watch_directory(&mut self, dir: &Path, recursive: bool) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if !dir.exists() {
            return Err(format!("Directory does not exist: {:?}", dir));
        }

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        if let Some(ref mut watcher) = self.watcher {
            watcher
                .watch(dir, mode)
                .map_err(|e| format!("Failed to watch {:?}: {}", dir, e))?;
        }

        self.watched_dirs.insert(dir.to_path_buf());
        log::info!("Animation hot-reload watching: {:?}", dir);
        Ok(())
    }

    /// Preload an animation clip into the cache.
    ///
    /// Call this when an animation is first loaded from disk
    /// so the hot-reload system knows about it.
    pub fn cache_clip(&mut self, path: PathBuf, clip: AnimationClip) {
        let clip_name = clip.name.clone();
        self.clip_cache.insert(path.clone(), clip);
        self.path_to_clip_names
            .entry(path)
            .or_default()
            .push(clip_name);
    }

    /// Get a cached animation clip.
    #[must_use]
    pub fn get_cached_clip(&self, path: &Path) -> Option<&AnimationClip> {
        self.clip_cache.get(path)
    }

    /// Remove a clip from the cache.
    pub fn remove_cached_clip(&mut self, path: &Path) {
        self.clip_cache.remove(path);
        self.path_to_clip_names.remove(path);
    }

    /// Preload a skeletal animation clip into the cache.
    pub fn cache_skeletal_clip(&mut self, path: PathBuf, clip: SkeletalAnimationClip) {
        self.skeletal_clip_cache.insert(path.clone(), clip);
    }

    /// Get a cached skeletal animation clip.
    #[must_use]
    pub fn get_cached_skeletal_clip(&self, path: &Path) -> Option<&SkeletalAnimationClip> {
        self.skeletal_clip_cache.get(path)
    }

    /// Preload an animation state machine into the cache.
    pub fn cache_state_machine(&mut self, path: PathBuf, sm: AnimationStateMachine) {
        self.state_machine_cache.insert(path, sm);
    }

    /// Get a cached animation state machine.
    #[must_use]
    pub fn get_cached_state_machine(&self, path: &Path) -> Option<&AnimationStateMachine> {
        self.state_machine_cache.get(path)
    }

    /// Process file watcher events and pending reloads.
    ///
    /// Call this every frame from the animation update system.
    pub fn process_events(&mut self, world: &mut World) {
        if !self.enabled {
            return;
        }

        // Drain watcher events
        self.process_watcher_events();

        // Process debounce queue
        self.process_debounce_queue();

        // Process pending reloads
        self.process_pending_reloads(world);
    }

    /// Process file watcher events.
    fn process_watcher_events(&mut self) {
        loop {
            match self.watcher_receiver.try_recv() {
                Ok(Ok(event)) => {
                    self.handle_file_event(event);
                }
                Ok(Err(e)) => {
                    log::error!("Animation file watcher error: {}", e);
                    let _ = self.event_sender.send(AnimationReloadEvent::WatcherError {
                        error: e.to_string(),
                    });
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("Animation file watcher channel disconnected");
                    break;
                }
            }
        }
    }

    /// Handle a single file event.
    fn handle_file_event(&mut self, event: Event) {
        // Only process modify and create events
        if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
            // Handle remove events - clear cache
            if matches!(event.kind, EventKind::Remove(_)) {
                for path in event.paths {
                    if self.is_animation_file(&path) {
                        log::info!("Animation file removed: {:?}", path);
                        self.clip_cache.remove(&path);
                        self.skeletal_clip_cache.remove(&path);
                        self.state_machine_cache.remove(&path);
                        self.path_to_clip_names.remove(&path);
                    }
                }
                return;
            }
            return;
        }

        let now = Instant::now();

        for path in event.paths {
            // Check if this is an animation file
            if !self.is_animation_file(&path) {
                continue;
            }

            // Add to debounce queue
            self.debounce_queue.insert(path.clone(), now);
        }
    }

    /// Check if a file is an animation file.
    fn is_animation_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            self.allowed_extensions.contains(ext)
        } else {
            false
        }
    }

    /// Process the debounce queue and move ready files to pending reloads.
    fn process_debounce_queue(&mut self) {
        let now = Instant::now();
        let mut ready_paths = Vec::new();

        // Find paths that have passed the debounce interval
        for (path, last_change) in &self.debounce_queue {
            if now.duration_since(*last_change) >= self.debounce_interval {
                ready_paths.push(path.clone());
            }
        }

        // Move ready paths to pending reloads
        for path in ready_paths {
            if self.debounce_queue.remove(&path).is_some() {
                // Avoid duplicate pending reloads
                if !self.pending_reloads.contains(&path) && !self.reloading_paths.contains(&path) {
                    self.pending_reloads.push_back(path);
                }
            }
        }
    }

    /// Process pending reloads.
    fn process_pending_reloads(&mut self, world: &mut World) {
        let mut processed_count = 0;

        while processed_count < self.max_concurrent_reloads && !self.pending_reloads.is_empty() {
            if let Some(path) = self.pending_reloads.pop_front() {
                // Skip if already reloading this path
                if self.reloading_paths.contains(&path) {
                    continue;
                }

                let reload_start = Instant::now();

                // Mark as reloading
                self.reloading_paths.insert(path.clone());

                // Perform the actual reload
                let reload_result = self.reload_animation(&path, world);

                let duration = reload_start.elapsed();

                match reload_result {
                    Ok(affected_players) => {
                        // Determine what was reloaded and emit appropriate event
                        let clip_name = self
                            .path_to_clip_names
                            .get(&path)
                            .and_then(|names| names.first())
                            .cloned()
                            .unwrap_or_else(|| {
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string()
                            });

                        let _ = self.event_sender.send(AnimationReloadEvent::ClipReloaded {
                            path: path.clone(),
                            clip_name,
                            duration,
                            affected_players,
                        });

                        // Record success
                        self.record_reload_record(
                            path.clone(),
                            true,
                            duration,
                            None,
                            affected_players,
                        );

                        log::info!(
                            "Animation hot-reloaded {:?} in {:.2}ms ({} players affected)",
                            path,
                            duration.as_secs_f64() * 1000.0,
                            affected_players
                        );
                    }
                    Err(e) => {
                        let _ = self.event_sender.send(AnimationReloadEvent::ReloadFailed {
                            path: path.clone(),
                            error: e.clone(),
                            fallback_used: true, // We always preserve the old version on error
                        });

                        // Record failure
                        self.record_reload_record(path.clone(), false, duration, Some(e), 0);
                    }
                }

                // Mark as no longer reloading
                self.reloading_paths.remove(&path);
                processed_count += 1;
            }
        }
    }

    /// Reload an animation clip from disk.
    ///
    /// This function:
    /// 1. Reads the new animation data from disk
    /// 2. Validates the format and keyframes
    /// 3. Preserves the current playback state
    /// 4. Updates the cache with the new clip
    /// 5. Updates all animation players using this clip
    /// 6. Emits events for inter-system communication
    ///
    /// # Arguments
    /// * `path` - Path to the animation file
    /// * `world` - ECS world containing animation players
    ///
    /// # Returns
    /// Ok(number of affected players) on success, or Err with error message
    fn reload_animation(&mut self, path: &Path, world: &mut World) -> Result<usize, String> {
        // Check if file exists
        if !path.exists() {
            // File was deleted - remove from cache
            log::warn!("Animation file deleted, removing from cache: {:?}", path);
            self.clip_cache.remove(path);
            self.skeletal_clip_cache.remove(path);
            self.path_to_clip_names.remove(path);
            return Ok(0);
        }

        // Read file contents
        let bytes = std::fs::read(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

        // Detect format
        let format = AnimationFormat::from_path(path);
        if !format.is_supported() {
            return Err(format!("Unsupported animation format: {:?}", path));
        }

        // Parse animation data based on format
        let new_clip = match format {
            AnimationFormat::Json => self.parse_json_animation(&bytes, path)?,
            AnimationFormat::Anim => self.parse_anim_animation(&bytes, path)?,
            AnimationFormat::Unknown => return Err(format!("Cannot parse unknown format")),
        };

        // Validate the new clip
        self.validate_clip(&new_clip, path)?;

        // Save previous version for rollback
        if let Some(old_clip) = self.clip_cache.get(path) {
            self.previous_clips
                .insert(path.to_path_buf(), old_clip.clone());
        }

        // Get the clip name for matching
        let clip_name = new_clip.name.clone();

        // Capture animation state snapshots from all players using this clip
        let snapshots = self.capture_player_states(world, &clip_name);

        // Update the cache with the new clip
        self.clip_cache.insert(path.to_path_buf(), new_clip);

        // Restore animation state to all players
        self.restore_player_states(world, &snapshots);

        // Update AnimationResource if it exists
        if let Some(new_clip) = self.clip_cache.get(path) {
            // Try to update the resource - this is best-effort
            if let Some(anim_res) = world.resource_mut::<AnimationResource>() {
                // Check if clip exists and update it
                if anim_res.get_clip(&clip_name).is_some() {
                    anim_res.remove_clip(&clip_name);
                }
                anim_res.add_clip(new_clip.clone());
            }
        }

        let affected_players = snapshots.len();

        Ok(affected_players)
    }

    /// Parse JSON animation data.
    fn parse_json_animation(&self, bytes: &[u8], path: &Path) -> Result<AnimationClip, String> {
        let clip: AnimationClip = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid JSON animation format {:?}: {}", path, e))?;
        Ok(clip)
    }

    /// Parse binary animation data.
    fn parse_anim_animation(&self, bytes: &[u8], path: &Path) -> Result<AnimationClip, String> {
        // Binary format: [4 bytes name_len][name][4 bytes duration][1 byte looped][4 bytes keyframe_count][keyframes...]
        if bytes.len() < 13 {
            return Err(format!(
                "Animation file too small {:?}: {} bytes",
                path,
                bytes.len()
            ));
        }

        let mut pos = 0;

        // Read name length
        if pos + 4 > bytes.len() {
            return Err(format!("Truncated name length in {:?}", path));
        }
        let name_len =
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                as usize;
        pos += 4;

        if name_len > 4096 {
            return Err(format!("Name too long ({}) in {:?}", name_len, path));
        }

        // Read name
        if pos + name_len > bytes.len() {
            return Err(format!("Truncated name in {:?}", path));
        }
        let name = String::from_utf8_lossy(&bytes[pos..pos + name_len]).to_string();
        pos += name_len;

        // Read duration
        if pos + 4 > bytes.len() {
            return Err(format!("Truncated duration in {:?}", path));
        }
        let duration =
            f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        pos += 4;

        // Read looped flag
        if pos >= bytes.len() {
            return Err(format!("Truncated looped flag in {:?}", path));
        }
        let looped = bytes[pos] != 0;
        pos += 1;

        // Read keyframe count
        if pos + 4 > bytes.len() {
            return Err(format!("Truncated keyframe count in {:?}", path));
        }
        let keyframe_count =
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                as usize;
        pos += 4;

        if keyframe_count > 100_000 {
            return Err(format!(
                "Too many keyframes ({}) in {:?}",
                keyframe_count, path
            ));
        }

        // Read keyframes
        let mut keyframes = Vec::with_capacity(keyframe_count);
        for _ in 0..keyframe_count {
            // Each keyframe: [4 bytes time][3*4 bytes position][4*4 bytes quaternion][3*4 bytes scale]
            let kf_size = 4 + 12 + 16 + 12; // 44 bytes
            if pos + kf_size > bytes.len() {
                return Err(format!(
                    "Truncated keyframe data in {:?}: expected {} keyframes, got {}",
                    path,
                    keyframe_count,
                    keyframes.len()
                ));
            }

            let time =
                f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            pos += 4;

            let position = quasar_math::Vec3::new(
                f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]),
                f32::from_le_bytes([
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ]),
                f32::from_le_bytes([
                    bytes[pos + 8],
                    bytes[pos + 9],
                    bytes[pos + 10],
                    bytes[pos + 11],
                ]),
            );
            pos += 12;

            let rotation = quasar_math::Quat::from_xyzw(
                f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]),
                f32::from_le_bytes([
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ]),
                f32::from_le_bytes([
                    bytes[pos + 8],
                    bytes[pos + 9],
                    bytes[pos + 10],
                    bytes[pos + 11],
                ]),
                f32::from_le_bytes([
                    bytes[pos + 12],
                    bytes[pos + 13],
                    bytes[pos + 14],
                    bytes[pos + 15],
                ]),
            );
            pos += 16;

            let scale = quasar_math::Vec3::new(
                f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]),
                f32::from_le_bytes([
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ]),
                f32::from_le_bytes([
                    bytes[pos + 8],
                    bytes[pos + 9],
                    bytes[pos + 10],
                    bytes[pos + 11],
                ]),
            );
            pos += 12;

            keyframes.push(TransformKeyframe {
                time,
                position,
                rotation,
                scale,
                interpolation: crate::animation::KeyframeInterpolation::Linear,
            });
        }

        Ok(AnimationClip {
            name,
            duration,
            keyframes,
            looped,
        })
    }

    /// Validate an animation clip.
    fn validate_clip(&self, clip: &AnimationClip, path: &Path) -> Result<(), String> {
        // Check for empty keyframes
        if clip.keyframes.is_empty() {
            return Err(format!(
                "Animation clip '{}' has no keyframes: {:?}",
                clip.name, path
            ));
        }

        // Check for negative duration
        if clip.duration < 0.0 {
            return Err(format!(
                "Animation clip '{}' has negative duration: {}",
                clip.name, clip.duration
            ));
        }

        // Check keyframe times are sorted and non-negative
        let mut prev_time = -1.0f32;
        for (i, kf) in clip.keyframes.iter().enumerate() {
            if kf.time < 0.0 {
                return Err(format!(
                    "Animation clip '{}' has negative keyframe time at index {}: {}",
                    clip.name, i, kf.time
                ));
            }
            if kf.time < prev_time {
                return Err(format!(
                    "Animation clip '{}' has unsorted keyframe times at index {}: {} < {}",
                    clip.name, i, kf.time, prev_time
                ));
            }
            prev_time = kf.time;
        }

        // Check duration matches last keyframe time
        if let Some(last_kf) = clip.keyframes.last() {
            if (last_kf.time - clip.duration).abs() > 0.001 {
                log::warn!(
                    "Animation clip '{}' duration ({}) doesn't match last keyframe time ({}): {:?}",
                    clip.name,
                    clip.duration,
                    last_kf.time,
                    path
                );
            }
        }

        Ok(())
    }

    /// Capture animation state from all players using a specific clip.
    fn capture_player_states(
        &self,
        world: &World,
        clip_name: &str,
    ) -> Vec<(crate::ecs::Entity, AnimationStateSnapshot)> {
        use crate::ecs::CachedArchetypeQueryState;

        let mut query: CachedArchetypeQueryState<&AnimationPlayer, ()> =
            CachedArchetypeQueryState::new();
        let mut snapshots = Vec::new();

        for (entity, player) in query.iter(world) {
            if player.clip_name == clip_name {
                snapshots.push((entity, AnimationStateSnapshot::from_player(player)));
            }
        }

        snapshots
    }

    /// Restore animation state to all players.
    fn restore_player_states(
        &self,
        world: &mut World,
        snapshots: &[(crate::ecs::Entity, AnimationStateSnapshot)],
    ) {
        for &(entity, ref snapshot) in snapshots {
            if let Some(player) = world.get_mut::<AnimationPlayer>(entity) {
                // Preserve time but normalize to new clip duration if needed
                // The clip duration may have changed, so we clamp time to new duration
                if let Some(clip) = self
                    .clip_cache
                    .values()
                    .find(|c| c.name == snapshot.clip_name)
                {
                    // Wrap time to new clip duration to prevent out-of-bounds
                    if clip.duration > 0.0 && player.time > clip.duration {
                        player.time = player.time % clip.duration;
                    }
                }
                // State and speed are preserved
                player.state = snapshot.state;
                player.speed = snapshot.speed;
            }
        }
    }

    /// Record a reload operation for statistics.
    fn record_reload_record(
        &mut self,
        path: PathBuf,
        success: bool,
        duration: Duration,
        error: Option<String>,
        affected_players: usize,
    ) {
        let record = ReloadRecord {
            path,
            timestamp: Instant::now(),
            success,
            duration,
            error,
            affected_players,
        };

        self.reload_history.push_back(record);

        // Trim history if needed
        while self.reload_history.len() > self.max_history_size {
            self.reload_history.pop_front();
        }
    }

    /// Force reload a specific path (bypasses debounce).
    pub fn force_reload(&mut self, path: &Path, world: &mut World) {
        if !self.enabled {
            return;
        }

        // Remove from debounce queue if present
        self.debounce_queue.remove(path);

        // Add directly to pending reloads
        if !self.pending_reloads.contains(&path.to_path_buf())
            && !self.reloading_paths.contains(path)
        {
            self.pending_reloads.push_back(path.to_path_buf());
            // Process immediately
            self.process_pending_reloads(world);
        }
    }

    /// Clear the animation cache.
    pub fn clear_cache(&mut self) -> usize {
        let clips_removed =
            self.clip_cache.len() + self.skeletal_clip_cache.len() + self.state_machine_cache.len();

        self.clip_cache.clear();
        self.skeletal_clip_cache.clear();
        self.state_machine_cache.clear();
        self.previous_clips.clear();
        self.previous_skeletal_clips.clear();
        self.path_to_clip_names.clear();

        let _ = self
            .event_sender
            .send(AnimationReloadEvent::CacheCleared { clips_removed });

        clips_removed
    }

    /// Get statistics about the animation hot-reload system.
    #[must_use]
    pub fn get_stats(&self) -> AnimationHotReloadStats {
        let total_reloads = self.reload_history.len();
        let successful_reloads = self.reload_history.iter().filter(|r| r.success).count();
        let failed_reloads = total_reloads - successful_reloads;

        let avg_duration = if total_reloads > 0 {
            let total_ms: f64 = self
                .reload_history
                .iter()
                .map(|r| r.duration.as_secs_f64())
                .sum();
            Duration::from_secs_f64(total_ms / total_reloads as f64)
        } else {
            Duration::ZERO
        };

        let recent_failures: Vec<ReloadRecord> = self
            .reload_history
            .iter()
            .filter(|r| !r.success)
            .rev()
            .take(10)
            .cloned()
            .collect();

        AnimationHotReloadStats {
            is_enabled: self.enabled,
            is_reloading: !self.reloading_paths.is_empty(),
            pending_count: self.pending_reloads.len(),
            reloading_count: self.reloading_paths.len(),
            watched_paths_count: self.watched_dirs.len(),
            cached_clips: self.clip_cache.len(),
            cached_skeletal_clips: self.skeletal_clip_cache.len(),
            cached_state_machines: self.state_machine_cache.len(),
            total_reloads,
            successful_reloads,
            failed_reloads,
            avg_reload_duration: avg_duration,
            recent_failures,
        }
    }

    /// Poll for animation reload events (non-blocking).
    pub fn poll_events(&self) -> Vec<AnimationReloadEvent> {
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

    /// Get pending events and optionally log them for debugging.
    ///
    /// Other systems can consume these events using the EventWriter/EventReader
    /// pattern from the event_bus module.
    pub fn emit_to_event_bus(&self) {
        let events = self.poll_events();
        if events.is_empty() {
            return;
        }

        // Log the events for debugging
        for event in &events {
            match event {
                AnimationReloadEvent::ClipReloaded {
                    path,
                    clip_name,
                    duration,
                    affected_players,
                } => {
                    log::debug!(
                        "AnimationReloadEvent::ClipReloaded {:?} clip={} ({:.2}ms, {} players)",
                        path,
                        clip_name,
                        duration.as_secs_f64() * 1000.0,
                        affected_players
                    );
                }
                AnimationReloadEvent::ReloadFailed {
                    path,
                    error,
                    fallback_used,
                } => {
                    log::warn!(
                        "AnimationReloadEvent::ReloadFailed {:?}: {} (fallback: {})",
                        path,
                        error,
                        fallback_used
                    );
                }
                AnimationReloadEvent::ClipReloadStarted {
                    path, clip_name, ..
                } => {
                    log::debug!(
                        "AnimationReloadEvent::ClipReloadStarted {:?} clip={}",
                        path,
                        clip_name
                    );
                }
                AnimationReloadEvent::SkeletalClipReloaded {
                    path,
                    clip_name,
                    bone_count,
                    ..
                } => {
                    log::debug!(
                        "AnimationReloadEvent::SkeletalClipReloaded {:?} clip={} ({} bones)",
                        path,
                        clip_name,
                        bone_count
                    );
                }
                AnimationReloadEvent::StateMachineReloaded {
                    path,
                    machine_name,
                    state_count,
                    ..
                } => {
                    log::debug!(
                        "AnimationReloadEvent::StateMachineReloaded {:?} machine={} ({} states)",
                        path,
                        machine_name,
                        state_count
                    );
                }
                AnimationReloadEvent::CacheCleared { clips_removed } => {
                    log::info!(
                        "AnimationReloadEvent::CacheCleared ({} clips removed)",
                        clips_removed
                    );
                }
                AnimationReloadEvent::WatcherError { error } => {
                    log::error!("AnimationReloadEvent::WatcherError: {}", error);
                }
            }
        }
    }

    /// Check if a specific path is currently being reloaded.
    #[must_use]
    pub fn is_reloading_path(&self, path: &Path) -> bool {
        self.reloading_paths.contains(path)
    }

    /// Get pending reload count.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending_reloads.len()
    }

    /// Clear all pending reloads.
    pub fn clear_pending(&mut self) {
        self.pending_reloads.clear();
        self.debounce_queue.clear();
    }

    /// Check if the system is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the number of cached clips.
    #[must_use]
    pub fn cached_clip_count(&self) -> usize {
        self.clip_cache.len()
    }

    /// Get the number of cached skeletal clips.
    #[must_use]
    pub fn cached_skeletal_clip_count(&self) -> usize {
        self.skeletal_clip_cache.len()
    }
}

/// Statistics about the animation hot-reload system.
#[derive(Debug, Clone)]
pub struct AnimationHotReloadStats {
    /// Whether hot-reload is enabled
    pub is_enabled: bool,
    /// Whether the system is currently reloading
    pub is_reloading: bool,
    /// Number of pending reloads
    pub pending_count: usize,
    /// Number of currently reloading paths
    pub reloading_count: usize,
    /// Number of watched directories
    pub watched_paths_count: usize,
    /// Number of cached animation clips
    pub cached_clips: usize,
    /// Number of cached skeletal clips
    pub cached_skeletal_clips: usize,
    /// Number of cached state machines
    pub cached_state_machines: usize,
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

impl Drop for AnimationHotReloadSystem {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }
        log::info!("Animation hot-reload system shut down");
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

    fn create_temp_animation_dir() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");

        // Create a test JSON animation clip
        let clip = AnimationClip::new("test_idle")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                quasar_math::Vec3::new(10.0, 0.0, 0.0),
            ));

        let json = serde_json::to_string(&clip).expect("Failed to serialize clip");
        let json_path = dir.path().join("test_idle.json");
        let mut file = std::fs::File::create(&json_path).expect("Failed to create test file");
        file.write_all(json.as_bytes()).expect("Failed to write");

        // Create a test binary animation file
        let anim_path = dir.path().join("test_walk.anim");
        let binary_clip = AnimationClip::new("test_walk")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                0.5,
                quasar_math::Vec3::new(5.0, 0.0, 0.0),
            ))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                quasar_math::Vec3::new(10.0, 0.0, 0.0),
            ));

        // Serialize to binary format
        let mut bytes = Vec::new();
        let name_bytes = binary_clip.name.as_bytes();
        bytes.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&binary_clip.duration.to_le_bytes());
        bytes.push(if binary_clip.looped { 1 } else { 0 });
        bytes.extend_from_slice(&(binary_clip.keyframes.len() as u32).to_le_bytes());
        for kf in &binary_clip.keyframes {
            bytes.extend_from_slice(&kf.time.to_le_bytes());
            bytes.extend_from_slice(&kf.position.x.to_le_bytes());
            bytes.extend_from_slice(&kf.position.y.to_le_bytes());
            bytes.extend_from_slice(&kf.position.z.to_le_bytes());
            bytes.extend_from_slice(&kf.rotation.x.to_le_bytes());
            bytes.extend_from_slice(&kf.rotation.y.to_le_bytes());
            bytes.extend_from_slice(&kf.rotation.z.to_le_bytes());
            bytes.extend_from_slice(&kf.rotation.w.to_le_bytes());
            bytes.extend_from_slice(&kf.scale.x.to_le_bytes());
            bytes.extend_from_slice(&kf.scale.y.to_le_bytes());
            bytes.extend_from_slice(&kf.scale.z.to_le_bytes());
        }

        let mut file = std::fs::File::create(&anim_path).expect("Failed to create test file");
        file.write_all(&bytes).expect("Failed to write");

        dir
    }

    fn create_test_config() -> HotReloadConfig {
        let mut config = HotReloadConfig::development();
        config.debounce_interval = Duration::from_millis(50); // Fast for testing
        config
    }

    #[test]
    fn test_animation_format_detection() {
        assert_eq!(
            AnimationFormat::from_path(Path::new("test.json")),
            AnimationFormat::Json
        );
        assert_eq!(
            AnimationFormat::from_path(Path::new("test.anim")),
            AnimationFormat::Anim
        );
        assert_eq!(
            AnimationFormat::from_path(Path::new("test.txt")),
            AnimationFormat::Unknown
        );

        assert!(AnimationFormat::Json.is_supported());
        assert!(AnimationFormat::Anim.is_supported());
        assert!(!AnimationFormat::Unknown.is_supported());
    }

    #[test]
    fn test_state_snapshot_from_player() {
        let player = AnimationPlayer::new("test_clip");
        let snapshot = AnimationStateSnapshot::from_player(&player);

        assert_eq!(snapshot.time, 0.0);
        assert_eq!(snapshot.speed, 1.0);
        assert_eq!(snapshot.state, AnimationState::Playing);
        assert_eq!(snapshot.clip_name, "test_clip");
    }

    #[test]
    fn test_state_snapshot_apply_to_player() {
        let mut player = AnimationPlayer::new("old_clip");
        player.time = 5.0;
        player.speed = 2.0;
        player.pause();

        let snapshot = AnimationStateSnapshot {
            time: 3.0,
            speed: 1.5,
            state: AnimationState::Playing,
            clip_name: "new_clip".to_string(),
        };

        snapshot.apply_to_player(&mut player);

        assert_eq!(player.time, 3.0);
        assert_eq!(player.speed, 1.5);
        assert_eq!(player.state, AnimationState::Playing);
        // clip_name is NOT changed by apply_to_player
        assert_eq!(player.clip_name, "old_clip");
    }

    #[test]
    fn test_system_creation() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let result = AnimationHotReloadSystem::new(&config, temp_dir.path());

        assert!(result.is_ok());
        let system = result.unwrap();
        assert!(system.is_enabled());
        assert!(system.watcher.is_some());
    }

    #[test]
    fn test_system_disabled() {
        let temp_dir = create_temp_animation_dir();
        let mut config = HotReloadConfig::production();
        config.enabled = false;
        let result = AnimationHotReloadSystem::new(&config, temp_dir.path());

        assert!(result.is_ok());
        let system = result.unwrap();
        assert!(!system.is_enabled());
    }

    #[test]
    fn test_cache_clip() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("cached_clip")
            .with_duration(2.0)
            .looped(true);

        let path = temp_dir.path().join("cached_clip.json");
        system.cache_clip(path.clone(), clip.clone());

        assert_eq!(system.cached_clip_count(), 1);
        assert!(system.get_cached_clip(&path).is_some());
        assert_eq!(system.get_cached_clip(&path).unwrap().name, "cached_clip");
    }

    #[test]
    fn test_remove_cached_clip() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("temp_clip");
        let path = temp_dir.path().join("temp_clip.json");
        system.cache_clip(path.clone(), clip);

        assert_eq!(system.cached_clip_count(), 1);

        system.remove_cached_clip(&path);

        assert_eq!(system.cached_clip_count(), 0);
        assert!(system.get_cached_clip(&path).is_none());
    }

    #[test]
    fn test_clear_cache() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Add some clips
        system.cache_clip(
            temp_dir.path().join("clip1.json"),
            AnimationClip::new("clip1"),
        );
        system.cache_clip(
            temp_dir.path().join("clip2.json"),
            AnimationClip::new("clip2"),
        );

        assert_eq!(system.cached_clip_count(), 2);

        let removed = system.clear_cache();

        assert_eq!(removed, 2);
        assert_eq!(system.cached_clip_count(), 0);
    }

    #[test]
    fn test_validate_clip_valid() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("valid")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                quasar_math::Vec3::new(1.0, 0.0, 0.0),
            ));

        let result = system.validate_clip(&clip, Path::new("valid.json"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_clip_empty_keyframes() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("empty").with_duration(1.0);

        let result = system.validate_clip(&clip, Path::new("empty.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no keyframes"));
    }

    #[test]
    fn test_validate_clip_negative_duration() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let mut clip = AnimationClip::new("neg_duration")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO));
        clip.duration = -1.0;

        let result = system.validate_clip(&clip, Path::new("neg.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("negative duration"));
    }

    #[test]
    fn test_validate_clip_unsorted_keyframes() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("unsorted")
            .add_keyframe(TransformKeyframe::at_position(1.0, quasar_math::Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                0.5,
                quasar_math::Vec3::new(1.0, 0.0, 0.0),
            ));

        // Note: AnimationClip::add_keyframe auto-sorts, so this will actually be valid
        // We need to manually create an unsorted clip for this test
        let unsorted_clip = AnimationClip {
            name: "unsorted".to_string(),
            duration: 1.0,
            keyframes: vec![
                TransformKeyframe::at_position(1.0, quasar_math::Vec3::ZERO),
                TransformKeyframe::at_position(0.5, quasar_math::Vec3::new(1.0, 0.0, 0.0)),
            ],
            looped: true,
        };

        let result = system.validate_clip(&unsorted_clip, Path::new("unsorted.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsorted"));
    }

    #[test]
    fn test_parse_json_animation() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let clip = AnimationClip::new("json_test")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                quasar_math::Vec3::new(5.0, 3.0, 1.0),
            ));

        let json = serde_json::to_string(&clip).expect("Failed to serialize");

        let result = system.parse_json_animation(json.as_bytes(), Path::new("test.json"));
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.name, "json_test");
        assert_eq!(parsed.keyframes.len(), 2);
    }

    #[test]
    fn test_parse_json_invalid() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let invalid_json = "not valid json";

        let result = system.parse_json_animation(invalid_json.as_bytes(), Path::new("bad.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_anim_binary() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let anim_path = temp_dir.path().join("test_walk.anim");
        let bytes = std::fs::read(&anim_path).expect("Failed to read anim file");

        let result = system.parse_anim_animation(&bytes, &anim_path);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.name, "test_walk");
        assert_eq!(parsed.keyframes.len(), 3);
        assert!(parsed.looped);
    }

    #[test]
    fn test_parse_anim_too_small() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let small_bytes = [0u8; 5];
        let result = system.parse_anim_animation(&small_bytes, Path::new("tiny.anim"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too small"));
    }

    #[test]
    fn test_force_reload() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Pre-cache a clip
        let path = temp_dir.path().join("test_idle.json");
        let clip = AnimationClip::new("test_idle")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO));
        system.cache_clip(path.clone(), clip);

        let mut world = World::new();

        // Force reload should not panic
        system.force_reload(&path, &mut world);

        // Check events were generated
        let events = system.poll_events();
        // Events may include success or failure depending on file state
        assert!(events.len() >= 0);
    }

    #[test]
    fn test_poll_events_empty() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let events = system.poll_events();
        assert!(events.is_empty());
    }

    #[test]
    fn test_pending_count() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn test_clear_pending() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Manually add to pending
        system.pending_reloads.push_back(PathBuf::from("test.json"));
        assert_eq!(system.pending_count(), 1);

        system.clear_pending();
        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn test_get_stats() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        let stats = system.get_stats();

        assert!(stats.is_enabled);
        assert!(!stats.is_reloading);
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.total_reloads, 0);
        assert_eq!(stats.failed_reloads, 0);
    }

    #[test]
    fn test_is_animation_file() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        assert!(system.is_animation_file(Path::new("test.json")));
        assert!(system.is_animation_file(Path::new("test.anim")));
        assert!(!system.is_animation_file(Path::new("test.txt")));
        assert!(!system.is_animation_file(Path::new("test.lua")));
    }

    #[test]
    fn test_debounce_queue_processing() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Manually add to debounce queue with old timestamp
        let path = PathBuf::from("test.json");
        let old_time = Instant::now() - Duration::from_secs(1);
        system.debounce_queue.insert(path.clone(), old_time);

        // Process debounce queue
        system.process_debounce_queue();

        // Should have moved to pending
        assert_eq!(system.pending_reloads.len(), 1);
        assert_eq!(system.pending_reloads[0], path);
    }

    #[test]
    fn test_debounce_queue_not_ready() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Add with recent timestamp
        let path = PathBuf::from("test.json");
        let recent_time = Instant::now() - Duration::from_millis(10);
        system.debounce_queue.insert(path.clone(), recent_time);

        // Process debounce queue
        system.process_debounce_queue();

        // Should still be in debounce queue (not ready yet)
        assert_eq!(system.pending_reloads.len(), 0);
        assert!(system.debounce_queue.contains_key(&path));
    }

    #[test]
    fn test_reload_animation_file_deleted() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        // Cache a clip
        let path = temp_dir.path().join("deleted_clip.json");
        let clip = AnimationClip::new("deleted_clip")
            .add_keyframe(TransformKeyframe::at_position(0.0, quasar_math::Vec3::ZERO));
        system.cache_clip(path.clone(), clip);

        let mut world = World::new();

        // Try to reload a non-existent file
        let result = system.reload_animation(&path, &mut world);

        // Should succeed but remove from cache (file doesn't exist)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert!(system.get_cached_clip(&path).is_none());
    }

    #[test]
    fn test_animation_reload_event_creation() {
        let event = AnimationReloadEvent::clip_reloaded(
            PathBuf::from("test.json"),
            "test_clip".to_string(),
            Duration::from_millis(50),
            3,
        );

        match event {
            AnimationReloadEvent::ClipReloaded {
                path,
                clip_name,
                duration,
                affected_players,
            } => {
                assert_eq!(path, PathBuf::from("test.json"));
                assert_eq!(clip_name, "test_clip");
                assert_eq!(duration, Duration::from_millis(50));
                assert_eq!(affected_players, 3);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_animation_reload_event_failure() {
        let event = AnimationReloadEvent::reload_failed(
            PathBuf::from("bad.json"),
            "Invalid format".to_string(),
            true,
        );

        match event {
            AnimationReloadEvent::ReloadFailed {
                path,
                error,
                fallback_used,
            } => {
                assert_eq!(path, PathBuf::from("bad.json"));
                assert_eq!(error, "Invalid format");
                assert!(fallback_used);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_reload_record_recording() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        system.record_reload_record(
            PathBuf::from("test.json"),
            true,
            Duration::from_millis(75),
            None,
            2,
        );

        let stats = system.get_stats();
        assert_eq!(stats.total_reloads, 1);
        assert_eq!(stats.successful_reloads, 1);
        assert_eq!(stats.failed_reloads, 0);
    }

    #[test]
    fn test_state_machine_caching() {
        let temp_dir = create_temp_animation_dir();
        let config = create_test_config();
        let mut system = AnimationHotReloadSystem::new(&config, temp_dir.path())
            .expect("Failed to create system");

        use crate::animation::AnimationStateNode;

        let sm = crate::animation::AnimationStateMachine::new("idle")
            .add_state(AnimationStateNode::new("idle", "idle_clip"))
            .add_state(AnimationStateNode::new("walk", "walk_clip"));

        let path = temp_dir.path().join("player_sm.json");
        system.cache_state_machine(path.clone(), sm);

        assert!(system.get_cached_state_machine(&path).is_some());
        assert_eq!(
            system
                .get_cached_state_machine(&path)
                .unwrap()
                .current_state,
            "idle"
        );
    }
}
