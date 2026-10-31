use crate::plugin::PluginMetadata;
use egui::{Align, Layout, Ui};
use std::path::PathBuf;

pub enum TitleBarAction {
    NewWindow,
    ToggleSidebar,
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
    pub sidebar_expanded: bool,
    pub word_count: usize,
    pub cursor_word_count: usize,
    pub selection_word_count: Option<usize>,
    pub writing_time: u64,
    pub document_path: Option<&'a str>,
    pub has_current_file: bool,
    pub chinese_fonts: &'a [String],
    pub current_font: &'a str,
    pub recent_files: &'a [PathBuf],
    pub plugins: &'a [PluginMetadata],
}

impl TitleBar {
    pub fn show(ui: &mut Ui, state: TitleBarState<'_>) -> Option<TitleBarAction> {
        let TitleBarState {
            title,
            sidebar_expanded,
            word_count,
            cursor_word_count,
            selection_word_count,
            writing_time,
            document_path,
            has_current_file,
            chinese_fonts,
            current_font,
            recent_files,
            plugins,
        } = state;

        let mut action = None;
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 20.0;
        let show_stats = ui.available_width()
            >= if selection_word_count.is_some() {
                700.0
            } else {
                600.0
            };
        let time = Self::format_writing_time(writing_time);
        let selection_detail = selection_word_count
            .map(|count| format!("\n选中字数: {count}"))
            .unwrap_or_default();
        let detail = format!(
            "{}\n光标字数: {} / 全文字数: {}{}\n写作时长: {}",
            document_path.unwrap_or("未命名文档"),
            cursor_word_count,
            word_count,
            selection_detail,
            time
        );

        // Register the drag surface first so menus and window controls take precedence.
        let drag = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("title_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }

        egui::MenuBar::new().ui(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                Self::window_controls(ui);
                if show_stats {
                    ui.add_space(8.0);
                    let selection_label = selection_word_count
                        .map(|count| format!("选中 {count}  ·  "))
                        .unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "{}{} / {}  ·  {}",
                            selection_label, cursor_word_count, word_count, time
                        ))
                        .small(),
                    )
                    .on_hover_text(&detail);
                    ui.add_space(8.0);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if ui.available_width() >= 260.0 {
                        let title_color = ui
                            .visuals()
                            .text_color()
                            .gamma_multiply(if sidebar_expanded { 1.0 } else { 0.82 });
                        let response = ui
                            .add_sized(
                                [36.0, 20.0],
                                egui::Label::new(
                                    egui::RichText::new(title).strong().color(title_color),
                                )
                                .truncate()
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(format!(
                                "点击{}文章列表\n{}",
                                if sidebar_expanded { "隐藏" } else { "显示" },
                                detail
                            ));
                        let dot = response.rect.left_top() + egui::vec2(1.0, 2.0);
                        let dot_color = crate::style::STATUS_GREEN
                            .gamma_multiply(if sidebar_expanded { 0.9 } else { 0.6 });
                        if sidebar_expanded {
                            ui.painter().circle_filled(dot, 1.4, dot_color);
                        } else {
                            ui.painter()
                                .circle_stroke(dot, 1.4, egui::Stroke::new(0.7, dot_color));
                        }
                        if response.clicked() {
                            action = Some(TitleBarAction::ToggleSidebar);
                        }
                    }
                    ui.menu_button(
                        format!("{} 文件", egui_phosphor::regular::FOLDER_OPEN),
                        |ui| {
                            ui.set_min_width(220.0);
                            if Self::menu_item(ui, "新建窗口", Some(egui::Key::N)).clicked() {
                                action = Some(TitleBarAction::NewWindow);
                                ui.close();
                            }
                            if Self::menu_item(ui, "打开文件…", Some(egui::Key::O)).clicked()
                            {
                                action = Some(TitleBarAction::Open);
                                ui.close();
                            }
                            ui.menu_button("最近打开", |ui| {
                                ui.set_width(240.0);
                                if recent_files.is_empty() {
                                    ui.add_enabled(false, egui::Button::new("暂无最近文件"));
                                }
                                for path in recent_files {
                                    let name = path
                                        .file_name()
                                        .unwrap_or(path.as_os_str())
                                        .to_string_lossy();
                                    let available = path.is_file();
                                    if ui
                                        .add_enabled(
                                            available,
                                            egui::Button::new(name.as_ref()).truncate(),
                                        )
                                        .on_hover_text(path.to_string_lossy())
                                        .on_disabled_hover_text(format!(
                                            "文件已移动或不存在\n{}",
                                            path.display()
                                        ))
                                        .clicked()
                                    {
                                        action = Some(TitleBarAction::OpenFile(path.clone()));
                                        ui.close();
                                    }
                                }
                            });
                            ui.separator();
                            if Self::menu_item(ui, "保存", Some(egui::Key::S)).clicked() {
                                action = Some(TitleBarAction::Save);
                                ui.close();
                            }
                            if ui
                                .add_enabled(has_current_file, egui::Button::new("版本历史…"))
                                .on_disabled_hover_text("保存文档后可查看版本历史")
                                .clicked()
                            {
                                action = Some(TitleBarAction::History);
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .add(
                                    egui::Button::new(if sidebar_expanded {
                                        "隐藏文章列表"
                                    } else {
                                        "显示文章列表"
                                    })
                                    .shortcut_text(
                                        ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
                                            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                                            egui::Key::E,
                                        )),
                                    ),
                                )
                                .clicked()
                            {
                                action = Some(TitleBarAction::ToggleSidebar);
                                ui.close();
                            }
                        },
                    );
                    ui.menu_button(
                        format!("{} 编辑", egui_phosphor::regular::NOTE_PENCIL),
                        |ui| {
                            ui.set_min_width(220.0);
                            if Self::menu_item(ui, "查找与替换…", Some(egui::Key::F)).clicked()
                            {
                                action = Some(TitleBarAction::SearchReplace);
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .button("整理段落缩进")
                                .on_hover_text("为段落首行添加缩进空格")
                                .clicked()
                            {
                                action = Some(TitleBarAction::Format);
                                ui.close();
                            }
                        },
                    );
                    ui.menu_button(format!("{} 字体", egui_phosphor::regular::TEXT_T), |ui| {
                        ui.set_min_width(200.0);
                        if ui
                            .selectable_label(current_font == "Default", "系统默认")
                            .clicked()
                        {
                            action = Some(TitleBarAction::FontChange("Default".to_string()));
                            ui.close();
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for font_name in chinese_fonts {
                                    if ui
                                        .selectable_label(font_name == current_font, font_name)
                                        .clicked()
                                    {
                                        action =
                                            Some(TitleBarAction::FontChange(font_name.clone()));
                                        ui.close();
                                    }
                                }
                            });
                    });
                    ui.menu_button(
                        format!("{} 插件", egui_phosphor::regular::PUZZLE_PIECE),
                        |ui| {
                            ui.set_min_width(220.0);
                            if plugins.is_empty() {
                                ui.add_enabled(false, egui::Button::new("暂无已安装插件"));
                            }
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
                            ui.separator();
                            if plugins.iter().any(|p| p.id == "github_publish")
                                && ui.button("配置 GitHub 发布…").clicked()
                            {
                                action = Some(TitleBarAction::ConfigurePlugin(
                                    "github_publish".to_string(),
                                ));
                                ui.close();
                            }
                            if ui
                                .button("打开插件目录…")
                                .on_hover_text("将插件文件夹放入此目录即可安装")
                                .clicked()
                            {
                                action = Some(TitleBarAction::OpenPluginsFolder);
                                ui.close();
                            }
                        },
                    );
                });
            });
        });
        action
    }

    fn menu_item(ui: &mut Ui, label: &str, key: Option<egui::Key>) -> egui::Response {
        let mut button = egui::Button::new(label);
        if let Some(key) = key {
            button = button.shortcut_text(
                ui.ctx()
                    .format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key)),
            );
        }
        ui.add(button)
    }

    fn window_controls(ui: &mut Ui) {
        if ui
            .add_sized([22.0, 20.0], egui::Button::new(egui_phosphor::regular::X))
            .on_hover_text("关闭窗口")
            .clicked()
        {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let icon = if fullscreen {
            egui_phosphor::regular::CORNERS_IN
        } else {
            egui_phosphor::regular::CORNERS_OUT
        };
        if ui
            .add_sized([22.0, 20.0], egui::Button::new(icon))
            .on_hover_text(if fullscreen { "退出全屏" } else { "全屏" })
            .clicked()
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
        }
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
    }

    /// Consume shortcuts before the editor can handle the same key events.
    pub fn shortcut(ctx: &egui::Context) -> Option<TitleBarAction> {
        ctx.input_mut(|input| {
            if input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::E,
            )) {
                return Some(TitleBarAction::ToggleSidebar);
            }
            for (key, action) in [
                (egui::Key::N, TitleBarAction::NewWindow),
                (egui::Key::O, TitleBarAction::Open),
                (egui::Key::S, TitleBarAction::Save),
                (egui::Key::F, TitleBarAction::SearchReplace),
            ] {
                if input
                    .consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key))
                {
                    return Some(action);
                }
            }
            None
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_title_bar_fits_narrow_windows_with_long_document_names() {
        for (width, selection_word_count) in [
            (300.0, None),
            (480.0, Some(0)),
            (600.0, None),
            (720.0, Some(999_999)),
            (750.0, Some(999_999)),
            (780.0, Some(999_999)),
            (1000.0, Some(999_999)),
        ] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 468.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::TopBottomPanel::top("title").show(ctx, |ui| {
                    let available = ui.available_width();
                    TitleBar::show(ui, TitleBarState {
                        title: "A very long document name that must leave window controls accessible.txt",
                        document_path: None,
                        word_count: 1_000_000,
                        cursor_word_count: 999_999,
                        selection_word_count,
                        writing_time: 360_000,
                        has_current_file: true,
                        chinese_fonts: &[],
                        current_font: "Default",
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: false,
                    });
                    assert!(ui.min_rect().width() <= available + 1.0, "title overflow at {width}");
                    assert!(ui.min_rect().height() <= 24.0, "title bar must remain a single compact row");
                });
            });
        }
    }

    #[test]
    fn command_shortcuts_dispatch_once_and_leave_plain_typing_alone() {
        for modifiers in [egui::Modifiers::COMMAND, egui::Modifiers::NONE] {
            for key in [egui::Key::N, egui::Key::O, egui::Key::S, egui::Key::F] {
                let ctx = egui::Context::default();
                let _ = ctx.run(
                    egui::RawInput {
                        modifiers,
                        events: vec![egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        }],
                        ..Default::default()
                    },
                    |ctx| {
                        let action = TitleBar::shortcut(ctx);
                        if modifiers.command {
                            assert!(matches!(
                                (key, action),
                                (egui::Key::N, Some(TitleBarAction::NewWindow))
                                    | (egui::Key::O, Some(TitleBarAction::Open))
                                    | (egui::Key::S, Some(TitleBarAction::Save))
                                    | (egui::Key::F, Some(TitleBarAction::SearchReplace))
                            ));
                            assert!(TitleBar::shortcut(ctx).is_none());
                        } else {
                            assert!(action.is_none());
                            assert!(ctx.input(|input| input.key_pressed(key)));
                        }
                    },
                );
            }
        }
    }
}
