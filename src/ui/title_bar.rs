use crate::plugin::PluginMetadata;
use egui::{Align, Layout, Ui};
use std::path::PathBuf;

pub enum TitleBarAction {
    NewWindow,
    Save,
    Open,
    OpenFile(PathBuf),
    History,
    Settings,
    Format,
    FontChange(String),
    ToggleAiPanel,
    SearchReplace,
    /// Run an installed plugin by its id.
    RunPlugin(String),
    /// Open the configuration window for a built-in plugin.
    ConfigurePlugin(String),
    /// Open the plugins directory in the system file manager.
    OpenPluginsFolder,
}

pub struct TitleBar;

pub struct TitleBarState<'a> {
    pub title: &'a str,
    pub word_count: usize,
    pub cursor_word_count: usize,
    pub writing_time: u64,
    pub has_current_file: bool,
    pub chinese_fonts: &'a [String],
    pub current_font: &'a str,
    pub recent_files: &'a [PathBuf],
    pub is_ai_panel_visible: bool,
    pub plugins: &'a [PluginMetadata],
}

impl TitleBar {
    pub fn show(
        ui: &mut Ui,
        _frame: &mut eframe::Frame,
        state: TitleBarState<'_>,
    ) -> Option<TitleBarAction> {
        let TitleBarState {
            title,
            word_count,
            cursor_word_count,
            writing_time,
            has_current_file,
            chinese_fonts,
            current_font,
            recent_files,
            is_ai_panel_visible,
            plugins,
        } = state;

        let mut action = None;
        let title_bar_rect = ui.available_rect_before_wrap();

        // Dragging logic - registered BEFORE widgets so they can steal input
        let interact = ui.interact(
            title_bar_rect,
            ui.id().with("title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if interact.dragged() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if interact.double_clicked() {
            let is_fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
        }

        ui.horizontal(|ui| {
            // Title label and actions
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(title);
                ui.add_space(16.0);

                ui.menu_button("📂", |ui| {
                    for path in recent_files {
                        let file_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown");
                        let path_str = path.to_string_lossy();
                        if ui
                            .button(file_name)
                            .on_hover_text(path_str.as_ref())
                            .clicked()
                        {
                            action = Some(TitleBarAction::OpenFile(path.clone()));
                            ui.close();
                        }
                    }
                    if !recent_files.is_empty() {
                        ui.separator();
                    }
                    if ui.button("Open File...").clicked() {
                        action = Some(TitleBarAction::Open);
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Open");
                if ui.button("💾").on_hover_text("Save").clicked() {
                    action = Some(TitleBarAction::Save);
                }
                if ui.button("⚙").on_hover_text("Settings").clicked() {
                    action = Some(TitleBarAction::Settings);
                }
                if ui.button("新建").on_hover_text("New Window").clicked() {
                    action = Some(TitleBarAction::NewWindow);
                }
                ui.menu_button("编辑", |ui| {
                    if ui.button("查找替换").clicked() {
                        action = Some(TitleBarAction::SearchReplace);
                        ui.close();
                    }
                    if ui.button("格式化").clicked() {
                        action = Some(TitleBarAction::Format);
                        ui.close();
                    }
                });
                ui.menu_button("字体", |ui| {
                    ui.label("中文:");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for font_name in chinese_fonts {
                                let is_selected = font_name == current_font;
                                if ui.selectable_label(is_selected, font_name).clicked() {
                                    action = Some(TitleBarAction::FontChange(font_name.clone()));
                                    ui.close();
                                }
                            }
                        });
                });
                ui.menu_button("插件", |ui| {
                    if plugins.is_empty() {
                        ui.label("暂无已安装插件");
                    } else {
                        for plugin in plugins {
                            if ui
                                .button(&plugin.name)
                                .on_hover_text(&plugin.description)
                                .clicked()
                            {
                                action = Some(TitleBarAction::RunPlugin(plugin.id.clone()));
                                ui.close();
                            }
                        }

                        if plugins.iter().any(|p| p.id == "github_publish") {
                            if ui.button("配置 GitHub 发布…").clicked() {
                                action = Some(TitleBarAction::ConfigurePlugin(
                                    "github_publish".to_string(),
                                ));
                                ui.close();
                            }
                        }
                    }
                    ui.separator();
                    if ui
                        .button("打开插件目录…")
                        .on_hover_text("在此目录放入插件文件夹即可安装")
                        .clicked()
                    {
                        action = Some(TitleBarAction::OpenPluginsFolder);
                        ui.close();
                    }
                });
                if ui
                    .add_enabled(has_current_file, egui::Button::new("历史"))
                    .on_hover_text("History")
                    .on_disabled_hover_text("No file opened")
                    .clicked()
                {
                    action = Some(TitleBarAction::History);
                }
            });

            // Window Controls
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Close button
                if ui.button("❌").on_hover_text("Close").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }

                // Fullscreen button
                let is_fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
                if ui.button("⛶").on_hover_text("Fullscreen").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                }

                // Minimize button
                if ui.button("➖").on_hover_text("Minimize").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                // Stats and AI toggle
                ui.add_space(16.0);
                let ai_icon = if is_ai_panel_visible { "[|]" } else { "[ ]" };
                if ui.label(egui::RichText::new(ai_icon).small()).clicked() {
                    action = Some(TitleBarAction::ToggleAiPanel);
                }

                let time_str = Self::format_writing_time(writing_time);
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {} | {}",
                        cursor_word_count, word_count, time_str
                    ))
                    .small(),
                );
            });
        });

        action
    }

    /// Format writing time in seconds to a readable string (MM:SS or HH:MM:SS)
    fn format_writing_time(seconds: u64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, secs)
        } else {
            format!("{:02}:{:02}", minutes, secs)
        }
    }
}
