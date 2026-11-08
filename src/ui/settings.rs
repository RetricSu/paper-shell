//! Dedicated Settings / Preferences window for Paper Shell.
//!
//! Provides a unified, well-structured configuration interface separated from
//! transient plugin management and editor toolbars:
//! 1. General (Vim mode, paragraph indent, autosave)
//! 2. Typography (default font, font size, line spacing, font color)
//! 3. Appearance & Skins (quick skin switcher, skin tuning, import/export)
//! 4. Skin Marketplace Source (remote GitHub repo, branch, folder)
//! 5. Plugin Settings (Cloud Backup, GitHub Publish, External Plugins)

use egui::{Align, Context, Layout, RichText, Ui};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Typography,
    Skin,
    Market,
    Plugins,
}

impl SettingsTab {
    pub fn label(&self) -> String {
        match self {
            Self::General => format!("{} 常规", egui_phosphor::regular::GEAR),
            Self::Typography => format!("{} 排版", egui_phosphor::regular::TEXT_ALIGN_LEFT),
            Self::Skin => format!("{} 皮肤与外观", egui_phosphor::regular::PALETTE),
            Self::Market => format!("{} 皮肤市场源", egui_phosphor::regular::STOREFRONT),
            Self::Plugins => format!("{} 插件设置", egui_phosphor::regular::PUZZLE_PIECE),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingsAction {
    // General settings
    ToggleVimMode,
    SetFormatUseTabs(bool),
    SetAutosaveInterval(u64),

    // Typography
    FontSizeChange(f32),
    LineSpacingChange(f32),
    FontChange(String),
    FontColorChange(String),
    ResetTypography,

    // Skin & Appearance
    SelectSkin(String),
    ImportSkin,
    ExportCurrentSkin,
    SaveCustomSkin(Box<crate::skin::Skin>),

    // Market Source
    UpdateMarketSource {
        repo: String,
        branch: String,
        folder: String,
    },
    ResetMarketSource,

    // Plugin Settings
    SaveCloudBackupConfig(crate::plugin::builtin::cloud_backup::CloudBackupConfig),
    SaveGithubPublishConfig(crate::plugin::builtin::github_publish::GithubPublishConfig),
    SaveExternalPluginConfig {
        plugin_id: String,
        values: BTreeMap<String, String>,
    },
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
    pub plugins: &'a [crate::plugin::PluginMetadata],
    pub cloud_backup: &'a crate::plugin::builtin::cloud_backup::CloudBackupConfig,
    pub github_publish: &'a crate::plugin::builtin::github_publish::GithubPublishConfig,
    pub external_plugin_settings: &'a BTreeMap<String, BTreeMap<String, String>>,
    pub plugin_definitions: &'a BTreeMap<String, Vec<crate::plugin::external::SettingDefinition>>,
}

pub struct SettingsWindow {
    open: bool,
    active_tab: SettingsTab,
    selected_plugin_id: String,
    status_message: Option<(bool, String)>,

    // Market source edit buffers
    repo_input: String,
    branch_input: String,
    folder_input: String,
    market_buffers_initialized: bool,

    // Cloud backup edit buffers
    cb_server_url: String,
    cb_username: String,
    cb_password: String,
    cb_remote_filename: String,
    cb_show_password: bool,
    cb_buffers_initialized: bool,

    // GitHub publish edit buffers
    gh_repo: String,
    gh_base_branch: String,
    gh_base_dir: String,
    gh_commit_message: String,
    gh_pr_title: String,
    gh_buffers_initialized: bool,

    // External plugin edit buffers
    ext_plugin_id: String,
    ext_values: BTreeMap<String, String>,
    ext_show_passwords: bool,

    // Skin tuning buffers
    tune_name: String,
    tune_font_size: f32,
    tune_line_spacing: f32,
    tune_font_color: String,
    tune_bg_opacity: f32,
    tune_editor_opacity: f32,
    tune_corner_radius: f32,
    tune_dark_mode: bool,
    tune_buffers_initialized: bool,
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            active_tab: SettingsTab::General,
            selected_plugin_id: "cloud_backup".to_string(),
            status_message: None,

            repo_input: String::new(),
            branch_input: String::new(),
            folder_input: String::new(),
            market_buffers_initialized: false,

            cb_server_url: String::new(),
            cb_username: String::new(),
            cb_password: String::new(),
            cb_remote_filename: String::new(),
            cb_show_password: false,
            cb_buffers_initialized: false,

            gh_repo: String::new(),
            gh_base_branch: String::new(),
            gh_base_dir: String::new(),
            gh_commit_message: String::new(),
            gh_pr_title: String::new(),
            gh_buffers_initialized: false,

            ext_plugin_id: String::new(),
            ext_values: BTreeMap::new(),
            ext_show_passwords: false,

            tune_name: "我的自定义皮肤".to_string(),
            tune_font_size: 14.0,
            tune_line_spacing: 1.5,
            tune_font_color: "Default".to_string(),
            tune_bg_opacity: 0.2,
            tune_editor_opacity: 0.96,
            tune_corner_radius: 2.0,
            tune_dark_mode: false,
            tune_buffers_initialized: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(
        &mut self,
        ctx: &Context,
        tab: Option<SettingsTab>,
        target_plugin_id: Option<String>,
    ) {
        if self.open {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of("settings_window"),
                egui::ViewportCommand::Focus,
            );
        } else {
            self.open = true;
            self.market_buffers_initialized = false;
            self.cb_buffers_initialized = false;
            self.gh_buffers_initialized = false;
            self.tune_buffers_initialized = false;
        }
        if let Some(t) = tab {
            self.active_tab = t;
        }
        if let Some(pid) = target_plugin_id {
            self.selected_plugin_id = pid;
            self.active_tab = SettingsTab::Plugins;
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

        self.init_buffers_if_needed(&state);

        let mut action = None;
        let mut open = self.open;

        crate::ui::viewport::show_tool_window(
            ctx,
            "settings_window",
            "设置 · 首选项",
            [680.0, 530.0],
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                ui.spacing_mut().interact_size.y = 22.0;

                // Status banner if present
                if let Some((is_error, ref msg)) = self.status_message {
                    let bg = if is_error {
                        ui.visuals().error_fg_color.gamma_multiply(0.15)
                    } else {
                        crate::style::STATUS_GREEN.gamma_multiply(0.15)
                    };
                    let fg = if is_error {
                        ui.visuals().error_fg_color
                    } else {
                        crate::style::STATUS_GREEN
                    };
                    egui::Frame::new()
                        .fill(bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon = if is_error {
                                    egui_phosphor::regular::WARNING_CIRCLE
                                } else {
                                    egui_phosphor::regular::CHECK_CIRCLE
                                };
                                ui.label(RichText::new(format!("{icon} {msg}")).color(fg).small());
                            });
                        });
                    ui.add_space(4.0);
                }

                // Two-column layout: Left sidebar tabs + Right tab content
                ui.horizontal(|ui| {
                    // Left navigation
                    ui.vertical(|ui| {
                        ui.set_width(140.0);
                        ui.add_space(4.0);

                        for tab in [
                            SettingsTab::General,
                            SettingsTab::Typography,
                            SettingsTab::Skin,
                            SettingsTab::Market,
                            SettingsTab::Plugins,
                        ] {
                            let is_selected = self.active_tab == tab;
                            let btn = egui::Button::selectable(is_selected, tab.label())
                                .min_size(egui::vec2(136.0, 26.0));
                            if ui.add(btn).clicked() {
                                self.active_tab = tab;
                                self.status_message = None;
                            }
                            ui.add_space(2.0);
                        }
                    });

                    ui.separator();

                    // Right detail panel
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.add_space(4.0);

                            match self.active_tab {
                                SettingsTab::General => {
                                    action = self.show_general_tab(ui, &state);
                                }
                                SettingsTab::Typography => {
                                    action = self.show_typography_tab(ui, &state);
                                }
                                SettingsTab::Skin => {
                                    action = self.show_skin_tab(ui, &state);
                                }
                                SettingsTab::Market => {
                                    action = self.show_market_tab(ui, &state);
                                }
                                SettingsTab::Plugins => {
                                    action = self.show_plugins_tab(ui, &state);
                                }
                            }
                        });
                });
            },
        );

        self.open = open;
        action
    }

    fn init_buffers_if_needed(&mut self, state: &SettingsWindowState<'_>) {
        if !self.market_buffers_initialized {
            self.repo_input = state.market_repo.to_string();
            self.branch_input = state.market_branch.to_string();
            self.folder_input = state.market_folder.to_string();
            self.market_buffers_initialized = true;
        }

        if !self.cb_buffers_initialized {
            self.cb_server_url = state.cloud_backup.server_url.clone();
            self.cb_username = state.cloud_backup.username.clone();
            self.cb_password = state.cloud_backup.password.clone();
            self.cb_remote_filename = state.cloud_backup.remote_filename.clone();
            self.cb_buffers_initialized = true;
        }

        if !self.gh_buffers_initialized {
            self.gh_repo = state.github_publish.repo.clone();
            self.gh_base_branch = state.github_publish.base_branch.clone();
            self.gh_base_dir = state.github_publish.base_dir.clone();
            self.gh_commit_message = state.github_publish.commit_message.clone();
            self.gh_pr_title = state.github_publish.pr_title.clone();
            self.gh_buffers_initialized = true;
        }

        if !self.tune_buffers_initialized {
            if let Some(current_skin) = state.skins.iter().find(|s| s.id == state.current_skin_id) {
                self.tune_name = format!("{} (自定义)", current_skin.name);
                self.tune_font_size = current_skin.typography.font_size;
                self.tune_line_spacing = current_skin.typography.line_spacing;
                self.tune_font_color = current_skin.typography.font_color.clone();
                self.tune_bg_opacity = current_skin.effects.background_image_opacity;
                self.tune_editor_opacity = current_skin.effects.editor_opacity;
                self.tune_corner_radius = current_skin.effects.corner_radius;
                self.tune_dark_mode = current_skin.colors.dark_mode;
            }
            self.tune_buffers_initialized = true;
        }
    }

    fn show_general_tab(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("常规设置");
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
        ui.horizontal(|ui| {
            if ui
                .radio_value(&mut use_tabs, false, "两个半角/全角空格 (推荐)")
                .clicked()
            {
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
        ui.heading("默认排版");
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
        ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
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
        ui.add_space(8.0);

        ui.label(RichText::new("当前安装的皮肤风格 (点击立即切换)").strong());
        ui.add_space(4.0);

        // Skin cards
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                for skin in state.skins {
                    let is_active = skin.id == state.current_skin_id;
                    egui::Frame::new()
                        .fill(if is_active {
                            ui.visuals().selection.bg_fill.gamma_multiply(0.2)
                        } else {
                            ui.visuals()
                                .widgets
                                .noninteractive
                                .bg_fill
                                .gamma_multiply(0.5)
                        })
                        .corner_radius(4.0)
                        .stroke(if is_active {
                            egui::Stroke::new(1.0, ui.visuals().selection.bg_fill)
                        } else {
                            egui::Stroke::NONE
                        })
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let badge = if is_active {
                                    format!("{} [使用中]", egui_phosphor::regular::CHECK)
                                } else {
                                    String::new()
                                };
                                ui.label(
                                    RichText::new(format!("{} {}", skin.name, badge)).strong(),
                                );

                                let mode_tag = if skin.colors.dark_mode {
                                    "🌙 暗色"
                                } else {
                                    "☀️ 亮色"
                                };
                                ui.label(RichText::new(mode_tag).small().weak());

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if !is_active && ui.button("应用此皮肤").clicked() {
                                        action = Some(SettingsAction::SelectSkin(skin.id.clone()));
                                    }
                                });
                            });
                            if !skin.description.is_empty() {
                                ui.label(RichText::new(&skin.description).small().weak());
                            }
                        });
                    ui.add_space(3.0);
                }
            });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);

        // Import & Export buttons
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{} 从文件导入皮肤…",
                    egui_phosphor::regular::DOWNLOAD_SIMPLE
                ))
                .clicked()
            {
                action = Some(SettingsAction::ImportSkin);
            }
            if ui
                .button(format!(
                    "{} 导出当前外观为皮肤…",
                    egui_phosphor::regular::EXPORT
                ))
                .clicked()
            {
                action = Some(SettingsAction::ExportCurrentSkin);
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Skin tuning
        ui.label(RichText::new("皮肤参数微调与另存为新皮肤").strong());
        ui.horizontal(|ui| {
            ui.label("皮肤名称：");
            ui.add(egui::TextEdit::singleline(&mut self.tune_name).desired_width(180.0));
            ui.checkbox(&mut self.tune_dark_mode, "暗色模式");
        });

        ui.horizontal(|ui| {
            ui.label("背景透明度：");
            ui.add(egui::Slider::new(&mut self.tune_bg_opacity, 0.0..=1.0).text("透明"));
        });

        ui.horizontal(|ui| {
            ui.label("编辑器透明度：");
            ui.add(egui::Slider::new(&mut self.tune_editor_opacity, 0.5..=1.0).text("透明"));
        });

        ui.horizontal(|ui| {
            ui.label("圆角大小：");
            ui.add(egui::Slider::new(&mut self.tune_corner_radius, 0.0..=12.0).text("px"));
        });

        ui.add_space(6.0);
        if ui
            .button(format!(
                "{} 将微调参数另存为新皮肤",
                egui_phosphor::regular::PLUS_CIRCLE
            ))
            .clicked()
        {
            let id_slug = format!("custom_{}", chrono::Utc::now().timestamp_millis() % 100000);
            let mut new_skin = crate::skin::Skin::classic_paper();
            new_skin.id = id_slug;
            new_skin.name = if self.tune_name.trim().is_empty() {
                "自定义皮肤".to_string()
            } else {
                self.tune_name.trim().to_string()
            };
            new_skin.colors.dark_mode = self.tune_dark_mode;
            new_skin.effects.background_image_opacity = self.tune_bg_opacity;
            new_skin.effects.editor_opacity = self.tune_editor_opacity;
            new_skin.effects.corner_radius = self.tune_corner_radius;
            action = Some(SettingsAction::SaveCustomSkin(Box::new(new_skin)));
            self.set_status(false, "已创建并应用新自定义皮肤".to_string());
        }

        action
    }

    fn show_market_tab(
        &mut self,
        ui: &mut Ui,
        _state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("皮肤市场源设置");
        ui.add_space(8.0);

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

        ui.horizontal(|ui| {
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
                    "{} 恢复官方默认配置 (RetricSu/paper-shell · master · skins)",
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
        ui.monospace(format!(
            "https://github.com/{}/{}/tree/{}/{}",
            self.repo_input.trim(),
            self.folder_input.trim(),
            self.branch_input.trim(),
            self.folder_input.trim()
        ));

        action
    }

    fn show_plugins_tab(
        &mut self,
        ui: &mut Ui,
        state: &SettingsWindowState<'_>,
    ) -> Option<SettingsAction> {
        let mut action = None;
        ui.heading("插件配置中心");
        ui.add_space(4.0);
        ui.label(
            RichText::new("在此集中管理与配置所有插件的连接和行为参数，与常规插件开关彻底解耦。")
                .weak()
                .small(),
        );
        ui.add_space(8.0);

        // Sub selector: which plugin to configure
        ui.horizontal(|ui| {
            ui.label(RichText::new("选择配置插件：").strong());
            if ui
                .selectable_label(self.selected_plugin_id == "cloud_backup", "☁️ 云端备份")
                .clicked()
            {
                self.selected_plugin_id = "cloud_backup".to_string();
                self.status_message = None;
            }
            if ui
                .selectable_label(
                    self.selected_plugin_id == "github_publish",
                    "🐙 GitHub 发布",
                )
                .clicked()
            {
                self.selected_plugin_id = "github_publish".to_string();
                self.status_message = None;
            }

            for p in state
                .plugins
                .iter()
                .filter(|p| p.source == crate::plugin::PluginSource::External)
            {
                if ui
                    .selectable_label(self.selected_plugin_id == p.id, format!("🧩 {}", p.name))
                    .clicked()
                {
                    self.selected_plugin_id = p.id.clone();
                    self.status_message = None;
                }
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        match self.selected_plugin_id.as_str() {
            "cloud_backup" => {
                ui.label(RichText::new("云端备份设置 (WebDAV)").strong());
                ui.add_space(4.0);

                egui::Grid::new("cloud_backup_settings_grid")
                    .num_columns(2)
                    .spacing([12.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("WebDAV 服务器地址：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cb_server_url)
                                .desired_width(280.0)
                                .hint_text("https://dav.example.com/remote.php/webdav/"),
                        );
                        ui.end_row();

                        ui.label("用户名 / 账号：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cb_username).desired_width(280.0),
                        );
                        ui.end_row();

                        ui.label("应用密码 / Token：");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.cb_password)
                                    .password(!self.cb_show_password)
                                    .desired_width(210.0),
                            );
                            if ui
                                .button(if self.cb_show_password {
                                    "隐藏"
                                } else {
                                    "显示"
                                })
                                .clicked()
                            {
                                self.cb_show_password = !self.cb_show_password;
                            }
                        });
                        ui.end_row();

                        ui.label("远端文件名格式：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cb_remote_filename)
                                .desired_width(280.0),
                        );
                        ui.end_row();
                    });

                ui.add_space(12.0);
                if ui
                    .button(format!(
                        "{} 保存云端备份设置",
                        egui_phosphor::regular::CHECK
                    ))
                    .clicked()
                {
                    let mut new_config = state.cloud_backup.clone();
                    new_config.server_url = self.cb_server_url.trim().to_string();
                    new_config.username = self.cb_username.trim().to_string();
                    new_config.password = self.cb_password.clone();
                    new_config.remote_filename = if self.cb_remote_filename.trim().is_empty() {
                        "{stem}_{doc_id}.backup".to_string()
                    } else {
                        self.cb_remote_filename.trim().to_string()
                    };
                    action = Some(SettingsAction::SaveCloudBackupConfig(new_config));
                    self.set_status(false, "云端备份配置已保存".to_string());
                }
            }
            "github_publish" => {
                ui.label(RichText::new("GitHub 博客发布设置").strong());
                ui.add_space(4.0);

                egui::Grid::new("github_publish_settings_grid")
                    .num_columns(2)
                    .spacing([12.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("博客仓库 (owner/repo)：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gh_repo)
                                .desired_width(280.0)
                                .hint_text("例如 RetricSu/blog"),
                        );
                        ui.end_row();

                        ui.label("目标基准分支：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gh_base_branch)
                                .desired_width(280.0)
                                .hint_text("例如 main 或 master"),
                        );
                        ui.end_row();

                        ui.label("文章保存目录：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gh_base_dir)
                                .desired_width(280.0)
                                .hint_text("例如 _posts 或 content/posts"),
                        );
                        ui.end_row();

                        ui.label("默认提交说明：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gh_commit_message)
                                .desired_width(280.0),
                        );
                        ui.end_row();

                        ui.label("默认 PR 标题模板：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gh_pr_title).desired_width(280.0),
                        );
                        ui.end_row();
                    });

                ui.add_space(12.0);
                if ui
                    .button(format!(
                        "{} 保存 GitHub 发布设置",
                        egui_phosphor::regular::CHECK
                    ))
                    .clicked()
                {
                    let mut new_config = state.github_publish.clone();
                    new_config.repo = self.gh_repo.trim().to_string();
                    new_config.base_branch = if self.gh_base_branch.trim().is_empty() {
                        "main".to_string()
                    } else {
                        self.gh_base_branch.trim().to_string()
                    };
                    new_config.base_dir = self.gh_base_dir.trim().to_string();
                    new_config.commit_message = self.gh_commit_message.trim().to_string();
                    new_config.pr_title = self.gh_pr_title.trim().to_string();

                    action = Some(SettingsAction::SaveGithubPublishConfig(new_config));
                    self.set_status(false, "GitHub 发布配置已保存".to_string());
                }
            }
            ext_id => {
                let plugin = state.plugins.iter().find(|p| p.id == ext_id);
                let defs = state.plugin_definitions.get(ext_id);
                if let Some(plugin) = plugin {
                    ui.label(RichText::new(format!("外部插件设置 · {}", plugin.name)).strong());
                    ui.add_space(4.0);

                    let Some(defs) = defs else {
                        ui.label("该插件无需任何配置。");
                        return action;
                    };

                    if defs.is_empty() {
                        ui.label("该插件无需任何配置。");
                    } else {
                        if self.ext_plugin_id != ext_id {
                            self.ext_plugin_id = ext_id.to_string();
                            let saved = state.external_plugin_settings.get(ext_id);
                            self.ext_values.clear();
                            for def in defs {
                                let val = saved
                                    .and_then(|m| m.get(&def.key))
                                    .cloned()
                                    .unwrap_or_else(|| def.default.clone());
                                self.ext_values.insert(def.key.clone(), val);
                            }
                        }

                        egui::Grid::new("ext_plugin_settings_grid")
                            .num_columns(2)
                            .spacing([12.0, 10.0])
                            .show(ui, |ui| {
                                for def in defs {
                                    let mut label = def.label.clone();
                                    if def.required {
                                        label.push_str(" *");
                                    }
                                    ui.label(label);
                                    let val = self.ext_values.entry(def.key.clone()).or_default();
                                    if def.setting_type
                                        == crate::plugin::external::SettingType::Password
                                    {
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(val)
                                                    .password(!self.ext_show_passwords)
                                                    .desired_width(200.0),
                                            );
                                            if ui
                                                .button(if self.ext_show_passwords {
                                                    "隐藏"
                                                } else {
                                                    "显示"
                                                })
                                                .clicked()
                                            {
                                                self.ext_show_passwords = !self.ext_show_passwords;
                                            }
                                        });
                                    } else {
                                        ui.add(
                                            egui::TextEdit::singleline(val).desired_width(260.0),
                                        );
                                    }
                                    ui.end_row();
                                }
                            });

                        ui.add_space(12.0);
                        if ui
                            .button(format!("{} 保存插件设置", egui_phosphor::regular::CHECK))
                            .clicked()
                        {
                            action = Some(SettingsAction::SaveExternalPluginConfig {
                                plugin_id: ext_id.to_string(),
                                values: self.ext_values.clone(),
                            });
                            self.set_status(false, "外部插件配置已保存".to_string());
                        }
                    }
                } else {
                    ui.label("请从上方选择要配置的插件。");
                }
            }
        }

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_window_open_and_close() {
        let mut window = SettingsWindow::new();
        assert!(!window.is_open());

        let ctx = egui::Context::default();
        window.open(&ctx, Some(SettingsTab::General), None);
        assert!(window.is_open());
        assert_eq!(window.active_tab, SettingsTab::General);

        window.open(
            &ctx,
            Some(SettingsTab::Plugins),
            Some("github_publish".to_string()),
        );
        assert_eq!(window.active_tab, SettingsTab::Plugins);
        assert_eq!(window.selected_plugin_id, "github_publish");

        window.close();
        assert!(!window.is_open());
    }

    #[test]
    fn settings_window_renders_and_dispatches_actions() {
        let mut window = SettingsWindow::new();
        let ctx = egui::Context::default();
        window.open(&ctx, Some(SettingsTab::Market), None);

        let skins = vec![crate::skin::Skin::classic_paper()];
        let cloud_backup = crate::plugin::builtin::cloud_backup::CloudBackupConfig::default();
        let github_publish = crate::plugin::builtin::github_publish::GithubPublishConfig::default();
        let ext_settings = BTreeMap::new();
        let plugin_defs = BTreeMap::new();

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
            plugins: &[],
            cloud_backup: &cloud_backup,
            github_publish: &github_publish,
            external_plugin_settings: &ext_settings,
            plugin_definitions: &plugin_defs,
        };

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let action = window.show(ctx, state);
            // Default market buffer is initialized
            assert_eq!(window.repo_input, "RetricSu/paper-shell");
            assert_eq!(window.branch_input, "master");
            assert_eq!(window.folder_input, "skins");
            assert!(action.is_none());
        });

        assert!(window.is_open());
    }

    #[test]
    fn settings_tab_labels_contain_expected_names() {
        assert!(SettingsTab::General.label().contains("常规"));
        assert!(SettingsTab::Typography.label().contains("排版"));
        assert!(SettingsTab::Skin.label().contains("皮肤与外观"));
        assert!(SettingsTab::Market.label().contains("皮肤市场源"));
        assert!(SettingsTab::Plugins.label().contains("插件设置"));
    }
}
