mod diff;
mod stats;
mod types;
mod ui;

use crate::backend::editor_backend::{EditorBackend, HistoryEntry};
use egui::{Color32, Context, RichText, ScrollArea, Ui};

// Re-export public types
pub use types::{DiffLine, DiffLineType, HistoryVersionData};

#[derive(Debug)]
pub enum HistoryAction {
    RollbackToVersion(String), // hash
}

pub struct HistoryWindow {
    open: bool,
    history_data: Option<Vec<HistoryVersionData>>,
    selected_index: Option<usize>,
    viewport_id: egui::ViewportId,
    pending_action: Option<HistoryAction>,
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
            pending_action: None,
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
                diff::compute_diff(prev_content, &content)
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
            let has_changes = history_data.is_empty() || diff::has_meaningful_changes(&diff_lines);

            if has_changes {
                // Calculate stats
                let rows = diff::group_into_rows(&diff_lines);
                let stats = stats::calculate_stats(&rows);

                history_data.push(HistoryVersionData {
                    entry: entry.clone(),
                    content,
                    diff_lines,
                    added_count: stats.added_count,
                    removed_count: stats.removed_count,
                });
            }
        }

        let data_len = history_data.len();
        self.history_data = Some(history_data);
        self.selected_index = Some(data_len.saturating_sub(1)); // Select latest
        Ok(())
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        let viewport_id = self.viewport_id;

        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_resizable(true)
                .with_transparent(true),
            |ctx, _class| {
                // Title bar
                egui::TopBottomPanel::top("history_title_bar").show(ctx, |ui| {
                    self.show_title_bar(ui);
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_content(ui);
                });

                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
            },
        );
    }

    pub fn take_pending_action(&mut self) -> Option<HistoryAction> {
        self.pending_action.take()
    }

    fn show_title_bar(&mut self, ui: &mut Ui) {
        let title_bar_rect = ui.available_rect_before_wrap();

        // Dragging logic - registered BEFORE widgets so they can steal input
        let interact = ui.interact(
            title_bar_rect,
            ui.id().with("history_title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if interact.dragged() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if interact.double_clicked() {
            let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        ui.horizontal(|ui| {
            // Title
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label("📜 History");
            });

            // Window Controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Close button
                if ui.button("❌").on_hover_text("Close").clicked() {
                    self.open = false;
                }

                // Maximize/Restore button
                let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                if ui.button("⛶").on_hover_text("Maximize/Restore").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                // Minimize button
                if ui.button("➖").on_hover_text("Minimize").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        });
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
                .resizable(true)
                .show_inside(ui, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        // Show in reverse order (newest first)
                        for (i, version_data) in history_data.iter().enumerate().rev() {
                            let is_selected = self.selected_index == Some(i);
                            let timestamp = version_data
                                .entry
                                .timestamp
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();

                            let version_label = timestamp.to_string();

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
                        ui.horizontal(|ui| {
                            // Stats (left-aligned)
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("+{}", version_data.added_count))
                                            .color(Color32::from_rgb(0, 100, 0)),
                                    );
                                    ui.label(
                                        RichText::new(format!("-{}", version_data.removed_count))
                                            .color(Color32::from_rgb(150, 0, 0)),
                                    );
                                    let time = version_data.entry.time_spent.unwrap_or(0);
                                    let hours = time / 3600;
                                    let minutes = (time % 3600) / 60;
                                    let seconds = time % 60;
                                    let time_str = if hours > 0 {
                                        format!(" {:02}:{:02}:{:02}", hours, minutes, seconds)
                                    } else {
                                        format!(" {:02}:{:02}", minutes, seconds)
                                    };
                                    ui.label(RichText::new(time_str));
                                },
                            );

                            // Hash (right-aligned)
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Rollback button
                                    ui.horizontal(|ui| {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button("🔄 回滚到此版本").clicked() {
                                                    self.pending_action =
                                                        Some(HistoryAction::RollbackToVersion(
                                                            version_data.entry.hash.clone(),
                                                        ));
                                                    self.open = false; // Close the window after rollback
                                                }
                                            },
                                        );
                                    });
                                    ui.label(
                                        RichText::new(format!("Hash:{}", &version_data.entry.hash))
                                            .monospace(),
                                    );
                                },
                            );
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui::render_diff_view(ui, &version_data.diff_lines);
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
}
