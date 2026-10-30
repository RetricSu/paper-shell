use crate::plugin::PluginMetadata;
use egui::{Align, Layout, Ui};
use std::path::PathBuf;

pub enum TitleBarAction {
    NewWindow,
    Save,
    Open,
    OpenFile(PathBuf),
    History,
    Format,
    FontChange(String),
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
                ui.label(egui::RichText::new(title).strong());
                ui.add_space(8.0);

                ui.menu_button(egui_phosphor::regular::FOLDER_OPEN, |ui| {
                    if recent_files.is_empty() {
                        ui.label(egui::RichText::new("无最近文件").weak());
                    } else {
                        ui.label(egui::RichText::new("最近文件").small().weak());
                        for path in recent_files {
                            let file_name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown");
                            let path_str = path.to_string_lossy();
                            if ui
                                .button(format!("{} {}", egui_phosphor::regular::FILE_TEXT, file_name))
                                .on_hover_text(path_str.as_ref())
                                .clicked()
                            {
                                action = Some(TitleBarAction::OpenFile(path.clone()));
                                ui.close();
                            }
                        }
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} 打开文件…", egui_phosphor::regular::FOLDER))
                        .clicked()
                    {
                        action = Some(TitleBarAction::Open);
                        ui.close();
                    }
                })
                .response
                .on_hover_text("打开 / 最近文件");

                if ui
                    .button(egui_phosphor::regular::FLOPPY_DISK)
                    .on_hover_text("保存 (Ctrl+S / ⌘S)")
                    .clicked()
                {
                    action = Some(TitleBarAction::Save);
                }

                if ui
                    .button(egui_phosphor::regular::FILE_PLUS)
                    .on_hover_text("新建窗口 (Ctrl+N / ⌘N)")
                    .clicked()
                {
                    action = Some(TitleBarAction::NewWindow);
                }

                if ui
                    .add_enabled(
                        has_current_file,
                        egui::Button::new(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE),
                    )
                    .on_hover_text("版本历史")
                    .on_disabled_hover_text("未打开任何文件")
                    .clicked()
                {
                    action = Some(TitleBarAction::History);
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                ui.menu_button("编辑", |ui| {
                    if ui
                        .button(format!("{} 查找替换…", egui_phosphor::regular::MAGNIFYING_GLASS))
                        .clicked()
                    {
                        action = Some(TitleBarAction::SearchReplace);
                        ui.close();
                    }
                    if ui
                        .button(format!("{} 整理段落缩进", egui_phosphor::regular::TEXT_ALIGN_LEFT))
                        .on_hover_text("自动为段落首行添加缩进空格")
                        .clicked()
                    {
                        action = Some(TitleBarAction::Format);
                        ui.close();
                    }
                });

                ui.menu_button("字体", |ui| {
                    ui.label(egui::RichText::new("选择字体:").small().weak());
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for font_name in chinese_fonts {
                                let is_selected = font_name == current_font;
                                let text = if is_selected {
                                    format!("{} {}", egui_phosphor::regular::CHECK, font_name)
                                } else {
                                    format!("   {}", font_name)
                                };
                                if ui.button(text).clicked() {
                                    action = Some(TitleBarAction::FontChange(font_name.clone()));
                                    ui.close();
                                }
                            }
                        });
                });

                ui.menu_button("插件", |ui| {
                    if plugins.is_empty() {
                        ui.label(egui::RichText::new("暂无已安装插件").weak());
                    } else {
                        for plugin in plugins {
                            let icon = match plugin.id.as_str() {
                                "github_publish" => egui_phosphor::regular::GITHUB_LOGO,
                                "print" => egui_phosphor::regular::PRINTER,
                                _ => egui_phosphor::regular::PUZZLE_PIECE,
                            };
                            if ui
                                .button(format!("{} {}", icon, &plugin.name))
                                .on_hover_text(&plugin.description)
                                .clicked()
                            {
                                action = Some(TitleBarAction::RunPlugin(plugin.id.clone()));
                                ui.close();
                            }
                        }

                        if plugins.iter().any(|p| p.id == "github_publish")
                            && ui
                                .button(format!("{} 配置 GitHub 发布…", egui_phosphor::regular::GEAR))
                                .clicked()
                        {
                            action = Some(TitleBarAction::ConfigurePlugin(
                                "github_publish".to_string(),
                            ));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} 打开插件目录…", egui_phosphor::regular::FOLDER))
                        .on_hover_text("在此目录放入插件文件夹即可安装")
                        .clicked()
                    {
                        action = Some(TitleBarAction::OpenPluginsFolder);
                        ui.close();
                    }
                });
            });

            // Window Controls
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                // Close button
                if ui
                    .button(egui_phosphor::regular::X)
                    .on_hover_text("关闭窗口")
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }

                // Fullscreen button
                let is_fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
                let fs_icon = if is_fullscreen {
                    egui_phosphor::regular::CORNERS_IN
                } else {
                    egui_phosphor::regular::CORNERS_OUT
                };
                let fs_tip = if is_fullscreen { "退出全屏" } else { "全屏" };
                if ui.button(fs_icon).on_hover_text(fs_tip).clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                }

                // Minimize button
                if ui
                    .button(egui_phosphor::regular::MINUS)
                    .on_hover_text("最小化")
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                // Stats
                ui.add_space(12.0);
                let time_str = Self::format_writing_time(writing_time);
                ui.label(
                    egui::RichText::new(format!(
                        "{} {} / {}   {} {}",
                        egui_phosphor::regular::TEXT_T,
                        cursor_word_count,
                        word_count,
                        egui_phosphor::regular::TIMER,
                        time_str
                    ))
                    .small(),
                )
                .on_hover_text(format!(
                    "光标字数: {} / 全文字数: {}\n写作时长: {}",
                    cursor_word_count, word_count, time_str
                ));
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
