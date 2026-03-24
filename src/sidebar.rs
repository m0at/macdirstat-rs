use std::path::PathBuf;
use std::time::Instant;

use egui::{Color32, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Ui, Vec2};
use sysinfo::Disks;

use crate::types::{AppSettings, QuickPreset, VolumeInfo, format_size, default_presets};

// ── palette ───────────────────────────────────────────────────────────────────
const BG: Color32 = Color32::from_rgb(16, 16, 28);
const BG_ITEM_HOVER: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 14);
const BG_ITEM_SEL: Color32 = Color32::from_rgba_premultiplied(76, 175, 80, 50);
const ACCENT: Color32 = Color32::from_rgb(76, 175, 80);
const TEXT: Color32 = Color32::from_rgb(200, 200, 210);
const TEXT_DIM: Color32 = Color32::from_rgb(100, 100, 120);
const TEXT_SEC: Color32 = Color32::from_rgb(140, 140, 160);
const SEP: Color32 = Color32::from_rgb(35, 35, 55);
const USAGE_GREEN: Color32 = Color32::from_rgb(76, 175, 80);
const USAGE_YELLOW: Color32 = Color32::from_rgb(255, 193, 7);
const USAGE_RED: Color32 = Color32::from_rgb(244, 67, 54);

// ── PRESET COLOR SWATCHES for right-click picker ──────────────────────────────
const SWATCH_COLORS: &[[u8; 3]] = &[
    [52, 120, 246],  // blue
    [255, 149, 0],   // orange
    [52, 199, 89],   // green
    [175, 82, 222],  // purple
    [255, 59, 48],   // red
    [90, 200, 250],  // cyan
    [255, 214, 10],  // yellow
    [142, 142, 147], // gray
    [255, 45, 85],   // pink
    [48, 209, 88],   // mint
    [100, 210, 255], // sky
    [255, 100, 130], // coral
];

pub enum SidebarAction {
    None,
    Scan(PathBuf),
}

pub struct Sidebar {
    pub volumes: Vec<VolumeInfo>,
    volumes_last_refresh: Instant,
    // right-click state for preset color picker
    color_picker_open: Option<usize>,
    // drives section collapsed
    drives_open: bool,
    // pending new preset path input
    adding_preset: bool,
    new_preset_input: String,
}

impl Sidebar {
    pub fn new() -> Self {
        let mut s = Self {
            volumes: vec![],
            volumes_last_refresh: Instant::now(),
            color_picker_open: None,
            drives_open: true,
            adding_preset: false,
            new_preset_input: String::new(),
        };
        s.refresh_volumes();
        s
    }

    pub fn refresh_volumes(&mut self) {
        let disks = Disks::new_with_refreshed_list();
        self.volumes = disks.iter().filter(|d| d.total_space() > 0).map(|d| VolumeInfo {
            name: d.name().to_string_lossy().into_owned(),
            mount_point: d.mount_point().to_path_buf(),
            total_space: d.total_space(),
            available_space: d.available_space(),
        }).collect();
        self.volumes_last_refresh = Instant::now();
    }

    pub fn show(&mut self, ui: &mut Ui, settings: &mut AppSettings, current_path: &Option<PathBuf>) -> SidebarAction {
        let mut action = SidebarAction::None;

        // Refresh volumes every 30s
        if self.volumes_last_refresh.elapsed().as_secs() > 30 {
            self.refresh_volumes();
        }

        let full_rect = ui.max_rect();
        ui.painter().rect_filled(full_rect, Rounding::ZERO, BG);

        // ── TOP: Quick preset grid ────────────────────────────────────────────
        ui.add_space(8.0);
        action = self.draw_preset_grid(ui, settings, current_path, action);

        // ── MIDDLE: Drives ────────────────────────────────────────────────────
        ui.add_space(6.0);
        draw_sep(ui);
        action = self.draw_drives(ui, current_path, settings, action);

        // ── BOTTOM: Recents (stick to bottom, scroll bottom-up) ───────────────
        draw_sep(ui);
        action = self.draw_recents(ui, settings, current_path, action);

        action
    }

    // ── Quick preset grid ─────────────────────────────────────────────────────

    fn draw_preset_grid(&mut self, ui: &mut Ui, settings: &mut AppSettings, current_path: &Option<PathBuf>, mut action: SidebarAction) -> SidebarAction {
        const PAD: f32 = 8.0;
        const GAP: f32 = 6.0;
        const TILE_H: f32 = 50.0;
        const COLS: usize = 2;

        let avail_w = ui.available_width();
        let tile_w = ((avail_w - PAD * 2.0 - GAP) / COLS as f32).max(30.0);

        // Total cells = presets + 1 for the + button
        let n_total = settings.presets.len() + 1;
        let rows = (n_total + COLS - 1) / COLS;
        let grid_h = rows as f32 * TILE_H + (rows.saturating_sub(1)) as f32 * GAP + PAD * 2.0;

        // Reserve space for the whole grid in one allocation so egui's layout
        // cursor advances correctly — then paint everything manually.
        let (base, _) = ui.allocate_exact_size(Vec2::new(avail_w, grid_h), Sense::hover());

        let origin = Pos2::new(base.min.x + PAD, base.min.y + PAD);

        let mut scan_path: Option<PathBuf> = None;
        let mut color_change: Option<(usize, [u8; 3])> = None;
        let mut open_color_for: Option<usize> = None;
        let mut color_picker_tile_rect = Rect::ZERO;

        let n_presets = settings.presets.len();

        for i in 0..n_total {
            let col = (i % COLS) as f32;
            let row = (i / COLS) as f32;
            let x = origin.x + col * (tile_w + GAP);
            let y = origin.y + row * (TILE_H + GAP);
            let tile = Rect::from_min_size(Pos2::new(x, y), Vec2::new(tile_w, TILE_H));

            if i < n_presets {
                let preset = &settings.presets[i];
                let is_sel = current_path.as_ref().map(|p| p == &preset.path).unwrap_or(false);
                let [r, g, b] = preset.color;

                let resp = ui.interact(tile, ui.id().with(("tile", i)), Sense::click());
                let alpha: u8 = if resp.hovered() { 230 } else { 185 };

                ui.painter().rect_filled(tile, Rounding::same(8.0),
                    Color32::from_rgba_premultiplied(r, g, b, alpha));
                if is_sel {
                    ui.painter().rect_stroke(tile, Rounding::same(8.0), Stroke::new(2.0, Color32::WHITE));
                }
                let max_chars = ((tile_w / 7.0) as usize).max(3);
                ui.painter().text(tile.center(), egui::Align2::CENTER_CENTER,
                    &truncate(&preset.name, max_chars),
                    egui::FontId::proportional(11.5), Color32::WHITE);

                if resp.clicked() { scan_path = Some(preset.path.clone()); }
                if resp.secondary_clicked() {
                    if self.color_picker_open == Some(i) {
                        self.color_picker_open = None;
                    } else {
                        open_color_for = Some(i);
                        color_picker_tile_rect = tile;
                    }
                }
            } else {
                // + button
                let resp = ui.interact(tile, ui.id().with("tile_plus"), Sense::click());
                let bg = if resp.hovered() { Color32::from_rgb(48, 48, 68) } else { Color32::from_rgb(30, 30, 48) };
                ui.painter().rect_filled(tile, Rounding::same(8.0), bg);
                ui.painter().text(tile.center(), egui::Align2::CENTER_CENTER,
                    "+", egui::FontId::proportional(20.0), TEXT_DIM);
                if resp.clicked() { self.adding_preset = true; }
            }
        }

        // Apply open_color_for after the loop (avoids double-borrow)
        if let Some(idx) = open_color_for {
            self.color_picker_open = Some(idx);
        }

        // Color picker popup (painted on Tooltip layer)
        if let Some(idx) = self.color_picker_open {
            if idx < n_presets {
                let col = (idx % COLS) as f32;
                let row = (idx / COLS) as f32;
                let tx = origin.x + col * (tile_w + GAP);
                let ty = origin.y + row * (TILE_H + GAP);
                let tile_bottom = Pos2::new(tx, ty + TILE_H + 4.0);

                const SW: f32 = 20.0;
                const SG: f32 = 4.0;
                const SCOLS: usize = 6;
                let pop_w = SCOLS as f32 * SW + (SCOLS - 1) as f32 * SG + 16.0;
                let pop_rows = (SWATCH_COLORS.len() + SCOLS - 1) / SCOLS;
                let pop_h = pop_rows as f32 * SW + (pop_rows - 1) as f32 * SG + 16.0;
                let pop = Rect::from_min_size(tile_bottom, Vec2::new(pop_w, pop_h));

                let layer = egui::LayerId::new(egui::Order::Tooltip, ui.id().with(("cpop", idx)));
                let p = ui.ctx().layer_painter(layer);
                p.rect_filled(pop, Rounding::same(6.0), Color32::from_rgb(28, 28, 46));
                p.rect_stroke(pop, Rounding::same(6.0), Stroke::new(1.0, Color32::from_rgb(55, 55, 80)));

                let start = pop.min + Vec2::new(8.0, 8.0);
                let mut clicked_swatch: Option<[u8; 3]> = None;
                let mut close_picker = false;

                for (ci, &swatch) in SWATCH_COLORS.iter().enumerate() {
                    let sc = ci % SCOLS;
                    let sr = ci / SCOLS;
                    let sx = start.x + sc as f32 * (SW + SG);
                    let sy = start.y + sr as f32 * (SW + SG);
                    let sr_rect = Rect::from_min_size(Pos2::new(sx, sy), Vec2::splat(SW));
                    p.rect_filled(sr_rect, Rounding::same(4.0), Color32::from_rgb(swatch[0], swatch[1], swatch[2]));
                    let r = ui.interact(sr_rect, ui.id().with(("sw", idx, ci)), Sense::click());
                    if r.clicked() { clicked_swatch = Some(swatch); close_picker = true; }
                }

                // Close picker if clicking outside
                if ui.input(|i| i.pointer.any_click()) {
                    let hover = ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO);
                    if !pop.contains(hover) { close_picker = true; }
                }

                if let Some(c) = clicked_swatch { color_change = Some((idx, c)); }
                if close_picker { self.color_picker_open = None; }
            }
        }

        // Apply mutations
        if let Some(p) = scan_path { action = SidebarAction::Scan(p); }
        if let Some((idx, color)) = color_change {
            if idx < settings.presets.len() { settings.presets[idx].color = color; }
        }

        // "Add preset" inline input below the grid
        if self.adding_preset {
            Frame::none().fill(Color32::from_rgb(26, 26, 42))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(avail_w - PAD * 2.0);
                    let te = egui::TextEdit::singleline(&mut self.new_preset_input)
                        .desired_width(ui.available_width())
                        .hint_text("~/path or /absolute/path")
                        .font(egui::FontId::monospace(11.0));
                    let r = ui.add(te);
                    r.request_focus();

                    let commit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));

                    if commit {
                        let raw = self.new_preset_input.trim().to_string();
                        if !raw.is_empty() {
                            let p = if raw.starts_with('~') {
                                dirs::home_dir().map(|h| h.join(raw.trim_start_matches("~/")))
                                    .unwrap_or_else(|| PathBuf::from(&raw))
                            } else { PathBuf::from(&raw) };
                            let name = p.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| raw.clone());
                            let ci = settings.presets.len() % SWATCH_COLORS.len();
                            settings.presets.push(QuickPreset { path: p, name, color: SWATCH_COLORS[ci] });
                        }
                        self.new_preset_input.clear();
                        self.adding_preset = false;
                    }
                    if cancel { self.adding_preset = false; self.new_preset_input.clear(); }
                });
        }

        action
    }

    // ── Drives ────────────────────────────────────────────────────────────────

    fn draw_drives(&mut self, ui: &mut Ui, current_path: &Option<PathBuf>, settings: &AppSettings, mut action: SidebarAction) -> SidebarAction {
        // Tiny header: "DRIVES" + collapse triangle + refresh
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let tri = if self.drives_open { "▾" } else { "▸" };
            if ui.add(egui::Label::new(
                egui::RichText::new(format!("{} DRIVES", tri)).color(TEXT_DIM).size(9.0)
            ).sense(Sense::click())).clicked() {
                self.drives_open = !self.drives_open;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let r = ui.add(egui::Label::new(
                    egui::RichText::new("↻").color(TEXT_DIM).size(11.0)
                ).sense(Sense::click()));
                if r.clicked() { self.refresh_volumes(); }
            });
        });
        ui.add_space(3.0);

        if !self.drives_open { return action; }

        for vol in &self.volumes {
            let sel = current_path.as_ref().map(|p| p == &vol.mount_point).unwrap_or(false);
            let vol_path = vol.mount_point.clone();
            let frac = vol.used_fraction();

            let avail_w = ui.available_width();
            let (item_rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, 42.0), Sense::click());

            if ui.is_rect_visible(item_rect) {
                let bg = if sel { BG_ITEM_SEL } else if resp.hovered() { BG_ITEM_HOVER } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(item_rect.shrink(2.0), Rounding::same(4.0), bg);

                let inner = item_rect.shrink2(Vec2::new(10.0, 4.0));

                // Volume display name
                let display = if vol.name.is_empty() || vol.name == "/" {
                    vol.mount_point.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Macintosh HD".into())
                } else { vol.name.clone() };

                let nc = if sel { ACCENT } else { TEXT };
                ui.painter().text(inner.left_top() + Vec2::new(0.0, 2.0), egui::Align2::LEFT_TOP,
                    &display, egui::FontId::proportional(12.0), nc);

                // Free space right-aligned
                let free = format!("{} free", format_size(vol.available_space));
                ui.painter().text(inner.right_top() + Vec2::new(0.0, 2.0), egui::Align2::RIGHT_TOP,
                    &free, egui::FontId::proportional(9.0), TEXT_DIM);

                // Usage bar
                let bar_y = inner.min.y + 20.0;
                let bar_rect = Rect::from_min_size(Pos2::new(inner.min.x, bar_y), Vec2::new(inner.width(), 4.0));
                ui.painter().rect_filled(bar_rect, Rounding::same(2.0), Color32::from_rgb(40,40,60));
                let bar_col = if frac > 0.85 { USAGE_RED } else if frac > 0.65 { USAGE_YELLOW } else { USAGE_GREEN };
                let fill_w = (bar_rect.width() * frac).max(0.0);
                if fill_w > 0.0 {
                    ui.painter().rect_filled(
                        Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, 4.0)),
                        Rounding::same(2.0), bar_col);
                }

                // Total size dim
                ui.painter().text(inner.left_top() + Vec2::new(0.0, 28.0), egui::Align2::LEFT_TOP,
                    &format!("{} total", format_size(vol.total_space)),
                    egui::FontId::proportional(9.0), TEXT_DIM);
            }

            if resp.clicked() { action = SidebarAction::Scan(vol_path); }
        }
        action
    }

    // ── Recents (bottom, newest at bottom, scroll bottom-up) ─────────────────

    fn draw_recents(&self, ui: &mut Ui, settings: &mut AppSettings, current_path: &Option<PathBuf>, mut action: SidebarAction) -> SidebarAction {
        // Tiny header
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("RECENTS").color(TEXT_DIM).size(9.0));
        });
        ui.add_space(2.0);

        let avail_h = ui.available_height().max(60.0);
        let mut to_remove: Option<PathBuf> = None;
        let mut scan_path: Option<PathBuf> = None;

        // Items ordered oldest→newest so newest appears at bottom
        let paths: Vec<PathBuf> = settings.recent_paths.iter().cloned().collect();

        egui::ScrollArea::vertical()
            .id_salt("recents_scroll")
            .max_height(avail_h)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if paths.is_empty() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("No recent paths").color(TEXT_DIM).size(10.0));
                    });
                    return;
                }

                for path in &paths {
                    let sel = current_path.as_ref() == Some(path);
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    let parent = path.parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();

                    let avail_w = ui.available_width();
                    let (item_rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, 32.0), Sense::click());

                    if ui.is_rect_visible(item_rect) {
                        let bg = if sel { BG_ITEM_SEL } else if resp.hovered() { BG_ITEM_HOVER } else { Color32::TRANSPARENT };
                        ui.painter().rect_filled(item_rect, Rounding::ZERO, bg);

                        let inner = item_rect.shrink2(Vec2::new(10.0, 3.0));

                        // Folder icon + name
                        ui.painter().text(inner.left_center() + Vec2::new(0.0, -6.0),
                            egui::Align2::LEFT_CENTER, "📁",
                            egui::FontId::proportional(11.0), TEXT_SEC);

                        let nc = if sel { ACCENT } else { TEXT };
                        ui.painter().text(inner.left_top() + Vec2::new(18.0, 2.0),
                            egui::Align2::LEFT_TOP, &truncate(&name, 20),
                            egui::FontId::proportional(11.5), nc);

                        // Parent path dim
                        let parent_short = if parent.len() > 22 {
                            format!("…{}", &parent[parent.len()-22..])
                        } else { parent.clone() };
                        ui.painter().text(inner.left_top() + Vec2::new(18.0, 18.0),
                            egui::Align2::LEFT_TOP, &parent_short,
                            egui::FontId::proportional(9.0), TEXT_DIM);

                        // × remove button
                        let x_rect = Rect::from_center_size(inner.right_center(), Vec2::splat(14.0));
                        let xr = ui.interact(x_rect, ui.id().with(("recent_x", path)), Sense::click());
                        let xc = if xr.hovered() { TEXT } else { TEXT_DIM };
                        ui.painter().text(x_rect.center(), egui::Align2::CENTER_CENTER, "✕",
                            egui::FontId::proportional(9.0), xc);

                        if xr.clicked() {
                            to_remove = Some(path.clone());
                        } else if resp.clicked() {
                            scan_path = Some(path.clone());
                        }
                    }
                }
            });

        if let Some(p) = to_remove { settings.recent_paths.retain(|r| r != &p); }
        if let Some(p) = scan_path { action = SidebarAction::Scan(p); }
        action
    }
}

impl Default for Sidebar {
    fn default() -> Self { Self::new() }
}

fn draw_sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, Rounding::ZERO, SEP);
    ui.add_space(4.0);
}

fn truncate(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n { return s.to_owned(); }
    if n <= 1 { return chars[..n].iter().collect(); }
    let mut out: String = chars[..n-1].iter().collect();
    out.push('…');
    out
}
