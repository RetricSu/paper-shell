mod diff;
mod stats;
mod types;
mod ui;

use crate::backend::editor_backend::{EditorBackend, HistoryEntry};
use chrono::Local;
use egui::{Color32, Context, RichText, ScrollArea, Ui, Vec2};

// Re-export public types
pub use types::{DiffLine, DiffLineType, HistoryVersionData};

#[derive(Debug, PartialEq, Eq)]
pub enum HistoryAction {
    RollbackToVersion(String), // hash
}

pub struct HistoryWindow {
    open: bool,
    history_data: Option<Vec<HistoryVersionData>>,
    error_message: Option<String>,
    selected_index: Option<usize>,
    viewport_id: egui::ViewportId,
    pending_action: Option<HistoryAction>,
    pub(crate) confirm_rollback: Option<String>,
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
            error_message: None,
            selected_index: None,
            viewport_id: egui::ViewportId::from_hash_of("history_window"),
            pending_action: None,
            confirm_rollback: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
        self.history_data = None;
        self.error_message = None;
        self.selected_index = None;
        self.confirm_rollback = None;
    }

    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
        self.history_data = None;
    }

    pub fn set_history(
        &mut self,
        entries: Vec<HistoryEntry>,
        backend: &EditorBackend,
    ) -> Result<(), String> {
        self.error_message = None;
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
                .with_title(format!(
                    "{} 版本历史",
                    egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                ))
                .with_inner_size(Vec2::new(840.0, 600.0))
                .with_min_inner_size(Vec2::new(600.0, 420.0))
                .with_decorations(false)
                .with_resizable(true)
                .with_transparent(true),
            |ctx, _class| {
                crate::style::configure_style(ctx);

                // Title bar with refined border
                egui::TopBottomPanel::top("history_title_bar")
                    .frame(
                        egui::Frame::default()
                            .fill(Color32::from_rgb(248, 248, 248))
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(225, 225, 225)))
                            .inner_margin(egui::Margin::symmetric(12, 8)),
                    )
                    .show(ctx, |ui| {
                        self.show_title_bar(ui);
                    });

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::default()
                            .fill(Color32::WHITE)
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(225, 225, 225)))
                            .inner_margin(egui::Margin::ZERO),
                    )
                    .show(ctx, |ui| {
                        self.show_content(ui);
                    });

                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
                // Only close the history window if the modal was not active and Escape was pressed
                if self.confirm_rollback.is_none()
                    && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
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
        if interact.drag_started() {
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
                ui.label(
                    egui::RichText::new(format!(
                        "{} 版本历史",
                        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                    ))
                    .strong(),
                );
            });

            // Window Controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                // Close button
                if ui
                    .add_sized([22.0, 20.0], egui::Button::new(egui_phosphor::regular::X))
                    .on_hover_text("关闭")
                    .clicked()
                {
                    self.open = false;
                }

                // Maximize/Restore button
                let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                let max_icon = if is_maximized {
                    egui_phosphor::regular::CORNERS_IN
                } else {
                    egui_phosphor::regular::CORNERS_OUT
                };
                let max_tip = if is_maximized { "还原" } else { "最大化" };
                if ui
                    .add_sized([22.0, 20.0], egui::Button::new(max_icon))
                    .on_hover_text(max_tip)
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                // Minimize button
                if ui
                    .add_sized(
                        [22.0, 20.0],
                        egui::Button::new(egui_phosphor::regular::MINUS),
                    )
                    .on_hover_text("最小化")
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        });
    }

    fn show_content(&mut self, ui: &mut Ui) {
        if let Some(err) = &self.error_message {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(
                    RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                        .size(32.0)
                        .color(Color32::from_rgb(200, 80, 80)),
                );
                ui.add_space(8.0);
                ui.heading("无法加载版本历史");
                ui.add_space(8.0);
                ui.label(RichText::new(err).color(Color32::from_rgb(200, 80, 80)));
            });
            return;
        }

        if let Some(history_data) = &self.history_data {
            if history_data.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading("暂无历史版本");
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("编辑并保存文档后，即可在此查看版本历史。")
                            .color(Color32::from_gray(120)),
                    );
                });
                return;
            }

            // Use SidePanel for better layout (left panel for versions)
            egui::SidePanel::left("version_list_panel")
                .frame(
                    egui::Frame::default()
                        .fill(Color32::from_rgb(250, 250, 250))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(230, 230, 230)))
                        .inner_margin(egui::Margin::symmetric(8, 8)),
                )
                .resizable(true)
                .default_width(220.0)
                .width_range(180.0..=360.0)
                .show_inside(ui, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        // Show in reverse order (newest first)
                        for (i, version_data) in history_data.iter().enumerate().rev() {
                            let is_selected = self.selected_index == Some(i);
                            let timestamp = version_data
                                .entry
                                .timestamp
                                .with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();

                            let version_label =
                                format!("{} {}", egui_phosphor::regular::GIT_COMMIT, timestamp);

                            if ui.selectable_label(is_selected, version_label).clicked() {
                                self.selected_index = Some(i);
                            }
                        }
                    });
                });

            // Central panel for diff view
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::default()
                        .fill(Color32::WHITE)
                        .inner_margin(egui::Margin::same(12)),
                )
                .show_inside(ui, |ui| {
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
                                            RichText::new(format!(
                                                "-{}",
                                                version_data.removed_count
                                            ))
                                            .color(Color32::from_rgb(150, 0, 0)),
                                        );
                                        let time = version_data.entry.time_spent.unwrap_or(0);
                                        let hours = time / 3600;
                                        let minutes = (time % 3600) / 60;
                                        let seconds = time % 60;
                                        let time_str = if hours > 0 {
                                            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                                        } else {
                                            format!("{:02}:{:02}", minutes, seconds)
                                        };
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {}",
                                                egui_phosphor::regular::CLOCK,
                                                time_str
                                            ))
                                            .small(),
                                        );
                                    },
                                );

                                // Hash & Rollback button (right-aligned)
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .button(format!(
                                                            "{} 回滚到此版本",
                                                            egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                                                        ))
                                                        .clicked()
                                                    {
                                                        self.confirm_rollback = Some(
                                                            version_data.entry.hash.clone(),
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {}",
                                                egui_phosphor::regular::HASH,
                                                version_data.entry.hash
                                            ))
                                            .monospace()
                                            .small(),
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
                            ui.add_space(80.0);
                            ui.heading("请选择版本");
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("从左侧列表中选择一个版本以查看内容比对。")
                                    .color(Color32::from_gray(120)),
                            );
                        });
                    }
                });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("正在加载历史版本…");
                ui.add_space(12.0);
                ui.spinner();
            });
        }

        // In-viewport confirmation modal for rollback
        if let Some(hash) = self.confirm_rollback.clone() {
            let config = crate::ui::dialog::ModalConfig {
                id: "history_confirm_rollback",
                title: "确认回滚到此版本",
                icon: egui_phosphor::regular::WARNING,
                icon_color: Color32::from_rgb(217, 119, 6),
                message: "确定要将文档回滚到此版本吗？\n当前编辑器的内容将被该历史版本替换。",
                width: 340.0,
            };

            let mut confirm_clicked = false;
            let mut cancel_clicked = false;

            let response = crate::ui::dialog::show_modal(ui.ctx(), config, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                RichText::new(format!(
                                    "{} 确认回滚",
                                    egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                                ))
                                .strong(),
                            )
                            .clicked()
                        {
                            confirm_clicked = true;
                        }

                        if ui.button("取消").clicked() {
                            cancel_clicked = true;
                        }
                    });
                });
            });

            if confirm_clicked || response.enter_pressed {
                self.pending_action = Some(HistoryAction::RollbackToVersion(hash));
                self.confirm_rollback = None;
                self.open = false;
            } else if cancel_clicked || response.escape_pressed {
                self.confirm_rollback = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_window_open_resets_state() {
        let mut window = HistoryWindow::new();
        assert!(!window.is_open());

        window.confirm_rollback = Some("abc".into());
        window.error_message = Some("err".into());

        window.open();
        assert!(window.is_open());
        assert!(window.confirm_rollback.is_none());
        assert!(window.error_message.is_none());
        assert!(window.history_data.is_none());
    }

    #[test]
    fn test_history_window_set_error() {
        let mut window = HistoryWindow::new();
        window.set_error("failed to read".into());
        assert_eq!(window.error_message, Some("failed to read".into()));
        assert!(window.history_data.is_none());
    }

    #[test]
    fn test_history_window_take_pending_action() {
        let mut window = HistoryWindow::new();
        assert!(window.take_pending_action().is_none());

        window.pending_action = Some(HistoryAction::RollbackToVersion("commit_hash".into()));
        assert_eq!(
            window.take_pending_action(),
            Some(HistoryAction::RollbackToVersion("commit_hash".into()))
        );
        assert!(window.take_pending_action().is_none());
    }
}
