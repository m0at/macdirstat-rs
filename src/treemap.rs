use std::path::PathBuf;
use egui::{Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use crate::types::{FileNode, NodeKind, format_size};

pub struct TreemapAction {
    pub navigate: Option<PathBuf>,
    pub right_click: Option<(PathBuf, bool, Pos2)>, // (path, is_dir, screen_pos)
}

pub struct TreemapRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub node_path: PathBuf,
    pub color: Color32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

impl TreemapRect {
    pub fn egui_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::new(self.x, self.y), Vec2::new(self.w, self.h))
    }
}

// ── Squarified treemap algorithm ────────────────────────────────────────────
//
// Key fix: compute row aspect ratio as max(strip_len/item_h, item_h/strip_len)
// where strip_len = sum(row_areas) / side
// and item_h = side * area_i / sum(row_areas)
// Never call with item_size=0 to avoid infinity.

fn row_aspect(areas: &[f64], side: f64) -> f64 {
    if areas.is_empty() || side <= 0.0 { return f64::MAX; }
    let sum: f64 = areas.iter().sum();
    if sum <= 0.0 { return f64::MAX; }
    // strip extends `strip_len = sum/side` into the long dimension
    // each item i: dims are strip_len × (side * area_i / sum)
    let strip_len = sum / side;
    areas.iter().fold(0.0_f64, |worst, &a| {
        if a <= 0.0 { return worst; }
        let item_h = side * a / sum;
        if item_h <= 0.0 { return worst; }
        let r = if strip_len > item_h { strip_len / item_h } else { item_h / strip_len };
        worst.max(r)
    })
}

fn squarify_inner(items: &[&FileNode], rx: f32, ry: f32, rw: f32, rh: f32, out: &mut Vec<TreemapRect>) {
    if items.is_empty() || rw < 1.0 || rh < 1.0 { return; }

    let area = rw as f64 * rh as f64;
    let side = rw.min(rh) as f64;
    let total: f64 = items.iter().map(|n| n.size as f64).sum();
    if total <= 0.0 { return; }

    // Normalize item areas to fill this rect
    let norm: Vec<f64> = items.iter().map(|n| n.size as f64 / total * area).collect();

    // Greedy: grow the row until adding the next item worsens aspect ratio
    let mut split = 1;
    let mut cur = row_aspect(&norm[..1], side);
    while split < items.len() {
        let next = row_aspect(&norm[..split + 1], side);
        if next > cur { break; }
        cur = next;
        split += 1;
    }

    // Layout this strip
    let row_sum: f64 = norm[..split].iter().sum();
    if rw >= rh {
        // vertical strip along left edge: fixed width, items stacked top→bottom
        let strip_w = (row_sum / rh as f64) as f32;
        let mut cy = ry;
        for i in 0..split {
            let cell_h = (rh as f64 * norm[i] / row_sum) as f32;
            push_rect(items[i], rx, cy, strip_w, cell_h, out);
            cy += cell_h;
        }
        // recurse on remaining rect (to the right)
        let remaining_w = (rw - strip_w).max(0.0);
        squarify_inner(&items[split..], rx + strip_w, ry, remaining_w, rh, out);
    } else {
        // horizontal strip along top edge: fixed height, items side by side
        let strip_h = (row_sum / rw as f64) as f32;
        let mut cx = rx;
        for i in 0..split {
            let cell_w = (rw as f64 * norm[i] / row_sum) as f32;
            push_rect(items[i], cx, ry, cell_w, strip_h, out);
            cx += cell_w;
        }
        // recurse on remaining rect (below)
        let remaining_h = (rh - strip_h).max(0.0);
        squarify_inner(&items[split..], rx, ry + strip_h, rw, remaining_h, out);
    }
}

fn push_rect(node: &FileNode, x: f32, y: f32, w: f32, h: f32, out: &mut Vec<TreemapRect>) {
    let [r, g, b] = node.color;
    out.push(TreemapRect {
        x, y, w, h,
        node_path: node.path.clone(),
        color: Color32::from_rgb(r, g, b),
        name: node.name.clone(),
        size: node.size,
        is_dir: node.kind == NodeKind::Directory,
    });
}

/// Public entry point used when caller supplies pre-filtered items (e.g. search results).
pub fn squarify_pub(items: &[&FileNode], x: f32, y: f32, w: f32, h: f32, out: &mut Vec<TreemapRect>) {
    let mut sorted: Vec<&FileNode> = items.iter().copied().filter(|n| n.size > 0).collect();
    sorted.sort_by(|a, b| b.size.cmp(&a.size));
    squarify_inner(&sorted, x, y, w, h, out);
}

pub fn build_rects(node: &FileNode, rect: Rect) -> Vec<TreemapRect> {
    let mut children: Vec<&FileNode> = node.children.iter().filter(|c| c.size > 0).collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
    let mut out = Vec::with_capacity(children.len());
    squarify_inner(&children, rect.min.x, rect.min.y, rect.width(), rect.height(), &mut out);
    out
}

// ── Drawing ──────────────────────────────────────────────────────────────────

const GAP_MAX: f32 = 1.5;
const GAP_MIN: f32 = 0.3;
const BG: Color32 = Color32::from_rgb(12, 12, 20);

fn lighten(c: Color32, amt: u8) -> Color32 {
    Color32::from_rgb(c.r().saturating_add(amt), c.g().saturating_add(amt), c.b().saturating_add(amt))
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars <= 1 { return s.to_owned(); }
    let mut out: String = chars[..max_chars - 1].iter().collect();
    out.push('…');
    out
}

pub fn draw_treemap(ui: &mut Ui, rects: &[TreemapRect], hovered_path: &mut Option<PathBuf>) -> TreemapAction {
    let available = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(available, Sense::click());
    let painter = ui.painter_at(available);
    painter.rect_filled(available, 0.0, BG);

    if rects.is_empty() {
        painter.text(available.center(), egui::Align2::CENTER_CENTER,
            "No data", FontId::proportional(16.0), Color32::from_rgb(80, 80, 100));
        return TreemapAction { navigate: None, right_click: None };
    }

    let hover_pos = response.hover_pos();
    let hovered_idx = hover_pos.and_then(|pos| {
        rects.iter().enumerate().find_map(|(i, tr)| {
            if tr.egui_rect().contains(pos) { Some(i) } else { None }
        })
    });
    *hovered_path = hovered_idx.map(|i| rects[i].node_path.clone());

    let font_sm = FontId::proportional(10.0);
    let font_md = FontId::proportional(11.5);

    for (i, tr) in rects.iter().enumerate() {
        let rect = tr.egui_rect();
        if !rect.intersects(available) { continue; }
        let hovered = hovered_idx == Some(i);

        // Proportional gap: scales with the smaller dimension of the block
        let min_dim = rect.width().min(rect.height());
        let gap = (min_dim * 0.06).clamp(GAP_MIN, GAP_MAX);
        let inner = Rect::from_min_size(
            Pos2::new(rect.min.x + gap, rect.min.y + gap),
            Vec2::new((rect.width() - gap * 2.0).max(0.0), (rect.height() - gap * 2.0).max(0.0)),
        );
        if inner.width() < 1.0 || inner.height() < 1.0 { continue; }

        painter.rect_filled(inner, 0.0, tr.color);

        // Directory: subtle lighter header strip
        if tr.is_dir && inner.height() > 8.0 {
            let hh = (inner.height() * 0.1).min(8.0).max(2.0);
            painter.rect_filled(
                Rect::from_min_size(inner.min, Vec2::new(inner.width(), hh)),
                0.0, lighten(tr.color, 50),
            );
        }

        if hovered {
            painter.rect_stroke(inner, 0.0, Stroke::new(2.0, Color32::WHITE));
        }

        let w = inner.width();
        let h = inner.height();
        if w > 48.0 && h > 24.0 {
            let center = inner.center();
            let max_chars = ((w / 6.5) as usize).max(3);
            let label = truncate_str(&tr.name, max_chars);
            if w > 90.0 && h > 48.0 {
                painter.text(Pos2::new(center.x, center.y - 7.0), egui::Align2::CENTER_CENTER,
                    &label, font_md.clone(), Color32::from_rgba_premultiplied(255,255,255,220));
                painter.text(Pos2::new(center.x, center.y + 7.0), egui::Align2::CENTER_CENTER,
                    &format_size(tr.size), font_sm.clone(), Color32::from_rgba_premultiplied(220,220,220,170));
            } else {
                painter.text(center, egui::Align2::CENTER_CENTER,
                    &label, font_sm.clone(), Color32::from_rgba_premultiplied(255,255,255,210));
            }
        }
    }

    // Tooltip
    if let Some(idx) = hovered_idx {
        let tr = &rects[idx];
        egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new("tm_tip"), |ui| {
            ui.label(egui::RichText::new(&tr.name).strong().size(13.0));
            ui.label(format!("{} — {}", if tr.is_dir {"Dir"} else {"File"}, format_size(tr.size)));
            ui.label(egui::RichText::new(tr.node_path.display().to_string()).weak().small());
        });
    }

    let mut action = TreemapAction { navigate: None, right_click: None };

    if response.clicked() {
        if let Some(idx) = hovered_idx {
            if rects[idx].is_dir {
                action.navigate = Some(rects[idx].node_path.clone());
            }
        }
    }

    if response.secondary_clicked() {
        if let Some(idx) = hovered_idx {
            let tr = &rects[idx];
            action.right_click = Some((tr.node_path.clone(), tr.is_dir, hover_pos.unwrap()));
        }
    }

    action
}

pub fn treemap_panel(ui: &mut Ui, node: Option<&FileNode>, hovered_path: &mut Option<PathBuf>) -> TreemapAction {
    let available = ui.available_rect_before_wrap();
    match node {
        None => {
            ui.allocate_rect(available, Sense::hover());
            ui.painter_at(available).rect_filled(available, 0.0, BG);
            ui.painter_at(available).text(available.center(), egui::Align2::CENTER_CENTER,
                "Select a directory to scan", FontId::proportional(15.0), Color32::from_rgb(80,80,100));
            TreemapAction { navigate: None, right_click: None }
        }
        Some(n) if n.children.is_empty() => {
            ui.allocate_rect(available, Sense::hover());
            ui.painter_at(available).rect_filled(available, 0.0, BG);
            ui.painter_at(available).text(available.center(), egui::Align2::CENTER_CENTER,
                "Empty directory", FontId::proportional(15.0), Color32::from_rgb(80,80,100));
            TreemapAction { navigate: None, right_click: None }
        }
        Some(n) => {
            let rects = build_rects(n, available);
            draw_treemap(ui, &rects, hovered_path)
        }
    }
}
