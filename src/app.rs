use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Color32, FontId, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

use crate::console_log::ConsoleLog;
use crate::fuzzy;
use crate::scanner;
use crate::settings::{load_settings, save_settings, SettingsWindow};
use crate::sidebar::{Sidebar, SidebarAction};
use crate::treemap::{build_rects, draw_treemap, TreemapRect};
use crate::types::*;

// ── palette ───────────────────────────────────────────────────────────────────
const BG: Color32 = Color32::from_rgb(14, 14, 22);
const BG_HEADER: Color32 = Color32::from_rgb(18, 18, 30);
const ACCENT: Color32 = Color32::from_rgb(76, 175, 80);
const TEXT: Color32 = Color32::from_rgb(205, 205, 215);
const TEXT_SEC: Color32 = Color32::from_rgb(130, 130, 148);
const TEXT_DIM: Color32 = Color32::from_rgb(80, 80, 100);
const HANDLE: Color32 = Color32::from_rgb(40, 40, 58);
const HANDLE_HOV: Color32 = Color32::from_rgb(76, 175, 80);

// ── Context menu ──────────────────────────────────────────────────────────────
struct ContextMenu {
    open: bool,
    just_opened: bool,
    path: PathBuf,
    is_dir: bool,
    pos: Pos2,
    compress_sub: bool,
}
impl Default for ContextMenu {
    fn default() -> Self {
        Self { open: false, just_opened: false, path: PathBuf::new(), is_dir: false, pos: Pos2::ZERO, compress_sub: false }
    }
}

// ── Treemap rect cache ────────────────────────────────────────────────────────
struct RectCache {
    rects: Vec<TreemapRect>,
    path: Option<PathBuf>,
    tree_gen: usize,     // increments each time tree data changes
    viewport: Rect,
}
impl Default for RectCache {
    fn default() -> Self {
        Self { rects: vec![], path: None, tree_gen: usize::MAX, viewport: Rect::ZERO }
    }
}

pub struct MacDirStatApp {
    scan_state: SharedScanState,
    // Arc so cloning is O(1) — no full tree copy every frame
    scan_index: Arc<Mutex<HashMap<PathBuf, Arc<FileNode>>>>,
    current_tree: Option<Arc<FileNode>>,
    tree_gen: usize,

    settings: AppSettings,
    root_path: Option<PathBuf>,
    current_path: Option<PathBuf>,

    path_input: String,
    path_editing: bool,
    // Path autocomplete
    path_suggestions: Vec<PathBuf>,
    path_sugg_idx: Option<usize>,
    path_input_last: String,

    search_query: String,
    // Filtered treemap nodes for search (cached)
    search_nodes: Vec<FileNode>,
    search_cache_key: String,      // (query + current_path serialised)

    sidebar: Sidebar,
    settings_window: SettingsWindow,
    console: ConsoleLog,           // kept for internal logging, NOT shown in UI
    treemap_hovered: Option<PathBuf>,
    context_menu: ContextMenu,
    rect_cache: RectCache,

    scan_logged_complete: bool,
}

impl MacDirStatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_visuals(&cc.egui_ctx);
        let settings = load_settings();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let scan_state: SharedScanState = Arc::new(Mutex::new(ScanState::default()));
        let scan_index: Arc<Mutex<HashMap<PathBuf, Arc<FileNode>>>> = Arc::new(Mutex::new(HashMap::new()));

        let mut app = Self {
            scan_state,
            scan_index,
            current_tree: None,
            tree_gen: 0,
            path_input: home.to_string_lossy().into_owned(),
            path_editing: false,
            path_suggestions: vec![],
            path_sugg_idx: None,
            path_input_last: String::new(),
            search_query: String::new(),
            search_nodes: vec![],
            search_cache_key: String::new(),
            root_path: None,
            current_path: None,
            settings,
            sidebar: Sidebar::new(),
            settings_window: SettingsWindow::new(),
            console: ConsoleLog::new(),
            treemap_hovered: None,
            context_menu: ContextMenu::default(),
            rect_cache: RectCache::default(),
            scan_logged_complete: false,
        };

        app.console.info("MacDirStat started");
        app.start_scan(home);
        app
    }

    fn start_scan(&mut self, path: PathBuf) {
        // Cache hit: load instantly, no scan
        {
            let cache = self.scan_index.lock().unwrap();
            if let Some(tree) = cache.get(&path) {
                self.root_path = Some(path.clone());
                self.current_path = Some(path.clone());
                self.path_input = path.to_string_lossy().into_owned();
                self.current_tree = Some(Arc::clone(tree));
                self.tree_gen += 1;
                self.console.info(format!("Cache hit: {}", path.display()));
                return;
            }
        }

        self.path_input = path.to_string_lossy().into_owned();
        self.root_path = Some(path.clone());
        self.current_path = Some(path.clone());
        self.current_tree = None;
        self.tree_gen += 1;
        self.scan_logged_complete = false;

        // Recents
        self.settings.recent_paths.retain(|p| p != &path);
        self.settings.recent_paths.insert(0, path.clone());
        self.settings.recent_paths.truncate(8);

        self.console.info(format!("Scanning {}", path.display()));
        scanner::start_scan(path, self.scan_state.clone(), self.settings.max_depth, self.settings.skip_hidden);
    }

    // ── Path autocomplete ─────────────────────────────────────────────────────

    fn update_path_suggestions(&mut self) {
        let raw = self.path_input.trim();
        if raw == self.path_input_last { return; }
        self.path_input_last = raw.to_string();
        self.path_suggestions.clear();
        self.path_sugg_idx = None;

        if raw.is_empty() { return; }

        // Expand ~ for filesystem probing
        let expanded = expand_path(raw);
        let (probe_dir, needle): (PathBuf, String) = if expanded.is_dir() {
            (expanded.clone(), String::new())
        } else {
            let parent = expanded.parent().unwrap_or(&expanded).to_path_buf();
            let base = expanded.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (parent, base)
        };

        // 1. Filesystem children of probe_dir, fuzzy-scored against needle
        let mut candidates: Vec<(i32, PathBuf)> = vec![];

        if let Ok(rd) = std::fs::read_dir(&probe_dir) {
            for entry in rd.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let p = entry.path();
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        if name.starts_with('.') { continue; }
                        if needle.is_empty() {
                            candidates.push((0, p));
                        } else if let Some(s) = fuzzy::score(&name, &needle) {
                            candidates.push((s, p));
                        }
                    }
                }
            }
        }

        // 2. Known paths from scan index — bias toward current_path subtree
        {
            if let Ok(cache) = self.scan_index.lock() {
                for known_root in cache.keys() {
                    let ks = known_root.to_string_lossy().into_owned();
                    if let Some(s) = fuzzy::score_path(&ks, raw) {
                        // Boost if under current path
                        let boost = if self.current_path.as_ref()
                            .map(|cp| known_root.starts_with(cp))
                            .unwrap_or(false) { 500 } else { 0 };
                        candidates.push((s + boost, known_root.clone()));
                    }
                }
            }
        }

        // Sort by score desc, dedup, take top 8
        candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        candidates.dedup_by(|a, b| a.1 == b.1);
        self.path_suggestions = candidates.into_iter().take(8).map(|(_, p)| p).collect();
    }

    // ── Treemap search: collect fuzzy-matching descendants ───────────────────

    fn refresh_search_nodes(&mut self) {
        let query = self.search_query.clone();
        let cp = self.current_path.clone().unwrap_or_default();
        let key = format!("{}|{}", query, cp.display());
        if key == self.search_cache_key { return; }
        self.search_cache_key = key;
        self.search_nodes.clear();

        if query.is_empty() { return; }

        let tree = match &self.current_tree {
            Some(t) => Arc::clone(t),
            None => return,
        };
        let node = match cp.as_os_str().is_empty().then_some(tree.as_ref())
            .or_else(|| tree.find_child(&cp))
        {
            Some(n) => n,
            None => return,
        };

        // Collect all descendants with fuzzy score, current-dir children first
        let mut direct: Vec<(i32, FileNode)> = vec![];
        let mut deeper: Vec<(i32, FileNode)> = vec![];

        collect_fuzzy(node, &query, &mut direct, &mut deeper, 0);

        direct.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        deeper.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        self.search_nodes = direct.into_iter().chain(deeper).map(|(_, n)| n).collect();
    }

    fn poll_scan(&mut self) {
        let status = self.scan_state.lock().unwrap().status.clone();
        match &status {
            ScanStatus::Complete if !self.scan_logged_complete => {
                let result = self.scan_state.lock().unwrap().result.take();
                if let Some(tree) = result {
                    let total = tree.size;
                    let items = tree.item_count;
                    let arc = Arc::new(tree);
                    if let Some(ref root) = self.root_path {
                        self.scan_index.lock().unwrap().insert(root.clone(), Arc::clone(&arc));
                    }
                    self.current_tree = Some(arc);
                    self.tree_gen += 1;
                    self.scan_logged_complete = true;
                    self.console.success(format!("Done — {} in {} items", format_size(total), items));
                    let mut s = self.scan_state.lock().unwrap();
                    s.status = ScanStatus::Idle;
                }
            }
            ScanStatus::Error(e) if !self.scan_logged_complete => {
                self.console.error(format!("Scan error: {e}"));
                self.scan_logged_complete = true;
                self.scan_state.lock().unwrap().status = ScanStatus::Idle;
            }
            _ => {}
        }
    }

    // ── Header ────────────────────────────────────────────────────────────────

    fn draw_header(&mut self, ui: &mut egui::Ui) {
        let is_scanning = self.scan_state.lock().unwrap().status == ScanStatus::Scanning;

        // We need the pill rect for the dropdown — capture it here
        let mut pill_bottom_left = Pos2::ZERO;
        let mut pill_width = 0.0f32;

        Frame::none().fill(BG_HEADER).inner_margin(Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("◉").color(ACCENT).size(14.0));
                ui.add_space(6.0);

                let path_w = (ui.available_width() * 0.42).min(420.0);

                if self.path_editing {
                    let frame_resp = Frame::none()
                        .fill(Color32::from_rgb(26, 26, 42))
                        .rounding(Rounding::same(14.0))
                        .inner_margin(Margin::symmetric(10.0, 4.0))
                        .show(ui, |ui| {
                            ui.set_width(path_w);
                            ui.add(egui::TextEdit::singleline(&mut self.path_input)
                                .id(egui::Id::new("path_input"))
                                .desired_width(path_w - 24.0)
                                .font(FontId::monospace(12.0))
                                .text_color(TEXT)
                                .frame(false)
                                .hint_text("~/path or /absolute/path"))
                        });
                    pill_bottom_left = frame_resp.response.rect.left_bottom();
                    pill_width = frame_resp.response.rect.width();
                    let r = frame_resp.inner;

                    // Update suggestions as user types
                    self.update_path_suggestions();

                    // Arrow key navigation through suggestions
                    if !self.path_suggestions.is_empty() {
                        let n = self.path_suggestions.len();
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            self.path_sugg_idx = Some(self.path_sugg_idx.map(|i| (i + 1).min(n - 1)).unwrap_or(0));
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            self.path_sugg_idx = Some(self.path_sugg_idx.map(|i| i.saturating_sub(1)).unwrap_or(0));
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Tab)) {
                            let idx = self.path_sugg_idx.unwrap_or(0);
                            self.path_input = self.path_suggestions[idx].to_string_lossy().into_owned();
                            self.path_input_last.clear(); // force refresh
                            self.update_path_suggestions();
                        }
                    }

                    if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        // Accept selection if Enter pressed
                        let entered = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if entered && !is_scanning {
                            let chosen = self.path_sugg_idx
                                .and_then(|i| self.path_suggestions.get(i))
                                .cloned()
                                .unwrap_or_else(|| expand_path(self.path_input.trim()));
                            self.path_input = chosen.to_string_lossy().into_owned();
                            self.path_sugg_idx = None;
                            self.path_suggestions.clear();
                            self.path_editing = false;
                            self.start_scan(chosen);
                        } else if !entered {
                            self.path_editing = false;
                            self.path_suggestions.clear();
                            self.path_sugg_idx = None;
                        }
                    }
                } else {
                    let crumb = self.current_path.as_ref().map(|cp| {
                        let s = cp.to_string_lossy();
                        if let Some(home) = dirs::home_dir() { s.replacen(&*home.to_string_lossy(), "~", 1) }
                        else { s.into_owned() }
                    }).unwrap_or_else(|| self.path_input.clone());

                    let pill = Frame::none()
                        .fill(Color32::from_rgb(22, 22, 38))
                        .rounding(Rounding::same(14.0))
                        .inner_margin(Margin::symmetric(10.0, 4.0))
                        .show(ui, |ui| {
                            ui.set_min_width(path_w);
                            ui.set_max_width(path_w);
                            ui.horizontal(|ui| {
                                let icon = if is_scanning { "⟳" } else { "⌘" };
                                ui.label(egui::RichText::new(icon).color(if is_scanning { ACCENT } else { TEXT_DIM }).size(11.0));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(&crumb).color(TEXT_SEC).font(FontId::monospace(11.0)));
                                if is_scanning {
                                    let items = self.scan_state.lock().unwrap().progress.items_scanned;
                                    if items > 0 {
                                        ui.add_space(6.0);
                                        ui.label(egui::RichText::new(format!("{} items", items)).color(TEXT_DIM).size(10.0));
                                    }
                                }
                            });
                        });
                    if pill.response.interact(Sense::click()).clicked() && !is_scanning {
                        self.path_input = self.current_path.as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| self.path_input.clone());
                        self.path_input_last.clear();
                        self.path_editing = true;
                    }
                }

                ui.add_space(8.0);

                // ── Search field (right side) ─────────────────────────────────
                let search_w = ui.available_width() - 30.0;
                let search_resp = Frame::none()
                    .fill(Color32::from_rgb(26, 26, 42))
                    .rounding(Rounding::same(14.0))
                    .inner_margin(Margin::symmetric(10.0, 4.0))
                    .show(ui, |ui| {
                        ui.set_width(search_w.max(80.0));
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🔍").size(12.0));
                            ui.add_space(4.0);
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.search_query)
                                    .desired_width(ui.available_width() - 22.0)
                                    .font(FontId::proportional(13.0))
                                    .text_color(TEXT)
                                    .frame(false)
                                    .hint_text(egui::RichText::new("Search files & folders…").color(TEXT_DIM))
                            );
                            if r.changed() { self.search_cache_key.clear(); self.rect_cache.tree_gen = usize::MAX; }
                            if !self.search_query.is_empty() {
                                if ui.add(egui::Label::new(egui::RichText::new("✕").color(TEXT_DIM).size(10.0)).sense(Sense::click())).clicked() {
                                    self.search_query.clear();
                                    self.search_nodes.clear();
                                    self.search_cache_key.clear();
                                    self.rect_cache.tree_gen = usize::MAX;
                                }
                            }
                        });
                    });

                ui.add_space(4.0);
                if ui.add(egui::Label::new(egui::RichText::new("⚙").color(TEXT_DIM).size(14.0)).sense(Sense::click())).clicked() {
                    self.settings_window.open = !self.settings_window.open;
                }
            });
        });

        // ── Path autocomplete dropdown (overlay, below the pill) ─────────────
        if self.path_editing && !self.path_suggestions.is_empty() {
            let suggestions = self.path_suggestions.clone(); // clone to avoid borrow conflict
            let item_h = 26.0;
            let drop_h = suggestions.len() as f32 * item_h + 8.0;
            let drop_rect = Rect::from_min_size(
                pill_bottom_left + Vec2::new(0.0, 2.0),
                Vec2::new(pill_width.max(300.0), drop_h),
            );

            let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("path_drop"));
            let painter = ui.ctx().layer_painter(layer);
            painter.rect_filled(drop_rect, Rounding::same(8.0), Color32::from_rgb(24, 24, 40));
            painter.rect_stroke(drop_rect, Rounding::same(8.0), Stroke::new(1.0, Color32::from_rgb(55, 55, 80)));

            let mut chosen: Option<PathBuf> = None;
            for (i, sug) in suggestions.iter().enumerate() {
                let row = Rect::from_min_size(
                    Pos2::new(drop_rect.min.x + 4.0, drop_rect.min.y + 4.0 + i as f32 * item_h),
                    Vec2::new(drop_rect.width() - 8.0, item_h - 2.0),
                );
                let is_sel = self.path_sugg_idx == Some(i);
                if is_sel {
                    painter.rect_filled(row, Rounding::same(5.0), Color32::from_rgba_premultiplied(76, 175, 80, 50));
                } else {
                    let resp = ui.interact(row, egui::Id::new(("pdrop", i)), Sense::hover());
                    if resp.hovered() {
                        painter.rect_filled(row, Rounding::same(5.0), Color32::from_rgba_premultiplied(255,255,255,12));
                        self.path_sugg_idx = Some(i);
                    }
                }

                // Draw path text — bold basename, dim parent
                let name = sug.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| sug.to_string_lossy().into_owned());
                let parent = sug.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                let parent_short = if parent.len() > 30 { format!("…{}", &parent[parent.len()-30..]) } else { parent };

                painter.text(row.left_center() + Vec2::new(10.0, -4.0), egui::Align2::LEFT_CENTER,
                    &name, FontId::proportional(12.0), TEXT);
                painter.text(row.left_center() + Vec2::new(10.0, 7.0), egui::Align2::LEFT_CENTER,
                    &parent_short, FontId::proportional(9.5), TEXT_DIM);

                // Click selects
                let click = ui.interact(row, egui::Id::new(("pdrop_click", i)), Sense::click());
                if click.clicked() {
                    chosen = Some(sug.clone());
                }
            }
            if let Some(p) = chosen {
                self.path_editing = false;
                self.path_suggestions.clear();
                self.path_sugg_idx = None;
                self.start_scan(p);
            }
        }
    }

    // ── Breadcrumb ────────────────────────────────────────────────────────────

    fn draw_breadcrumb(&mut self, ui: &mut egui::Ui) {
        let (root, current) = match (&self.root_path, &self.current_path) {
            (Some(r), Some(c)) => (r.clone(), c.clone()),
            _ => return,
        };

        let mut parts = vec![];
        let mut p = current.clone();
        loop {
            parts.push(p.clone());
            if p == root || p.parent().is_none() { break; }
            if let Some(par) = p.parent() { p = par.to_path_buf(); } else { break; }
        }
        parts.reverse();

        let mut nav_to: Option<PathBuf> = None;
        Frame::none().fill(BG).inner_margin(Margin::symmetric(10.0, 3.0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, part) in parts.iter().enumerate() {
                    let name = part.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| part.to_string_lossy().into_owned());
                    let is_last = i == parts.len() - 1;
                    let color = if is_last { ACCENT } else { TEXT_SEC };
                    if ui.add(egui::Label::new(
                        egui::RichText::new(&name).color(color).font(FontId::proportional(11.0))
                    ).sense(Sense::click())).clicked() {
                        nav_to = Some(part.clone());
                    }
                    if !is_last {
                        ui.label(egui::RichText::new("  ›  ").color(TEXT_DIM).font(FontId::proportional(10.0)));
                    }
                }
                // Size info right-aligned
                if let Some(ref tree) = self.current_tree {
                    if let Some(node) = tree.find_child(&current) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!(
                                "{} · {} items", format_size(node.size), node.item_count
                            )).color(TEXT_DIM).font(FontId::proportional(10.0)));
                        });
                    }
                }
            });
        });
        if let Some(p) = nav_to {
            self.current_path = Some(p);
        }
    }

    // ── Context menu ──────────────────────────────────────────────────────────

    fn handle_context_menu(&mut self, ui: &mut egui::Ui) {
        if !self.context_menu.open { return; }

        let pos = self.context_menu.pos;
        let path = self.context_menu.path.clone();
        let is_dir = self.context_menu.is_dir;

        // Skip close check on the frame the menu was opened (the right-click itself triggers any_click)
        if self.context_menu.just_opened {
            self.context_menu.just_opened = false;
        } else if ui.input(|i| i.pointer.any_click()) {
            let hover = ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO);
            let menu_h = if self.context_menu.compress_sub { 200.0 } else { 125.0 };
            if !Rect::from_min_size(pos, Vec2::new(180.0, menu_h)).contains(hover) {
                self.context_menu.open = false;
                return;
            }
        }

        let id = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("ctx_menu"));
        let painter = ui.ctx().layer_painter(id);
        let item_h = 28.0;
        let menu_h = if self.context_menu.compress_sub { 200.0 } else { 125.0 };
        let bg = Rect::from_min_size(pos, Vec2::new(180.0, menu_h));
        painter.rect_filled(bg, Rounding::same(8.0), Color32::from_rgb(28, 28, 46));
        painter.rect_stroke(bg, Rounding::same(8.0), Stroke::new(1.0, Color32::from_rgb(55, 55, 80)));

        let mut y = pos.y + 6.0;

        // Trash
        {
            let r = Rect::from_min_size(Pos2::new(pos.x + 4.0, y), Vec2::new(172.0, item_h - 2.0));
            let resp = ui.interact(r, egui::Id::new("ctx_trash"), Sense::click());
            if resp.hovered() { painter.rect_filled(r, Rounding::same(4.0), Color32::from_rgba_premultiplied(255,255,255,15)); }
            painter.text(r.left_center() + Vec2::new(10.0, 0.0), egui::Align2::LEFT_CENTER,
                "🗑  Move to Trash", FontId::proportional(12.0), TEXT);
            if resp.clicked() {
                move_to_trash(&path);
                self.console.info(format!("Trashed: {}", path.display()));
                self.context_menu.open = false;
                // Invalidate cache
                if let Some(ref root) = self.root_path { self.scan_index.lock().unwrap().remove(root); }
            }
            y += item_h;
        }

        // Delete perm
        {
            let r = Rect::from_min_size(Pos2::new(pos.x + 4.0, y), Vec2::new(172.0, item_h - 2.0));
            let resp = ui.interact(r, egui::Id::new("ctx_del"), Sense::click());
            if resp.hovered() { painter.rect_filled(r, Rounding::same(4.0), Color32::from_rgba_premultiplied(255,80,80,20)); }
            painter.text(r.left_center() + Vec2::new(10.0, 0.0), egui::Align2::LEFT_CENTER,
                "✕  Delete Permanently", FontId::proportional(12.0), Color32::from_rgb(255, 80, 80));
            if resp.clicked() {
                if is_dir { let _ = std::fs::remove_dir_all(&path); } else { let _ = std::fs::remove_file(&path); }
                self.console.warn(format!("Deleted: {}", path.display()));
                self.context_menu.open = false;
                if let Some(ref root) = self.root_path { self.scan_index.lock().unwrap().remove(root); }
            }
            y += item_h;
        }

        // Separator
        painter.line_segment([Pos2::new(pos.x + 8.0, y), Pos2::new(pos.x + 172.0, y)],
            Stroke::new(1.0, Color32::from_rgb(45, 45, 68)));
        y += 5.0;

        // Compress submenu
        {
            let r = Rect::from_min_size(Pos2::new(pos.x + 4.0, y), Vec2::new(172.0, item_h - 2.0));
            let resp = ui.interact(r, egui::Id::new("ctx_compress"), Sense::click());
            if resp.hovered() { painter.rect_filled(r, Rounding::same(4.0), Color32::from_rgba_premultiplied(255,255,255,15)); }
            painter.text(r.left_center() + Vec2::new(10.0, 0.0), egui::Align2::LEFT_CENTER,
                "⇲  Compress…", FontId::proportional(12.0), TEXT);
            painter.text(r.right_center() - Vec2::new(10.0, 0.0), egui::Align2::RIGHT_CENTER,
                "▸", FontId::proportional(10.0), TEXT_DIM);
            if resp.clicked() { self.context_menu.compress_sub = !self.context_menu.compress_sub; }
            y += item_h;
        }

        if self.context_menu.compress_sub {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let already = ["zip","gz","bz2","7z","rar","xz"].contains(&ext.as_str());
            let fmts: &[(&str, &str)] = if already {
                &[("  (already compressed)", "")]
            } else if is_dir {
                &[("  zip", "zip"), ("  tar.gz", "tar.gz"), ("  tar.bz2", "tar.bz2")]
            } else {
                &[("  zip", "zip"), ("  gzip", "gz"), ("  bzip2", "bz2")]
            };

            for &(label, fmt) in fmts {
                let r = Rect::from_min_size(Pos2::new(pos.x + 4.0, y), Vec2::new(172.0, item_h - 2.0));
                let resp = ui.interact(r, egui::Id::new((label, "cfmt")), Sense::click());
                if resp.hovered() { painter.rect_filled(r, Rounding::same(4.0), Color32::from_rgba_premultiplied(255,255,255,15)); }
                painter.text(r.left_center() + Vec2::new(20.0, 0.0), egui::Align2::LEFT_CENTER,
                    label, FontId::proportional(11.5), TEXT_SEC);
                if resp.clicked() && !fmt.is_empty() {
                    compress_path(&path, fmt, &mut self.console);
                    self.context_menu.open = false;
                }
                y += item_h;
            }
        }
    }
}

/// Recursively collect fuzzy-matching nodes. depth=0 → direct children (prioritised).
fn collect_fuzzy(
    node: &FileNode,
    query: &str,
    direct: &mut Vec<(i32, FileNode)>,
    deeper: &mut Vec<(i32, FileNode)>,
    depth: usize,
) {
    for child in &node.children {
        if let Some(score) = fuzzy::score(&child.name, query) {
            // Return a shallow clone (no grandchildren) for display purposes
            let display = FileNode {
                name: child.name.clone(),
                path: child.path.clone(),
                size: child.size,
                kind: child.kind.clone(),
                extension: child.extension.clone(),
                color: child.color,
                children: vec![],
                item_count: child.item_count,
            };
            if depth == 0 { direct.push((score, display)); }
            else          { deeper.push((score, display)); }
        }
        if child.kind == NodeKind::Directory {
            collect_fuzzy(child, query, direct, deeper, depth + 1);
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for MacDirStatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();

        // Only repaint while scanning — hover repaints are handled by egui automatically
        if self.scan_state.lock().unwrap().status == ScanStatus::Scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }

        self.settings_window.show(ctx, &mut self.settings);

        // Sidebar
        let action = egui::SidePanel::left("sidebar")
            .default_width(self.settings.sidebar_width)
            .width_range(140.0..=360.0)
            .resizable(true)
            .frame(Frame::none().fill(Color32::from_rgb(16, 16, 28)))
            .show(ctx, |ui| self.sidebar.show(ui, &mut self.settings, &self.current_path))
            .inner;

        if let SidebarAction::Scan(p) = action {
            self.start_scan(p);
        }

        // Main panel — treemap fills everything
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG))
            .show(ctx, |ui| {
                self.draw_header(ui);
                self.draw_breadcrumb(ui);

                let is_scanning = self.scan_state.lock().unwrap().status == ScanStatus::Scanning;

                // Treemap fills entire remaining area
                let available = ui.available_rect_before_wrap();

                if is_scanning && self.current_tree.is_none() {
                    // Loading state
                    ui.allocate_rect(available, Sense::hover());
                    let p = ui.painter_at(available);
                    p.rect_filled(available, Rounding::ZERO, Color32::from_rgb(12, 12, 20));
                    let prog = self.scan_state.lock().unwrap().progress.clone();
                    let msg = if prog.items_scanned == 0 {
                        "Please wait — building index…".to_string()
                    } else {
                        format!("Scanning… {} items · {}", prog.items_scanned, format_size(prog.total_size))
                    };
                    p.text(available.center(), egui::Align2::CENTER_CENTER,
                        &msg, FontId::proportional(15.0), ACCENT);
                } else {
                    // Update search nodes (no-op if cache key unchanged)
                    self.refresh_search_nodes();

                    // Build rects — cached: only rebuild if path/tree/viewport/search changed
                    let current = self.current_path.clone();
                    let search_key = self.search_cache_key.clone();
                    let need_rebuild = self.rect_cache.path != current
                        || self.rect_cache.tree_gen != self.tree_gen
                        || (self.rect_cache.viewport.size() - available.size()).length() > 1.0;

                    if need_rebuild {
                        self.rect_cache.rects = if !self.search_query.is_empty() && !self.search_nodes.is_empty() {
                            // Search mode: flat view of all fuzzy matches
                            let refs: Vec<&FileNode> = self.search_nodes.iter().collect();
                            let mut out = Vec::with_capacity(refs.len());
                            crate::treemap::squarify_pub(&refs, available.min.x, available.min.y, available.width(), available.height(), &mut out);
                            out
                        } else {
                            self.current_tree.as_ref()
                                .and_then(|t| current.as_ref().and_then(|cp| t.find_child(cp)).or(Some(t.as_ref())))
                                .map(|node| build_rects(node, available))
                                .unwrap_or_default()
                        };
                        self.rect_cache.path = current.clone();
                        self.rect_cache.tree_gen = self.tree_gen;
                        self.rect_cache.viewport = available;
                    }

                    // Draw (O(n) paint, no allocation)
                    let action = draw_treemap(ui, &self.rect_cache.rects, &mut self.treemap_hovered);
                    if let Some(nav) = action.navigate {
                        self.current_path = Some(nav);
                    }
                    if let Some((path, is_dir, pos)) = action.right_click {
                        self.context_menu = ContextMenu {
                            open: true,
                            just_opened: true,
                            path,
                            is_dir,
                            pos,
                            compress_sub: false,
                        };
                    }
                }

                self.handle_context_menu(ui);
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        save_settings(&self.settings);
        self.console.info("Exiting");
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = Color32::from_rgb(20, 20, 34);
    v.extreme_bg_color = Color32::from_rgb(10, 10, 18);
    v.selection.bg_fill = Color32::from_rgba_premultiplied(76, 175, 80, 70);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SEC);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.5, ACCENT);
    v.widgets.inactive.bg_fill = Color32::from_rgb(30, 30, 50);
    v.widgets.hovered.bg_fill = Color32::from_rgb(40, 40, 62);
    v.widgets.active.bg_fill = Color32::from_rgb(55, 110, 60);
    v.window_rounding = Rounding::same(8.0);
    ctx.set_visuals(v);
}

fn expand_path(raw: &str) -> PathBuf {
    if raw.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(raw.trim_start_matches("~/"));
        }
    }
    PathBuf::from(raw)
}

fn move_to_trash(path: &PathBuf) {
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(r#"tell application "Finder" to delete POSIX file "{}""#, path.display()))
        .output();
}

fn compress_path(path: &PathBuf, fmt: &str, console: &mut ConsoleLog) {
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "archive".into());
    let out = parent.join(format!("{}.{}", stem, fmt));

    let result = match fmt {
        "zip" if path.is_dir() => std::process::Command::new("zip").arg("-r").arg(&out).arg(path).current_dir(parent).output(),
        "zip"                  => std::process::Command::new("zip").arg(&out).arg(path).current_dir(parent).output(),
        "tar.gz"  => std::process::Command::new("tar").args(["-czf", out.to_str().unwrap_or(""), &path.to_string_lossy()]).current_dir(parent).output(),
        "tar.bz2" => std::process::Command::new("tar").args(["-cjf", out.to_str().unwrap_or(""), &path.to_string_lossy()]).current_dir(parent).output(),
        "gz"  => std::process::Command::new("gzip").arg("-k").arg(path).output(),
        "bz2" => std::process::Command::new("bzip2").arg("-k").arg(path).output(),
        _ => { console.warn(format!("Unknown format: {fmt}")); return; }
    };

    match result {
        Ok(o) if o.status.success() => console.success(format!("Compressed → {}.{}", stem, fmt)),
        Ok(o) => console.error(format!("Compress failed: {}", String::from_utf8_lossy(&o.stderr))),
        Err(e) => console.error(format!("Compress error: {e}")),
    }
}
