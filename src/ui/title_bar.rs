use crate::export::ExportFormat;
use crate::plugin::PluginMetadata;
use egui::{Align, Layout, Ui};
use std::path::PathBuf;

pub enum TitleBarAction {
    NewWindow,
    ToggleSidebar,
    ToggleVimMode,
    Save,
    Export(ExportFormat),
    Open,
    OpenFile(PathBuf),
    History,
    Statistics,
    Format,
    SetFormatUseTabs(bool),
    FontChange(String),
    FontColorChange(String),
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
    About,
    SelectSkin(String),
    ImportSkin,
    ExportCurrentSkin,
    OpenSkinMarket,
}

pub struct TitleBar;

pub struct TitleBarState<'a> {
    pub update_available: bool,
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
    pub font_color: &'a str,
    pub format_use_tabs: bool,
    pub recent_files: &'a [PathBuf],
    pub plugins: &'a [PluginMetadata],
    pub vim_mode: bool,
    pub vim_mode_label: Option<&'a str>,
    pub skins: &'a [crate::skin::Skin],
    pub current_skin_id: &'a str,
}

impl TitleBar {
    pub fn show(ui: &mut Ui, state: TitleBarState<'_>) -> Option<TitleBarAction> {
        let TitleBarState {
            update_available,
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
            font_color,
            format_use_tabs,
            recent_files,
            plugins,
            vim_mode,
            vim_mode_label,
            skins,
            current_skin_id,
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
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{}{} / {}  ·  {}",
                                    selection_label, cursor_word_count, word_count, time
                                ))
                                .small(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(format!("{detail}\n点击查看写作统计"))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action = Some(TitleBarAction::Statistics);
                    }
                    if let Some(label) = vim_mode_label {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(label).small().strong());
                    }
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
                            ui.menu_button("导出", |ui| {
                                ui.set_min_width(180.0);
                                for format in [
                                    ExportFormat::Pdf,
                                    ExportFormat::Html,
                                    ExportFormat::Markdown,
                                    ExportFormat::PlainText,
                                ] {
                                    if ui.button(format.menu_label()).clicked() {
                                        action = Some(TitleBarAction::Export(format));
                                        ui.close();
                                    }
                                }
                            });
                            if ui
                                .add_enabled(has_current_file, egui::Button::new("版本历史…"))
                                .on_disabled_hover_text("保存文档后可查看版本历史")
                                .clicked()
                            {
                                action = Some(TitleBarAction::History);
                                ui.close();
                            }
                            if ui.button("写作统计…").clicked() {
                                action = Some(TitleBarAction::Statistics);
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .button(if update_available {
                                    "关于纸壳 · 有新版本"
                                } else {
                                    "关于纸壳"
                                })
                                .on_hover_text("查看版本与检查更新")
                                .clicked()
                            {
                                action = Some(TitleBarAction::About);
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
                            let mut checked = vim_mode;
                            if ui.checkbox(&mut checked, "Vim 模式 (Esc)").clicked() {
                                action = Some(TitleBarAction::ToggleVimMode);
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
                        .config(
                            egui::containers::menu::MenuConfig::default()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
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
                                        }
                                    }
                                });
                        });

                        ui.separator();

                        // 4. 字体颜色选择 (子菜单)
                        let current_color_label =
                            crate::config::font_color_display_name(font_color);
                        egui::containers::menu::SubMenuButton::from_button(
                            egui::Button::new(format!(
                                "{} 字体颜色",
                                egui_phosphor::regular::PALETTE
                            ))
                            .right_text(format!("{current_color_label} ⏵")),
                        )
                        .config(
                            egui::containers::menu::MenuConfig::default()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            let sub_width = 196.0;
                            ui.set_width(sub_width);
                            ui.set_max_width(sub_width);

                            let is_default = font_color == "Default" || font_color.is_empty();
                            if ui
                                .add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::Button::selectable(is_default, "系统默认"),
                                )
                                .on_hover_text("跟随主题/系统默认文字颜色")
                                .clicked()
                            {
                                action =
                                    Some(TitleBarAction::FontColorChange("Default".to_string()));
                            }

                            ui.separator();

                            egui::ScrollArea::vertical()
                                .max_height(280.0)
                                .show(ui, |ui| {
                                    for (name, hex, color) in crate::config::FONT_COLOR_PRESETS {
                                        let is_selected = font_color.eq_ignore_ascii_case(hex);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 6.0;
                                            let (rect, swatch_resp) = ui.allocate_exact_size(
                                                egui::vec2(14.0, 14.0),
                                                egui::Sense::click(),
                                            );
                                            ui.painter().rect_filled(rect, 2.0, *color);
                                            ui.painter().rect_stroke(
                                                rect,
                                                2.0,
                                                egui::Stroke::new(
                                                    1.0,
                                                    ui.visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color,
                                                ),
                                                egui::StrokeKind::Inside,
                                            );
                                            if swatch_resp.clicked() {
                                                action = Some(TitleBarAction::FontColorChange(
                                                    hex.to_string(),
                                                ));
                                            }
                                            if ui
                                                .add_sized(
                                                    [ui.available_width(), 20.0],
                                                    egui::Button::selectable(
                                                        is_selected,
                                                        format!("{name}  {hex}"),
                                                    ),
                                                )
                                                .clicked()
                                            {
                                                action = Some(TitleBarAction::FontColorChange(
                                                    hex.to_string(),
                                                ));
                                            }
                                        });
                                    }

                                    ui.separator();

                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("自定义").small());
                                        let mut custom_srgb = if let Some(c) =
                                            crate::config::parse_hex_color(font_color)
                                        {
                                            [c.r(), c.g(), c.b()]
                                        } else {
                                            let dc = ui.visuals().text_color();
                                            [dc.r(), dc.g(), dc.b()]
                                        };
                                        if ui.color_edit_button_srgb(&mut custom_srgb).changed() {
                                            let hex = crate::config::color_to_hex(
                                                egui::Color32::from_rgb(
                                                    custom_srgb[0],
                                                    custom_srgb[1],
                                                    custom_srgb[2],
                                                ),
                                            );
                                            action = Some(TitleBarAction::FontColorChange(hex));
                                        }
                                        if !is_default
                                            && !crate::config::FONT_COLOR_PRESETS
                                                .iter()
                                                .any(|(_, h, _)| font_color.eq_ignore_ascii_case(h))
                                        {
                                            ui.monospace(font_color);
                                        }
                                    });
                                });
                        });

                        ui.separator();

                        // 5. 皮肤风格 (Skin Style)
                        let current_skin_name = skins
                            .iter()
                            .find(|s| s.id == current_skin_id)
                            .map(|s| s.name.as_str())
                            .unwrap_or("默认");

                        egui::containers::menu::SubMenuButton::from_button(
                            egui::Button::new(format!(
                                "{} 皮肤风格",
                                egui_phosphor::regular::PALETTE
                            ))
                            .right_text(format!("{current_skin_name} ⏵")),
                        )
                        .config(
                            egui::containers::menu::MenuConfig::default()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            let skin_menu_width = 180.0;
                            ui.set_width(skin_menu_width);
                            ui.set_max_width(skin_menu_width);

                            for skin in skins {
                                let is_selected = skin.id == current_skin_id;
                                let label = if is_selected {
                                    format!("{} {}", egui_phosphor::regular::CHECK, skin.name)
                                } else {
                                    format!("   {}", skin.name)
                                };
                                let mut response = ui.selectable_label(is_selected, label);
                                if !skin.description.is_empty() {
                                    response = response.on_hover_text(&skin.description);
                                }
                                if response.clicked() {
                                    action = Some(TitleBarAction::SelectSkin(skin.id.clone()));
                                }
                            }

                            ui.separator();

                            if ui
                                .button(format!(
                                    "{} 导入皮肤…",
                                    egui_phosphor::regular::DOWNLOAD_SIMPLE
                                ))
                                .clicked()
                            {
                                action = Some(TitleBarAction::ImportSkin);
                            }
                            if ui
                                .button(format!("{} 导出当前皮肤…", egui_phosphor::regular::EXPORT))
                                .clicked()
                            {
                                action = Some(TitleBarAction::ExportCurrentSkin);
                            }
                            if ui
                                .button(format!(
                                    "{} 皮肤市场与管理…",
                                    egui_phosphor::regular::STOREFRONT
                                ))
                                .clicked()
                            {
                                action = Some(TitleBarAction::OpenSkinMarket);
                            }
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            if ui
                                .button("整理段落缩进")
                                .on_hover_text(if format_use_tabs {
                                    "为段落首行添加 Tab 缩进"
                                } else {
                                    "为段落首行添加两个空格"
                                })
                                .clicked()
                            {
                                action = Some(TitleBarAction::Format);
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let mut use_tabs = format_use_tabs;
                                if Self::format_indent_switch(ui, &mut use_tabs)
                                    .on_hover_text(if format_use_tabs {
                                        "当前使用 Tab 缩进，点击切换为空格"
                                    } else {
                                        "当前使用空格缩进，点击切换为 Tab"
                                    })
                                    .changed()
                                {
                                    action = Some(TitleBarAction::SetFormatUseTabs(use_tabs));
                                }
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(if use_tabs { "Tab" } else { "空格" })
                                        .small(),
                                )
                                .on_hover_text("一键 Format 的缩进方式");
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
                                .button(format!(
                                    "{} 皮肤市场与管理…",
                                    egui_phosphor::regular::PALETTE
                                ))
                                .on_hover_text("管理皮肤、从 GitHub 市场安装或导入导出")
                                .clicked()
                            {
                                action = Some(TitleBarAction::OpenSkinMarket);
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

    pub fn paint_bottom_border(ctx: &egui::Context, panel_rect: egui::Rect) {
        // The panels disable their built-in separator so this remains the only
        // title-bar hairline even though the shared separator style is visible.
        let color = ctx.style().visuals.text_color().gamma_multiply(0.18);
        ctx.layer_painter(egui::LayerId::background()).hline(
            panel_rect.x_range(),
            panel_rect.bottom() - 0.5,
            egui::Stroke::new(1.0_f32, color),
        );
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

    fn format_indent_switch(ui: &mut Ui, use_tabs: &mut bool) -> egui::Response {
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(26.0, 14.0), egui::Sense::click());
        if response.clicked() {
            *use_tabs = !*use_tabs;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                *use_tabs,
                "段落缩进使用 Tab",
            )
        });

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, *use_tabs);
            let track_rect = rect.expand(visuals.expansion);
            let radius = track_rect.height() / 2.0;
            ui.painter().rect(
                track_rect,
                radius,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            let thumb_x = if *use_tabs {
                track_rect.right() - radius
            } else {
                track_rect.left() + radius
            };
            ui.painter().circle_filled(
                egui::pos2(thumb_x, track_rect.center().y),
                radius - 2.0,
                ui.visuals().window_fill(),
            );
        }
        response
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

    const TEST_SCREEN_SIZE: egui::Vec2 = egui::vec2(1000.0, 700.0);

    fn test_screen_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, TEST_SCREEN_SIZE)
    }

    fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn text_position(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        output.shapes.iter().find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text().contains(needle)
            {
                Some(text.pos + text.galley.size() * 0.5)
            } else {
                None
            }
        })
    }

    fn has_text(output: &egui::FullOutput, needle: &str) -> bool {
        text_position(output, needle).is_some()
    }

    fn render_typography_title_bar(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        render_typography_title_bar_with_color(
            ctx,
            events,
            fonts,
            current_font,
            font_size,
            line_spacing,
            "Default",
        )
    }

    fn render_typography_title_bar_with_color(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
        font_color: &str,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        render_typography_title_bar_with_options(
            ctx,
            events,
            fonts,
            current_font,
            font_size,
            line_spacing,
            font_color,
            false,
        )
    }

    fn render_typography_title_bar_with_indent(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
        format_use_tabs: bool,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        render_typography_title_bar_with_options(
            ctx,
            events,
            fonts,
            current_font,
            font_size,
            line_spacing,
            "Default",
            format_use_tabs,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_typography_title_bar_with_options(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
        font_color: &str,
        format_use_tabs: bool,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        let mut action = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(test_screen_rect()),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::TopBottomPanel::top("title").show(ctx, |ui| {
                    action = TitleBar::show(
                        ui,
                        TitleBarState {
                            update_available: false,
                            title: "Test Document",
                            document_path: Some("/tmp/test.txt"),
                            word_count: 100,
                            cursor_word_count: 50,
                            selection_word_count: None,
                            writing_time: 120,
                            has_current_file: true,
                            chinese_fonts: fonts,
                            current_font,
                            font_size,
                            line_spacing,
                            font_color,
                            format_use_tabs,
                            recent_files: &[],
                            plugins: &[],
                            sidebar_expanded: true,
                            vim_mode: false,
                            vim_mode_label: None,
                            skins: &[],
                            current_skin_id: "classic_paper",
                        },
                    );
                });
            },
        );
        (output, action)
    }

    fn click_text(
        ctx: &egui::Context,
        output: &egui::FullOutput,
        needle: &str,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        click_text_with_color(
            ctx,
            output,
            needle,
            fonts,
            current_font,
            font_size,
            line_spacing,
            "Default",
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn click_text_with_color(
        ctx: &egui::Context,
        output: &egui::FullOutput,
        needle: &str,
        fonts: &[String],
        current_font: &str,
        font_size: f32,
        line_spacing: f32,
        font_color: &str,
    ) -> (egui::FullOutput, Option<TitleBarAction>) {
        let pos = text_position(output, needle)
            .unwrap_or_else(|| panic!("expected visible text containing {needle:?}"));
        render_typography_title_bar_with_color(
            ctx,
            click_events(pos),
            fonts,
            current_font,
            font_size,
            line_spacing,
            font_color,
        )
    }

    #[test]
    fn file_menu_about_dispatches_action() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = [];
        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "文件", &fonts, "Default", 14.0, 1.5);
        let (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&menu, "关于纸壳"));

        let (_, action) = click_text(&ctx, &menu, "关于纸壳", &fonts, "Default", 14.0, 1.5);
        assert!(matches!(action, Some(TitleBarAction::About)));
    }

    #[test]
    fn file_menu_export_submenu_shows_and_dispatches_actions() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = [];
        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "文件", &fonts, "Default", 14.0, 1.5);
        let (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&menu, "导出"));

        let _ = click_text(&ctx, &menu, "导出", &fonts, "Default", 14.0, 1.5);
        let (export_menu, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&export_menu, "PDF 文档 (.pdf)…"));
        assert!(has_text(&export_menu, "HTML 文档 (.html)…"));
        assert!(has_text(&export_menu, "Markdown (.md)…"));
        assert!(has_text(&export_menu, "纯文本 (.txt)…"));

        let (_, action) = click_text(
            &ctx,
            &export_menu,
            "PDF 文档 (.pdf)…",
            &fonts,
            "Default",
            14.0,
            1.5,
        );
        assert!(matches!(
            action,
            Some(TitleBarAction::Export(ExportFormat::Pdf))
        ));
    }

    #[test]
    fn clicking_title_word_count_opens_statistics() {
        let ctx = egui::Context::default();
        let fonts = [];
        let _ = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let (output, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        // This label is right-aligned, so its galley origin is its right edge.
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().contains("50 / 100") => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap();
        let (_, action) =
            render_typography_title_bar(&ctx, click_events(pos), &fonts, "Default", 14.0, 1.5);
        assert!(matches!(action, Some(TitleBarAction::Statistics)));
    }

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
                        update_available: false,
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
                        font_color: "Default",
                        format_use_tabs: false,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: false,
                        vim_mode: false,
                        vim_mode_label: None,
                        skins: &[],
                        current_skin_id: "classic_paper",
                    });
                    assert!(ui.min_rect().width() <= available + 1.0, "title overflow at {width}");
                    assert!(ui.min_rect().height() <= 24.0, "title bar must remain a single compact row");
                });
            });
        }
    }

    #[test]
    fn title_bar_paints_one_custom_bottom_border_across_frames() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };

        for _ in 0..3 {
            let mut panel_bottom = 0.0;
            let output = ctx.run(input.clone(), |ctx| {
                let title_bar = egui::TopBottomPanel::top("title").show_separator_line(false);
                let panel = title_bar.show(ctx, |ui| {
                    let _ = TitleBar::show(
                        ui,
                        TitleBarState {
                            update_available: false,
                            title: "Test Document",
                            document_path: None,
                            word_count: 100,
                            cursor_word_count: 50,
                            selection_word_count: None,
                            writing_time: 120,
                            has_current_file: true,
                            chinese_fonts: &[],
                            current_font: "Default",
                            font_size: 14.0,
                            line_spacing: 1.5,
                            font_color: "Default",
                            format_use_tabs: false,
                            recent_files: &[],
                            plugins: &[],
                            sidebar_expanded: false,
                            vim_mode: false,
                            vim_mode_label: None,
                            skins: &[],
                            current_skin_id: "classic_paper",
                        },
                    );
                });
                panel_bottom = panel.response.rect.bottom();
                TitleBar::paint_bottom_border(ctx, panel.response.rect);
            });

            let border_y = panel_bottom - 0.5;
            let border_count = output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::LineSegment { points, stroke }
                        if points[0].y == points[1].y
                            && points[0].y == border_y
                            && (points[1].x - points[0].x).abs() >= 999.0
                            && stroke.width == 1.0
                            && stroke.color.a() > 0)
                })
                .count();
            assert_eq!(
                border_count, 1,
                "title bar must paint exactly one visible bottom border on every frame"
            );
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
            screen_rect: Some(test_screen_rect()),
            ..Default::default()
        };
        // Frame 1: render title bar
        let output1 = ctx.run(input.clone(), |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        update_available: false,
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
                        font_color: "Default",
                        format_use_tabs: false,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                        vim_mode: false,
                        vim_mode_label: None,
                        skins: &[],
                        current_skin_id: "classic_paper",
                    },
                );
            });
        });

        // Frame 2: locate and click the "排版" menu button from rendered text.
        let typography_pos =
            text_position(&output1, "排版").expect("typography menu button must exist");
        let input2 = egui::RawInput {
            screen_rect: Some(test_screen_rect()),
            events: click_events(typography_pos),
            ..Default::default()
        };
        let _ = ctx.run(input2, |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        update_available: false,
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
                        font_color: "Default",
                        format_use_tabs: false,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                        vim_mode: false,
                        vim_mode_label: None,
                        skins: &[],
                        current_skin_id: "classic_paper",
                    },
                );
            });
        });

        // Frame 3: popup is open; verify popup exists and has a compact, elegant width
        let input3 = egui::RawInput {
            screen_rect: Some(test_screen_rect()),
            ..Default::default()
        };
        let _ = ctx.run(input3, |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |ui| {
                let _ = TitleBar::show(
                    ui,
                    TitleBarState {
                        update_available: false,
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
                        font_color: "Default",
                        format_use_tabs: false,
                        recent_files: &[],
                        plugins: &[],
                        sidebar_expanded: true,
                        vim_mode: false,
                        vim_mode_label: None,
                        skins: &[],
                        current_skin_id: "classic_paper",
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
    fn format_is_in_typography_menu_and_not_edit_menu() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string(), "STSong".to_string()];
        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);

        let _ = click_text(&ctx, &initial, "编辑", &fonts, "Default", 14.0, 1.5);
        let (edit_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&edit_open, "查找与替换"));
        assert!(has_text(&edit_open, "Vim 模式"));
        assert!(!has_text(&edit_open, "整理段落缩进"));

        let _ = click_text(&ctx, &edit_open, "排版", &fonts, "Default", 14.0, 1.5);
        let (typography_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&typography_open, "字号"));
        assert!(has_text(&typography_open, "行间距"));
        assert!(has_text(&typography_open, "整理段落缩进"));
        assert!(has_text(&typography_open, "重置默认排版"));
    }

    #[test]
    fn format_indent_switch_dispatches_selection_and_keeps_menu_open() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string()];

        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "排版", &fonts, "Default", 14.0, 1.5);
        let (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let mode_rect = menu
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "空格" => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("space indentation label must be visible");
        let switch_rect = menu
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - 26.0).abs() < f32::EPSILON
                        && (rect.rect.height() - 14.0).abs() < f32::EPSILON
                        && (rect.rect.center().y - mode_rect.center().y).abs() < 2.0
                        && rect.rect.left() >= mode_rect.right() =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("small switch must be right-aligned beside its label");

        let (_, action) = render_typography_title_bar_with_indent(
            &ctx,
            click_events(switch_rect.center()),
            &fonts,
            "Default",
            14.0,
            1.5,
            false,
        );
        assert!(matches!(
            action,
            Some(TitleBarAction::SetFormatUseTabs(true))
        ));

        let (menu, _) = render_typography_title_bar_with_indent(
            &ctx,
            vec![],
            &fonts,
            "Default",
            14.0,
            1.5,
            true,
        );
        assert!(has_text(&menu, "字号"));
        assert!(has_text(&menu, "Tab"));
    }

    #[test]
    fn font_selection_keeps_both_menus_open_for_consecutive_changes() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string(), "STSong".to_string()];

        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "排版", &fonts, "Default", 14.0, 1.5);
        let (typography_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &typography_open, "字体", &fonts, "Default", 14.0, 1.5);
        let (font_menu_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&font_menu_open, "系统默认"));
        assert!(has_text(&font_menu_open, "PingFang SC"));
        assert!(has_text(&font_menu_open, "STSong"));

        let (_, first_action) = click_text(
            &ctx,
            &font_menu_open,
            "STSong",
            &fonts,
            "Default",
            14.0,
            1.5,
        );
        assert!(matches!(
            first_action,
            Some(TitleBarAction::FontChange(ref font)) if font == "STSong"
        ));
        let (after_first_change, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "STSong", 14.0, 1.5);
        assert!(has_text(&after_first_change, "字号"));
        assert!(has_text(&after_first_change, "系统默认"));
        assert!(has_text(&after_first_change, "PingFang SC"));

        let (_, second_action) = click_text(
            &ctx,
            &after_first_change,
            "PingFang SC",
            &fonts,
            "STSong",
            14.0,
            1.5,
        );
        assert!(matches!(
            second_action,
            Some(TitleBarAction::FontChange(ref font)) if font == "PingFang SC"
        ));
        let (after_second_change, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "PingFang SC", 14.0, 1.5);
        assert!(has_text(&after_second_change, "字号"));
        assert!(has_text(&after_second_change, "系统默认"));
        assert!(has_text(&after_second_change, "STSong"));
    }

    #[test]
    fn font_color_selection_keeps_both_menus_open_for_consecutive_changes() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string()];

        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "排版", &fonts, "Default", 14.0, 1.5);
        let (typography_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&typography_open, "字体颜色"));

        let _ = click_text(
            &ctx,
            &typography_open,
            "字体颜色",
            &fonts,
            "Default",
            14.0,
            1.5,
        );
        let (color_menu_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&color_menu_open, "系统默认"));
        assert!(has_text(&color_menu_open, "墨黑"));
        assert!(has_text(&color_menu_open, "墨蓝"));

        let (_, first_action) =
            click_text(&ctx, &color_menu_open, "墨蓝", &fonts, "Default", 14.0, 1.5);
        assert!(matches!(
            first_action,
            Some(TitleBarAction::FontColorChange(ref hex)) if hex == "#1E3A5F"
        ));

        let (after_first_change, _) = render_typography_title_bar_with_color(
            &ctx,
            vec![],
            &fonts,
            "Default",
            14.0,
            1.5,
            "#1E3A5F",
        );
        assert!(has_text(&after_first_change, "字号"));
        assert!(has_text(&after_first_change, "系统默认"));
        assert!(has_text(&after_first_change, "墨黑"));
        assert!(has_text(&after_first_change, "深灰"));

        let (_, second_action) = click_text_with_color(
            &ctx,
            &after_first_change,
            "深灰",
            &fonts,
            "Default",
            14.0,
            1.5,
            "#1E3A5F",
        );
        assert!(matches!(
            second_action,
            Some(TitleBarAction::FontColorChange(ref hex)) if hex == "#4A4A4A"
        ));
    }

    #[test]
    fn typography_actions_dispatch_and_keep_the_menu_open() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string()];

        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "排版", &fonts, "Default", 14.0, 1.5);
        let (mut menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);

        let (_, format_action) =
            click_text(&ctx, &menu, "整理段落缩进", &fonts, "Default", 14.0, 1.5);
        assert!(matches!(format_action, Some(TitleBarAction::Format)));
        (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&menu, "字号"));

        let (_, reset_action) =
            click_text(&ctx, &menu, "重置默认排版", &fonts, "Default", 14.0, 1.5);
        assert!(matches!(
            reset_action,
            Some(TitleBarAction::ResetTypography)
        ));
        (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&menu, "行间距"));

        let (_, size_action) = click_text(&ctx, &menu, "16", &fonts, "Default", 14.0, 1.5);
        assert!(matches!(
            size_action,
            Some(TitleBarAction::FontSizeChange(size)) if (size - 16.0).abs() < f32::EPSILON
        ));
        (menu, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 16.0, 1.5);
        assert!(has_text(&menu, "整理段落缩进"));

        let (_, spacing_action) = click_text(&ctx, &menu, "1.8", &fonts, "Default", 16.0, 1.5);
        assert!(matches!(
            spacing_action,
            Some(TitleBarAction::LineSpacingChange(spacing))
                if (spacing - 1.8).abs() < f32::EPSILON
        ));
        let (after_spacing, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 16.0, 1.8);
        assert!(has_text(&after_spacing, "重置默认排版"));
    }

    #[test]
    fn clicking_outside_closes_typography_and_font_menus() {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let fonts = vec!["PingFang SC".to_string(), "STSong".to_string()];

        let (initial, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &initial, "排版", &fonts, "Default", 14.0, 1.5);
        let (typography_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        let _ = click_text(&ctx, &typography_open, "字体", &fonts, "Default", 14.0, 1.5);
        let (font_menu_open, _) =
            render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(has_text(&font_menu_open, "STSong"));

        let outside = test_screen_rect().right_bottom() - egui::vec2(16.0, 16.0);
        let _ =
            render_typography_title_bar(&ctx, click_events(outside), &fonts, "Default", 14.0, 1.5);
        let (closed, _) = render_typography_title_bar(&ctx, vec![], &fonts, "Default", 14.0, 1.5);
        assert!(!has_text(&closed, "字号"));
        assert!(!has_text(&closed, "整理段落缩进"));
        assert!(!has_text(&closed, "STSong"));
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
                                    update_available: false,
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
                                    font_color: "Default",
                                    format_use_tabs: false,
                                    recent_files: &[],
                                    plugins: &[],
                                    sidebar_expanded: false,
                                    vim_mode: false,
                                    vim_mode_label: None,
                                    skins: &[],
                                    current_skin_id: "classic_paper",
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
            output3.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::LineSegment { points, stroke }
                    if points[0].y == points[1].y
                        && points[0].y > 35.0
                        && (180.0..=240.0).contains(&(points[1].x - points[0].x).abs())
                        && stroke.width > 0.0
                        && stroke.color.a() > 0)
            }),
            "File menu should paint a visible separator"
        );
        assert!(
            has_text(&output3, "新建窗口"),
            "File menu should contain '新建窗口'"
        );
        assert!(
            has_text(&output3, "保存"),
            "File menu should contain '保存'"
        );
        assert!(
            has_text(&output3, "导出"),
            "File menu should contain '导出'"
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
