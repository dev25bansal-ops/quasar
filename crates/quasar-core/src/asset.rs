//! # Asset Manager
//!
//! A centralized, handle-based asset management system for the Quasar Engine.
//!
//! ## Design
//!
//! - **Handles**: Lightweight, copyable references (`AssetHandle<T>`) that point
//!   into internal storage. Handles use generational indices to detect stale
//!   references after an asset is freed.
//! - **Type-erased storage**: Each concrete asset type (`T: Asset`) gets its own
//!   typed slab, but `AssetManager` keeps a single map from `TypeId` to the
//!   corresponding storage via `Box<dyn Any>`.
//! - **Path-based de-duplication**: Assets loaded from the same path are returned
//!   from cache.
//! - **Eager loading**: Assets are loaded synchronously on `load()`. Future work
//!   can extend this to async / streaming loads.
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
//!     fn asset_type_name() -> &'static str { "MyTexture" }
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
/// Each asset type gets its own internal slab, keyed by `TypeId`.  You can
/// store any type that implements [`Asset`].
pub struct AssetManager {
    slabs: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
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

    /// Look up a cached handle by path.  Returns `None` if the asset was never
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

    /// Free an asset, releasing the data.  Subsequent `get()` calls with this
    /// handle will return `None`.
    pub fn free<T: Asset>(&mut self, handle: &AssetHandle<T>) -> bool {
        self.slab_mut::<T>().free(handle)
    }

    /// How many live assets of this type?
    pub fn count<T: Asset>(&self) -> usize {
        self.slab::<T>().map_or(0, |s| s.len())
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
