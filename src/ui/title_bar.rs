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
    FontSizeChange(f32),
    LineSpacingChange(f32),
    ResetTypography,
    SearchReplace,
    /// Run an installed plugin by its id.
    RunPlugin(String),
    /// Open the configuration window for a built-in plugin.
    ConfigurePlugin(String),
    /// Open the plugins directory in the system file manager.
    OpenPluginsFolder,
    /// Open the unified plugin management window.
    ManagePlugins,
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
    pub font_size: f32,
    pub line_spacing: f32,
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
            font_size,
            line_spacing,
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
                            ui.painter().circle_stroke(
                                dot,
                                1.4,
                                egui::Stroke::new(0.7_f32, dot_color),
                            );
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
                                } else {
                                    egui::ScrollArea::vertical()
                                        .max_height(320.0)
                                        .show(ui, |ui| {
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
                                                    action = Some(TitleBarAction::OpenFile(
                                                        path.clone(),
                                                    ));
                                                    ui.close();
                                                }
                                            }
                                        });
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
                    egui::containers::menu::MenuButton::new(format!(
                        "{} 排版",
                        egui_phosphor::regular::TEXT_ALIGN_LEFT
                    ))
                    .config(
                        egui::containers::menu::MenuConfig::default()
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                    )
                    .ui(ui, |ui| {
                        let menu_width = 196.0;
                        ui.set_width(menu_width);
                        ui.set_max_width(menu_width);

                        // 1. 字号调整
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("字号").strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if ui
                                    .add_enabled(
                                        font_size < 36.0,
                                        egui::Button::new(egui_phosphor::regular::PLUS),
                                    )
                                    .on_hover_text("增大字号")
                                    .clicked()
                                {
                                    action = Some(TitleBarAction::FontSizeChange(
                                        (font_size + 1.0).min(36.0),
                                    ));
                                }
                                ui.add_sized(
                                    [32.0, 20.0],
                                    egui::Label::new(
                                        egui::RichText::new(format!("{font_size:.0}")).monospace(),
                                    ),
                                );
                                if ui
                                    .add_enabled(
                                        font_size > 10.0,
                                        egui::Button::new(egui_phosphor::regular::MINUS),
                                    )
                                    .on_hover_text("减小字号")
                                    .clicked()
                                {
                                    action = Some(TitleBarAction::FontSizeChange(
                                        (font_size - 1.0).max(10.0),
                                    ));
                                }
                            });
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            for size in [12.0, 14.0, 16.0, 18.0, 20.0, 24.0] {
                                let is_selected = (font_size - size).abs() < 0.5;
                                if ui
                                    .add_sized(
                                        [26.0, 20.0],
                                        egui::Button::selectable(is_selected, format!("{size:.0}")),
                                    )
                                    .clicked()
                                {
                                    action = Some(TitleBarAction::FontSizeChange(size));
                                }
                            }
                        });

                        ui.separator();

                        // 2. 行间距调整
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("行间距").strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if ui
                                    .add_enabled(
                                        line_spacing < 3.0,
                                        egui::Button::new(egui_phosphor::regular::PLUS),
                                    )
                                    .on_hover_text("增加行间距")
                                    .clicked()
                                {
                                    let next = ((line_spacing + 0.1) * 10.0).round() / 10.0;
                                    action = Some(TitleBarAction::LineSpacingChange(next.min(3.0)));
                                }
                                ui.add_sized(
                                    [34.0, 20.0],
                                    egui::Label::new(
                                        egui::RichText::new(format!("{line_spacing:.1}x"))
                                            .monospace(),
                                    ),
                                );
                                if ui
                                    .add_enabled(
                                        line_spacing > 1.0,
                                        egui::Button::new(egui_phosphor::regular::MINUS),
                                    )
                                    .on_hover_text("减小行间距")
                                    .clicked()
                                {
                                    let next = ((line_spacing - 0.1) * 10.0).round() / 10.0;
                                    action = Some(TitleBarAction::LineSpacingChange(next.max(1.0)));
                                }
                            });
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (spacing, label, tip) in [
                                (1.2, "1.2", "紧凑 1.2x"),
                                (1.5, "1.5", "标准 1.5x"),
                                (1.8, "1.8", "宽松 1.8x"),
                                (2.0, "2.0", "双倍 2.0x"),
                            ] {
                                let is_selected = (line_spacing - spacing).abs() < 0.05;
                                if ui
                                    .add_sized(
                                        [40.0, 20.0],
                                        egui::Button::selectable(is_selected, label),
                                    )
                                    .on_hover_text(tip)
                                    .clicked()
                                {
                                    action = Some(TitleBarAction::LineSpacingChange(spacing));
                                }
                            }
                        });

                        ui.separator();

                        // 3. 字体选择 (子菜单)
                        let current_font_label = if current_font == "Default" {
                            "系统默认"
                        } else {
                            current_font
                        };
                        egui::containers::menu::SubMenuButton::from_button(
                            egui::Button::new(format!("{} 字体", egui_phosphor::regular::TEXT_T))
                                .right_text(format!("{current_font_label} ⏵")),
                        )
                        .ui(ui, |ui| {
                            let sub_width = 196.0;
                            ui.set_width(sub_width);
                            ui.set_max_width(sub_width);
                            let is_default = current_font == "Default";
                            if ui
                                .add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::Button::selectable(is_default, "系统默认"),
                                )
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
                                        let is_selected = font_name == current_font;
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 20.0],
                                                egui::Button::selectable(
                                                    is_selected,
                                                    font_name.as_str(),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            action =
                                                Some(TitleBarAction::FontChange(font_name.clone()));
                                            ui.close();
                                        }
                                    }
                                });
                        });

                        ui.separator();

                        if ui
                            .add_sized(
                                [ui.available_width(), 22.0],
                                egui::Button::new(format!(
                                    "{} 重置默认排版",
                                    egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                                )),
                            )
                            .clicked()
                        {
                            action = Some(TitleBarAction::ResetTypography);
                            ui.close();
                        }
                    });
                    ui.menu_button(
                        format!("{} 插件", egui_phosphor::regular::PUZZLE_PIECE),
                        |ui| {
                            ui.set_min_width(220.0);
                            if plugins.is_empty() {
                                ui.add_enabled(false, egui::Button::new("暂无已启用插件"));
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
                            if plugins.iter().any(|p| p.id == "cloud_backup")
                                && ui.button("配置云端备份…").clicked()
                            {
                                action = Some(TitleBarAction::ConfigurePlugin(
                                    "cloud_backup".to_string(),
                                ));
                                ui.close();
                            }
                            if plugins.iter().any(|p| p.id == "github_publish")
                                && ui.button("配置 GitHub 发布…").clicked()
                            {
                                action = Some(TitleBarAction::ConfigurePlugin(
                                    "github_publish".to_string(),
                                ));
                                ui.close();
                            }
                            if ui
                                .button("管理插件…")
                                .on_hover_text("启用、禁用、刷新、设置或卸载插件")
                                .clicked()
                            {
                                action = Some(TitleBarAction::ManagePlugins);
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
                        font_size: 14.0,
                        line_spacing: 1.5,
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

    #[test]
    fn title_bar_renders_typography_state() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string(), "STSong".to_string()];
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        // Frame 1: render title bar
        let _ = ctx.run(input.clone(), |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        title: "Test Document",
                        document_path: Some("/tmp/test.txt"),
                        word_count: 100,
                        cursor_word_count: 50,
                        selection_word_count: None,
                        writing_time: 120,
                        has_current_file: true,
                        chinese_fonts: &fonts,
                        current_font: "PingFang SC",
                        font_size: 16.0,
                        line_spacing: 1.8,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                    },
                );
            });
        });

        // Frame 2: click the "排版" menu button (x = 174.0, y = 12.0)
        let input2 = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events: vec![
                egui::Event::PointerButton {
                    pos: egui::pos2(174.0, 12.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: egui::pos2(174.0, 12.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run(input2, |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        title: "Test Document",
                        document_path: Some("/tmp/test.txt"),
                        word_count: 100,
                        cursor_word_count: 50,
                        selection_word_count: None,
                        writing_time: 120,
                        has_current_file: true,
                        chinese_fonts: &fonts,
                        current_font: "PingFang SC",
                        font_size: 16.0,
                        line_spacing: 1.8,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                    },
                );
            });
        });

        // Frame 3: popup is open; verify popup exists and has a compact, elegant width
        let input3 = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input3, |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        title: "Test Document",
                        document_path: Some("/tmp/test.txt"),
                        word_count: 100,
                        cursor_word_count: 50,
                        selection_word_count: None,
                        writing_time: 120,
                        has_current_file: true,
                        chinese_fonts: &fonts,
                        current_font: "PingFang SC",
                        font_size: 16.0,
                        line_spacing: 1.8,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                    },
                );
            });
            // Inspect popup layer bounds
            ctx.memory(|mem| {
                for layer_id in mem.layer_ids() {
                    if layer_id.order != egui::Order::Background
                        && let Some(rect) = mem.area_rect(layer_id.id)
                    {
                        assert!(
                            rect.width() <= 240.0,
                            "popup width {} exceeds limit 240.0",
                            rect.width()
                        );
                    }
                }
            });
        });
    }

    #[test]
    fn file_menu_does_not_contain_toggle_article_list() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);

        let find_text_position = |output: &egui::FullOutput, needle: &str| -> Option<egui::Pos2> {
            output.shapes.iter().find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text().contains(needle)
                {
                    Some(text.pos + text.galley.size() * 0.5)
                } else {
                    None
                }
            })
        };

        let has_text = |output: &egui::FullOutput, needle: &str| -> bool {
            output.shapes.iter().any(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    text.galley.text().contains(needle)
                } else {
                    false
                }
            })
        };

        let render_title_bar =
            |ctx: &egui::Context, events: Vec<egui::Event>| -> egui::FullOutput {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 700.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::TopBottomPanel::top("title").show(ctx, |ui| {
                            let _ = TitleBar::show(
                                ui,
                                TitleBarState {
                                    title: "Test Document",
                                    document_path: Some("/tmp/test.txt"),
                                    word_count: 100,
                                    cursor_word_count: 50,
                                    selection_word_count: None,
                                    writing_time: 120,
                                    has_current_file: true,
                                    chinese_fonts: &[],
                                    current_font: "Default",
                                    font_size: 14.0,
                                    line_spacing: 1.5,
                                    recent_files: &[],
                                    plugins: &[],
                                    sidebar_expanded: false,
                                },
                            );
                        });
                    },
                )
            };

        // Frame 1: initial render
        let output1 = render_title_bar(&ctx, vec![]);
        let file_pos = find_text_position(&output1, "文件").expect("File menu button must exist");

        // Frame 2: click the File menu
        let click_events = vec![
            egui::Event::PointerButton {
                pos: file_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: file_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let _ = render_title_bar(&ctx, click_events);

        // Frame 3: file menu is open
        let output3 = render_title_bar(&ctx, vec![]);
        assert!(
            has_text(&output3, "新建窗口"),
            "File menu should contain '新建窗口'"
        );
        assert!(
            has_text(&output3, "保存"),
            "File menu should contain '保存'"
        );
        assert!(
            has_text(&output3, "版本历史…"),
            "File menu should contain '版本历史…'"
        );
        assert!(
            !has_text(&output3, "显示文章列表"),
            "File menu should not contain '显示文章列表'"
        );
        assert!(
            !has_text(&output3, "隐藏文章列表"),
            "File menu should not contain '隐藏文章列表'"
        );
    }
}
