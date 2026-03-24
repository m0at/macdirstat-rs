use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub kind: NodeKind,
    pub extension: Option<String>,
    pub children: Vec<FileNode>,
    pub item_count: usize,
    pub color: [u8; 3],
}

impl FileNode {
    pub fn find_child(&self, target: &PathBuf) -> Option<&FileNode> {
        if &self.path == target {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_child(target) {
                return Some(found);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum NodeKind {
    File,
    #[default]
    Directory,
}

#[derive(Debug, Clone, Default)]
pub struct ScanProgress {
    pub current_path: String,
    pub items_scanned: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ScanStatus {
    #[default]
    Idle,
    Scanning,
    Complete,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickPreset {
    pub path: PathBuf,
    pub name: String,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub max_depth: usize,
    pub skip_hidden: bool,
    pub thread_count: usize,
    pub max_api_tokens: u32,
    pub show_console: bool,
    pub sidebar_width: f32,
    pub treemap_fraction: f32,
    pub table_split: f32,
    pub recent_paths: Vec<PathBuf>,
    pub presets: Vec<QuickPreset>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            max_depth: 20,
            skip_hidden: true,
            thread_count: 8,
            max_api_tokens: 4096,
            show_console: true,
            sidebar_width: 200.0,
            treemap_fraction: 0.55,
            table_split: 0.5,
            recent_paths: vec![],
            presets: default_presets(),
        }
    }
}

pub fn default_presets() -> Vec<QuickPreset> {
    let mut p = vec![];
    if let Some(home) = dirs::home_dir() {
        p.push(QuickPreset { path: home.join("Desktop"),   name: "Desktop".into(),   color: [52, 120, 246] });
        p.push(QuickPreset { path: home.join("Documents"), name: "Documents".into(), color: [255, 149, 0]  });
        p.push(QuickPreset { path: home.join("Downloads"), name: "Downloads".into(), color: [52, 199, 89]  });
        p.push(QuickPreset { path: PathBuf::from("/Applications"), name: "Apps".into(), color: [175, 82, 222] });
    }
    p
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub query: String,
    pub mode: SearchMode,
    pub min_size_mb: Option<f64>,
    pub max_size_mb: Option<f64>,
    pub type_filter: FileTypeFilter,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SearchMode {
    #[default]
    Name,
    Extension,
    Both,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum FileTypeFilter {
    #[default]
    All,
    Images,
    Videos,
    Audio,
    Documents,
    Code,
    Archives,
    Other,
}

impl FileTypeFilter {
    pub fn label(&self) -> &str {
        match self {
            Self::All => "All",
            Self::Images => "Images",
            Self::Videos => "Videos",
            Self::Audio => "Audio",
            Self::Documents => "Docs",
            Self::Code => "Code",
            Self::Archives => "Archives",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Default)]
pub struct ScanState {
    pub status: ScanStatus,
    pub progress: ScanProgress,
    pub result: Option<FileNode>,
    pub scanning_path: Option<PathBuf>,
}

pub type SharedScanState = Arc<Mutex<ScanState>>;

/// Lightweight display node — no children, used for table rendering.
/// Avoids cloning the full FileNode tree every frame.
#[derive(Clone)]
pub struct DisplayNode {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub color: [u8; 3],
    pub extension: Option<String>,
    pub item_count: usize,
    pub is_dir: bool,
}

impl DisplayNode {
    pub fn from_node(n: &FileNode) -> Self {
        Self {
            name: n.name.clone(),
            path: n.path.clone(),
            size: n.size,
            color: n.color,
            extension: n.extension.clone(),
            item_count: n.item_count,
            is_dir: n.kind == NodeKind::Directory,
        }
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", value, UNITS[unit_idx])
    }
}

#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub mount_point: PathBuf,
    pub total_space: u64,
    pub available_space: u64,
}

impl VolumeInfo {
    pub fn used_fraction(&self) -> f32 {
        if self.total_space == 0 {
            0.0
        } else {
            1.0 - (self.available_space as f32 / self.total_space as f32)
        }
    }
}
