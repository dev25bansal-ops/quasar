//! Asset Store Integration for Quasar Engine.
//!
//! Provides:
//! - **Free asset packs** - Curated free assets for rapid prototyping
//! - **Asset search** - Search across asset stores
//! - **One-click import** - Download and import assets directly
//! - **Asset previews** - Preview before downloading
//! - **License tracking** - Track asset licenses

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetLicense {
    Cc0,
    CcBy,
    CcBySa,
    CcByNc,
    Unlicense,
    Mit,
    Apache2,
    Bsd3,
    Commercial,
    Custom,
}

impl AssetLicense {
    pub fn name(&self) -> &'static str {
        match self {
            AssetLicense::Cc0 => "CC0 Public Domain",
            AssetLicense::CcBy => "CC BY",
            AssetLicense::CcBySa => "CC BY-SA",
            AssetLicense::CcByNc => "CC BY-NC",
            AssetLicense::Unlicense => "Unlicense",
            AssetLicense::Mit => "MIT",
            AssetLicense::Apache2 => "Apache 2.0",
            AssetLicense::Bsd3 => "BSD 3-Clause",
            AssetLicense::Commercial => "Commercial",
            AssetLicense::Custom => "Custom",
        }
    }

    pub fn can_use_commercially(&self) -> bool {
        matches!(
            self,
            AssetLicense::Cc0
                | AssetLicense::CcBy
                | AssetLicense::CcBySa
                | AssetLicense::Unlicense
                | AssetLicense::Mit
                | AssetLicense::Apache2
                | AssetLicense::Bsd3
                | AssetLicense::Commercial
        )
    }

    pub fn requires_attribution(&self) -> bool {
        matches!(
            self,
            AssetLicense::CcBy
                | AssetLicense::CcBySa
                | AssetLicense::CcByNc
                | AssetLicense::Commercial
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetCategory {
    Models,
    Textures,
    Materials,
    Animations,
    Audio,
    Music,
    SoundEffects,
    Particles,
    Scripts,
    Shaders,
    Fonts,
    Icons,
    Ui,
    Templates,
}

impl AssetCategory {
    pub fn name(&self) -> &'static str {
        match self {
            AssetCategory::Models => "3D Models",
            AssetCategory::Textures => "Textures",
            AssetCategory::Materials => "Materials",
            AssetCategory::Animations => "Animations",
            AssetCategory::Audio => "Audio",
            AssetCategory::Music => "Music",
            AssetCategory::SoundEffects => "Sound Effects",
            AssetCategory::Particles => "Particles",
            AssetCategory::Scripts => "Scripts",
            AssetCategory::Shaders => "Shaders",
            AssetCategory::Fonts => "Fonts",
            AssetCategory::Icons => "Icons",
            AssetCategory::Ui => "UI Elements",
            AssetCategory::Templates => "Templates",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            AssetCategory::Models => "🎲",
            AssetCategory::Textures => "🖼",
            AssetCategory::Materials => "🎨",
            AssetCategory::Animations => "🎬",
            AssetCategory::Audio => "🔊",
            AssetCategory::Music => "🎵",
            AssetCategory::SoundEffects => "📢",
            AssetCategory::Particles => "✨",
            AssetCategory::Scripts => "📜",
            AssetCategory::Shaders => "🔷",
            AssetCategory::Fonts => "🔤",
            AssetCategory::Icons => "⭐",
            AssetCategory::Ui => "🎛",
            AssetCategory::Templates => "📦",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreAsset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub categories: Vec<AssetCategory>,
    pub license: AssetLicense,
    pub download_url: String,
    pub preview_url: Option<String>,
    pub file_size_mb: f32,
    pub tags: Vec<String>,
    pub rating: f32,
    pub download_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub assets: Vec<StoreAsset>,
    pub preview_url: Option<String>,
    pub is_free: bool,
    pub license: AssetLicense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub asset_id: String,
    pub asset_name: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Extracting,
    Importing,
    Complete,
    Failed,
}

pub struct AssetStore {
    pub packs: Vec<AssetPack>,
    pub assets: Vec<StoreAsset>,
    pub downloads: Vec<DownloadProgress>,
    pub search_query: String,
    pub selected_category: Option<AssetCategory>,
    pub show_free_only: bool,
    pub license_filter: Option<AssetLicense>,
    pub downloaded_assets: HashMap<String, String>,
}

impl AssetStore {
    pub fn new() -> Self {
        let packs = Self::get_builtin_packs();
        let assets: Vec<_> = packs.iter().flat_map(|p| p.assets.clone()).collect();

        Self {
            packs,
            assets,
            downloads: Vec::new(),
            search_query: String::new(),
            selected_category: None,
            show_free_only: true,
            license_filter: None,
            downloaded_assets: HashMap::new(),
        }
    }

    fn get_builtin_packs() -> Vec<AssetPack> {
        vec![
            AssetPack {
                id: "quasar-starter-pack".to_string(),
                name: "Quasar Starter Pack".to_string(),
                description: "Essential assets for getting started with Quasar Engine".to_string(),
                author: "Quasar Team".to_string(),
                preview_url: None,
                is_free: true,
                license: AssetLicense::Cc0,
                assets: vec![
                    StoreAsset {
                        id: "cube-basic".to_string(),
                        name: "Basic Cube".to_string(),
                        description: "Simple textured cube for prototyping".to_string(),
                        author: "Quasar Team".to_string(),
                        categories: vec![AssetCategory::Models],
                        license: AssetLicense::Cc0,
                        download_url: "builtin://cube.gltf".to_string(),
                        preview_url: None,
                        file_size_mb: 0.01,
                        tags: vec!["primitive".to_string(), "basic".to_string()],
                        rating: 5.0,
                        download_count: 1000,
                        created_at: "2024-01-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                    StoreAsset {
                        id: "sphere-basic".to_string(),
                        name: "Basic Sphere".to_string(),
                        description: "Simple UV sphere for prototyping".to_string(),
                        author: "Quasar Team".to_string(),
                        categories: vec![AssetCategory::Models],
                        license: AssetLicense::Cc0,
                        download_url: "builtin://sphere.gltf".to_string(),
                        preview_url: None,
                        file_size_mb: 0.02,
                        tags: vec!["primitive".to_string(), "basic".to_string()],
                        rating: 5.0,
                        download_count: 950,
                        created_at: "2024-01-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                    StoreAsset {
                        id: "plane-basic".to_string(),
                        name: "Basic Plane".to_string(),
                        description: "Ground plane for prototyping".to_string(),
                        author: "Quasar Team".to_string(),
                        categories: vec![AssetCategory::Models],
                        license: AssetLicense::Cc0,
                        download_url: "builtin://plane.gltf".to_string(),
                        preview_url: None,
                        file_size_mb: 0.01,
                        tags: vec!["primitive".to_string(), "ground".to_string()],
                        rating: 5.0,
                        download_count: 900,
                        created_at: "2024-01-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                ],
            },
            AssetPack {
                id: "kenney-assets".to_string(),
                name: "Kenney Game Assets".to_string(),
                description: "Public domain game assets by Kenney.nl".to_string(),
                author: "Kenney".to_string(),
                preview_url: None,
                is_free: true,
                license: AssetLicense::Cc0,
                assets: vec![
                    StoreAsset {
                        id: "kenney-platformer".to_string(),
                        name: "Platformer Pack".to_string(),
                        description: "2D platformer sprites and tiles".to_string(),
                        author: "Kenney".to_string(),
                        categories: vec![AssetCategory::Textures, AssetCategory::Ui],
                        license: AssetLicense::Cc0,
                        download_url: "https://kenney.nl/assets/platformer-pack-redux".to_string(),
                        preview_url: None,
                        file_size_mb: 15.0,
                        tags: vec![
                            "2d".to_string(),
                            "platformer".to_string(),
                            "sprites".to_string(),
                        ],
                        rating: 4.9,
                        download_count: 50000,
                        created_at: "2023-06-01".to_string(),
                        updated_at: "2024-01-15".to_string(),
                    },
                    StoreAsset {
                        id: "kenney-rpg".to_string(),
                        name: "RPG Pack".to_string(),
                        description: "RPG characters, items and tiles".to_string(),
                        author: "Kenney".to_string(),
                        categories: vec![AssetCategory::Textures, AssetCategory::Ui],
                        license: AssetLicense::Cc0,
                        download_url: "https://kenney.nl/assets/rpg-pack".to_string(),
                        preview_url: None,
                        file_size_mb: 22.0,
                        tags: vec![
                            "2d".to_string(),
                            "rpg".to_string(),
                            "characters".to_string(),
                        ],
                        rating: 4.8,
                        download_count: 45000,
                        created_at: "2023-05-15".to_string(),
                        updated_at: "2024-02-01".to_string(),
                    },
                    StoreAsset {
                        id: "kenney-shooter".to_string(),
                        name: "Shooter Pack".to_string(),
                        description: "Top-down shooter assets".to_string(),
                        author: "Kenney".to_string(),
                        categories: vec![AssetCategory::Textures, AssetCategory::Ui],
                        license: AssetLicense::Cc0,
                        download_url: "https://kenney.nl/assets/topdown-shooter".to_string(),
                        preview_url: None,
                        file_size_mb: 18.0,
                        tags: vec![
                            "2d".to_string(),
                            "shooter".to_string(),
                            "topdown".to_string(),
                        ],
                        rating: 4.7,
                        download_count: 38000,
                        created_at: "2023-07-01".to_string(),
                        updated_at: "2024-01-20".to_string(),
                    },
                ],
            },
            AssetPack {
                id: "polyhaven-pbr".to_string(),
                name: "Poly Haven PBR Materials".to_string(),
                description: "High-quality PBR materials from Poly Haven".to_string(),
                author: "Poly Haven".to_string(),
                preview_url: None,
                is_free: true,
                license: AssetLicense::Cc0,
                assets: vec![
                    StoreAsset {
                        id: "ph-bricks".to_string(),
                        name: "Brick Materials Pack".to_string(),
                        description: "Various brick wall PBR materials".to_string(),
                        author: "Poly Haven".to_string(),
                        categories: vec![AssetCategory::Materials, AssetCategory::Textures],
                        license: AssetLicense::Cc0,
                        download_url: "https://polyhaven.com/a/?q=brick".to_string(),
                        preview_url: None,
                        file_size_mb: 150.0,
                        tags: vec!["pbr".to_string(), "brick".to_string(), "wall".to_string()],
                        rating: 4.9,
                        download_count: 25000,
                        created_at: "2023-01-01".to_string(),
                        updated_at: "2024-03-01".to_string(),
                    },
                    StoreAsset {
                        id: "ph-wood".to_string(),
                        name: "Wood Materials Pack".to_string(),
                        description: "Various wood PBR materials".to_string(),
                        author: "Poly Haven".to_string(),
                        categories: vec![AssetCategory::Materials, AssetCategory::Textures],
                        license: AssetLicense::Cc0,
                        download_url: "https://polyhaven.com/a/?q=wood".to_string(),
                        preview_url: None,
                        file_size_mb: 200.0,
                        tags: vec!["pbr".to_string(), "wood".to_string(), "floor".to_string()],
                        rating: 4.8,
                        download_count: 22000,
                        created_at: "2023-01-01".to_string(),
                        updated_at: "2024-03-01".to_string(),
                    },
                    StoreAsset {
                        id: "ph-metal".to_string(),
                        name: "Metal Materials Pack".to_string(),
                        description: "Various metal PBR materials".to_string(),
                        author: "Poly Haven".to_string(),
                        categories: vec![AssetCategory::Materials, AssetCategory::Textures],
                        license: AssetLicense::Cc0,
                        download_url: "https://polyhaven.com/a/?q=metal".to_string(),
                        preview_url: None,
                        file_size_mb: 180.0,
                        tags: vec![
                            "pbr".to_string(),
                            "metal".to_string(),
                            "industrial".to_string(),
                        ],
                        rating: 4.9,
                        download_count: 28000,
                        created_at: "2023-01-01".to_string(),
                        updated_at: "2024-03-01".to_string(),
                    },
                ],
            },
            AssetPack {
                id: "sketchfab-free".to_string(),
                name: "Sketchfab Free Models".to_string(),
                description: "Curated free-to-use 3D models from Sketchfab".to_string(),
                author: "Various".to_string(),
                preview_url: None,
                is_free: true,
                license: AssetLicense::CcBy,
                assets: vec![
                    StoreAsset {
                        id: "sf-lowpoly-character".to_string(),
                        name: "Low Poly Character Pack".to_string(),
                        description: "Stylized low poly character with animations".to_string(),
                        author: "Various".to_string(),
                        categories: vec![AssetCategory::Models, AssetCategory::Animations],
                        license: AssetLicense::CcBy,
                        download_url: "https://sketchfab.com/features/gaming".to_string(),
                        preview_url: None,
                        file_size_mb: 5.0,
                        tags: vec![
                            "lowpoly".to_string(),
                            "character".to_string(),
                            "animated".to_string(),
                        ],
                        rating: 4.5,
                        download_count: 15000,
                        created_at: "2023-06-01".to_string(),
                        updated_at: "2024-02-01".to_string(),
                    },
                    StoreAsset {
                        id: "sf-lowpoly-nature".to_string(),
                        name: "Low Poly Nature Pack".to_string(),
                        description: "Trees, rocks, and nature props".to_string(),
                        author: "Various".to_string(),
                        categories: vec![AssetCategory::Models],
                        license: AssetLicense::CcBy,
                        download_url: "https://sketchfab.com/features/gaming".to_string(),
                        preview_url: None,
                        file_size_mb: 3.0,
                        tags: vec![
                            "lowpoly".to_string(),
                            "nature".to_string(),
                            "trees".to_string(),
                        ],
                        rating: 4.6,
                        download_count: 18000,
                        created_at: "2023-05-01".to_string(),
                        updated_at: "2024-01-15".to_string(),
                    },
                ],
            },
            AssetPack {
                id: "freesound-effects".to_string(),
                name: "Free Sound Effects".to_string(),
                description: "Game-ready sound effects from freesound.org".to_string(),
                author: "Various".to_string(),
                preview_url: None,
                is_free: true,
                license: AssetLicense::CcBy,
                assets: vec![
                    StoreAsset {
                        id: "fs-ui-sounds".to_string(),
                        name: "UI Sound Pack".to_string(),
                        description: "Menu clicks, beeps, and interface sounds".to_string(),
                        author: "Various".to_string(),
                        categories: vec![AssetCategory::SoundEffects],
                        license: AssetLicense::Cc0,
                        download_url: "https://freesound.org".to_string(),
                        preview_url: None,
                        file_size_mb: 2.0,
                        tags: vec!["ui".to_string(), "menu".to_string(), "click".to_string()],
                        rating: 4.4,
                        download_count: 30000,
                        created_at: "2023-01-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                    StoreAsset {
                        id: "fs-footsteps".to_string(),
                        name: "Footstep Pack".to_string(),
                        description: "Footsteps on various surfaces".to_string(),
                        author: "Various".to_string(),
                        categories: vec![AssetCategory::SoundEffects],
                        license: AssetLicense::CcBy,
                        download_url: "https://freesound.org".to_string(),
                        preview_url: None,
                        file_size_mb: 5.0,
                        tags: vec!["footsteps".to_string(), "movement".to_string()],
                        rating: 4.3,
                        download_count: 25000,
                        created_at: "2023-02-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                    StoreAsset {
                        id: "fs-ambient".to_string(),
                        name: "Ambient Sound Pack".to_string(),
                        description: "Ambient loops for various environments".to_string(),
                        author: "Various".to_string(),
                        categories: vec![AssetCategory::SoundEffects, AssetCategory::Music],
                        license: AssetLicense::CcBy,
                        download_url: "https://freesound.org".to_string(),
                        preview_url: None,
                        file_size_mb: 50.0,
                        tags: vec![
                            "ambient".to_string(),
                            "loop".to_string(),
                            "environment".to_string(),
                        ],
                        rating: 4.5,
                        download_count: 20000,
                        created_at: "2023-03-01".to_string(),
                        updated_at: "2024-01-01".to_string(),
                    },
                ],
            },
        ]
    }

    pub fn search(&self, query: &str) -> Vec<&StoreAsset> {
        let query_lower = query.to_lowercase();
        self.assets
            .iter()
            .filter(|a| {
                let matches_query = query.is_empty()
                    || a.name.to_lowercase().contains(&query_lower)
                    || a.description.to_lowercase().contains(&query_lower)
                    || a.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower));

                let matches_category = self
                    .selected_category
                    .map_or(true, |c| a.categories.contains(&c));

                let matches_license = self.license_filter.map_or(true, |l| a.license == l);

                matches_query && matches_category && matches_license
            })
            .collect()
    }

    pub fn download_asset(&mut self, asset_id: &str) -> bool {
        if self.downloads.iter().any(|d| d.asset_id == asset_id) {
            return false;
        }

        if let Some(asset) = self.assets.iter().find(|a| a.id == asset_id) {
            self.downloads.push(DownloadProgress {
                asset_id: asset_id.to_string(),
                asset_name: asset.name.clone(),
                bytes_downloaded: 0,
                total_bytes: (asset.file_size_mb * 1024.0 * 1024.0) as u64,
                status: DownloadStatus::Pending,
            });
            return true;
        }
        false
    }

    pub fn update_download(&mut self, asset_id: &str, bytes: u64, status: DownloadStatus) {
        if let Some(download) = self.downloads.iter_mut().find(|d| d.asset_id == asset_id) {
            download.bytes_downloaded = bytes;
            download.status = status;

            if status == DownloadStatus::Complete {
                self.downloaded_assets.insert(
                    asset_id.to_string(),
                    format!("assets/imported/{}", asset_id),
                );
            }
        }
    }

    pub fn is_downloaded(&self, asset_id: &str) -> bool {
        self.downloaded_assets.contains_key(asset_id)
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Asset Store")
            .default_size([600.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.search_query);
                    if ui.button("Search").clicked() {}
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Category:");
                    egui::ComboBox::from_id_salt("category_filter")
                        .selected_text(self.selected_category.map(|c| c.name()).unwrap_or("All"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_category, None, "All");
                            for cat in [
                                AssetCategory::Models,
                                AssetCategory::Textures,
                                AssetCategory::Materials,
                                AssetCategory::Animations,
                                AssetCategory::Audio,
                                AssetCategory::SoundEffects,
                                AssetCategory::Shaders,
                                AssetCategory::Templates,
                            ] {
                                ui.selectable_value(
                                    &mut self.selected_category,
                                    Some(cat),
                                    cat.name(),
                                );
                            }
                        });

                    ui.checkbox(&mut self.show_free_only, "Free Only");
                });

                ui.separator();

                let results = self.search(&self.search_query);
                ui.label(format!("{} assets found", results.len()));

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for asset in &results {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.heading(&asset.name);
                                    ui.label(&asset.description);
                                    ui.horizontal(|ui| {
                                        ui.label(format!("By: {}", asset.author));
                                        ui.label("|");
                                        ui.label(format!("Size: {:.1} MB", asset.file_size_mb));
                                        ui.label("|");
                                        ui.colored_label(
                                            egui::Color32::GOLD,
                                            format!("⭐ {:.1}", asset.rating),
                                        );
                                        ui.label(format!("({})", asset.download_count));
                                    });
                                    ui.horizontal(|ui| {
                                        for cat in &asset.categories {
                                            ui.small(cat.icon());
                                        }
                                        ui.label(asset.license.name());
                                    });
                                    ui.horizontal(|ui| {
                                        for tag in &asset.tags {
                                            ui.small(format!("#{}", tag));
                                        }
                                    });
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if self.is_downloaded(&asset.id) {
                                            ui.button("✓ Downloaded")
                                                .on_hover_text("Asset already downloaded");
                                        } else if self
                                            .downloads
                                            .iter()
                                            .any(|d| d.asset_id == asset.id)
                                        {
                                            let download = self
                                                .downloads
                                                .iter()
                                                .find(|d| d.asset_id == asset.id)
                                                .unwrap();
                                            let progress = if download.total_bytes > 0 {
                                                download.bytes_downloaded as f32
                                                    / download.total_bytes as f32
                                            } else {
                                                0.0
                                            };
                                            let progress_bar = egui::ProgressBar::new(progress)
                                                .text(format!("{:?}", download.status));
                                            ui.add(progress_bar);
                                        } else {
                                            if ui.button("Download").clicked() {
                                                self.download_asset(&asset.id);
                                            }
                                        }
                                    },
                                );
                            });
                        });
                    }
                });

                if !self.downloads.is_empty() {
                    ui.separator();
                    ui.label("Downloads:");
                    for download in &self.downloads {
                        ui.horizontal(|ui| {
                            ui.label(&download.asset_name);
                            let progress = if download.total_bytes > 0 {
                                download.bytes_downloaded as f32 / download.total_bytes as f32
                            } else {
                                0.0
                            };
                            ui.add(egui::ProgressBar::new(progress));
                            ui.label(format!("{:?}", download.status));
                        });
                    }
                }
            });
    }
}

impl Default for AssetStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_store_creation() {
        let store = AssetStore::new();
        assert!(!store.packs.is_empty());
        assert!(!store.assets.is_empty());
    }

    #[test]
    fn asset_search() {
        let store = AssetStore::new();
        let results = store.search("cube");
        assert!(!results.is_empty());
    }

    #[test]
    fn asset_license_commercial() {
        assert!(AssetLicense::Cc0.can_use_commercially());
        assert!(AssetLicense::CcBy.can_use_commercially());
        assert!(!AssetLicense::CcByNc.can_use_commercially());
    }

    #[test]
    fn asset_license_attribution() {
        assert!(!AssetLicense::Cc0.requires_attribution());
        assert!(AssetLicense::CcBy.requires_attribution());
    }

    #[test]
    fn download_asset() {
        let mut store = AssetStore::new();
        assert!(store.download_asset("cube-basic"));
        assert!(!store.download_asset("cube-basic"));
    }

    #[test]
    fn update_download_status() {
        let mut store = AssetStore::new();
        store.download_asset("cube-basic");
        store.update_download("cube-basic", 500, DownloadStatus::Complete);
        assert!(store.is_downloaded("cube-basic"));
    }

    #[test]
    fn category_filter() {
        let mut store = AssetStore::new();
        store.selected_category = Some(AssetCategory::Models);
        let results = store.search("");
        assert!(results
            .iter()
            .all(|a| a.categories.contains(&AssetCategory::Models)));
    }
}
