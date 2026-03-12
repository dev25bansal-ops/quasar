//! Editor Asset Browser panel.
//!
//! Scans an `assets/` directory, displays files in a grid with icons/thumbnails,
//! and supports selection and drag-and-drop onto the scene hierarchy.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Recognised asset kinds (drives icon and thumbnail generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Texture,
    Model,
    Script,
    Audio,
    Shader,
    Scene,
    Unknown,
}

impl AssetKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "bmp" | "tga" | "hdr" | "exr" => Self::Texture,
            "gltf" | "glb" | "obj" | "fbx" => Self::Model,
            "lua" | "luau" => Self::Script,
            "wav" | "ogg" | "mp3" | "flac" => Self::Audio,
            "wgsl" | "glsl" | "hlsl" | "spv" => Self::Shader,
            "json" | "ron" | "toml" | "scene" => Self::Scene,
            _ => Self::Unknown,
        }
    }

    /// Icon character for the asset kind.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Texture => "🖼",
            Self::Model => "🏗",
            Self::Script => "📜",
            Self::Audio => "🔊",
            Self::Shader => "🎨",
            Self::Scene => "🗺",
            Self::Unknown => "📄",
        }
    }
}

/// A single entry in the asset browser.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    /// Display name (file name without directory parts).
    pub name: String,
    /// Relative path from the assets root.
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Detected asset kind.
    pub kind: AssetKind,
    /// File size in bytes (0 for directories).
    pub size: u64,
}

/// Asset browser state.
pub struct AssetBrowser {
    /// Root directory to scan.
    pub root: PathBuf,
    /// Currently displayed directory (relative to root).
    pub current_dir: PathBuf,
    /// Cached entries in the current directory.
    pub entries: Vec<AssetEntry>,
    /// Currently selected asset path (if any).
    pub selected: Option<String>,
    /// Search/filter string.
    pub filter: String,
    /// Whether a rescan is needed.
    pub dirty: bool,
}

impl AssetBrowser {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            current_dir: PathBuf::new(),
            root,
            entries: Vec::new(),
            selected: None,
            filter: String::new(),
            dirty: true,
        }
    }

    /// Scan (or re-scan) the current directory and populate `entries`.
    pub fn refresh(&mut self) {
        self.entries.clear();
        let scan_path = self.root.join(&self.current_dir);

        // Add parent directory entry if not at root.
        if self.current_dir != PathBuf::new() {
            self.entries.push(AssetEntry {
                name: "..".into(),
                relative_path: "..".into(),
                absolute_path: scan_path.join(".."),
                is_dir: true,
                kind: AssetKind::Unknown,
                size: 0,
            });
        }

        let read_dir = match std::fs::read_dir(&scan_path) {
            Ok(rd) => rd,
            Err(e) => {
                log::warn!("AssetBrowser: cannot read {:?}: {}", scan_path, e);
                return;
            }
        };

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in read_dir.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_name = entry.file_name().to_string_lossy().to_string();
            let rel = self.current_dir.join(&file_name);

            if meta.is_dir() {
                dirs.push(AssetEntry {
                    name: file_name,
                    relative_path: rel.to_string_lossy().to_string(),
                    absolute_path: entry.path(),
                    is_dir: true,
                    kind: AssetKind::Unknown,
                    size: 0,
                });
            } else {
                let kind = Path::new(&file_name)
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(AssetKind::from_extension)
                    .unwrap_or(AssetKind::Unknown);
                files.push(AssetEntry {
                    name: file_name,
                    relative_path: rel.to_string_lossy().to_string(),
                    absolute_path: entry.path(),
                    is_dir: false,
                    kind,
                    size: meta.len(),
                });
            }
        }

        // Directories first (alphabetical), then files (alphabetical).
        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.entries.extend(dirs);
        self.entries.extend(files);
        self.dirty = false;
    }

    /// Navigate into a sub-directory (relative to current).
    pub fn navigate(&mut self, subdir: &str) {
        if subdir == ".." {
            self.current_dir.pop();
        } else {
            self.current_dir.push(subdir);
        }
        self.dirty = true;
    }

    /// Draw the asset browser panel using egui.
    ///
    /// Returns the path of any asset that was drag-started (for drag-and-drop).
    pub fn panel(&mut self, ctx: &egui::Context) -> Option<String> {
        if self.dirty {
            self.refresh();
        }

        let mut drag_path: Option<String> = None;

        egui::TopBottomPanel::bottom("asset_browser")
            .resizable(true)
            .min_height(120.0)
            .default_height(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("📁 Assets");
                    ui.separator();
                    ui.label(format!("/{}", self.current_dir.to_string_lossy()));
                    ui.separator();
                    if ui.button("⟳ Refresh").clicked() {
                        self.dirty = true;
                    }
                    ui.separator();
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.filter);
                });

                ui.separator();

                let filter_lower = self.filter.to_lowercase();

                // Collect deferred actions to avoid borrowing self mutably
                // while iterating self.entries.
                let mut navigate_to: Option<String> = None;
                let mut select_path: Option<String> = None;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let column_width = 90.0;
                    let available = ui.available_width();
                    let cols = ((available / column_width) as usize).max(1);

                    egui::Grid::new("asset_grid")
                        .num_columns(cols)
                        .spacing([8.0, 8.0])
                        .show(ui, |ui| {
                            let mut col = 0;
                            for entry in &self.entries {
                                // Apply filter.
                                if !filter_lower.is_empty()
                                    && !entry.name.to_lowercase().contains(&filter_lower)
                                {
                                    continue;
                                }

                                let is_selected =
                                    self.selected.as_deref() == Some(entry.relative_path.as_str());

                                let entry_name = entry.name.clone();
                                let entry_rel = entry.relative_path.clone();
                                let entry_is_dir = entry.is_dir;
                                let entry_kind = entry.kind;
                                let entry_size = entry.size;

                                ui.vertical(|ui| {
                                    let icon = if entry_is_dir {
                                        "📁"
                                    } else {
                                        entry_kind.icon()
                                    };

                                    let label = format!("{}\n{}", icon, truncate(&entry_name, 12));

                                    let response = ui.selectable_label(is_selected, label);

                                    if response.clicked() {
                                        if entry_is_dir {
                                            navigate_to = Some(entry_name.clone());
                                        } else {
                                            select_path = Some(entry_rel.clone());
                                        }
                                    }

                                    // Drag source for non-directories.
                                    if !entry_is_dir && response.dragged() {
                                        drag_path = Some(entry_rel.clone());
                                    }

                                    // Tooltip with details
                                    response.on_hover_text(format!(
                                        "{}\nKind: {:?}\nSize: {} bytes",
                                        entry_rel, entry_kind, entry_size
                                    ));
                                });

                                col += 1;
                                if col >= cols {
                                    ui.end_row();
                                    col = 0;
                                }
                            }
                        });
                });

                // Apply deferred actions.
                if let Some(dir) = navigate_to {
                    self.navigate(&dir);
                }
                if let Some(path) = select_path {
                    self.selected = Some(path);
                }
            });

        drag_path
    }
}

/// Truncate a string and append "…" if longer than `max_len`.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_kind_detection() {
        assert_eq!(AssetKind::from_extension("png"), AssetKind::Texture);
        assert_eq!(AssetKind::from_extension("glb"), AssetKind::Model);
        assert_eq!(AssetKind::from_extension("lua"), AssetKind::Script);
        assert_eq!(AssetKind::from_extension("wav"), AssetKind::Audio);
        assert_eq!(AssetKind::from_extension("wgsl"), AssetKind::Shader);
        assert_eq!(AssetKind::from_extension("xyz"), AssetKind::Unknown);
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello_world_long", 8), "hello_w…");
    }
}
