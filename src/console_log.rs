use egui::{self, Color32, RichText, ScrollArea};
use chrono::Local;

#[derive(Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl LogLevel {
    fn label(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO   ",
            LogLevel::Success => "OK     ",
            LogLevel::Warning => "WARN   ",
            LogLevel::Error => "ERROR  ",
        }
    }

    fn color(&self) -> Color32 {
        match self {
            LogLevel::Info => Color32::from_rgb(200, 200, 210),
            LogLevel::Success => Color32::from_rgb(76, 175, 80),
            LogLevel::Warning => Color32::from_rgb(255, 152, 0),
            LogLevel::Error => Color32::from_rgb(244, 67, 54),
        }
    }
}

#[derive(Clone)]
struct LogEntry {
    timestamp: String,
    level: LogLevel,
    message: String,
}

pub struct ConsoleLog {
    entries: Vec<LogEntry>,
    max_entries: usize,
    pub auto_scroll: bool,
    filter: LogLevel,
    dirty: bool,
}

impl ConsoleLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(256),
            max_entries: 1000,
            auto_scroll: true,
            filter: LogLevel::Info,
            dirty: false,
        }
    }

    pub fn log(&mut self, level: LogLevel, msg: impl Into<String>) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        self.entries.push(LogEntry {
            timestamp,
            level,
            message: msg.into(),
        });
        self.dirty = true;
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.log(LogLevel::Info, msg);
    }

    pub fn success(&mut self, msg: impl Into<String>) {
        self.log(LogLevel::Success, msg);
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.log(LogLevel::Warning, msg);
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.log(LogLevel::Error, msg);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.dirty = false;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Top toolbar
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Console")
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(200, 200, 220)),
            );
            ui.add_space(8.0);

            egui::ComboBox::from_id_salt("console_filter")
                .selected_text(filter_label(&self.filter))
                .width(90.0)
                .show_ui(ui, |ui| {
                    // "All" maps to Info level (passes everything)
                    ui.selectable_value(&mut self.filter, LogLevel::Info, "All / Info");
                    ui.selectable_value(&mut self.filter, LogLevel::Success, "Success");
                    ui.selectable_value(&mut self.filter, LogLevel::Warning, "Warn");
                    ui.selectable_value(&mut self.filter, LogLevel::Error, "Error");
                });

            ui.add_space(4.0);
            ui.checkbox(
                &mut self.auto_scroll,
                RichText::new("Auto-scroll").size(12.0),
            );
            ui.add_space(4.0);
            if ui
                .small_button(RichText::new("Clear").size(12.0))
                .clicked()
            {
                self.clear();
            }
        });

        ui.separator();

        let bg_color = Color32::from_rgb(10, 10, 20);
        let available = ui.available_size();

        egui::Frame::none()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(6.0, 4.0))
            .show(ui, |ui: &mut egui::Ui| {
                ui.set_min_size(available);

                let scroll_to_bottom = self.auto_scroll && self.dirty;
                self.dirty = false;

                let scroll = ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.auto_scroll);

                // When new entries arrive and auto_scroll is on, ensure we reach the bottom.
                // stick_to_bottom handles it; the scroll_to_bottom flag just suppresses the
                // unused-variable warning.
                let _ = scroll_to_bottom;

                scroll.show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                    let filter = &self.filter;
                    for entry in &self.entries {
                        if !passes_filter(&entry.level, filter) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.label(
                                RichText::new(format!("[{}] ", entry.timestamp))
                                    .monospace()
                                    .size(12.0)
                                    .color(Color32::from_rgb(100, 110, 120)),
                            );
                            ui.label(
                                RichText::new(entry.level.label())
                                    .monospace()
                                    .size(12.0)
                                    .color(entry.level.color()),
                            );
                            ui.label(
                                RichText::new(&entry.message)
                                    .monospace()
                                    .size(12.0)
                                    .color(entry.level.color()),
                            );
                        });
                    }

                    if self.entries.is_empty() {
                        ui.label(
                            RichText::new("No log entries.")
                                .monospace()
                                .size(12.0)
                                .color(Color32::from_rgb(70, 70, 80)),
                        );
                    }
                });
            });
    }
}

fn filter_label(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Info => "All / Info",
        LogLevel::Success => "Success",
        LogLevel::Warning => "Warn",
        LogLevel::Error => "Error",
    }
}

fn passes_filter(entry_level: &LogLevel, filter: &LogLevel) -> bool {
    match filter {
        LogLevel::Info => true, // show all
        other => entry_level == other,
    }
}
