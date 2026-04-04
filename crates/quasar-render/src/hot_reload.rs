//! Hot-reload system — rebuilds GPU resources when asset files change.
//!
//! Consumes `AssetReloadedEvent`s produced by the asset server's `poll_events`
//! and recompiles the affected shader pipelines / re-uploads textures.

use std::collections::HashMap;
use std::path::PathBuf;

use quasar_core::asset_server::{AssetReloadedEvent, ReloadKind};

use crate::pipeline_cache::PipelineCache;
use crate::texture::Texture;

/// Tracks which shader files map to which pipeline cache entries and
/// provides a single `process_reload` method to drive hot-reload each frame.
pub struct HotReloadSystem {
    /// Maps canonical asset-relative paths → the last source string we compiled.
    shader_sources: HashMap<PathBuf, String>,
    /// Counter of successful hot-reloads (useful for diagnostics).
    pub reload_count: u64,
}

impl Default for HotReloadSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl HotReloadSystem {
    pub fn new() -> Self {
        Self {
            shader_sources: HashMap::new(),
            reload_count: 0,
        }
    }

    /// Register a shader path so the hot-reload system knows it exists.
    pub fn track_shader(&mut self, path: PathBuf, source: String) {
        self.shader_sources.insert(path, source);
    }

    /// Process a single reload event. Returns `true` if a GPU resource was
    /// actually rebuilt.
    pub fn process_reload(
        &mut self,
        event: &AssetReloadedEvent,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline_cache: &mut PipelineCache,
        textures: &mut [Texture],
        texture_paths: &HashMap<PathBuf, usize>,
    ) -> bool {
        match event.kind {
            ReloadKind::Shader => self.reload_shader(event, device, pipeline_cache),
            ReloadKind::Texture => {
                self.reload_texture(event, device, queue, textures, texture_paths)
            }
            ReloadKind::Hdr
            | ReloadKind::Other
            | ReloadKind::Lua
            | ReloadKind::Scene
            | ReloadKind::Prefab
            | ReloadKind::Audio => false,
        }
    }

    /// Re-read a `.wgsl` file from disk and rebuild affected pipelines.
    fn reload_shader(
        &mut self,
        event: &AssetReloadedEvent,
        _device: &wgpu::Device,
        pipeline_cache: &mut PipelineCache,
    ) -> bool {
        let path = &event.path;

        let new_source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Hot-reload: failed to read shader {:?}: {}", path, e);
                return false;
            }
        };

        // Avoid rebuilding if the source hasn't actually changed.
        if let Some(old) = self.shader_sources.get(path) {
            if *old == new_source {
                return false;
            }
        }

        // Invalidate the old pipeline; it will be recreated next frame by
        // the renderer when it calls `get_or_create`.
        let invalidated = pipeline_cache.invalidate(path);

        self.shader_sources.insert(path.clone(), new_source);

        if invalidated {
            self.reload_count += 1;
            log::info!("Hot-reload: invalidated pipeline for {:?}", path);
        }

        invalidated
    }

    /// Re-read a texture image from disk and re-upload to the GPU.
    fn reload_texture(
        &mut self,
        event: &AssetReloadedEvent,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &mut [Texture],
        texture_paths: &HashMap<PathBuf, usize>,
    ) -> bool {
        let path = &event.path;

        let Some(&index) = texture_paths.get(path) else {
            return false;
        };

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Hot-reload: failed to read texture {:?}: {}", path, e);
                return false;
            }
        };

        match Texture::from_bytes(device, queue, &bytes, path.to_str().unwrap_or("texture")) {
            Ok(new_tex) => {
                if let Some(slot) = textures.get_mut(index) {
                    *slot = new_tex;
                    self.reload_count += 1;
                    log::info!("Hot-reload: re-uploaded texture {:?}", path);
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                log::warn!("Hot-reload: failed to decode texture {:?}: {:?}", path, e);
                false
            }
        }
    }
}

/// Scene hot-reload handler.
pub struct SceneHotReloader {
    scene_paths: HashMap<PathBuf, String>,
    prefab_paths: HashMap<PathBuf, String>,
    reload_count: u64,
}

impl SceneHotReloader {
    pub fn new() -> Self {
        Self {
            scene_paths: HashMap::new(),
            prefab_paths: HashMap::new(),
            reload_count: 0,
        }
    }

    pub fn track_scene(&mut self, path: PathBuf, scene_name: String) {
        self.scene_paths.insert(path, scene_name);
    }

    pub fn track_prefab(&mut self, path: PathBuf, prefab_name: String) {
        self.prefab_paths.insert(path, prefab_name);
    }

    pub fn process_reload(&mut self, event: &AssetReloadedEvent) -> Option<ReloadAction> {
        match event.kind {
            ReloadKind::Scene => self.reload_scene(event),
            ReloadKind::Prefab => self.reload_prefab(event),
            _ => None,
        }
    }

    fn reload_scene(&mut self, event: &AssetReloadedEvent) -> Option<ReloadAction> {
        let path = &event.path;
        let scene_name = self.scene_paths.get(path)?.clone();

        let json = std::fs::read_to_string(path).ok()?;
        let new_hash = blake3::hash(json.as_bytes());

        self.reload_count += 1;
        log::info!(
            "Hot-reload: scene '{}' reloaded from {:?}",
            scene_name,
            path
        );

        Some(ReloadAction::ReloadScene {
            name: scene_name,
            json,
            hash: new_hash.to_hex().to_string(),
        })
    }

    fn reload_prefab(&mut self, event: &AssetReloadedEvent) -> Option<ReloadAction> {
        let path = &event.path;
        let prefab_name = self.prefab_paths.get(path)?.clone();

        let json = std::fs::read_to_string(path).ok()?;
        let new_hash = blake3::hash(json.as_bytes());

        self.reload_count += 1;
        log::info!(
            "Hot-reload: prefab '{}' reloaded from {:?}",
            prefab_name,
            path
        );

        Some(ReloadAction::ReloadPrefab {
            name: prefab_name,
            json,
            hash: new_hash.to_hex().to_string(),
        })
    }

    pub fn reload_count(&self) -> u64 {
        self.reload_count
    }
}

impl Default for SceneHotReloader {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum ReloadAction {
    ReloadScene {
        name: String,
        json: String,
        hash: String,
    },
    ReloadPrefab {
        name: String,
        json: String,
        hash: String,
    },
    ReloadShader {
        path: PathBuf,
        source: String,
    },
    ReloadTexture {
        path: PathBuf,
    },
}

pub struct HotReloadManager {
    shader_reloader: HotReloadSystem,
    scene_reloader: SceneHotReloader,
    pending_actions: Vec<ReloadAction>,
}

impl HotReloadManager {
    pub fn new() -> Self {
        Self {
            shader_reloader: HotReloadSystem::new(),
            scene_reloader: SceneHotReloader::new(),
            pending_actions: Vec::new(),
        }
    }

    pub fn track_shader(&mut self, path: PathBuf, source: String) {
        self.shader_reloader.track_shader(path, source);
    }

    pub fn track_scene(&mut self, path: PathBuf, name: String) {
        self.scene_reloader.track_scene(path, name);
    }

    pub fn track_prefab(&mut self, path: PathBuf, name: String) {
        self.scene_reloader.track_prefab(path, name);
    }

    pub fn process_event(&mut self, event: &AssetReloadedEvent) {
        if let Some(action) = self.scene_reloader.process_reload(event) {
            self.pending_actions.push(action);
        }
    }

    pub fn drain_actions(&mut self) -> Vec<ReloadAction> {
        std::mem::take(&mut self.pending_actions)
    }

    pub fn total_reloads(&self) -> u64 {
        self.shader_reloader.reload_count + self.scene_reloader.reload_count()
    }
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new()
    }
}
