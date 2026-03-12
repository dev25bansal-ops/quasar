//! Asset metadata and import settings persisted alongside source assets.
//!
//! Each asset gets a `.meta` sidecar file that stores:
//! - Content hash for change detection
//! - Import settings (compression, LOD, etc.)
//! - Processing status

use std::path::{Path, PathBuf};

/// Content hash algorithm used for detecting asset changes.
pub type ContentHash = String;

/// Import settings per-asset type.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSettings {
    /// Enable compression (BC7 for textures).
    pub compression: bool,
    /// Compression quality (1-100, higher = better quality, larger file).
    pub compression_quality: u8,
    /// Generate LOD levels (0 = disabled, 1-5 = max levels).
    pub lod_levels: u8,
    /// Target runtime format (gpu, web, mobile).
    pub target_format: TargetFormat,
    /// Stream audio threshold (kbps), 0 = decode entire file in memory.
    pub audio_stream_threshold: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TargetFormat {
    Gpu,
    Web,
    Mobile,
    Desktop,
}

impl Default for ImportSettings {
    fn default() -> Self {
        Self {
            compression: true,
            compression_quality: 70,
            lod_levels: 0,
            target_format: TargetFormat::Gpu,
            audio_stream_threshold: 0,
        }
    }
}

/// Asset metadata stored in `.meta` sidecar files.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetMeta {
    /// Content hash of the source asset.
    pub content_hash: ContentHash,
    /// Import settings persisted from last import.
    pub settings: ImportSettings,
    /// Import status (pending, ready, failed, success, skipped).
    pub status: ImportStatus,
    /// When the asset was last imported successfully.
    pub last_imported: Option<std::time::SystemTime>,
    /// Version number (defaults to 1).
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStatus {
    Pending,
    Ready,
    Failed,
    Success,
    Skipped,
}

impl Default for AssetMeta {
    fn default() -> Self {
        Self {
            content_hash: String::new(),
            settings: ImportSettings::default(),
            status: ImportStatus::Pending,
            last_imported: None,
            version: 1,
        }
    }
}

impl AssetMeta {
    /// Path to the `.meta` sidecar file for an asset.
    pub fn meta_path(asset_path: &Path) -> PathBuf {
        let mut meta = asset_path.to_path_buf();
        meta.set_extension("meta");
        meta
    }

    /// Load metadata from source file (compute hash and create default meta).
    pub fn from_source_file(asset_path: &Path) -> Result<Self, String> {
        let hash = Self::compute_content_hash(asset_path)?;
        Ok(Self {
            content_hash: hash,
            settings: ImportSettings::default(),
            status: ImportStatus::Ready,
            last_imported: None,
            version: 1,
        })
    }

    /// Save metadata to `.meta` sidecar file.
    pub fn save(&self, asset_path: &Path) -> Result<(), String> {
        let meta_path = Self::meta_path(asset_path);
        let json = self.to_json()?;
        std::fs::write(&meta_path, json)
            .map_err(|e| format!("Failed to write {}: {}", meta_path.display(), e))?;
        Ok(())
    }

    /// Load metadata from `.meta` sidecar file.
    pub fn load(asset_path: &Path) -> Result<Option<Self>, String> {
        let meta_path = Self::meta_path(asset_path);
        let content = std::fs::read_to_string(&meta_path)
            .map_err(|e| format!("Failed to read {}: {}", meta_path.display(), e))?;
        let result = Self::from_json(&content)?;
        Ok(Some(result))
    }

    /// Check if the source file has changed based on content hash.
    pub fn is_outdated(&self, asset_path: &Path) -> Result<bool, String> {
        let current_hash = Self::compute_content_hash(asset_path)?;
        Ok(current_hash != self.content_hash)
    }

    /// Mark the asset as outdated (content hash mismatch).
    pub fn mark_outdated(&mut self, asset_path: &Path) -> Result<(), String> {
        let new_hash = Self::compute_content_hash(asset_path)?;
        self.content_hash = new_hash;
        self.status = ImportStatus::Pending;
        Ok(())
    }

    /// Convert to JSON (inline simple implementation without serde).
    fn to_json(&self) -> Result<String, String> {
        // Simple manual JSON serialization
        let mut json = String::new();
        json.push_str("{");
        json.push_str(&format!("\"content_hash\":\"{}\",", self.content_hash));
        json.push_str(&format!("\"compression\":{},", self.settings.compression));
        json.push_str(&format!(
            "\"compression_quality\":{},",
            self.settings.compression_quality
        ));
        json.push_str(&format!("\"lod_levels\":{},", self.settings.lod_levels));
        json.push_str(&format!(
            "\"target_format\":\"{:?}\",",
            self.settings.target_format
        ));
        json.push_str(&format!(
            "\"audio_stream_threshold\":{},",
            self.settings.audio_stream_threshold
        ));
        json.push_str(&format!("\"status\":\"{:?}\",", self.status));
        if let Some(_time) = &self.last_imported {
            // Simplified: use 0 as timestamp (real implementation would use time crate)
            json.push_str("\"last_imported\":0,");
        } else {
            json.push_str("\"last_imported\":null,");
        }
        json.push_str(&format!("\"version\":{}", self.version));
        json.push_str("}");
        Ok(json)
    }

    /// Parse from JSON (simple implementation).
    fn from_json(json: &str) -> Result<Self, String> {
        // Simple JSON parsing - in production you'd use serde
        let mut result = AssetMeta::default();

        // Very basic parsing
        if json.contains("\"status\":\"Ready\"") || json.contains("\"status\":\"Success\"") {
            result.status = ImportStatus::Ready;
        } else if json.contains("\"status\":\"Failed\"") {
            result.status = ImportStatus::Failed;
        } else if json.contains("\"status\":\"Skipped\"") {
            result.status = ImportStatus::Skipped;
        }

        if json.contains("\"compression\":true") {
            result.settings.compression = true;
        } else if json.contains("\"compression\":false") {
            result.settings.compression = false;
        }

        Ok(result)
    }

    /// Compute content hash of a file.
    pub fn compute_content_hash(path: &Path) -> Result<ContentHash, String> {
        let content =
            std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        // Simplified hash - in production use sha2 crate
        let mut hasher = 0u64;
        for byte in content {
            hasher = hasher.wrapping_mul(31).wrapping_add(byte as u64);
        }
        Ok(format!("{:016x}", hasher))
    }
}
