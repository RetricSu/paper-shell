use crate::backend::{EditorBackend, HistoryEntry};
use egui::{Color32, Context, RichText, ScrollArea, Ui};
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineType {
    Added,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct HistoryVersionData {
    pub entry: HistoryEntry,
    pub content: String,
    pub diff_lines: Vec<DiffLine>,
}

pub struct HistoryWindow {
    open: bool,
    history_data: Option<Vec<HistoryVersionData>>,
    selected_index: Option<usize>,
    viewport_id: egui::ViewportId,
}

impl Default for HistoryWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            history_data: None,
            selected_index: None,
            viewport_id: egui::ViewportId::from_hash_of("history_window"),
        }
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn set_history(
        &mut self,
        entries: Vec<HistoryEntry>,
        backend: &EditorBackend,
    ) -> Result<(), String> {
        let mut history_data: Vec<HistoryVersionData> = Vec::new();

        for entry in entries.iter() {
            // Load content for this version
            let content = backend
                .restore_version(&entry.hash)
                .map_err(|e| e.to_string())?;

            // Calculate diff with previous meaningful version
            let diff_lines = if !history_data.is_empty() {
                let prev_content = &history_data.last().unwrap().content;
                Self::compute_diff(prev_content, &content)
            } else {
                // First version - show full content as unchanged
                content
                    .lines()
                    .map(|line| DiffLine {
                        line_type: DiffLineType::Unchanged,
                        content: line.to_string(),
                    })
                    .collect()
            };

            // Check if this version has meaningful changes
            let has_changes = history_data.is_empty() || Self::has_meaningful_changes(&diff_lines);

            if has_changes {
                history_data.push(HistoryVersionData {
                    entry: entry.clone(),
                    content,
                    diff_lines,
                });
            }
        }

        let data_len = history_data.len();
        self.history_data = Some(history_data);
        self.selected_index = Some(data_len.saturating_sub(1)); // Select latest
        Ok(())
    }

    fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
        let diff = TextDiff::from_lines(old, new);
        let mut diff_lines = Vec::new();

        for change in diff.iter_all_changes() {
            let line_type = match change.tag() {
                ChangeTag::Delete => DiffLineType::Removed,
                ChangeTag::Insert => DiffLineType::Added,
                ChangeTag::Equal => DiffLineType::Unchanged,
            };

            diff_lines.push(DiffLine {
                line_type,
                content: change.to_string().trim_end().to_string(),
            });
        }

        diff_lines
    }

    fn has_meaningful_changes(diff_lines: &[DiffLine]) -> bool {
        diff_lines.iter().any(|line| {
            matches!(line.line_type, DiffLineType::Added | DiffLineType::Removed)
                && !line.content.trim().is_empty()
        })
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        let viewport_id = self.viewport_id;

        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title("📜 File History")
                .with_inner_size([1200.0, 800.0])
                .with_min_inner_size([900.0, 600.0])
                .with_resizable(true)
                .with_close_button(true),
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_content(ui);
                });

                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
            },
        );
    }

    fn show_content(&mut self, ui: &mut Ui) {
        if let Some(history_data) = &self.history_data {
            if history_data.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.heading("No history available for this file");
                    ui.add_space(10.0);
                    ui.label("Make some edits and save to build up version history.");
                });
                return;
            }

            // Use SidePanel for better layout (left panel for versions)
            egui::SidePanel::left("version_list_panel")
                .min_width(300.0)
                .default_width(350.0)
                .resizable(true)
                .show_inside(ui, |ui| {
                    ui.heading("📚 Versions");
                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(5.0);

                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Show in reverse order (newest first)
                            for (i, version_data) in history_data.iter().enumerate().rev() {
                                let is_selected = self.selected_index == Some(i);
                                let timestamp = version_data
                                    .entry
                                    .timestamp
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string();

                                let version_label = format!("Version  {} - {}", i + 1, timestamp);

                                if ui.selectable_label(is_selected, version_label).clicked() {
                                    self.selected_index = Some(i);
                                }
                            }
                        });
                });

            // Central panel for diff view
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if let Some(selected_idx) = self.selected_index {
                    if let Some(version_data) = history_data.get(selected_idx) {
                        ui.heading(format!("Version {} Details", selected_idx + 1));
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⏰ Timestamp:").strong());
                            ui.label(
                                version_data
                                    .entry
                                    .timestamp
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string(),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔑 Hash:").strong());
                            ui.label(RichText::new(&version_data.entry.hash).monospace());
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if selected_idx == 0 {
                            ui.heading("📄 Initial Version");
                        } else {
                            ui.heading(format!("📝 Changes from Version {}", selected_idx));
                        }

                        ui.add_space(8.0);

                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                Self::show_diff_static(ui, &version_data.diff_lines);
                            });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.heading("Select a version to view details");
                        ui.add_space(10.0);
                        ui.label("Choose a version from the list on the left");
                    });
                }
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Loading history...");
                ui.add_space(10.0);
                ui.spinner();
            });
        }
    }

    fn show_diff_static(ui: &mut Ui, diff_lines: &[DiffLine]) {
        ui.style_mut().spacing.item_spacing.y = 1.0;

        for line in diff_lines {
            let (prefix, color, bg_color) = match line.line_type {
                DiffLineType::Added => (
                    "+ ",
                    Color32::from_rgb(0, 130, 0),
                    Some(Color32::from_rgb(225, 255, 225)),
                ),
                DiffLineType::Removed => (
                    "- ",
                    Color32::from_rgb(170, 0, 0),
                    Some(Color32::from_rgb(255, 225, 225)),
                ),
                DiffLineType::Unchanged => ("  ", ui.visuals().text_color(), None),
            };

            let text = format!("{}{}", prefix, line.content);

            // Use standard egui layout for left alignment
            if let Some(bg) = bg_color {
                // For changed lines with background, create a frame
                egui::Frame::NONE.fill(bg).show(ui, |ui| {
                    ui.label(RichText::new(text).color(color).monospace());
                });
            } else {
                // For unchanged lines, just use a regular label
                ui.label(RichText::new(text).color(color).monospace());
            }
        }
    }
}
