//! Save / Load game state.
//!
//! Provides a serializable `GameSave` snapshot that captures entity
//! transforms and names from the ECS world plus any user-provided
//! key-value metadata.  The snapshot can be written to disk as JSON
//! or as compressed binary for large saves.
//!
//! # Formats
//!
//! - **JSON**: Human-readable, good for debugging, slow for large saves
//! - **Binary**: Fast, compact, compressed with gzip, versioned for compatibility
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::save_load::*;
//!
//! // Capture a save
//! let save = capture_game_save(&world);
//! save.meta.slot_name = "Save Slot 1".to_string();
//!
//! // Save as JSON (debugging)
//! save.save_to_json_file("save.json")?;
//!
//! // Save as binary (production)
//! save.save_to_binary_file("save.qsave")?;
//!
//! // Load binary
//! let loaded = GameSave::load_from_binary_file("save.qsave")?;
//! ```

use crate::ecs::{Entity, World};
use crate::scene::SceneGraph;
use crate::scene_serde::{EntityData, SceneData};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use quasar_math::Transform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

pub const SAVE_MAGIC: &[u8; 4] = b"QSAV";
pub const SAVE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Per-entity snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEntity {
    /// Entity index at save time (used to rebuild references).
    pub index: u32,
    /// Optional human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Transform at save time.
    pub transform: Transform,
    /// Children indices in the `GameSave::entities` vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<usize>,
    /// Arbitrary per-entity data that game code can populate.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

/// Metadata attached to a save file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMeta {
    /// Descriptive name for the save.
    pub slot_name: String,
    /// Timestamp (ISO-8601 or free-form string).
    #[serde(default)]
    pub timestamp: String,
    /// Arbitrary key-value pairs (playtime, chapter, etc.).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

/// Top-level game save structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSave {
    pub meta: SaveMeta,
    pub entities: Vec<SavedEntity>,
}

/// Error type for save/load operations.
#[derive(Debug)]
pub enum SaveLoadError {
    Io(std::io::Error),
    Serialization(String),
    Deserialization(String),
    InvalidMagic([u8; 4]),
    UnsupportedVersion(u32),
    Compression(String),
    Decompression(String),
}

impl std::fmt::Display for SaveLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
            Self::Deserialization(e) => write!(f, "Deserialization error: {}", e),
            Self::InvalidMagic(m) => write!(f, "Invalid magic bytes: {:?}", m),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported save version: {}", v),
            Self::Compression(e) => write!(f, "Compression error: {}", e),
            Self::Decompression(e) => write!(f, "Decompression error: {}", e),
        }
    }
}

impl std::error::Error for SaveLoadError {}

impl From<std::io::Error> for SaveLoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SaveLoadError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<bincode::error::EncodeError> for SaveLoadError {
    fn from(e: bincode::error::EncodeError) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<bincode::error::DecodeError> for SaveLoadError {
    fn from(e: bincode::error::DecodeError) -> Self {
        Self::Deserialization(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Binary save format
// ---------------------------------------------------------------------------

/// Binary save file header.
#[derive(Debug, Clone, Copy)]
struct BinaryHeader {
    magic: [u8; 4],
    version: u32,
    uncompressed_size: u64,
    compressed_size: u64,
    checksum: u32,
}

impl BinaryHeader {
    const SIZE: usize = 4 + 4 + 8 + 8 + 4;

    fn new(uncompressed_size: u64, compressed_size: u64, checksum: u32) -> Self {
        Self {
            magic: *SAVE_MAGIC,
            version: SAVE_VERSION,
            uncompressed_size,
            compressed_size,
            checksum,
        }
    }

    fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.version.to_le_bytes());
        buf[8..16].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        buf[16..24].copy_from_slice(&self.compressed_size.to_le_bytes());
        buf[24..28].copy_from_slice(&self.checksum.to_le_bytes());
        buf
    }

    fn from_bytes(buf: &[u8; Self::SIZE]) -> Self {
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        let version = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        // buf[8..16] and buf[16..24] are guaranteed 8 bytes each (SIZE=28, fixed array).
        let uncompressed_size = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let compressed_size = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let checksum = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
        Self {
            magic,
            version,
            uncompressed_size,
            compressed_size,
            checksum,
        }
    }
}

impl GameSave {
    // ------------------------------------------------------------------
    // JSON Serialization
    // ------------------------------------------------------------------

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Write to a JSON file.
    pub fn save_to_json_file(&self, path: impl AsRef<Path>) -> Result<(), SaveLoadError> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Read from a JSON file.
    pub fn load_from_json_file(path: impl AsRef<Path>) -> Result<Self, SaveLoadError> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(SaveLoadError::from)
    }

    // ------------------------------------------------------------------
    // Binary Serialization (Compressed)
    // ------------------------------------------------------------------

    /// Serialize to compressed binary format.
    ///
    /// The format is:
    /// - 4 bytes: magic "QSAV"
    /// - 4 bytes: version (little-endian u32)
    /// - 8 bytes: uncompressed size (little-endian u64)
    /// - 8 bytes: compressed size (little-endian u64)
    /// - 4 bytes: CRC32 checksum of uncompressed data
    /// - N bytes: gzip-compressed bincode payload
    pub fn to_binary(&self) -> Result<Vec<u8>, SaveLoadError> {
        let json = self.to_json()?;
        let uncompressed = json.into_bytes();

        let checksum = crc32(&uncompressed);

        let mut compressed_buf = Vec::new();
        {
            let mut encoder = GzEncoder::new(&mut compressed_buf, Compression::default());
            encoder.write_all(&uncompressed)?;
            encoder.finish()?;
        }

        let header = BinaryHeader::new(
            uncompressed.len() as u64,
            compressed_buf.len() as u64,
            checksum,
        );

        let mut output = Vec::with_capacity(BinaryHeader::SIZE + compressed_buf.len());
        output.extend_from_slice(&header.to_bytes());
        output.extend_from_slice(&compressed_buf);

        Ok(output)
    }

    pub fn from_binary(data: &[u8]) -> Result<Self, SaveLoadError> {
        if data.len() < BinaryHeader::SIZE {
            return Err(SaveLoadError::Deserialization("Data too short".into()));
        }

        let header_bytes: [u8; BinaryHeader::SIZE] = data[..BinaryHeader::SIZE]
            .try_into()
            .map_err(|_| {
                SaveLoadError::Deserialization("Failed to parse binary header".into())
            })?;
        let header = BinaryHeader::from_bytes(&header_bytes);

        if header.magic != *SAVE_MAGIC {
            return Err(SaveLoadError::InvalidMagic(header.magic));
        }

        if header.version != SAVE_VERSION {
            return Err(SaveLoadError::UnsupportedVersion(header.version));
        }

        let compressed_data = &data[BinaryHeader::SIZE..];

        let mut uncompressed = Vec::with_capacity(header.uncompressed_size as usize);
        {
            let mut decoder = GzDecoder::new(compressed_data);
            decoder
                .read_to_end(&mut uncompressed)
                .map_err(|e| SaveLoadError::Decompression(e.to_string()))?;
        }

        let computed_checksum = crc32(&uncompressed);
        if computed_checksum != header.checksum {
            return Err(SaveLoadError::Deserialization(
                "Checksum mismatch - save file may be corrupted".into(),
            ));
        }

        let json = String::from_utf8(uncompressed)
            .map_err(|e| SaveLoadError::Deserialization(e.to_string()))?;
        Self::from_json(&json).map_err(SaveLoadError::from)
    }

    /// Write to a binary file.
    pub fn save_to_binary_file(&self, path: impl AsRef<Path>) -> Result<(), SaveLoadError> {
        let data = self.to_binary()?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Read from a binary file.
    pub fn load_from_binary_file(path: impl AsRef<Path>) -> Result<Self, SaveLoadError> {
        let data = std::fs::read(path)?;
        Self::from_binary(&data)
    }

    /// Auto-detect format and load from file.
    ///
    /// Detects JSON vs binary by file extension and magic bytes.
    pub fn load_auto(path: impl AsRef<Path>) -> Result<Self, SaveLoadError> {
        let data = std::fs::read(&path)?;
        let path = path.as_ref();

        if data.len() >= 4 && &data[0..4] == SAVE_MAGIC {
            Self::from_binary(&data)
        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
            let json = String::from_utf8_lossy(&data);
            Self::from_json(&json).map_err(SaveLoadError::from)
        } else {
            Self::from_binary(&data)
        }
    }

    /// Convenience: convert into `SceneData` for the existing scene pipeline.
    pub fn to_scene_data(&self) -> SceneData {
        let mut sd = SceneData::new(&self.meta.slot_name);
        for se in &self.entities {
            sd.entities.push(EntityData {
                name: se.name.clone(),
                transform: se.transform,
                mesh_shape: None,
                children: se.children.clone(),
            });
        }
        sd
    }

    // ------------------------------------------------------------------
    // Statistics
    // ------------------------------------------------------------------

    /// Get estimated memory size of the save.
    pub fn estimated_size(&self) -> usize {
        let entity_size: usize = self
            .entities
            .iter()
            .map(|e| {
                std::mem::size_of::<SavedEntity>()
                    + e.name.as_ref().map(|n| n.len()).unwrap_or(0)
                    + e.children.len() * std::mem::size_of::<usize>()
                    + e.custom_data
                        .values()
                        .map(|v| v.to_string().len())
                        .sum::<usize>()
            })
            .sum();

        std::mem::size_of::<Self>() + entity_size
    }

    /// Get compression ratio when saved as binary.
    pub fn compression_ratio(&self) -> Result<f32, SaveLoadError> {
        let json = self.to_json()?;
        let binary = self.to_binary()?;

        if binary.is_empty() {
            return Ok(0.0);
        }

        Ok(json.len() as f32 / binary.len() as f32)
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Calculate CRC32 checksum.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Capture a snapshot of all entities that have a `Transform` in `world`.
///
/// If a `SceneGraph` resource is available the entity names and parent-child
/// relationships are captured; otherwise names and children will be empty.
///
/// The returned `GameSave` has `meta.timestamp` auto-populated from the
/// system clock (UTC, RFC-3339). Callers should set `meta.slot_name`.
pub fn capture_game_save(world: &World) -> GameSave {
    let transforms: Vec<(Entity, Transform)> = world
        .query::<Transform>()
        .into_iter()
        .map(|(e, t)| (e, *t))
        .collect();

    let graph = world.resource::<SceneGraph>();

    let index_to_pos: HashMap<u32, usize> = transforms
        .iter()
        .enumerate()
        .map(|(pos, (e, _))| (e.index(), pos))
        .collect();

    let entities: Vec<SavedEntity> = transforms
        .iter()
        .map(|(e, t)| {
            let name = graph.and_then(|g| g.name(*e).map(|s| s.to_string()));
            let children = graph
                .map(|g| {
                    g.children(*e)
                        .iter()
                        .filter_map(|c| index_to_pos.get(&c.index()).copied())
                        .collect()
                })
                .unwrap_or_default();
            SavedEntity {
                index: e.index(),
                name,
                transform: *t,
                children,
                custom_data: HashMap::new(),
            }
        })
        .collect();

    let timestamp = {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}", dur.as_secs())
        }
        #[cfg(target_arch = "wasm32")]
        {
            String::new()
        }
    };

    GameSave {
        meta: SaveMeta {
            slot_name: String::new(),
            timestamp,
            extra: HashMap::new(),
        },
        entities,
    }
}

/// Load a `GameSave` into a fresh world, spawning entities with their saved
/// transforms.  Returns `(Entity, &SavedEntity)` pairs so callers can
/// process `custom_data` and other per-entity fields.
pub fn load_game_save<'a>(world: &mut World, save: &'a GameSave) -> Vec<(Entity, &'a SavedEntity)> {
    let mut spawned: Vec<(Entity, &'a SavedEntity)> = Vec::with_capacity(save.entities.len());

    for se in &save.entities {
        let entity = world.spawn();
        world.insert(entity, se.transform);
        spawned.push((entity, se));
    }

    spawned
}

// ---------------------------------------------------------------------------
// Quick save slots
// ---------------------------------------------------------------------------

/// Quick save slot manager for common save slot operations.
pub struct SaveSlotManager {
    base_path: std::path::PathBuf,
    slots: Vec<Option<SaveMeta>>,
    max_slots: usize,
}

impl SaveSlotManager {
    pub fn new(base_path: impl AsRef<Path>, max_slots: usize) -> Self {
        let base_path = base_path.as_ref().to_path_buf();
        let mut slots = Vec::with_capacity(max_slots);
        for i in 0..max_slots {
            let slot_file = base_path.join(format!("slot_{}.qsave", i));
            let meta = if slot_file.exists() {
                GameSave::load_from_binary_file(&slot_file)
                    .ok()
                    .map(|s| s.meta)
            } else {
                None
            };
            slots.push(meta);
        }

        Self {
            base_path,
            slots,
            max_slots,
        }
    }

    pub fn slot_path(&self, slot: usize) -> std::path::PathBuf {
        self.base_path.join(format!("slot_{}.qsave", slot))
    }

    pub fn save(&mut self, slot: usize, save: &GameSave) -> Result<(), SaveLoadError> {
        if slot >= self.max_slots {
            return Err(SaveLoadError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Slot index out of range",
            )));
        }

        let path = self.slot_path(slot);
        save.save_to_binary_file(&path)?;
        self.slots[slot] = Some(save.meta.clone());
        Ok(())
    }

    pub fn load(&self, slot: usize) -> Result<GameSave, SaveLoadError> {
        if slot >= self.max_slots {
            return Err(SaveLoadError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Slot index out of range",
            )));
        }

        let path = self.slot_path(slot);
        GameSave::load_from_binary_file(&path)
    }

    pub fn delete(&mut self, slot: usize) -> Result<(), SaveLoadError> {
        if slot >= self.max_slots {
            return Err(SaveLoadError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Slot index out of range",
            )));
        }

        let path = self.slot_path(slot);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.slots[slot] = None;
        Ok(())
    }

    pub fn slot_meta(&self, slot: usize) -> Option<&SaveMeta> {
        self.slots.get(slot)?.as_ref()
    }

    pub fn slot_exists(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|m| m.is_some())
    }

    pub fn slot_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn all_slots(&self) -> &[Option<SaveMeta>] {
        &self.slots
    }

    pub fn find_empty_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    pub fn quick_save(&mut self, save: &GameSave) -> Result<usize, SaveLoadError> {
        if let Some(slot) = self.find_empty_slot() {
            self.save(slot, save)?;
            return Ok(slot);
        }

        let oldest = self.find_oldest_slot();
        if let Some(slot) = oldest {
            self.save(slot, save)?;
            return Ok(slot);
        }

        Err(SaveLoadError::Io(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "No save slots available",
        )))
    }

    fn find_oldest_slot(&self) -> Option<usize> {
        let mut oldest: Option<(usize, String)> = None;

        for (i, meta) in self.slots.iter().enumerate() {
            if let Some(m) = meta {
                if oldest.is_none() || m.timestamp < oldest.as_ref().unwrap().1 {
                    oldest = Some((i, m.timestamp.clone()));
                }
            }
        }

        oldest.map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_meta_default() {
        let meta = SaveMeta {
            slot_name: "Test".to_string(),
            timestamp: String::new(),
            extra: HashMap::new(),
        };
        assert_eq!(meta.slot_name, "Test");
    }

    #[test]
    fn test_saved_entity_default() {
        let entity = SavedEntity {
            index: 0,
            name: None,
            transform: Transform::IDENTITY,
            children: Vec::new(),
            custom_data: HashMap::new(),
        };
        assert_eq!(entity.index, 0);
    }

    #[test]
    fn test_game_save_json_roundtrip() {
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Test Save".to_string(),
                timestamp: "12345".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Player".to_string()),
                transform: Transform::IDENTITY,
                children: vec![1],
                custom_data: HashMap::new(),
            }],
        };

        let json = save.to_json().unwrap();
        let loaded = GameSave::from_json(&json).unwrap();

        assert_eq!(loaded.meta.slot_name, "Test Save");
        assert_eq!(loaded.entities.len(), 1);
        assert_eq!(loaded.entities[0].name, Some("Player".to_string()));
    }

    #[test]
    fn test_game_save_binary_roundtrip() {
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Binary Test".to_string(),
                timestamp: "99999".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![
                SavedEntity {
                    index: 0,
                    name: Some("Entity0".to_string()),
                    transform: Transform::IDENTITY,
                    children: vec![1, 2],
                    custom_data: HashMap::new(),
                },
                SavedEntity {
                    index: 1,
                    name: Some("Entity1".to_string()),
                    transform: Transform::IDENTITY,
                    children: vec![],
                    custom_data: HashMap::new(),
                },
            ],
        };

        let binary = save.to_binary().unwrap();
        assert!(binary.len() > BinaryHeader::SIZE);

        let loaded = GameSave::from_binary(&binary).unwrap();
        assert_eq!(loaded.meta.slot_name, "Binary Test");
        assert_eq!(loaded.entities.len(), 2);
    }

    #[test]
    fn test_binary_header() {
        let header = BinaryHeader::new(1000, 500, 0x12345678);
        let bytes = header.to_bytes();
        let loaded = BinaryHeader::from_bytes(&bytes);

        assert_eq!(loaded.magic, *SAVE_MAGIC);
        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.uncompressed_size, 1000);
        assert_eq!(loaded.compressed_size, 500);
        assert_eq!(loaded.checksum, 0x12345678);
    }

    #[test]
    fn test_crc32() {
        let data = b"Hello, World!";
        let crc = crc32(data);
        assert_ne!(crc, 0);

        let same_data = b"Hello, World!";
        assert_eq!(crc, crc32(same_data));

        let different_data = b"Hello, World?";
        assert_ne!(crc, crc32(different_data));
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = vec![0u8; BinaryHeader::SIZE + 10];
        data[0..4].copy_from_slice(b"BAD!");

        let result = GameSave::from_binary(&data);
        assert!(matches!(result, Err(SaveLoadError::InvalidMagic(_))));
    }

    #[test]
    fn test_compression_ratio() {
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: (0..100)
                .map(|i| SavedEntity {
                    index: i,
                    name: Some(format!("Entity_{}", i)),
                    transform: Transform::IDENTITY,
                    children: vec![],
                    custom_data: HashMap::new(),
                })
                .collect(),
        };

        let ratio = save.compression_ratio().unwrap();
        assert!(ratio > 1.0, "Binary should be smaller than JSON");
    }

    #[test]
    fn test_estimated_size() {
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Test".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            }],
        };

        let size = save.estimated_size();
        assert!(size > 0);
    }

    // =========================================================================
    // Corruption & Edge Case Tests
    // =========================================================================

    #[test]
    fn test_truncated_binary_data() {
        // Data shorter than the header size should be rejected
        let short_data = vec![0u8; BinaryHeader::SIZE - 1];
        let result = GameSave::from_binary(&short_data);
        assert!(
            matches!(result, Err(SaveLoadError::Deserialization(_))),
            "Expected Deserialization error for truncated data, got: {:?}",
            result
        );

        // Empty data should also fail
        let empty_data: Vec<u8> = vec![];
        let result = GameSave::from_binary(&empty_data);
        assert!(
            matches!(result, Err(SaveLoadError::Deserialization(_))),
            "Expected Deserialization error for empty data"
        );

        // Exactly header size but invalid content should fail at magic check
        let header_only = vec![0u8; BinaryHeader::SIZE];
        let result = GameSave::from_binary(&header_only);
        assert!(
            matches!(result, Err(SaveLoadError::InvalidMagic(_))),
            "Expected InvalidMagic error for header-only data"
        );
    }

    #[test]
    fn test_version_mismatch() {
        // Create a valid save, then tamper with the version byte
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Version Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Test".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            }],
        };

        let mut binary = save.to_binary().unwrap();

        // Tamper with version byte (offset 4-7, little-endian u32)
        // Set version to 99 (unsupported)
        binary[4] = 99;
        binary[5] = 0;
        binary[6] = 0;
        binary[7] = 0;

        let result = GameSave::from_binary(&binary);
        assert!(
            matches!(result, Err(SaveLoadError::UnsupportedVersion(99))),
            "Expected UnsupportedVersion(99) error, got: {:?}",
            result
        );

        // Test with version = 0
        binary[4] = 0;
        let result = GameSave::from_binary(&binary);
        assert!(
            matches!(result, Err(SaveLoadError::UnsupportedVersion(0))),
            "Expected UnsupportedVersion(0) error"
        );
    }

    #[test]
    fn test_corrupt_gzip_data() {
        // Create a valid save, then corrupt the compressed payload
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Corrupt Gzip Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Test".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            }],
        };

        let mut binary = save.to_binary().unwrap();

        // Corrupt bytes in the compressed payload (after header)
        let corrupt_start = BinaryHeader::SIZE + 5;
        if binary.len() > corrupt_start + 10 {
            for i in corrupt_start..(corrupt_start + 10) {
                binary[i] ^= 0xFF; // Flip bits
            }
        }

        let result = GameSave::from_binary(&binary);
        assert!(
            matches!(result, Err(SaveLoadError::Decompression(_))),
            "Expected Decompression error for corrupt gzip data, got: {:?}",
            result
        );

        // Test with completely invalid gzip magic (corrupt first byte after header)
        let mut binary2 = save.to_binary().unwrap();
        if binary2.len() > BinaryHeader::SIZE {
            binary2[BinaryHeader::SIZE] = 0x00;
            binary2[BinaryHeader::SIZE + 1] = 0x00;
        }
        let result2 = GameSave::from_binary(&binary2);
        assert!(
            matches!(result2, Err(SaveLoadError::Decompression(_))),
            "Expected Decompression error for invalid gzip magic"
        );
    }

    #[test]
    fn test_checksum_mismatch() {
        // Create a valid save, decompress successfully, but then checksum should catch corruption
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Checksum Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Entity".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            }],
        };

        let mut binary = save.to_binary().unwrap();

        // Corrupt the checksum in the header (offset 24-27)
        let original_checksum = u32::from_le_bytes([
            binary[24], binary[25], binary[26], binary[27],
        ]);
        binary[24] = binary[24].wrapping_add(1); // Increment first byte

        let result = GameSave::from_binary(&binary);
        assert!(
            matches!(result, Err(SaveLoadError::Deserialization(ref msg)) if msg.contains("Checksum mismatch")),
            "Expected Checksum mismatch error, got: {:?}",
            result
        );

        // Verify the original checksum was different
        assert_ne!(original_checksum, 0, "Original checksum should be non-zero");
    }

    #[test]
    fn test_save_slot_manager_bounds_checking() {
        let temp_dir = std::env::temp_dir().join("quasar_test_bounds_checking");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager = SaveSlotManager::new(&temp_dir, 3);
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Bounds Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![],
        };

        // Valid slot indices
        assert!(manager.save(0, &save).is_ok());
        assert!(manager.save(2, &save).is_ok());
        assert!(manager.load(0).is_ok());
        assert!(manager.load(2).is_ok());
        assert!(manager.delete(0).is_ok());
        assert!(manager.delete(2).is_ok());

        // Out-of-bounds indices
        assert!(manager.save(3, &save).is_err());
        assert!(manager.save(100, &save).is_err());
        assert!(manager.load(3).is_err());
        assert!(manager.load(usize::MAX).is_err());
        assert!(manager.delete(3).is_err());

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_save_slot_manager_quick_save_overflow() {
        let temp_dir = std::env::temp_dir().join("quasar_test_quick_save_overflow");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager = SaveSlotManager::new(&temp_dir, 3);
        let make_save = |ts: &str| GameSave {
            meta: SaveMeta {
                slot_name: format!("Save {}", ts),
                timestamp: ts.to_string(),
                extra: HashMap::new(),
            },
            entities: vec![],
        };

        // Fill all slots
        let slot0 = manager.quick_save(&make_save("100")).unwrap();
        assert_eq!(slot0, 0);
        let slot1 = manager.quick_save(&make_save("200")).unwrap();
        assert_eq!(slot1, 1);
        let slot2 = manager.quick_save(&make_save("300")).unwrap();
        assert_eq!(slot2, 2);

        assert_eq!(manager.slot_count(), 3);

        // Next quick save should overwrite the oldest (timestamp "100" = slot 0)
        let slot3 = manager.quick_save(&make_save("400")).unwrap();
        assert_eq!(slot3, 0, "Should overwrite oldest slot (slot 0)");

        // Verify slot 0 now has the new save
        let loaded = manager.load(0).unwrap();
        assert_eq!(loaded.meta.slot_name, "Save 400");

        // Slots 1 and 2 should still have their original data
        let loaded1 = manager.load(1).unwrap();
        assert_eq!(loaded1.meta.slot_name, "Save 200");
        let loaded2 = manager.load(2).unwrap();
        assert_eq!(loaded2.meta.slot_name, "Save 300");

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_save_slot_manager_find_empty_slot() {
        let temp_dir = std::env::temp_dir().join("quasar_test_find_empty");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager = SaveSlotManager::new(&temp_dir, 4);
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Empty Slot Test".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![],
        };

        // All slots should be empty initially, first empty is slot 0
        assert_eq!(manager.find_empty_slot(), Some(0));

        // Fill slots 0 and 2
        manager.save(0, &save).unwrap();
        manager.save(2, &save).unwrap();

        // First empty slot should be slot 1
        assert_eq!(manager.find_empty_slot(), Some(1));

        // Fill remaining slots
        manager.save(1, &save).unwrap();
        manager.save(3, &save).unwrap();

        // No empty slots left
        assert_eq!(manager.find_empty_slot(), None);

        // Delete a slot and verify empty detection works again
        manager.delete(2).unwrap();
        assert_eq!(manager.find_empty_slot(), Some(2));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_save_slot_manager_delete_nonexistent() {
        let temp_dir = std::env::temp_dir().join("quasar_test_delete_nonexistent");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager = SaveSlotManager::new(&temp_dir, 3);

        // Deleting a nonexistent slot should be idempotent (not error)
        assert!(manager.delete(0).is_ok());
        assert!(manager.delete(1).is_ok());
        assert!(manager.delete(2).is_ok());

        // Double delete should also succeed
        assert!(manager.delete(0).is_ok());
        assert!(manager.delete(0).is_ok());

        // Slot count should remain zero
        assert_eq!(manager.slot_count(), 0);

        // Verify slot meta is None after delete
        assert!(manager.slot_meta(0).is_none());

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_compression_empty_save() {
        // Test compression ratio for a save with no entities
        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Empty Save".to_string(),
                timestamp: "0".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![],
        };

        let ratio = save.compression_ratio().unwrap();
        // Even empty saves have metadata, so ratio should be reasonable
        assert!(
            ratio > 0.0,
            "Compression ratio should be positive for empty save"
        );

        // Verify binary roundtrip works for empty save
        let binary = save.to_binary().unwrap();
        let loaded = GameSave::from_binary(&binary).unwrap();
        assert_eq!(loaded.meta.slot_name, "Empty Save");
        assert_eq!(loaded.entities.len(), 0);

        // Verify JSON roundtrip works for empty save
        let json = save.to_json().unwrap();
        let loaded_json = GameSave::from_json(&json).unwrap();
        assert_eq!(loaded_json.entities.len(), 0);

        // File I/O roundtrip for empty save
        let temp_dir = std::env::temp_dir().join("quasar_test_empty_save");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let bin_path = temp_dir.join("empty.qsave");
        save.save_to_binary_file(&bin_path).unwrap();
        let loaded_file = GameSave::load_from_binary_file(&bin_path).unwrap();
        assert_eq!(loaded_file.meta.slot_name, "Empty Save");

        let json_path = temp_dir.join("empty.json");
        save.save_to_json_file(&json_path).unwrap();
        let loaded_json_file = GameSave::load_from_json_file(&json_path).unwrap();
        assert_eq!(loaded_json_file.meta.slot_name, "Empty Save");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_load_auto_detects_format() {
        let temp_dir = std::env::temp_dir().join("quasar_test_auto_detect");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let save = GameSave {
            meta: SaveMeta {
                slot_name: "Auto Detect Test".to_string(),
                timestamp: "42".to_string(),
                extra: HashMap::new(),
            },
            entities: vec![SavedEntity {
                index: 0,
                name: Some("Player".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            }],
        };

        // Save as binary and verify auto-detect with .qsave extension
        let bin_path = temp_dir.join("test.qsave");
        save.save_to_binary_file(&bin_path).unwrap();
        let loaded_binary = GameSave::load_auto(&bin_path).unwrap();
        assert_eq!(loaded_binary.meta.slot_name, "Auto Detect Test");

        // Save as JSON and verify auto-detect with .json extension
        let json_path = temp_dir.join("test.json");
        save.save_to_json_file(&json_path).unwrap();
        let loaded_json = GameSave::load_auto(&json_path).unwrap();
        assert_eq!(loaded_json.meta.slot_name, "Auto Detect Test");

        // Test binary detection via magic bytes (no extension)
        let no_ext_bin = temp_dir.join("test_no_ext_binary");
        save.save_to_binary_file(&no_ext_bin).unwrap();
        let loaded_no_ext = GameSave::load_auto(&no_ext_bin).unwrap();
        assert_eq!(loaded_no_ext.meta.slot_name, "Auto Detect Test");

        // Test that binary data with .json extension still tries binary first (magic bytes win)
        // This tests that magic bytes take priority over extension
        let fake_json = temp_dir.join("fake_binary.json");
        save.save_to_binary_file(&fake_json).unwrap();
        let loaded_fake_json = GameSave::load_auto(&fake_json).unwrap();
        assert_eq!(loaded_fake_json.meta.slot_name, "Auto Detect Test");

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
