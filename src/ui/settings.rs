//! Application preferences. Plugin lifecycle and configuration live in the plugin manager.
use crate::ui::dialog::{ModalConfig, modal_button, show_modal};
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
    ExportSkin(String),
    UninstallSkin(String),
    SaveCustomSkin(crate::skin::Skin),
    FetchMarketSkins,
    InstallMarketSkin(crate::skin::MarketSkin),
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
    pub market_skins: &'a [crate::skin::MarketSkin],
    pub market_error: Option<&'a str>,
    pub market_repo: &'a str,
    pub market_branch: &'a str,
    pub market_folder: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SkinLibraryTab {
    #[default]
    Installed,
    Market,
}

struct CustomSkinTuner {
    base_skin: Option<crate::skin::Skin>,
    name: String,
    id: String,
    font_size: f32,
    line_spacing: f32,
    font_color: String,
    item_spacing: f32,
    editor_padding: f32,
    background_opacity: f32,
    editor_opacity: f32,
    corner_radius: f32,
    dark_mode: bool,
}

impl Default for CustomSkinTuner {
    fn default() -> Self {
        Self {
            base_skin: None,
            name: String::new(),
            id: String::new(),
            font_size: 14.0,
            line_spacing: 1.5,
            font_color: "#1A1A1A".to_string(),
            item_spacing: 8.0,
            editor_padding: 2.0,
            background_opacity: 0.25,
            editor_opacity: 1.0,
            corner_radius: 0.0,
            dark_mode: false,
        }
    }
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
    skin_library_tab: SkinLibraryTab,
    skin_search: String,
    market_search: String,
    custom_skin_tuning: bool,
    tuner: CustomSkinTuner,
    confirm_uninstall_skin: Option<(String, String)>,
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
        self.confirm_uninstall_skin = None;
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
                if self.confirm_uninstall_skin.is_some() {
                    ui.disable();
                }
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
                if let Some(modal_action) = self.show_uninstall_skin_modal(ui.ctx()) {
                    action = Some(modal_action);
                }
            },
        );
        self.open = open;
        if !self.open {
            self.confirm_uninstall_skin = None;
        }
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
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("settings_skin")
                .selected_text(current.map_or("默认", |s| s.name.as_str()))
                .width(220.0)
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
            if ui
                .button(format!("{} 导出当前外观…", egui_phosphor::regular::EXPORT))
                .clicked()
            {
                action = Some(SettingsAction::ExportCurrentSkin);
            }
            let tuning_label = if self.custom_skin_tuning {
                format!("{} 收起自定义", egui_phosphor::regular::CARET_UP)
            } else {
                format!("{} 自定义外观", egui_phosphor::regular::SLIDERS)
            };
            if ui.button(tuning_label).clicked() {
                if self.custom_skin_tuning {
                    self.custom_skin_tuning = false;
                } else {
                    self.begin_custom_skin_tuning(state);
                }
            }
        });
        if let Some(skin) = current {
            ui.label(RichText::new(&skin.description).weak().small());
            ui.horizontal_wrapped(|ui| {
                Self::paint_color_swatches(ui, &skin.colors);
                ui.label(
                    RichText::new(if skin.colors.dark_mode {
                        "深色"
                    } else {
                        "浅色"
                    })
                    .small()
                    .weak(),
                );
            });
        }

        if self.custom_skin_tuning
            && let Some(skin) = current
            && let Some(tuner_action) = self.show_custom_skin_tuner(ui, state, skin)
        {
            action = Some(tuner_action);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("皮肤库").strong());
            ui.selectable_value(
                &mut self.skin_library_tab,
                SkinLibraryTab::Installed,
                "已安装",
            );
            ui.selectable_value(
                &mut self.skin_library_tab,
                SkinLibraryTab::Market,
                "皮肤市场",
            );
        });
        ui.label(
            RichText::new(match self.skin_library_tab {
                SkinLibraryTab::Installed => "应用、导入或管理已经保存在本机的皮肤。",
                SkinLibraryTab::Market => "浏览社区皮肤，安装后会自动应用并加入已安装列表。",
            })
            .weak()
            .small(),
        );
        ui.add_space(4.0);

        match self.skin_library_tab {
            SkinLibraryTab::Installed => {
                if let Some(list_action) = self.show_installed_skins(ui, state) {
                    action = Some(list_action);
                }
            }
            SkinLibraryTab::Market => {
                if let Some(market_action) = self.show_market_skins(ui, state) {
                    action = Some(market_action);
                }
                ui.add_space(12.0);
                ui.collapsing("高级：皮肤市场源", |ui| {
                    if let Some(source_action) = self.show_market_tab(ui, state) {
                        action = Some(source_action);
                    }
                });
            }
        }
        action
    }

    fn begin_custom_skin_tuning(&mut self, state: &SettingsWindowState<'_>) {
        let base = state
            .skins
            .iter()
            .find(|skin| skin.id == state.current_skin_id)
            .cloned()
            .unwrap_or_else(crate::skin::Skin::classic_paper);
        let font_color = if state.font_color == "Default" {
            base.typography.font_color.clone()
        } else {
            state.font_color.to_string()
        };
        self.tuner = CustomSkinTuner {
            name: format!("{} 副本", base.name),
            id: format!("{}_custom", base.id),
            font_size: state.font_size,
            line_spacing: state.line_spacing,
            font_color,
            item_spacing: base.spacing.item_spacing_x,
            editor_padding: base.spacing.editor_padding_left,
            background_opacity: base.effects.background_image_opacity,
            editor_opacity: base.effects.editor_opacity,
            corner_radius: base.effects.corner_radius,
            dark_mode: base.colors.dark_mode,
            base_skin: Some(base),
        };
        self.custom_skin_tuning = true;
    }

    fn show_custom_skin_tuner(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
        current: &crate::skin::Skin,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(ui.visuals().widgets.inactive.bg_fill)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(RichText::new("基于当前皮肤新建副本").strong());
                ui.label(
                    RichText::new("保留当前配色与背景，只调整需要修改的项目。保存后自动应用。")
                        .weak()
                        .small(),
                );
                ui.add_space(6.0);
                egui::Grid::new("custom_skin_tuner_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("皮肤名称");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tuner.name).desired_width(240.0),
                        );
                        ui.end_row();
                        ui.label("皮肤 ID");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tuner.id)
                                .desired_width(240.0)
                                .hint_text("仅限字母、数字、_ 和 -"),
                        );
                        ui.end_row();
                        ui.label("字号");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.font_size, 10.0..=32.0).suffix(" pt"),
                        );
                        ui.end_row();
                        ui.label("行间距");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.line_spacing, 1.0..=2.5)
                                .step_by(0.1)
                                .suffix(" x"),
                        );
                        ui.end_row();
                        ui.label("字体颜色");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.tuner.font_color)
                                .desired_width(160.0)
                                .hint_text("#1A1A1A"),
                        );
                        ui.end_row();
                        ui.label("元素间距");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.item_spacing, 4.0..=20.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("编辑器边距");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.editor_padding, 2.0..=24.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("编辑器透明度");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.editor_opacity, 0.2..=1.0)
                                .step_by(0.05),
                        );
                        ui.end_row();
                        ui.label("背景图透明度");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.background_opacity, 0.0..=1.0)
                                .step_by(0.05),
                        );
                        ui.end_row();
                        ui.label("界面圆角");
                        ui.add(
                            egui::Slider::new(&mut self.tuner.corner_radius, 0.0..=12.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
                ui.checkbox(&mut self.tuner.dark_mode, "使用深色模式配色");
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(format!("{} 保存副本并应用", egui_phosphor::regular::CHECK))
                        .clicked()
                    {
                        let mut skin = self
                            .tuner
                            .base_skin
                            .clone()
                            .unwrap_or_else(|| current.clone());
                        let dark_mode_changed = skin.colors.dark_mode != self.tuner.dark_mode;
                        skin.id = self.tuner.id.trim().to_string();
                        skin.name = self.tuner.name.trim().to_string();
                        skin.description = "基于现有皮肤创建的自定义副本".to_string();
                        skin.author = "我".to_string();
                        skin.builtin = false;
                        skin.dir = None;
                        skin.typography.font_family = state.current_font.to_string();
                        skin.typography.font_size = self.tuner.font_size;
                        skin.typography.line_spacing = self.tuner.line_spacing;
                        skin.typography.font_color = self.tuner.font_color.clone();
                        skin.typography.format_use_tabs = state.format_use_tabs;
                        skin.spacing.item_spacing_x = self.tuner.item_spacing;
                        skin.spacing.item_spacing_y = self.tuner.item_spacing;
                        skin.spacing.editor_padding_left = self.tuner.editor_padding;
                        skin.spacing.editor_padding_right = self.tuner.editor_padding;
                        skin.spacing.editor_padding_top = self.tuner.editor_padding;
                        skin.spacing.editor_padding_bottom = self.tuner.editor_padding;
                        if dark_mode_changed {
                            skin.colors.background_color = Some(if self.tuner.dark_mode {
                                "#16161A".to_string()
                            } else {
                                "#FAFAFA".to_string()
                            });
                            skin.colors.editor_background_color = Some(if self.tuner.dark_mode {
                                "#1F1F24".to_string()
                            } else {
                                "#FFFFFF".to_string()
                            });
                            skin.colors.sidebar_background_color = None;
                            skin.colors.titlebar_background_color = None;
                            skin.colors.selection_color = None;
                        }
                        skin.colors.dark_mode = self.tuner.dark_mode;
                        skin.colors.text_color = Some(self.tuner.font_color.clone());
                        skin.effects.background_image_opacity = self.tuner.background_opacity;
                        skin.effects.editor_opacity = self.tuner.editor_opacity;
                        skin.effects.corner_radius = self.tuner.corner_radius;
                        action = Some(SettingsAction::SaveCustomSkin(skin));
                        self.custom_skin_tuning = false;
                    }
                    if ui.button("取消").clicked() {
                        self.custom_skin_tuning = false;
                    }
                });
            });
        action
    }

    fn show_installed_skins(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.skin_search)
                    .hint_text("搜索已安装皮肤…")
                    .desired_width(200.0),
            );
            if ui
                .button(format!(
                    "{} 导入皮肤…",
                    egui_phosphor::regular::DOWNLOAD_SIMPLE
                ))
                .on_hover_text("支持 .toml、.json、.paperskin 和 .zip")
                .clicked()
            {
                action = Some(SettingsAction::ImportSkin);
            }
        });
        ui.add_space(8.0);

        let query = self.skin_search.trim().to_lowercase();
        let skins: Vec<_> = state
            .skins
            .iter()
            .filter(|skin| {
                query.is_empty()
                    || skin.name.to_lowercase().contains(&query)
                    || skin.id.to_lowercase().contains(&query)
                    || skin.description.to_lowercase().contains(&query)
            })
            .collect();
        if skins.is_empty() {
            ui.label(RichText::new("没有匹配的皮肤，请尝试其他关键词。").weak());
        }
        for (index, skin) in skins.iter().enumerate() {
            if index > 0 {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
            }
            let is_current = skin.id == state.current_skin_id;
            ui.push_id(&skin.id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&skin.name).size(14.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "v{} · {} · {}",
                            skin.version,
                            if skin.builtin { "内置" } else { "自定义" },
                            if skin.colors.dark_mode {
                                "深色"
                            } else {
                                "浅色"
                            }
                        ))
                        .small()
                        .weak(),
                    );
                    if is_current {
                        ui.label(
                            RichText::new("✓ 当前使用")
                                .color(crate::style::STATUS_GREEN)
                                .strong(),
                        );
                    } else if ui.button("应用皮肤").clicked() {
                        action = Some(SettingsAction::SelectSkin(skin.id.clone()));
                    }
                    if ui
                        .button(format!("{} 导出", egui_phosphor::regular::EXPORT))
                        .clicked()
                    {
                        action = Some(SettingsAction::ExportSkin(skin.id.clone()));
                    }
                    if !skin.builtin
                        && ui
                            .button(
                                RichText::new(format!("{} 卸载", egui_phosphor::regular::TRASH))
                                    .color(ui.visuals().error_fg_color),
                            )
                            .clicked()
                    {
                        self.confirm_uninstall_skin = Some((skin.id.clone(), skin.name.clone()));
                    }
                });
                if !skin.description.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&skin.description)
                                .color(ui.visuals().text_color().gamma_multiply(0.72)),
                        )
                        .wrap(),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "字号 {}pt · 行距 {}x · 内边距 {}px",
                            skin.typography.font_size,
                            skin.typography.line_spacing,
                            skin.spacing.editor_padding_left
                        ))
                        .small()
                        .weak(),
                    );
                    if skin.effects.background_image.is_some()
                        || skin.effects.background_image_data.is_some()
                    {
                        ui.label(
                            RichText::new(format!(
                                "背景图 {:.0}%",
                                skin.effects.background_image_opacity * 100.0
                            ))
                            .small()
                            .color(crate::style::STATUS_GREEN),
                        );
                    }
                    Self::paint_color_swatches(ui, &skin.colors);
                });
            });
        }
        action
    }

    fn show_market_skins(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.market_search)
                    .hint_text("搜索市场皮肤…")
                    .desired_width(200.0),
            );
            if ui
                .button(format!(
                    "{} 从 GitHub 刷新",
                    egui_phosphor::regular::ARROW_CLOCKWISE
                ))
                .on_hover_text("从高级设置中的市场源拉取最新皮肤列表")
                .clicked()
            {
                action = Some(SettingsAction::FetchMarketSkins);
            }
        });
        if let Some(error) = state.market_error {
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "远程刷新提示：{error}。当前显示已缓存或内置精选皮肤。"
                    ))
                    .small()
                    .color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
        }
        ui.add_space(8.0);

        let market_skins = if state.market_skins.is_empty() {
            crate::skin::SkinMarketClient::community_templates()
        } else {
            state.market_skins.to_vec()
        };
        let query = self.market_search.trim().to_lowercase();
        let filtered: Vec<_> = market_skins
            .iter()
            .filter(|skin| {
                query.is_empty()
                    || skin.name.to_lowercase().contains(&query)
                    || skin.id.to_lowercase().contains(&query)
                    || skin.author.to_lowercase().contains(&query)
                    || skin.description.to_lowercase().contains(&query)
            })
            .collect();
        if filtered.is_empty() {
            ui.label(RichText::new("没有匹配的市场皮肤，请尝试其他关键词。").weak());
        }
        for (index, market_skin) in filtered.iter().enumerate() {
            if index > 0 {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
            }
            let is_installed = state.skins.iter().any(|skin| skin.id == market_skin.id);
            ui.push_id(("market", &market_skin.id), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&market_skin.name).size(14.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "v{} · {} · {}",
                            market_skin.version,
                            market_skin.author,
                            if market_skin.skin.colors.dark_mode {
                                "深色"
                            } else {
                                "浅色"
                            }
                        ))
                        .small()
                        .weak(),
                    );
                    if is_installed {
                        ui.label(
                            RichText::new("✓ 已安装")
                                .color(crate::style::STATUS_GREEN)
                                .strong(),
                        );
                        if ui.button("重新安装").clicked() {
                            action =
                                Some(SettingsAction::InstallMarketSkin((*market_skin).clone()));
                        }
                    } else if ui
                        .button(format!(
                            "{} 安装皮肤",
                            egui_phosphor::regular::DOWNLOAD_SIMPLE
                        ))
                        .clicked()
                    {
                        action = Some(SettingsAction::InstallMarketSkin((*market_skin).clone()));
                    }
                });
                if !market_skin.description.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&market_skin.description)
                                .color(ui.visuals().text_color().gamma_multiply(0.72)),
                        )
                        .wrap(),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "字号 {}pt · 行距 {}x",
                            market_skin.skin.typography.font_size,
                            market_skin.skin.typography.line_spacing
                        ))
                        .small()
                        .weak(),
                    );
                    if market_skin.skin.effects.background_image.is_some()
                        || market_skin.skin.effects.background_image_data.is_some()
                    {
                        ui.label(
                            RichText::new("含背景图")
                                .small()
                                .color(crate::style::STATUS_GREEN),
                        );
                    }
                    Self::paint_color_swatches(ui, &market_skin.skin.colors);
                });
            });
        }
        action
    }

    fn paint_color_swatches(ui: &mut Ui, colors: &crate::skin::SkinColors) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for hex in [
                colors.background_color.as_deref(),
                colors.editor_background_color.as_deref(),
                colors.text_color.as_deref(),
                colors.accent_color.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(color) = crate::config::parse_hex_color(hex) {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.painter().rect(
                        rect,
                        2.0,
                        color,
                        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                        egui::StrokeKind::Inside,
                    );
                    response.on_hover_text(hex);
                }
            }
        });
    }

    fn show_uninstall_skin_modal(&mut self, ctx: &Context) -> Option<SettingsAction> {
        let (id, name) = self.confirm_uninstall_skin.clone()?;
        let message = format!("确定永久删除自定义皮肤“{name}”吗？此操作无法撤销。");
        let config = ModalConfig {
            id: "settings_skin_confirm_uninstall",
            title: "确认卸载皮肤",
            icon: egui_phosphor::regular::WARNING,
            icon_color: ctx.style().visuals.warn_fg_color,
            message: &message,
            width: 340.0,
        };
        let mut confirmed = false;
        let mut cancelled = false;
        let response = show_modal(ctx, config, |ui| {
            ui.horizontal(|ui| {
                if modal_button(
                    ui,
                    RichText::new(format!("{} 永久删除", egui_phosphor::regular::TRASH))
                        .color(ui.visuals().error_fg_color),
                )
                .clicked()
                {
                    confirmed = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if modal_button(ui, RichText::new("取消").strong()).clicked() {
                        cancelled = true;
                    }
                });
            });
        });
        if confirmed {
            self.confirm_uninstall_skin = None;
            Some(SettingsAction::UninstallSkin(id))
        } else {
            if cancelled || response.escape_pressed {
                self.confirm_uninstall_skin = None;
            }
            None
        }
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

    fn pointer_click(pos: egui::Pos2) -> Vec<egui::Event> {
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

    #[test]
    fn custom_tuner_starts_from_the_current_skin() {
        let skins = vec![crate::skin::Skin::mint_breeze()];
        let state = SettingsWindowState {
            vim_mode: false,
            format_use_tabs: true,
            autosave_interval: 300,
            font_size: 18.0,
            line_spacing: 1.8,
            current_font: "Default",
            chinese_fonts: &[],
            font_color: "#123456",
            skins: &skins,
            current_skin_id: "mint_breeze",
            market_skins: &[],
            market_error: None,
            market_repo: "RetricSu/paper-shell",
            market_branch: "master",
            market_folder: "skins",
        };
        let mut window = SettingsWindow::new();
        window.begin_custom_skin_tuning(&state);
        assert!(window.custom_skin_tuning);
        assert_eq!(window.tuner.name, "薄荷清夏 副本");
        assert_eq!(window.tuner.id, "mint_breeze_custom");
        assert_eq!(window.tuner.font_size, 18.0);
        assert_eq!(window.tuner.line_spacing, 1.8);
        assert_eq!(window.tuner.font_color, "#123456");
        assert_eq!(
            window.tuner.base_skin.as_ref().map(|skin| &skin.colors),
            Some(&skins[0].colors)
        );
    }

    #[test]
    fn preferences_integrates_skin_library_and_market_at_both_widths() {
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
            let market_skins = crate::skin::SkinMarketClient::community_templates();
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
                market_skins: &market_skins,
                market_error: None,
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
            assert!(!text_positions(&output, "已安装").is_empty());
            let import_label = format!("{} 导入皮肤…", egui_phosphor::regular::DOWNLOAD_SIMPLE);
            assert!(!text_positions(&output, &import_label).is_empty());
            assert!(!text_positions(&output, "经典白纸").is_empty());
            for tab in SettingsTab::ALL {
                assert!(!text_positions(&output, tab.label()).is_empty());
            }

            let pos = text_positions(&output, "皮肤市场")[0];
            *events.borrow_mut() = pointer_click(pos);
            let _ = ctx.run(Default::default(), |ctx| {
                window.show(ctx, state);
            });
            let _ = ctx.run(Default::default(), |ctx| {
                window.show(ctx, state);
            });
            let output = captured.borrow().clone().unwrap();
            assert!(!text_positions(&output, "赛博霓虹").is_empty());
            let install_label = format!("{} 安装皮肤", egui_phosphor::regular::DOWNLOAD_SIMPLE);
            let pos = text_positions(&output, &install_label)[0];
            *events.borrow_mut() = pointer_click(pos);
            let mut action = None;
            let _ = ctx.run(Default::default(), |ctx| {
                action = window.show(ctx, state);
            });
            assert!(matches!(
                action,
                Some(SettingsAction::InstallMarketSkin(skin)) if skin.id == "cyberpunk_neon"
            ));

            // Each category stays within the window at the minimum and default widths.
            for tab in SettingsTab::ALL {
                window.active_tab = tab;
                let _ = ctx.run(Default::default(), |ctx| {
                    window.show(ctx, state);
                });
                let output = captured.borrow().clone().unwrap();
                for s in &output.shapes {
                    if let egui::Shape::Text(t) = &s.shape {
                        let right = t.visual_bounding_rect().right();
                        if right.is_finite() {
                            assert!(
                                right <= width + 2.0,
                                "{} overflowed at {width}: {:?}",
                                t.galley.text(),
                                t.visual_bounding_rect()
                            );
                        }
                    }
                }
            }
            window.close();
            assert!(!window.is_open());
        }
    }
}
