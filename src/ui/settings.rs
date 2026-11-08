//! Application preferences. Plugin lifecycle and configuration live in the plugin manager.
use egui::{Context, RichText, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Typography,
    Skin,
}

impl SettingsTab {
    const ALL: [Self; 3] = [Self::General, Self::Typography, Self::Skin];

    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "写作与保存",
            Self::Typography => "文字排版",
            Self::Skin => "皮肤与外观",
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingsAction {
    ToggleVimMode,
    SetFormatUseTabs(bool),
    SetAutosaveInterval(u64),
    FontSizeChange(f32),
    LineSpacingChange(f32),
    FontChange(String),
    FontColorChange(String),
    ResetTypography,
    SelectSkin(String),
    ImportSkin,
    ExportCurrentSkin,
    ManageSkins,
    OpenSkinMarket,
    UpdateMarketSource {
        repo: String,
        branch: String,
        folder: String,
    },
    ResetMarketSource,
}

#[derive(Clone, Copy)]
pub struct SettingsWindowState<'a> {
    pub vim_mode: bool,
    pub format_use_tabs: bool,
    pub autosave_interval: u64,
    pub font_size: f32,
    pub line_spacing: f32,
    pub current_font: &'a str,
    pub chinese_fonts: &'a [String],
    pub font_color: &'a str,
    pub skins: &'a [crate::skin::Skin],
    pub current_skin_id: &'a str,
    pub market_repo: &'a str,
    pub market_branch: &'a str,
    pub market_folder: &'a str,
}

#[derive(Default)]
pub struct SettingsWindow {
    open: bool,
    active_tab: SettingsTab,
    status_message: Option<(bool, String)>,
    repo_input: String,
    branch_input: String,
    folder_input: String,
    market_buffers_initialized: bool,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, ctx: &Context, tab: Option<SettingsTab>) {
        if self.open {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of("settings_window"),
                egui::ViewportCommand::Focus,
            );
        } else {
            self.open = true;
            self.market_buffers_initialized = false;
        }
        if let Some(tab) = tab {
            self.active_tab = tab;
        }
        self.status_message = None;
        ctx.request_repaint();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.status_message = None;
    }

    pub fn set_status(&mut self, is_error: bool, text: String) {
        self.status_message = Some((is_error, text));
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        state: SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        if !self.open {
            return None;
        }
        if !self.market_buffers_initialized {
            self.repo_input = state.market_repo.into();
            self.branch_input = state.market_branch.into();
            self.folder_input = state.market_folder.into();
            self.market_buffers_initialized = true;
        }
        let mut action = None;
        let mut open = self.open;
        crate::ui::viewport::show_tool_window_with_min_size(
            ctx,
            "settings_window",
            "设置",
            [800.0, 600.0],
            [640.0, 420.0],
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                ui.spacing_mut().interact_size.y = 28.0;
                if ui.available_width() < 720.0 {
                    ui.horizontal_wrapped(|ui| self.show_navigation(ui));
                    ui.separator();
                    action = self.show_detail(ui, &state);
                } else {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(148.0);
                            ui.add_space(16.0);
                            ui.label(RichText::new("设置").size(22.0).strong());
                            ui.add_space(20.0);
                            self.show_navigation(ui);
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            action = self.show_detail(ui, &state);
                        });
                    });
                }
            },
        );
        self.open = open;
        action
    }

    fn show_navigation(&mut self, ui: &mut Ui) {
        for tab in SettingsTab::ALL {
            if ui
                .selectable_label(self.active_tab == tab, tab.label())
                .clicked()
            {
                self.active_tab = tab;
                self.status_message = None;
            }
        }
    }

    fn show_detail(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt(("settings_content", self.active_tab))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_max_width(600.0);
                        if let Some((is_error, msg)) = &self.status_message {
                            let (icon, color) = if *is_error {
                                (
                                    egui_phosphor::regular::WARNING_CIRCLE,
                                    ui.visuals().error_fg_color,
                                )
                            } else {
                                (
                                    egui_phosphor::regular::CHECK_CIRCLE,
                                    crate::style::STATUS_GREEN,
                                )
                            };
                            ui.label(RichText::new(format!("{icon} {msg}")).color(color));
                            ui.add_space(8.0);
                        }
                        action = match self.active_tab {
                            SettingsTab::General => self.show_general_tab(ui, state),
                            SettingsTab::Typography => self.show_typography_tab(ui, state),
                            SettingsTab::Skin => self.show_skin_tab(ui, state),
                        };
                        ui.add_space(20.0);
                    });
            });
        action
    }

    fn show_general_tab(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("写作与保存");
        ui.add_space(8.0);

        // 1. Vim Mode
        ui.label(RichText::new("编辑器交互").strong());
        let mut vim = state.vim_mode;
        if ui
            .checkbox(&mut vim, "启用 Vim 模式 (Esc 键切换模式)")
            .clicked()
        {
            action = Some(SettingsAction::ToggleVimMode);
        }
        ui.label(
            RichText::new(
                "开启后在普通模式下支持 h/j/k/l 移动，d/y/p 剪切复制，/ 查找等经典快捷键。",
            )
            .weak()
            .small(),
        );

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // 2. Format Indentation
        ui.label(RichText::new("段落首行整理缩进").strong());
        let mut use_tabs = state.format_use_tabs;
        ui.horizontal_wrapped(|ui| {
            if ui.radio_value(&mut use_tabs, false, "两个空格").clicked() {
                action = Some(SettingsAction::SetFormatUseTabs(false));
            }
            if ui.radio_value(&mut use_tabs, true, "Tab 制表符").clicked() {
                action = Some(SettingsAction::SetFormatUseTabs(true));
            }
        });
        ui.label(
            RichText::new("点击排版菜单「整理段落缩进」或执行一键排版时应用的缩进字符。")
                .weak()
                .small(),
        );

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // 3. Autosave interval
        ui.label(RichText::new("自动保存间隔").strong());
        let intervals = [
            (60, "1 分钟"),
            (180, "3 分钟"),
            (300, "5 分钟 (默认)"),
            (600, "10 分钟"),
            (0, "关闭自动保存"),
        ];
        let mut current_interval = state.autosave_interval;
        let selected_text = intervals
            .iter()
            .find(|(sec, _)| *sec == current_interval)
            .map(|(_, text)| *text)
            .unwrap_or("自定义");

        egui::ComboBox::from_id_salt("autosave_interval_combo")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (sec, label) in intervals {
                    if ui
                        .selectable_value(&mut current_interval, sec, label)
                        .clicked()
                    {
                        action = Some(SettingsAction::SetAutosaveInterval(sec));
                    }
                }
            });
        ui.label(
            RichText::new("文档编辑时后台定时自动保存更改至本地或草稿。")
                .weak()
                .small(),
        );

        action
    }

    fn show_typography_tab(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("文字排版");
        ui.add_space(8.0);

        // Font Family
        ui.label(RichText::new("字体选择").strong());
        let font_label = if state.current_font == "Default" {
            "系统默认"
        } else {
            state.current_font
        };
        egui::ComboBox::from_id_salt("typography_font_combo")
            .selected_text(font_label)
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.current_font == "Default", "系统默认")
                    .clicked()
                {
                    action = Some(SettingsAction::FontChange("Default".to_string()));
                }
                ui.separator();
                for f in state.chinese_fonts {
                    if ui
                        .selectable_label(state.current_font == f, f.as_str())
                        .clicked()
                    {
                        action = Some(SettingsAction::FontChange(f.clone()));
                    }
                }
            });

        ui.add_space(12.0);

        // Font Size
        ui.label(RichText::new("字号大小").strong());
        ui.horizontal_wrapped(|ui| {
            for size in [12.0, 14.0, 16.0, 18.0, 20.0, 24.0] {
                let is_selected = (state.font_size - size).abs() < 0.5;
                if ui
                    .selectable_label(is_selected, format!("{size:.0} pt"))
                    .clicked()
                {
                    action = Some(SettingsAction::FontSizeChange(size));
                }
            }
        });

        ui.add_space(12.0);

        // Line Spacing
        ui.label(RichText::new("行间距").strong());
        ui.horizontal_wrapped(|ui| {
            for (spacing, label) in [
                (1.2, "1.2x 紧凑"),
                (1.5, "1.5x 标准"),
                (1.8, "1.8x 宽松"),
                (2.0, "2.0x 双倍"),
            ] {
                let is_selected = (state.line_spacing - spacing).abs() < 0.05;
                if ui.selectable_label(is_selected, label).clicked() {
                    action = Some(SettingsAction::LineSpacingChange(spacing));
                }
            }
        });

        ui.add_space(12.0);

        // Font Color
        ui.label(RichText::new("字体颜色").strong());
        let current_color_label = crate::config::font_color_display_name(state.font_color);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("当前颜色：{current_color_label}"));
            if ui.button("恢复默认").clicked() {
                action = Some(SettingsAction::FontColorChange("Default".to_string()));
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (name, hex, color) in crate::config::FONT_COLOR_PRESETS {
                let is_selected = state.font_color.eq_ignore_ascii_case(hex);
                let (rect, swatch_resp) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 3.0, *color);
                if is_selected {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        egui::Stroke::new(2.0, ui.visuals().text_color()),
                        egui::StrokeKind::Outside,
                    );
                }
                if swatch_resp
                    .on_hover_text(format!("{name} ({hex})"))
                    .clicked()
                {
                    action = Some(SettingsAction::FontColorChange(hex.to_string()));
                }
            }
        });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        if ui
            .button(format!(
                "{} 重置所有排版为官方默认",
                egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
            ))
            .clicked()
        {
            action = Some(SettingsAction::ResetTypography);
            self.set_status(false, "排版已恢复为官方默认".to_string());
        }

        action
    }

    fn show_skin_tab(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("皮肤与外观");
        ui.label(RichText::new("选择写作环境的配色与质感。切换后立即生效。").weak());
        ui.add_space(12.0);
        ui.label(RichText::new("当前皮肤").strong());
        let current = state.skins.iter().find(|s| s.id == state.current_skin_id);
        egui::ComboBox::from_id_salt("settings_skin")
            .selected_text(current.map_or("默认", |s| s.name.as_str()))
            .width(240.0)
            .show_ui(ui, |ui| {
                for skin in state.skins {
                    if ui
                        .selectable_label(skin.id == state.current_skin_id, &skin.name)
                        .on_hover_text(&skin.description)
                        .clicked()
                    {
                        action = Some(SettingsAction::SelectSkin(skin.id.clone()));
                    }
                }
            });
        if let Some(skin) = current {
            ui.label(RichText::new(&skin.description).weak().small());
            ui.horizontal(|ui| {
                for hex in [
                    &skin.colors.background_color,
                    &skin.colors.editor_background_color,
                    &skin.colors.text_color,
                    &skin.colors.accent_color,
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(color) = crate::config::parse_hex_color(hex) {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(30.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, color);
                        response.on_hover_text(hex);
                    }
                }
                ui.label(if skin.colors.dark_mode {
                    "暗色"
                } else {
                    "亮色"
                });
            });
        }
        ui.add_space(16.0);
        ui.separator();
        ui.label(RichText::new("皮肤库").strong());
        ui.label(
            RichText::new("管理已安装皮肤、微调外观，或从市场添加新皮肤。")
                .weak()
                .small(),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("管理与微调…").clicked() {
                action = Some(SettingsAction::ManageSkins);
            }
            if ui.button("浏览皮肤市场…").clicked() {
                action = Some(SettingsAction::OpenSkinMarket);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("导入皮肤…").clicked() {
                action = Some(SettingsAction::ImportSkin);
            }
            if ui.button("导出当前外观…").clicked() {
                action = Some(SettingsAction::ExportCurrentSkin);
            }
        });
        ui.add_space(20.0);
        ui.collapsing("高级：皮肤市场源", |ui| {
            if let Some(a) = self.show_market_tab(ui, state) {
                action = Some(a);
            }
        });
        action
    }

    fn show_market_tab(
        &mut self,
        ui: &mut Ui,
        _state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;

        ui.label(RichText::new("远程 GitHub 皮肤仓库与目录").strong());
        ui.label(
            RichText::new(
                "纸壳内置皮肤市场通过 GitHub 远程 API 直接同步社区皮肤。默认配置指向官方仓库及 skins 目录。",
            )
            .weak()
            .small(),
        );

        ui.add_space(10.0);

        egui::Grid::new("market_repo_settings_grid")
            .num_columns(2)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                ui.label("GitHub 仓库 (owner/repo):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.repo_input)
                        .desired_width(260.0)
                        .hint_text("例如 RetricSu/paper-shell"),
                );
                ui.end_row();

                ui.label("默认分支 (Branch):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.branch_input)
                        .desired_width(260.0)
                        .hint_text("例如 master 或 main"),
                );
                ui.end_row();

                ui.label("皮肤子目录 (Folder):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.folder_input)
                        .desired_width(260.0)
                        .hint_text("例如 skins"),
                );
                ui.end_row();
            });

        ui.add_space(14.0);

        ui.horizontal_wrapped(|ui| {
            if ui
                .button(format!("{} 保存市场源配置", egui_phosphor::regular::CHECK))
                .clicked()
            {
                let repo = self.repo_input.trim().to_string();
                let branch = if self.branch_input.trim().is_empty() {
                    "master".to_string()
                } else {
                    self.branch_input.trim().to_string()
                };
                let folder = if self.folder_input.trim().is_empty() {
                    "skins".to_string()
                } else {
                    self.folder_input.trim().to_string()
                };

                action = Some(SettingsAction::UpdateMarketSource {
                    repo,
                    branch,
                    folder,
                });
                self.set_status(false, "皮肤市场源已保存".to_string());
            }

            if ui
                .button(format!(
                    "{} 恢复默认源",
                    egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                ))
                .clicked()
            {
                self.repo_input = crate::config::default_skin_market_repo();
                self.branch_input = crate::config::default_skin_market_branch();
                self.folder_input = crate::config::default_skin_market_folder();
                action = Some(SettingsAction::ResetMarketSource);
                self.set_status(false, "已恢复为官方皮肤市场源".to_string());
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(RichText::new("当前有效源预览：").small().weak());
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "https://github.com/{}/tree/{}/{}",
                    self.repo_input.trim(),
                    self.branch_input.trim(),
                    self.folder_input.trim()
                ))
                .monospace(),
            )
            .wrap(),
        );

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_positions(output: &egui::FullOutput, label: &str) -> Vec<egui::Pos2> {
        fn collect(shape: &egui::Shape, label: &str, positions: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    positions.push(t.visual_bounding_rect().center())
                }
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect(s, label, positions);
                    }
                }
                _ => {}
            }
        }
        let mut positions = vec![];
        for s in &output.shapes {
            collect(&s.shape, label, &mut positions);
        }
        positions
    }

    #[test]
    fn preferences_navigation_fits_and_appearance_dispatches_at_both_widths() {
        for width in [640.0, 800.0] {
            let ctx = Context::default();
            ctx.set_embed_viewports(false);
            let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
            let captured_output = captured.clone();
            let events = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
            let next_events = events.clone();
            Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
                let mut input = egui::RawInput {
                    viewport_id: viewport.ids.this,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 600.0),
                    )),
                    events: next_events.take(),
                    ..Default::default()
                };
                input.viewports.entry(viewport.ids.this).or_default().parent =
                    Some(viewport.ids.parent);
                *captured_output.borrow_mut() =
                    Some(ctx.run(input, |ctx| (viewport.viewport_ui_cb)(ctx)));
            });
            let skins = vec![crate::skin::Skin::classic_paper()];
            let state = SettingsWindowState {
                vim_mode: false,
                format_use_tabs: false,
                autosave_interval: 300,
                font_size: 14.0,
                line_spacing: 1.5,
                current_font: "Default",
                chinese_fonts: &[],
                font_color: "Default",
                skins: &skins,
                current_skin_id: "classic_paper",
                market_repo: "RetricSu/paper-shell",
                market_branch: "master",
                market_folder: "skins",
            };
            let mut window = SettingsWindow::new();
            window.open(&ctx, Some(SettingsTab::Skin));
            for _ in 0..2 {
                let _ = ctx.run(Default::default(), |ctx| {
                    window.show(ctx, state);
                });
            }
            let output = captured.borrow().clone().unwrap();
            assert!(!text_positions(&output, "管理与微调…").is_empty());
            assert!(!text_positions(&output, "浏览皮肤市场…").is_empty());
            assert!(text_positions(&output, "插件设置").is_empty());
            for tab in SettingsTab::ALL {
                assert!(!text_positions(&output, tab.label()).is_empty());
            }
            let pos = text_positions(&output, "浏览皮肤市场…")[0];
            *events.borrow_mut() = vec![
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
            ];
            let mut action = None;
            let _ = ctx.run(Default::default(), |ctx| {
                action = window.show(ctx, state);
            });
            assert!(matches!(action, Some(SettingsAction::OpenSkinMarket)));
            // Each category, including the expanded advanced form, stays within the window.
            for tab in SettingsTab::ALL {
                window.active_tab = tab;
                let _ = ctx.run(Default::default(), |ctx| {
                    window.show(ctx, state);
                });
                if tab == SettingsTab::Skin {
                    let output = captured.borrow().clone().unwrap();
                    let pos = text_positions(&output, "高级：皮肤市场源")[0];
                    *events.borrow_mut() = vec![
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
                    ];
                    let _ = ctx.run(Default::default(), |ctx| {
                        window.show(ctx, state);
                    });
                    let _ = ctx.run(Default::default(), |ctx| {
                        window.show(ctx, state);
                    });
                }
                let output = captured.borrow().clone().unwrap();
                for s in &output.shapes {
                    if let egui::Shape::Text(t) = &s.shape {
                        assert!(
                            t.visual_bounding_rect().right() <= width + 2.0,
                            "{} overflowed at {width}: {:?}",
                            t.galley.text(),
                            t.visual_bounding_rect()
                        );
                    }
                }
            }
            window.close();
            assert!(!window.is_open());
        }
    }
}
