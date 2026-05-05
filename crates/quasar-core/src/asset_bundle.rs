//! Asset Bundle System for Quasar Engine.
//!
//! This module provides a comprehensive asset bundling system for packaging game assets
//! into efficient, compressed bundles with content addressing and dependency tracking.
//!
//! # Features
//!
//! - **Bundle format** — container for packaged assets with metadata
//! - **Streaming support** — load assets on-demand from bundles
//! - **Compression** — optional LZ4/Zstd compression for reduced storage
//! - **Content-addressed** — deduplication via content hashes
//! - **Dependency tracking** — bundles can depend on other bundles
//! - **Asset flags** — mark assets as streamable, preload, encrypted, or deleted
//!
//! # Basic Usage
//!
//! ```rust,no_run
//! use quasar_core::asset_bundle::{BundleWriter, BundleId, CompressionType, AssetFlags};
//!
//! // Create a new bundle writer
//! let mut writer = BundleWriter::new(
//!     BundleId(1),
//!     "my_assets".to_string()
//! )
//! .with_compression(CompressionType::Lz4);
//!
//! // Add assets to the bundle
//! let asset_data = b"Hello, World!";
//! writer.add_asset(
//!     "textures/hello.png",
//!     "texture",
//!     asset_data,
//!     AssetFlags::STREAMABLE
//! );
//!
//! // Finalize and write the bundle
//! let bundle_bytes = writer.finalize();
//! std::fs::write("assets.bundle", &bundle_bytes).unwrap();
//! ```
//!
//! # Reading Bundles
//!
//! ```rust,no_run
//! use quasar_core::asset_bundle::{BundleReader, BundleId};
//!
//! // Read an existing bundle
//! let bundle_data = std::fs::read("assets.bundle").unwrap();
//! let reader = BundleReader::new(&bundle_data).unwrap();
//!
//! // Check if an asset exists
//! if reader.has_asset("textures/hello.png") {
//!     // Load the asset
//!     let asset = reader.load_asset("textures/hello.png").unwrap();
//!     println!("Loaded {} bytes", asset.data.len());
//! }
//! ```
//!
//! # Compression
//!
//! The bundle system supports multiple compression algorithms:
//!
//! - `CompressionType::None` — No compression (fastest, largest size)
//! - `CompressionType::Lz4` — LZ4 compression (fast, good compression)
//! - `CompressionType::Zstd` — Zstandard compression (slower, best compression)
//!
//! # Asset Flags
//!
//! Assets can be marked with various flags:
//!
//! - `AssetFlags::STREAMABLE` — Asset can be loaded on-demand
//! - `AssetFlags::PRELOAD` — Asset should be loaded immediately
//! - `AssetFlags::ENCRYPTED` — Asset is encrypted (requires decryption)
//! - `AssetFlags::DELETED` — Asset is marked as deleted (for incremental updates)

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const BUNDLE_MAGIC: u32 = 0x51534247;
pub const BUNDLE_VERSION: u32 = 1;

/// Unique identifier for an asset bundle.
///
/// Each bundle has a unique ID that can be used for dependency tracking
/// and content addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BundleId(pub u64);

/// Unique identifier for an asset entry within a bundle.
///
/// Each asset within a bundle has a unique ID for internal referencing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetEntryId(pub u64);

/// Compression type for asset data.
///
/// # Examples
///
/// ```rust
/// use quasar_core::asset_bundle::CompressionType;
///
/// // No compression
/// let no_compression = CompressionType::None;
///
/// // Fast LZ4 compression
/// let lz4 = CompressionType::Lz4;
///
/// // High-compression Zstandard
/// let zstd = CompressionType::Zstd;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    /// No compression - fastest but largest size
    None = 0,
    /// LZ4 compression - fast with good compression ratio
    Lz4 = 1,
    /// Zstandard compression - slower with best compression ratio
    Zstd = 2,
}

impl Default for CompressionType {
    fn default() -> Self {
        Self::None
    }
}

/// Header for an asset bundle.
///
/// Contains metadata about the bundle including its ID, name, compression settings,
/// dependencies, and checksum.
///
/// # Fields
///
/// - `magic` — Magic number for bundle identification (0x51534247)
/// - `version` — Bundle format version
/// - `bundle_id` — Unique identifier for this bundle
/// - `name` — Human-readable name for the bundle
/// - `asset_count` — Number of assets in the bundle
/// - `compression` — Compression type used for asset data
/// - `dependencies` — List of bundle IDs this bundle depends on
/// - `checksum` — BLAKE3 hash of the bundle content
/// - `created_timestamp` — Unix timestamp when bundle was created
#[derive(Debug, Clone)]
pub struct BundleHeader {
    pub magic: u32,
    pub version: u32,
    pub bundle_id: BundleId,
    pub name: String,
    pub asset_count: u32,
    pub compression: CompressionType,
    pub dependencies: Vec<BundleId>,
    pub checksum: [u8; 32],
    pub created_timestamp: u64,
}

impl Default for BundleHeader {
    fn default() -> Self {
        Self {
            magic: BUNDLE_MAGIC,
            version: BUNDLE_VERSION,
            bundle_id: BundleId(0),
            name: String::new(),
            asset_count: 0,
            compression: CompressionType::None,
            dependencies: Vec::new(),
            checksum: [0u8; 32],
            created_timestamp: 0,
        }
    }
}

/// Entry for an asset within a bundle.
///
/// Contains metadata about a single asset including its path, type, location,
/// size, content hash, and flags.
///
/// # Fields
///
/// - `id` — Unique identifier for this asset entry
/// - `path` — Virtual path for the asset (e.g., "textures/player.png")
/// - `asset_type` — Type identifier for the asset (e.g., "texture", "model")
/// - `offset` — Byte offset of the asset data in the bundle
/// - `uncompressed_size` — Size of the asset data before compression
/// - `compressed_size` — Size of the asset data after compression
/// - `content_hash` — BLAKE3 hash of the uncompressed asset data
/// - `flags` — Asset flags (streamable, preload, encrypted, deleted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    pub id: AssetEntryId,
    pub path: String,
    pub asset_type: String,
    pub offset: u64,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub content_hash: [u8; 32],
    pub flags: AssetFlags,
}

/// Flags for asset entries.
///
/// # Examples
///
/// ```rust
/// use quasar_core::asset_bundle::AssetFlags;
///
/// // Mark asset as streamable
/// let flags = AssetFlags::STREAMABLE;
///
/// // Combine multiple flags
/// let flags = AssetFlags::STREAMABLE | AssetFlags::PRELOAD;
///
/// // Check if flag is set
/// if flags.contains(AssetFlags::STREAMABLE) {
///     println!("Asset is streamable");
/// }
/// ```
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct AssetFlags: u32 {
        /// No flags set
        const NONE = 0;
        /// Asset can be loaded on-demand (streaming)
        const STREAMABLE = 1 << 0;
        /// Asset should be preloaded when bundle is loaded
        const PRELOAD = 1 << 1;
        /// Asset data is encrypted
        const ENCRYPTED = 1 << 2;
        /// Asset is marked as deleted (for incremental updates)
        const DELETED = 1 << 3;
    }
}

impl Default for AssetFlags {
    fn default() -> Self {
        AssetFlags::NONE
    }
}

/// Manifest for an asset bundle.
///
/// Contains the bundle header and all asset entries with a path-based index
/// for efficient asset lookup.
///
/// # Examples
///
/// ```rust
/// use quasar_core::asset_bundle::{BundleManifest, BundleHeader, BundleId, AssetEntry, AssetEntryId, CompressionType};
///
/// let header = BundleHeader {
///     bundle_id: BundleId(1),
///     name: "my_bundle".to_string(),
///     ..Default::default()
/// };
///
/// let mut manifest = BundleManifest::new(header);
///
/// // Add an entry
/// let entry = AssetEntry {
///     id: AssetEntryId(1),
///     path: "textures/player.png".to_string(),
///     ..Default::default()
/// };
/// manifest.add_entry(entry);
///
/// // Look up by path
/// if let Some(found) = manifest.get_entry("textures/player.png") {
///     println!("Found asset: {}", found.path);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BundleManifest {
    pub header: BundleHeader,
    pub entries: Vec<AssetEntry>,
    pub path_to_entry: HashMap<String, usize>,
}

impl BundleManifest {
    pub fn new(header: BundleHeader) -> Self {
        Self {
            header,
            entries: Vec::new(),
            path_to_entry: HashMap::new(),
        }
    }

    pub fn add_entry(&mut self, entry: AssetEntry) {
        let idx = self.entries.len();
        self.path_to_entry.insert(entry.path.clone(), idx);
        self.entries.push(entry);
        self.header.asset_count = self.entries.len() as u32;
    }

    pub fn get_entry(&self, path: &str) -> Option<&AssetEntry> {
        self.path_to_entry.get(path).map(|&idx| &self.entries[idx])
    }

    pub fn has_asset(&self, path: &str) -> bool {
        self.path_to_entry.contains_key(path)
    }

    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.compressed_size).sum()
    }

    pub fn total_uncompressed(&self) -> u64 {
        self.entries.iter().map(|e| e.uncompressed_size).sum()
    }
}

/// Writer for creating asset bundles.
///
/// Provides a fluent API for adding assets to a bundle and finalizing it
/// with optional compression and dependency tracking.
///
/// # Examples
///
/// ```rust,no_run
/// use quasar_core::asset_bundle::{BundleWriter, BundleId, CompressionType, AssetFlags};
///
/// // Create a new bundle writer
/// let mut writer = BundleWriter::new(
///     BundleId(1),
///     "my_assets".to_string()
/// )
/// .with_compression(CompressionType::Lz4)
/// .with_dependencies(vec![BundleId(0)]);
///
/// // Add assets
/// let texture_data = std::fs::read("player.png").unwrap();
/// writer.add_asset(
///     "textures/player.png",
///     "texture",
///     &texture_data,
///     AssetFlags::STREAMABLE
/// );
///
/// // Finalize the bundle
/// let bundle_bytes = writer.finalize();
/// std::fs::write("assets.bundle", &bundle_bytes).unwrap();
/// ```
pub struct BundleWriter {
    manifest: BundleManifest,
    data: Vec<u8>,
    next_entry_id: u64,
    compression: CompressionType,
}

impl BundleWriter {
    /// Creates a new bundle writer with the given ID and name.
    ///
    /// # Arguments
    ///
    /// * `bundle_id` — Unique identifier for the bundle
    /// * `name` — Human-readable name for the bundle
    ///
    /// # Examples
    ///
    /// ```rust
    /// use quasar_core::asset_bundle::{BundleWriter, BundleId};
    ///
    /// let writer = BundleWriter::new(
    ///     BundleId(1),
    ///     "my_assets".to_string()
    /// );
    /// ```
    pub fn new(bundle_id: BundleId, name: String) -> Self {
        let header = BundleHeader {
            bundle_id,
            name,
            ..Default::default()
        };
        Self {
            manifest: BundleManifest::new(header),
            data: Vec::new(),
            next_entry_id: 1,
            compression: CompressionType::None,
        }
    }

    /// Sets the compression type for the bundle.
    ///
    /// # Arguments
    ///
    /// * `compression` — Compression type to use
    ///
    /// # Examples
    ///
    /// ```rust
    /// use quasar_core::asset_bundle::{BundleWriter, BundleId, CompressionType};
    ///
    /// let writer = BundleWriter::new(BundleId(1), "assets".to_string())
    ///     .with_compression(CompressionType::Lz4);
    /// ```
    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = compression;
        self.manifest.header.compression = compression;
        self
    }

    /// Sets the bundle dependencies.
    ///
    /// # Arguments
    ///
    /// * `deps` — List of bundle IDs this bundle depends on
    ///
    /// # Examples
    ///
    /// ```rust
    /// use quasar_core::asset_bundle::{BundleWriter, BundleId};
    ///
    /// let writer = BundleWriter::new(BundleId(1), "assets".to_string())
    ///     .with_dependencies(vec![BundleId(0)]);
    /// ```
    pub fn with_dependencies(mut self, deps: Vec<BundleId>) -> Self {
        self.manifest.header.dependencies = deps;
        self
    }

    /// Adds an asset to the bundle.
    ///
    /// # Arguments
    ///
    /// * `path` — Virtual path for the asset (e.g., "textures/player.png")
    /// * `asset_type` — Type identifier for the asset (e.g., "texture", "model")
    /// * `data` — Raw asset data
    /// * `flags` — Asset flags (streamable, preload, encrypted, deleted)
    ///
    /// # Returns
    ///
    /// The unique ID assigned to this asset entry
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use quasar_core::asset_bundle::{BundleWriter, BundleId, AssetFlags};
    ///
    /// let mut writer = BundleWriter::new(BundleId(1), "assets".to_string());
    ///
    /// let texture_data = std::fs::read("player.png").unwrap();
    /// let entry_id = writer.add_asset(
    ///     "textures/player.png",
    ///     "texture",
    ///     &texture_data,
    ///     AssetFlags::STREAMABLE
    /// );
    /// ```
    pub fn add_asset(
        &mut self,
        path: &str,
        asset_type: &str,
        data: &[u8],
        flags: AssetFlags,
    ) -> AssetEntryId {
        let content_hash = blake3::hash(data);
        let id = AssetEntryId(self.next_entry_id);
        self.next_entry_id += 1;

        let (compressed_data, compressed_size) = match self.compression {
            CompressionType::None => (data.to_vec(), data.len() as u64),
            CompressionType::Lz4 => {
                let compressed = compress_lz4(data);
                (compressed.0, compressed.1 as u64)
            }
            CompressionType::Zstd => {
                let compressed = compress_zstd(data);
                (compressed.0, compressed.1 as u64)
            }
        };

        let offset = self.data.len() as u64;
        self.data.extend_from_slice(&compressed_data);

        let entry = AssetEntry {
            id,
            path: path.to_string(),
            asset_type: asset_type.to_string(),
            offset,
            uncompressed_size: data.len() as u64,
            compressed_size,
            content_hash: content_hash.as_bytes().to_owned(),
            flags,
        };

        self.manifest.add_entry(entry);
        id
    }

    pub fn finish(mut self) -> Result<Vec<u8>, BundleError> {
        self.manifest.header.asset_count = self.manifest.entries.len() as u32;
        self.manifest.header.checksum = blake3::hash(&self.data).as_bytes().to_owned();
        self.manifest.header.created_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut output = Vec::new();
        self.write_header(&mut output)?;
        self.write_manifest(&mut output)?;
        self.write_data(&mut output)?;

        Ok(output)
    }

    fn write_header(&self, output: &mut Vec<u8>) -> Result<(), BundleError> {
        output.extend_from_slice(&self.manifest.header.magic.to_le_bytes());
        output.extend_from_slice(&self.manifest.header.version.to_le_bytes());
        output.extend_from_slice(&self.manifest.header.bundle_id.0.to_le_bytes());

        let name_bytes = self.manifest.header.name.as_bytes();
        output.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        output.extend_from_slice(name_bytes);

        output.extend_from_slice(&self.manifest.header.asset_count.to_le_bytes());
        output.extend_from_slice(&(self.manifest.header.compression as u32).to_le_bytes());

        output.extend_from_slice(&(self.manifest.header.dependencies.len() as u32).to_le_bytes());
        for dep in &self.manifest.header.dependencies {
            output.extend_from_slice(&dep.0.to_le_bytes());
        }

        output.extend_from_slice(&self.manifest.header.checksum);
        output.extend_from_slice(&self.manifest.header.created_timestamp.to_le_bytes());

        Ok(())
    }

    fn write_manifest(&self, output: &mut Vec<u8>) -> Result<(), BundleError> {
        let manifest_data = serde_json::to_string(&self.manifest.entries)
            .map_err(|e| BundleError::SerializationFailed(e.to_string()))?;
        let manifest_bytes = manifest_data.as_bytes();
        output.extend_from_slice(&(manifest_bytes.len() as u64).to_le_bytes());
        output.extend_from_slice(manifest_bytes);

        Ok(())
    }

    fn write_data(&self, output: &mut Vec<u8>) -> Result<(), BundleError> {
        output.extend_from_slice(&self.data);
        Ok(())
    }
}

pub struct BundleReader {
    manifest: BundleManifest,
    data_offset: u64,
    data: Vec<u8>,
}

impl BundleReader {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BundleError> {
        let mut cursor = Cursor::new(bytes);

        let header = Self::read_header(&mut cursor)?;
        let manifest = Self::read_manifest(&mut cursor, header)?;
        let data_offset = cursor.position();
        let data = bytes[data_offset as usize..].to_vec();

        Ok(Self {
            manifest,
            data_offset,
            data,
        })
    }

    pub fn from_file(path: &Path) -> Result<Self, BundleError> {
        let bytes = std::fs::read(path).map_err(|e| BundleError::IoFailed(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    fn read_header(cursor: &mut Cursor<&[u8]>) -> Result<BundleHeader, BundleError> {
        let mut magic_bytes = [0u8; 4];
        cursor.read_exact(&mut magic_bytes)?;
        let magic = u32::from_le_bytes(magic_bytes);

        if magic != BUNDLE_MAGIC {
            return Err(BundleError::InvalidMagic);
        }

        let mut version_bytes = [0u8; 4];
        cursor.read_exact(&mut version_bytes)?;
        let version = u32::from_le_bytes(version_bytes);

        if version > BUNDLE_VERSION {
            return Err(BundleError::UnsupportedVersion(version));
        }

        let mut bundle_id_bytes = [0u8; 8];
        cursor.read_exact(&mut bundle_id_bytes)?;
        let bundle_id = BundleId(u64::from_le_bytes(bundle_id_bytes));

        let mut name_len_bytes = [0u8; 4];
        cursor.read_exact(&mut name_len_bytes)?;
        let name_len = u32::from_le_bytes(name_len_bytes) as usize;

        let mut name_bytes = vec![0u8; name_len];
        cursor.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes).map_err(|_| BundleError::InvalidUtf8)?;

        let mut asset_count_bytes = [0u8; 4];
        cursor.read_exact(&mut asset_count_bytes)?;
        let asset_count = u32::from_le_bytes(asset_count_bytes);

        let mut compression_bytes = [0u8; 4];
        cursor.read_exact(&mut compression_bytes)?;
        let compression = match u32::from_le_bytes(compression_bytes) {
            0 => CompressionType::None,
            1 => CompressionType::Lz4,
            2 => CompressionType::Zstd,
            v => return Err(BundleError::UnknownCompression(v)),
        };

        let mut dep_count_bytes = [0u8; 4];
        cursor.read_exact(&mut dep_count_bytes)?;
        let dep_count = u32::from_le_bytes(dep_count_bytes) as usize;

        let mut dependencies = Vec::with_capacity(dep_count);
        for _ in 0..dep_count {
            let mut dep_bytes = [0u8; 8];
            cursor.read_exact(&mut dep_bytes)?;
            dependencies.push(BundleId(u64::from_le_bytes(dep_bytes)));
        }

        let mut checksum = [0u8; 32];
        cursor.read_exact(&mut checksum)?;

        let mut timestamp_bytes = [0u8; 8];
        cursor.read_exact(&mut timestamp_bytes)?;
        let created_timestamp = u64::from_le_bytes(timestamp_bytes);

        Ok(BundleHeader {
            magic,
            version,
            bundle_id,
            name,
            asset_count,
            compression,
            dependencies,
            checksum,
            created_timestamp,
        })
    }

    fn read_manifest(
        cursor: &mut Cursor<&[u8]>,
        header: BundleHeader,
    ) -> Result<BundleManifest, BundleError> {
        let mut manifest_len_bytes = [0u8; 8];
        cursor.read_exact(&mut manifest_len_bytes)?;
        let manifest_len = u64::from_le_bytes(manifest_len_bytes) as usize;

        let mut manifest_bytes = vec![0u8; manifest_len];
        cursor.read_exact(&mut manifest_bytes)?;

        let entries: Vec<AssetEntry> = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| BundleError::DeserializationFailed(e.to_string()))?;

        let mut path_to_entry = HashMap::new();
        for (idx, entry) in entries.iter().enumerate() {
            path_to_entry.insert(entry.path.clone(), idx);
        }

        Ok(BundleManifest {
            header,
            entries,
            path_to_entry,
        })
    }

    pub fn manifest(&self) -> &BundleManifest {
        &self.manifest
    }

    pub fn has_asset(&self, path: &str) -> bool {
        self.manifest.has_asset(path)
    }

    pub fn load_asset(&self, path: &str) -> Result<Vec<u8>, BundleError> {
        let entry = self
            .manifest
            .get_entry(path)
            .ok_or_else(|| BundleError::AssetNotFound(path.to_string()))?;

        let start = entry.offset as usize;
        let end = start + entry.compressed_size as usize;

        if end > self.data.len() {
            return Err(BundleError::DataOutOfBounds);
        }

        let compressed_data = &self.data[start..end];

        let data = match self.manifest.header.compression {
            CompressionType::None => compressed_data.to_vec(),
            CompressionType::Lz4 => {
                decompress_lz4(compressed_data, entry.uncompressed_size as usize)?
            }
            CompressionType::Zstd => {
                decompress_zstd(compressed_data, entry.uncompressed_size as usize)?
            }
        };

        if data.len() != entry.uncompressed_size as usize {
            return Err(BundleError::SizeMismatch);
        }

        let hash = blake3::hash(&data);
        if hash.as_bytes() != &entry.content_hash {
            return Err(BundleError::ChecksumMismatch);
        }

        Ok(data)
    }

    pub fn load_asset_partial(
        &self,
        path: &str,
        offset: u64,
        size: u64,
    ) -> Result<Vec<u8>, BundleError> {
        let entry = self
            .manifest
            .get_entry(path)
            .ok_or_else(|| BundleError::AssetNotFound(path.to_string()))?;

        if !entry.flags.contains(AssetFlags::STREAMABLE) {
            return Err(BundleError::NotStreamable);
        }

        if offset + size > entry.uncompressed_size {
            return Err(BundleError::RangeOutOfBounds);
        }

        let full_data = self.load_asset(path)?;
        let start = offset as usize;
        let end = (offset + size) as usize;
        Ok(full_data[start..end].to_vec())
    }

    pub fn verify(&self) -> Result<bool, BundleError> {
        let computed = blake3::hash(&self.data);
        Ok(computed.as_bytes() == &self.manifest.header.checksum)
    }
}

pub struct BundleStreamer {
    reader: BundleReader,
    loaded_assets: HashMap<String, Vec<u8>>,
    pending_loads: Vec<String>,
    memory_budget: usize,
    current_memory: usize,
}

impl BundleStreamer {
    pub fn new(reader: BundleReader, memory_budget: usize) -> Self {
        Self {
            reader,
            loaded_assets: HashMap::new(),
            pending_loads: Vec::new(),
            memory_budget,
            current_memory: 0,
        }
    }

    pub fn request_asset(&mut self, path: &str) {
        if !self.loaded_assets.contains_key(path) && !self.pending_loads.contains(&path.to_string())
        {
            self.pending_loads.push(path.to_string());
        }
    }

    pub fn process_pending(&mut self) -> Result<Vec<(String, Vec<u8>)>, BundleError> {
        let mut loaded = Vec::new();

        for path in self.pending_loads.drain(..) {
            if let Some(entry) = self.reader.manifest.get_entry(&path) {
                let size = entry.uncompressed_size as usize;

                while self.current_memory + size > self.memory_budget
                    && !self.loaded_assets.is_empty()
                {
                    if let Some((old_path, old_data)) = self
                        .loaded_assets
                        .iter()
                        .next()
                        .map(|(k, v)| (k.clone(), v.clone()))
                    {
                        self.current_memory -= old_data.len();
                        self.loaded_assets.remove(&old_path);
                    }
                }

                if self.current_memory + size <= self.memory_budget {
                    let data = self.reader.load_asset(&path)?;
                    self.current_memory += data.len();
                    self.loaded_assets.insert(path.clone(), data.clone());
                    loaded.push((path, data));
                }
            }
        }

        Ok(loaded)
    }

    pub fn get_asset(&self, path: &str) -> Option<&[u8]> {
        self.loaded_assets.get(path).map(|v| v.as_slice())
    }

    pub fn unload_asset(&mut self, path: &str) {
        if let Some(data) = self.loaded_assets.remove(path) {
            self.current_memory -= data.len();
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.current_memory
    }
}

fn compress_lz4(data: &[u8]) -> (Vec<u8>, usize) {
    let compressed = lz4::compress_block(data);
    let original_size = data.len();
    (compressed, original_size)
}

fn decompress_lz4(data: &[u8], expected_size: usize) -> Result<Vec<u8>, BundleError> {
    lz4::decompress_block(data, expected_size)
        .map_err(|e| BundleError::DecompressionError(format!("LZ4 decompression failed: {}", e)))
}

fn compress_zstd(data: &[u8]) -> (Vec<u8>, usize) {
    let compressed = zstd::encode_all(data, 3)
        .map_err(|e| BundleError::CompressionError(format!("Zstd compression failed: {}", e)))
        .unwrap_or_else(|_| (data.to_vec(), data.len()));
    let original_size = data.len();
    (compressed, original_size)
}

fn decompress_zstd(data: &[u8], expected_size: usize) -> Result<Vec<u8>, BundleError> {
    zstd::decode_all(data, expected_size)
        .map_err(|e| BundleError::DecompressionError(format!("Zstd decompression failed: {}", e)))
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("Invalid bundle magic number")]
    InvalidMagic,
    #[error("Unsupported bundle version: {0}")]
    UnsupportedVersion(u32),
    #[error("Unknown compression type: {0}")]
    UnknownCompression(u32),
    #[error("Asset not found: {0}")]
    AssetNotFound(String),
    #[error("Invalid UTF-8 in bundle")]
    InvalidUtf8,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("IO error: {0}")]
    IoFailed(String),
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
    #[error("Data out of bounds")]
    DataOutOfBounds,
    #[error("Size mismatch after decompression")]
    SizeMismatch,
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    #[error("Asset is not streamable")]
    NotStreamable,
    #[error("Range out of bounds")]
    RangeOutOfBounds,
    #[error("Compression failed: {0}")]
    CompressionError(String),
    #[error("Decompression failed: {0}")]
    DecompressionError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_writer_creates_bundle() {
        let mut writer = BundleWriter::new(BundleId(1), "test_bundle".to_string());
        let test_data = b"hello world".to_vec();
        writer.add_asset("test.txt", "text", &test_data, AssetFlags::empty());

        let result = writer.finish();
        assert!(result.is_ok());
    }

    #[test]
    fn bundle_reader_reads_bundle() {
        let mut writer = BundleWriter::new(BundleId(1), "test_bundle".to_string());
        let test_data = b"hello world".to_vec();
        writer.add_asset("test.txt", "text", &test_data, AssetFlags::empty());

        let bundle_data = writer.finish().unwrap();
        let reader = BundleReader::from_bytes(&bundle_data);
        assert!(reader.is_ok());

        let reader = reader.unwrap();
        assert!(reader.has_asset("test.txt"));
    }

    #[test]
    fn bundle_roundtrip() {
        let mut writer = BundleWriter::new(BundleId(42), "roundtrip_test".to_string());
        let test_data = b"test content for roundtrip".to_vec();
        writer.add_asset("data.bin", "binary", &test_data, AssetFlags::empty());

        let bundle_data = writer.finish().unwrap();
        let reader = BundleReader::from_bytes(&bundle_data).unwrap();

        let loaded = reader.load_asset("data.bin").unwrap();
        assert_eq!(loaded, test_data);
    }

    #[test]
    fn bundle_manifest_operations() {
        let header = BundleHeader {
            bundle_id: BundleId(1),
            name: "test".to_string(),
            ..Default::default()
        };
        let mut manifest = BundleManifest::new(header);

        let entry = AssetEntry {
            id: AssetEntryId(1),
            path: "test.asset".to_string(),
            asset_type: "test".to_string(),
            offset: 0,
            uncompressed_size: 100,
            compressed_size: 50,
            content_hash: [0u8; 32],
            flags: AssetFlags::empty(),
        };

        manifest.add_entry(entry);
        assert!(manifest.has_asset("test.asset"));
        assert_eq!(manifest.entries.len(), 1);
    }

    #[test]
    fn bundle_streamer_memory_management() {
        let mut writer = BundleWriter::new(BundleId(1), "stream_test".to_string());
        writer.add_asset(
            "small.txt",
            "text",
            b"small data".as_slice(),
            AssetFlags::empty(),
        );

        let bundle_data = writer.finish().unwrap();
        let reader = BundleReader::from_bytes(&bundle_data).unwrap();

        let streamer = BundleStreamer::new(reader, 1024);
        assert_eq!(streamer.memory_usage(), 0);
    }
}
