//! # Asset Manager
//!
//! A centralized, handle-based asset management system for the Quasar Engine.
//!
//! ## Design
//!
//! - **Handles**: Lightweight, copyable references (`AssetHandle<T>`) that point
//! into internal storage. Handles use generational indices to detect stale
//! references after an asset is freed.
//! - **Type-erased storage**: Each concrete asset type (`T: Asset`) gets its own
//! typed slab, but `AssetManager` keeps a single map from `TypeId` to the
//! corresponding storage via `Box<dyn Any>`.
//! - **Path-based de-duplication**: Assets loaded from the same path are returned
//! from cache.
//! - **Async loading**: Assets can be loaded in background threads with
//! `LoadingState` tracking (Pending/Ready/Failed).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use quasar_core::asset::{AssetManager, Asset, AssetHandle};
//!
//! #[derive(Debug)]
//! struct MyTexture { width: u32, height: u32 }
//!
//! impl Asset for MyTexture {
//! fn asset_type_name() -> &'static str { "MyTexture" }
//! }
//!
//! let mut assets = AssetManager::new();
//! let handle: AssetHandle<MyTexture> = assets.add(MyTexture { width: 512, height: 512 });
//! assert!(assets.get(&handle).is_some());
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Loading state for async assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingState<T> {
    /// Asset is currently being loaded in background.
    Pending,
    /// Asset has finished loading successfully.
    Ready(T),
    /// Asset failed to load with the given error message.
    Failed(String),
}

impl<T> LoadingState<T> {
    pub fn is_pending(&self) -> bool {
        matches!(self, LoadingState::Pending)
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, LoadingState::Ready(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, LoadingState::Failed(_))
    }

    pub fn unwrap(self) -> T {
        match self {
            LoadingState::Ready(v) => v,
            LoadingState::Pending => panic!("called unwrap on Pending state"),
            LoadingState::Failed(e) => panic!("called unwrap on Failed state: {}", e),
        }
    }

    pub fn unwrap_or_else<E>(self, f: impl FnOnce(String) -> T) -> T {
        match self {
            LoadingState::Ready(v) => v,
            LoadingState::Pending => f("Asset still loading".into()),
            LoadingState::Failed(e) => f(e),
        }
    }
}

/// A handle to an async loading operation.
#[derive(Debug, Clone)]
pub struct AsyncHandle<T: Asset> {
    pub(crate) path: PathBuf,
    _marker: PhantomData<T>,
}

impl<T: Asset> AsyncHandle<T> {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Type alias for a thread-safe loading state container.
pub type AsyncState<T> = Arc<Mutex<LoadingState<T>>>;

// ---------------------------------------------------------------------------
// Asset trait
// ---------------------------------------------------------------------------

/// Marker trait for anything storable in the asset manager.
///
/// Implementations only need to provide a human-readable name for logging /
/// debug purposes.
pub trait Asset: 'static + Send + Sync {
    /// A short name used in log messages (e.g. `"Texture"`, `"Mesh"`).
    fn asset_type_name() -> &'static str;
}

// ---------------------------------------------------------------------------
// AssetHandle
// ---------------------------------------------------------------------------

/// A lightweight, copyable handle pointing to a loaded asset.
///
/// The handle embeds a generational index so that a stale handle can be
/// detected after `free()`.
pub struct AssetHandle<T: Asset> {
    index: u32,
    generation: u32,
    _marker: PhantomData<T>,
}

// Manual impls because PhantomData<T> would otherwise require T: Clone/Copy.

impl<T: Asset> Clone for AssetHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for AssetHandle<T> {}

impl<T: Asset> PartialEq for AssetHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T: Asset> Eq for AssetHandle<T> {}

impl<T: Asset> std::hash::Hash for AssetHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T: Asset> fmt::Debug for AssetHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AssetHandle<{}>({}:{})",
            T::asset_type_name(),
            self.index,
            self.generation
        )
    }
}

// ---------------------------------------------------------------------------
// Typed slab (per-type storage)
// ---------------------------------------------------------------------------

/// Stores assets of a single type in a flat `Vec`, supporting generational
/// indexing.
struct AssetSlab<T: Asset> {
    /// Dense storage — some slots may be `None` after freeing.
    entries: Vec<Option<T>>,
    /// Parallel generation counters.  Incremented on free.
    generations: Vec<u32>,
    /// Free-list of recycled indices.
    free_list: Vec<u32>,
    /// Reverse map: canonical path → handle index (for dedup).
    path_to_index: HashMap<PathBuf, u32>,
}

impl<T: Asset> Default for AssetSlab<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            path_to_index: HashMap::new(),
        }
    }
}

impl<T: Asset> AssetSlab<T> {
    /// Insert an asset, return its handle.
    fn insert(&mut self, asset: T) -> AssetHandle<T> {
        if let Some(idx) = self.free_list.pop() {
            let i = idx as usize;
            self.entries[i] = Some(asset);
            AssetHandle {
                index: idx,
                generation: self.generations[i],
                _marker: PhantomData,
            }
        } else {
            let idx = self.entries.len() as u32;
            self.entries.push(Some(asset));
            self.generations.push(0);
            AssetHandle {
                index: idx,
                generation: 0,
                _marker: PhantomData,
            }
        }
    }

    /// Insert an asset and associate it with a path (for dedup).
    fn insert_with_path(&mut self, asset: T, path: PathBuf) -> AssetHandle<T> {
        let handle = self.insert(asset);
        self.path_to_index.insert(path, handle.index);
        handle
    }

    /// Look up an asset by handle.  Returns `None` if freed or generation
    /// mismatch.
    fn get(&self, handle: &AssetHandle<T>) -> Option<&T> {
        let i = handle.index as usize;
        if i < self.generations.len() && self.generations[i] == handle.generation {
            self.entries[i].as_ref()
        } else {
            None
        }
    }

    /// Mutable access.
    fn get_mut(&mut self, handle: &AssetHandle<T>) -> Option<&mut T> {
        let i = handle.index as usize;
        if i < self.generations.len() && self.generations[i] == handle.generation {
            self.entries[i].as_mut()
        } else {
            None
        }
    }

    /// Free an asset.  The slot may be reused later (with bumped generation).
    fn free(&mut self, handle: &AssetHandle<T>) -> bool {
        let i = handle.index as usize;
        if i < self.generations.len() && self.generations[i] == handle.generation {
            self.entries[i] = None;
            self.generations[i] += 1;
            self.free_list.push(handle.index);
            // Remove path mapping if present.
            self.path_to_index.retain(|_, &mut idx| idx != handle.index);
            true
        } else {
            false
        }
    }

    /// Check if a path is already loaded.
    fn handle_for_path(&self, path: &Path) -> Option<AssetHandle<T>> {
        self.path_to_index.get(path).map(|&idx| {
            let gen = self.generations[idx as usize];
            AssetHandle {
                index: idx,
                generation: gen,
                _marker: PhantomData,
            }
        })
    }

    /// Number of live assets.
    fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }
}

// ---------------------------------------------------------------------------
// AssetManager
// ---------------------------------------------------------------------------

/// Centralized asset store.
///
/// Each asset type gets its own internal slab, keyed by `TypeId`. You can
/// store any type that implements [`Asset`].
pub struct AssetManager {
    slabs: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    #[allow(dead_code)]
    async_states: HashMap<TypeId, HashMap<PathBuf, Arc<Mutex<Box<dyn Any + Send>>>>>,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    /// Create an empty asset manager.
    pub fn new() -> Self {
        Self {
            slabs: HashMap::new(),
            async_states: HashMap::new(),
        }
    }

    // -- helpers --

    fn slab<T: Asset>(&self) -> Option<&AssetSlab<T>> {
        self.slabs
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<AssetSlab<T>>())
    }

    fn slab_mut<T: Asset>(&mut self) -> &mut AssetSlab<T> {
        self.slabs
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(AssetSlab::<T>::default()))
            .downcast_mut::<AssetSlab<T>>()
            .expect("type mismatch in asset slab")
    }

    // -- public API --

    /// Store an already-created asset and receive a handle.
    pub fn add<T: Asset>(&mut self, asset: T) -> AssetHandle<T> {
        log::trace!("AssetManager: adding {}", T::asset_type_name());
        self.slab_mut::<T>().insert(asset)
    }

    /// Store an asset and associate it with a filesystem path for dedup.
    pub fn add_with_path<T: Asset>(
        &mut self,
        asset: T,
        path: impl Into<PathBuf>,
    ) -> AssetHandle<T> {
        let path = path.into();
        log::trace!(
            "AssetManager: adding {} from {:?}",
            T::asset_type_name(),
            path
        );
        self.slab_mut::<T>().insert_with_path(asset, path)
    }

    /// Look up a cached handle by path. Returns `None` if the asset was never
    /// loaded from that path (or has been freed).
    pub fn handle_for_path<T: Asset>(&self, path: impl AsRef<Path>) -> Option<AssetHandle<T>> {
        self.slab::<T>()
            .and_then(|s| s.handle_for_path(path.as_ref()))
    }

    /// Borrow an asset immutably.
    pub fn get<T: Asset>(&self, handle: &AssetHandle<T>) -> Option<&T> {
        self.slab::<T>().and_then(|s| s.get(handle))
    }

    /// Borrow an asset mutably.
    pub fn get_mut<T: Asset>(&mut self, handle: &AssetHandle<T>) -> Option<&mut T> {
        self.slab_mut::<T>().get_mut(handle)
    }

    /// Free an asset, releasing the data. Subsequent `get()` calls with this
    /// handle will return `None`.
    pub fn free<T: Asset>(&mut self, handle: &AssetHandle<T>) -> bool {
        self.slab_mut::<T>().free(handle)
    }

    /// How many live assets of this type?
    pub fn count<T: Asset>(&self) -> usize {
        self.slab::<T>().map_or(0, |s| s.len())
    }

    /// Begin an async load operation. Returns an `AsyncState` that can be polled.
    ///
    /// The `loader` closure is called in a background thread and should return
    /// the loaded asset or an error string.
    pub fn load_async<T, F>(&mut self, path: impl Into<PathBuf>, loader: F) -> AsyncState<T>
    where
        T: Asset,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let path = path.into();

        // Check if already loaded
        if let Some(_handle) = self.handle_for_path::<T>(&path) {
            // Return a "ready" state - caller should check handle_for_path
            return Arc::new(Mutex::new(LoadingState::Pending));
        }

        let state: Arc<Mutex<LoadingState<T>>> = Arc::new(Mutex::new(LoadingState::Pending));
        let state_clone = state.clone();
        let path_clone = path.clone();

        std::thread::spawn(move || {
            let result = loader();
            let mut state = state_clone.lock().unwrap();
            match result {
                Ok(asset) => {
                    *state = LoadingState::Ready(asset);
                    log::trace!("Async load complete: {:?}", path_clone);
                }
                Err(e) => {
                    *state = LoadingState::Failed(e.clone());
                    log::warn!("Async load failed for {:?}: {}", path_clone, e);
                }
            }
        });

        state
    }

    /// Check if an async load has completed and insert the asset if ready.
    ///
    /// Returns `Some(handle)` if the asset was loaded and inserted,
    /// `None` if still pending or failed.
    pub fn poll_async<T: Asset>(
        &mut self,
        state: &mut LoadingState<T>,
        path: impl Into<PathBuf>,
    ) -> Option<AssetHandle<T>> {
        match state {
            LoadingState::Ready(_asset) => {
                let path = path.into();
                // Check if already inserted
                if let Some(handle) = self.handle_for_path::<T>(&path) {
                    return Some(handle);
                }
                let asset = std::mem::replace(state, LoadingState::Pending);
                match asset {
                    LoadingState::Ready(a) => {
                        let handle = self.add_with_path(a, path);
                        Some(handle)
                    }
                    _ => None,
                }
            }
            LoadingState::Pending => None,
            LoadingState::Failed(_) => None,
        }
    }

    /// Check loading state without consuming it.
    pub fn check_async<T: Asset>(state: &LoadingState<T>) -> LoadingState<()> {
        match state {
            LoadingState::Pending => LoadingState::Pending,
            LoadingState::Ready(_) => LoadingState::Ready(()),
            LoadingState::Failed(e) => LoadingState::Failed(e.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Content-addressed caching
// ---------------------------------------------------------------------------

/// A blake3-based content hash for asset deduplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Hash raw bytes.
    pub fn from_bytes(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        Self(*hash.as_bytes())
    }

    /// Zero hash (sentinel).
    pub const ZERO: Self = Self([0u8; 32]);
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0[..8] {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, "…")
    }
}

// ---------------------------------------------------------------------------
// Dependency graph
// ---------------------------------------------------------------------------

/// Tracks parent → child dependency edges between assets for hot-reload
/// propagation. When a "leaf" asset (e.g. a texture file) changes, all
/// dependents (e.g. materials referencing that texture) are also flagged dirty.
pub struct AssetDepGraph {
    /// child → set of parents that depend on it
    dependents: HashMap<PathBuf, Vec<PathBuf>>,
    /// parent → set of children it depends on
    dependencies: HashMap<PathBuf, Vec<PathBuf>>,
    /// content hash per path for change detection
    hashes: HashMap<PathBuf, ContentHash>,
}

impl Default for AssetDepGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetDepGraph {
    pub fn new() -> Self {
        Self {
            dependents: HashMap::new(),
            dependencies: HashMap::new(),
            hashes: HashMap::new(),
        }
    }

    /// Register that `parent` depends on `child`.
    /// When `child` changes, `parent` should be reloaded too.
    pub fn add_dependency(&mut self, parent: impl Into<PathBuf>, child: impl Into<PathBuf>) {
        let parent = parent.into();
        let child = child.into();
        self.dependents
            .entry(child.clone())
            .or_default()
            .push(parent.clone());
        self.dependencies
            .entry(parent)
            .or_default()
            .push(child);
    }

    /// Remove all dependencies of a parent.
    pub fn clear_dependencies(&mut self, parent: &Path) {
        if let Some(children) = self.dependencies.remove(parent) {
            for child in &children {
                if let Some(deps) = self.dependents.get_mut(child) {
                    deps.retain(|p| p != parent);
                }
            }
        }
    }

    /// Get all transitive dependents of a changed path (BFS).
    pub fn transitive_dependents(&self, changed: &Path) -> Vec<PathBuf> {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(changed.to_path_buf());
        visited.insert(changed.to_path_buf());

        let mut result = Vec::new();
        while let Some(current) = queue.pop_front() {
            if let Some(parents) = self.dependents.get(&current) {
                for parent in parents {
                    if visited.insert(parent.clone()) {
                        result.push(parent.clone());
                        queue.push_back(parent.clone());
                    }
                }
            }
        }
        result
    }

    /// Store/update the content hash for a path. Returns `true` if the hash changed.
    pub fn update_hash(&mut self, path: impl Into<PathBuf>, hash: ContentHash) -> bool {
        let path = path.into();
        let prev = self.hashes.insert(path, hash);
        prev.map_or(true, |old| old != hash)
    }

    /// Get the stored content hash.
    pub fn hash_of(&self, path: &Path) -> Option<ContentHash> {
        self.hashes.get(path).copied()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct TestTexture {
        width: u32,
        height: u32,
    }

    impl Asset for TestTexture {
        fn asset_type_name() -> &'static str {
            "TestTexture"
        }
    }

    #[derive(Debug, PartialEq)]
    struct TestMesh {
        vertex_count: usize,
    }

    impl Asset for TestMesh {
        fn asset_type_name() -> &'static str {
            "TestMesh"
        }
    }

    #[test]
    fn add_and_get() {
        let mut mgr = AssetManager::new();
        let h = mgr.add(TestTexture {
            width: 256,
            height: 256,
        });
        assert_eq!(
            mgr.get(&h),
            Some(&TestTexture {
                width: 256,
                height: 256
            })
        );
    }

    #[test]
    fn free_invalidates_handle() {
        let mut mgr = AssetManager::new();
        let h = mgr.add(TestTexture {
            width: 64,
            height: 64,
        });
        assert!(mgr.free(&h));
        assert!(mgr.get(&h).is_none());
    }

    #[test]
    fn generation_prevents_aliasing() {
        let mut mgr = AssetManager::new();
        let h1 = mgr.add(TestTexture {
            width: 1,
            height: 1,
        });
        mgr.free(&h1);
        let h2 = mgr.add(TestTexture {
            width: 2,
            height: 2,
        });
        // h1 and h2 may share the same index but different generation.
        assert!(mgr.get(&h1).is_none(), "stale handle must not resolve");
        assert_eq!(
            mgr.get(&h2),
            Some(&TestTexture {
                width: 2,
                height: 2
            })
        );
    }

    #[test]
    fn path_dedup() {
        let mut mgr = AssetManager::new();
        let h = mgr.add_with_path(
            TestTexture {
                width: 512,
                height: 512,
            },
            "assets/player.png",
        );
        let found = mgr.handle_for_path::<TestTexture>("assets/player.png");
        assert_eq!(found, Some(h));
        assert!(mgr
            .handle_for_path::<TestTexture>("assets/enemy.png")
            .is_none());
    }

    #[test]
    fn multiple_types_independent() {
        let mut mgr = AssetManager::new();
        let tex = mgr.add(TestTexture {
            width: 128,
            height: 128,
        });
        let mesh = mgr.add(TestMesh { vertex_count: 36 });
        assert_eq!(mgr.count::<TestTexture>(), 1);
        assert_eq!(mgr.count::<TestMesh>(), 1);
        mgr.free(&tex);
        assert_eq!(mgr.count::<TestTexture>(), 0);
        assert_eq!(mgr.count::<TestMesh>(), 1);
        assert!(mgr.get(&mesh).is_some());
    }

    #[test]
    fn get_mut_modifies_asset() {
        let mut mgr = AssetManager::new();
        let h = mgr.add(TestTexture {
            width: 64,
            height: 64,
        });
        if let Some(tex) = mgr.get_mut(&h) {
            tex.width = 128;
        }
        assert_eq!(
            mgr.get(&h),
            Some(&TestTexture {
                width: 128,
                height: 64
            })
        );
    }

    #[test]
    fn count_tracks_live_assets() {
        let mut mgr = AssetManager::new();
        assert_eq!(mgr.count::<TestTexture>(), 0);
        let h1 = mgr.add(TestTexture {
            width: 1,
            height: 1,
        });
        let _h2 = mgr.add(TestTexture {
            width: 2,
            height: 2,
        });
        assert_eq!(mgr.count::<TestTexture>(), 2);
        mgr.free(&h1);
        assert_eq!(mgr.count::<TestTexture>(), 1);
    }
}
