use std::path::PathBuf;
use egui::{self, RichText, Color32};
use crate::types::AppSettings;

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<AppSettings>(&data) {
                return s;
            }
        }
    }
    AppSettings::default()
}

pub fn save_settings(settings: &AppSettings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, json);
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("macdirstat")
        .join("settings.json")
}

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    Scan,
    Display,
    Advanced,
}

pub struct SettingsWindow {
    pub open: bool,
    active_tab: SettingsTab,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            active_tab: SettingsTab::Scan,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, settings: &mut AppSettings) {
        if !self.open {
            return;
        }

        let screen = ctx.screen_rect();
        let win_w = 500.0_f32;
        let win_h = 420.0_f32;
        let pos = egui::pos2(
            (screen.width() - win_w) * 0.5,
            (screen.height() - win_h) * 0.5,
        );

        let mut open = self.open;
        egui::Window::new("Settings")
            .open(&mut open)
            .fixed_size([win_w, win_h])
            .default_pos(pos)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                // Tab bar
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.active_tab,
                        SettingsTab::Scan,
                        RichText::new("  Scan  ").size(14.0),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        SettingsTab::Display,
                        RichText::new("  Display  ").size(14.0),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        SettingsTab::Advanced,
                        RichText::new("  Advanced  ").size(14.0),
                    );
                });
                ui.separator();
                ui.add_space(6.0);

                match self.active_tab {
                    SettingsTab::Scan => self.show_scan_tab(ui, settings),
                    SettingsTab::Display => self.show_display_tab(ui, settings),
                    SettingsTab::Advanced => self.show_advanced_tab(ui, settings),
                }

                ui.add_space(10.0);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("  Close  ").size(13.0)).clicked() {
                        self.open = false;
                    }
                    if ui.button(RichText::new("  Save  ").size(13.0)).clicked() {
                        save_settings(settings);
                        self.open = false;
                    }
                });
            });

        self.open = open;
    }

    fn show_scan_tab(&self, ui: &mut egui::Ui, settings: &mut AppSettings) {
        egui::Grid::new("scan_grid")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(180.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Scan Root Path").size(13.0));
                ui.label(
                    RichText::new("Chosen via directory picker")
                        .size(12.0)
                        .color(Color32::from_rgb(150, 150, 160)),
                );
                ui.end_row();

                ui.label(RichText::new("Max Depth").size(13.0));
                ui.add(
                    egui::DragValue::new(&mut settings.max_depth)
                        .range(1..=50)
                        .speed(0.3),
                );
                ui.end_row();

                ui.label(RichText::new("Skip Hidden Files").size(13.0));
                ui.checkbox(&mut settings.skip_hidden, "");
                ui.end_row();

                ui.label(RichText::new("Worker Threads").size(13.0));
                ui.add(
                    egui::Slider::new(&mut settings.thread_count, 1..=16)
                        .text("threads"),
                );
                ui.end_row();

                ui.label(RichText::new("Skip System Directories").size(13.0));
                // stored as a convention — we surface it here but drive it from skip_hidden
                // for now reuse skip_hidden semantics via a local note
                ui.label(
                    RichText::new("node_modules, ~/Library (shallow)")
                        .size(11.0)
                        .color(Color32::from_rgb(120, 120, 130)),
                );
                ui.end_row();
            });
    }

    fn show_display_tab(&self, ui: &mut egui::Ui, settings: &mut AppSettings) {
        egui::Grid::new("display_grid")
            .num_columns(2)
            .spacing([16.0, 12.0])
            .min_col_width(180.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Show Console Output").size(13.0));
                ui.checkbox(&mut settings.show_console, "");
                ui.end_row();

                ui.label(RichText::new("Treemap Height").size(13.0));
                ui.vertical(|ui| {
                    ui.add(
                        egui::Slider::new(&mut settings.treemap_fraction, 0.2..=0.8)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                            .text("of view"),
                    );
                });
                ui.end_row();

                ui.label(RichText::new("Sidebar Width").size(13.0));
                ui.add(
                    egui::Slider::new(&mut settings.sidebar_width, 150.0..=400.0)
                        .custom_formatter(|v, _| format!("{:.0} px", v))
                        .text("px"),
                );
                ui.end_row();
            });
    }

    fn show_advanced_tab(&self, ui: &mut egui::Ui, settings: &mut AppSettings) {
        ui.label(
            RichText::new("API Configuration")
                .size(16.0)
                .strong()
                .color(Color32::from_rgb(220, 200, 255)),
        );
        ui.add_space(8.0);

        egui::Grid::new("advanced_grid")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(180.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Max Tokens").size(13.0));
                ui.add(
                    egui::DragValue::new(&mut settings.max_api_tokens)
                        .range(256..=128000)
                        .speed(64.0),
                );
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.label(
            RichText::new("Maximum tokens for AI API calls (for future integrations)")
                .size(11.0)
                .color(Color32::from_rgb(130, 130, 145)),
        );
        ui.add_space(10.0);

        // Token budget visual
        ui.label(RichText::new("Token Budget").size(12.0).color(Color32::from_rgb(180, 180, 195)));
        ui.add_space(4.0);
        let fraction = (settings.max_api_tokens as f32) / 128_000.0;
        let bar_color = if fraction < 0.33 {
            Color32::from_rgb(76, 175, 80)
        } else if fraction < 0.66 {
            Color32::from_rgb(255, 152, 0)
        } else {
            Color32::from_rgb(244, 67, 54)
        };
        let desired = egui::vec2(ui.available_width() - 8.0, 16.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 4.0, Color32::from_rgb(30, 30, 40));
            let filled = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * fraction, rect.height()),
            );
            painter.rect_filled(filled, 4.0, bar_color);
            let label = format!(
                "{} / 128k tokens",
                if settings.max_api_tokens >= 1000 {
                    format!("{:.1}k", settings.max_api_tokens as f32 / 1000.0)
                } else {
                    settings.max_api_tokens.to_string()
                }
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(RichText::new("Config File Location").size(13.0).color(Color32::from_rgb(180, 180, 195)));
        ui.add_space(2.0);
        let path_str = settings_path().to_string_lossy().to_string();
        ui.label(
            RichText::new(&path_str)
                .size(11.0)
                .monospace()
                .color(Color32::from_rgb(120, 200, 255)),
        );

        ui.add_space(12.0);
        let reset_btn = egui::Button::new(
            RichText::new("  Reset to Defaults  ")
                .size(13.0)
                .color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(180, 60, 30));

        if ui.add(reset_btn).clicked() {
            *settings = AppSettings::default();
        }
    }
}
