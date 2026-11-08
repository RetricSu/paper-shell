use crate::plugin::external::{SettingDefinition, SettingType};
use crate::plugin::{PluginLoadError, PluginMetadata, PluginSource};
use crate::style::STATUS_GREEN;
use crate::ui::dialog::{ModalConfig, modal_button, show_modal};
use crate::ui::title_bar::TitleBar;
use egui::{Align, Color32, Context, Layout, RichText, Ui, Vec2};
use std::collections::BTreeMap;

// Restrained semantic palette aligned with HistoryWindow and dialog.rs.
const SUCCESS_TEXT_COLOR: Color32 = Color32::from_rgb(44, 105, 60);
const ERROR_TEXT_COLOR: Color32 = Color32::from_rgb(145, 64, 64);
const ERROR_LINE_BG: Color32 = Color32::from_rgb(250, 240, 240);
const WARNING_ICON_COLOR: Color32 = Color32::from_rgb(173, 120, 24);

const DEFAULT_MANAGER_WIDTH: f32 = 720.0;
const DEFAULT_MANAGER_HEIGHT: f32 = 520.0;
const MIN_MANAGER_WIDTH: f32 = 520.0;
const MIN_MANAGER_HEIGHT: f32 = 360.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManagerAction {
    SetEnabled { id: String, enabled: bool },
    Rescan,
    OpenPluginsFolder,
    Configure(String),
    Uninstall(String),
}

pub struct PluginManagerWindow {
    open: bool,
    viewport_id: egui::ViewportId,
    status: Option<(bool, String)>,
    confirm_uninstall: Option<(String, String)>,
}

impl Default for PluginManagerWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManagerWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            viewport_id: egui::ViewportId::from_hash_of("plugin_manager_window"),
            status: None,
            confirm_uninstall: None,
        }
    }

    pub fn viewport_id(&self) -> egui::ViewportId {
        self.viewport_id
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, ctx: &Context) {
        if self.open {
            ctx.send_viewport_cmd_to(self.viewport_id, egui::ViewportCommand::Focus);
        } else {
            self.open = true;
            self.status = None;
            self.confirm_uninstall = None;
        }
        ctx.request_repaint();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.confirm_uninstall = None;
    }

    pub fn set_status(&mut self, is_error: bool, message: String) {
        self.status = Some((is_error, message));
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        plugins: &[PluginMetadata],
        load_errors: &[PluginLoadError],
        config_window: &mut ExternalPluginConfigWindow,
    ) -> (Option<PluginManagerAction>, Option<ExternalPluginSettings>) {
        if !self.open {
            return (None, None);
        }

        let viewport_id = self.viewport_id;
        let mut action = None;
        let mut saved_settings = None;

        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(format!(
                    "{} 插件管理 · {}",
                    egui_phosphor::regular::PUZZLE_PIECE,
                    crate::constant::DEFAULT_WINDOW_TITLE
                ))
                .with_inner_size(Vec2::new(DEFAULT_MANAGER_WIDTH, DEFAULT_MANAGER_HEIGHT))
                .with_min_inner_size(Vec2::new(MIN_MANAGER_WIDTH, MIN_MANAGER_HEIGHT))
                .with_decorations(false)
                .with_resizable(true)
                .with_transparent(true),
            |ctx, _class| {
                crate::style::configure_style(ctx);

                let had_modal = self.confirm_uninstall.is_some()
                    || (ctx.embed_viewports() && config_window.is_open());

                let title_bar_panel = egui::TopBottomPanel::top("plugin_manager_title_bar")
                    .show_separator_line(false)
                    .show(ctx, |ui| {
                        if had_modal {
                            ui.disable();
                        }
                        if let Some(bar_action) = self.show_title_bar(ui, plugins) {
                            action = Some(bar_action);
                        }
                    });
                TitleBar::paint_bottom_border(ctx, title_bar_panel.response.rect);

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::ZERO),
                    )
                    .show(ctx, |ui| {
                        if had_modal {
                            ui.disable();
                        }
                        if let Some(body_action) = self.show_content(ui, plugins, load_errors) {
                            action = Some(body_action);
                        }
                    });

                if let Some(settings) = config_window.show(ctx) {
                    saved_settings = Some(settings);
                }

                if let Some(uninstall_action) = self.show_uninstall_modal(ctx) {
                    action = Some(uninstall_action);
                }

                let close_requested = ctx.input(|i| i.viewport().close_requested())
                    || (!had_modal
                        && ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        }));
                if close_requested {
                    self.close();
                    config_window.close();
                }

                if !self.open {
                    self.confirm_uninstall = None;
                    config_window.close();
                }
            },
        );

        (action, saved_settings)
    }

    fn show_title_bar(
        &mut self,
        ui: &mut Ui,
        plugins: &[PluginMetadata],
    ) -> Option<PluginManagerAction> {
        let mut action = None;
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 20.0;

        let drag = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("plugin_manager_title_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        let show_summary = ui.available_width() >= 480.0;
        let enabled_count = plugins.iter().filter(|p| p.enabled).count();

        egui::MenuBar::new().ui(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_sized(
                        [22.0, 20.0],
                        egui::Button::new(egui_phosphor::regular::X).frame(false),
                    )
                    .on_hover_text("关闭窗口")
                    .clicked()
                {
                    self.close();
                }

                let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                let max_icon = if is_maximized {
                    egui_phosphor::regular::CORNERS_IN
                } else {
                    egui_phosphor::regular::CORNERS_OUT
                };
                if ui
                    .add_sized([22.0, 20.0], egui::Button::new(max_icon).frame(false))
                    .on_hover_text(if is_maximized { "还原" } else { "最大化" })
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                if ui
                    .add_sized(
                        [22.0, 20.0],
                        egui::Button::new(egui_phosphor::regular::MINUS).frame(false),
                    )
                    .on_hover_text("最小化")
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                if show_summary {
                    ui.add_space(8.0);
                    let summary_color = ui.visuals().text_color().gamma_multiply(0.58);
                    ui.label(
                        RichText::new(format!("已启用 {} / {}", enabled_count, plugins.len()))
                            .small()
                            .color(summary_color),
                    );
                    ui.add_space(8.0);
                }

                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let title_color = ui.visuals().text_color().gamma_multiply(0.82);
                    ui.label(
                        RichText::new(format!("{} 插件管理", egui_phosphor::regular::PUZZLE_PIECE))
                            .strong()
                            .color(title_color),
                    );
                    ui.add_space(6.0);
                    if quiet_button(
                        ui,
                        format!("{} 重新扫描", egui_phosphor::regular::ARROW_CLOCKWISE),
                    )
                    .on_hover_text("重新读取插件目录，加载新插件并显示配置错误")
                    .clicked()
                    {
                        action = Some(PluginManagerAction::Rescan);
                    }
                    if quiet_button(
                        ui,
                        format!("{} 打开插件目录", egui_phosphor::regular::FOLDER_OPEN),
                    )
                    .on_hover_text("将外部插件文件夹放入此目录后点击“重新扫描”")
                    .clicked()
                    {
                        action = Some(PluginManagerAction::OpenPluginsFolder);
                    }
                });
            });
        });

        action
    }

    fn show_content(
        &mut self,
        ui: &mut Ui,
        plugins: &[PluginMetadata],
        load_errors: &[PluginLoadError],
    ) -> Option<PluginManagerAction> {
        let mut action = None;
        let text_color = ui.visuals().text_color();

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(20, 10))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "外部插件可执行本机命令。只安装来源可信的插件；密码设置保存在应用配置中。",
                        )
                        .small()
                        .color(text_color.gamma_multiply(0.58)),
                    )
                    .wrap(),
                );

                if let Some((is_error, message)) = self.status.clone() {
                    let (icon, color) = if is_error {
                        (egui_phosphor::regular::WARNING_CIRCLE, ERROR_TEXT_COLOR)
                    } else {
                        (egui_phosphor::regular::CHECK_CIRCLE, SUCCESS_TEXT_COLOR)
                    };
                    let mut dismiss = false;
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if ui
                                .add_sized(
                                    [20.0, 20.0],
                                    egui::Button::new(
                                        RichText::new(egui_phosphor::regular::X)
                                            .small()
                                            .color(text_color.gamma_multiply(0.55)),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("关闭提示")
                                .clicked()
                            {
                                dismiss = true;
                            }
                            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{icon} {message}"))
                                            .size(12.5)
                                            .color(color),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    });
                    if dismiss {
                        self.status = None;
                    }
                }
            });

        hairline_separator(ui);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(20, 12))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;

                        if !load_errors.is_empty() {
                            self.show_load_errors(ui, load_errors);
                            ui.add_space(8.0);
                            hairline_separator(ui);
                            ui.add_space(10.0);
                        }

                        if plugins.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(48.0);
                                ui.label(
                                    RichText::new("尚未发现插件")
                                        .size(15.0)
                                        .strong()
                                        .color(text_color.gamma_multiply(0.82)),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("打开插件目录放入插件后，点击上方“重新扫描”。")
                                        .color(text_color.gamma_multiply(0.58)),
                                );
                            });
                            return;
                        }

                        for (index, plugin) in plugins.iter().enumerate() {
                            ui.push_id(&plugin.id, |ui| {
                                if let Some(row_action) = self.show_plugin_row(ui, plugin) {
                                    action = Some(row_action);
                                }
                            });
                            if index + 1 < plugins.len() {
                                ui.add_space(10.0);
                                hairline_separator(ui);
                                ui.add_space(10.0);
                            }
                        }
                    });
            });

        action
    }

    fn show_load_errors(&self, ui: &mut Ui, load_errors: &[PluginLoadError]) {
        let text_color = ui.visuals().text_color();
        ui.label(
            RichText::new(format!(
                "{} 加载问题（{}）",
                egui_phosphor::regular::WARNING_CIRCLE,
                load_errors.len()
            ))
            .size(13.0)
            .strong()
            .color(ERROR_TEXT_COLOR),
        );
        ui.add_space(4.0);

        for (idx, error) in load_errors.iter().enumerate() {
            ui.push_id(("plugin_load_error", idx), |ui| {
                egui::Frame::new()
                    .fill(ERROR_LINE_BG)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ERROR_TEXT_COLOR.gamma_multiply(0.22),
                    ))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 3.0;
                        if error.name.is_some() || error.id.is_some() {
                            let mut header_parts = Vec::new();
                            if let Some(name) = &error.name {
                                header_parts.push(name.clone());
                            }
                            if let Some(id) = &error.id {
                                header_parts.push(format!("ID：{id}"));
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(header_parts.join("  ·  "))
                                        .size(13.0)
                                        .strong()
                                        .color(text_color.gamma_multiply(0.85)),
                                )
                                .wrap(),
                            );
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(&error.error)
                                    .size(12.5)
                                    .color(ERROR_TEXT_COLOR),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(error.path.display().to_string())
                                    .small()
                                    .monospace()
                                    .color(text_color.gamma_multiply(0.55)),
                            )
                            .wrap(),
                        );
                    });
            });
            if idx + 1 < load_errors.len() {
                ui.add_space(4.0);
            }
        }
    }

    fn show_plugin_row(
        &mut self,
        ui: &mut Ui,
        plugin: &PluginMetadata,
    ) -> Option<PluginManagerAction> {
        let mut action = None;
        let text_color = ui.visuals().text_color();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut enabled = plugin.enabled;
                if paper_checkbox(ui, &mut enabled, "启用").changed() {
                    action = Some(PluginManagerAction::SetEnabled {
                        id: plugin.id.clone(),
                        enabled,
                    });
                }

                if plugin.has_settings
                    && quiet_button(
                        ui,
                        format!("{} 设置", egui_phosphor::regular::SLIDERS_HORIZONTAL),
                    )
                    .clicked()
                {
                    action = Some(PluginManagerAction::Configure(plugin.id.clone()));
                }

                if plugin.source == PluginSource::External
                    && quiet_button(
                        ui,
                        RichText::new(format!("{} 卸载", egui_phosphor::regular::TRASH))
                            .color(ERROR_TEXT_COLOR),
                    )
                    .clicked()
                {
                    self.confirm_uninstall = Some((plugin.id.clone(), plugin.name.clone()));
                }

                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 20.0), egui::Sense::hover());
                    let dot_center = egui::pos2(dot_rect.left() + 3.5, dot_rect.center().y);
                    let dot_color =
                        STATUS_GREEN.gamma_multiply(if plugin.enabled { 0.9 } else { 0.55 });
                    if plugin.enabled {
                        ui.painter().circle_filled(dot_center, 2.4, dot_color);
                    } else {
                        ui.painter().circle_stroke(
                            dot_center,
                            2.4,
                            egui::Stroke::new(1.0_f32, dot_color),
                        );
                    }

                    let name_color = if plugin.enabled {
                        text_color
                    } else {
                        text_color.gamma_multiply(0.62)
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(&plugin.name)
                                .size(14.0)
                                .strong()
                                .color(name_color),
                        )
                        .truncate(),
                    );
                });
            });
        });

        let mut meta_parts = vec![plugin.source.to_string(), format!("v{}", plugin.version)];
        if !plugin.author.trim().is_empty() {
            meta_parts.push(plugin.author.trim().to_string());
        }
        meta_parts.push(plugin.id.clone());
        if !plugin.enabled {
            meta_parts.push("已禁用".to_string());
        }

        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 16,
                right: 0,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(meta_parts.join("  ·  "))
                            .small()
                            .color(text_color.gamma_multiply(0.55)),
                    )
                    .wrap(),
                );
                if !plugin.description.trim().is_empty() {
                    let desc_color =
                        text_color.gamma_multiply(if plugin.enabled { 0.76 } else { 0.55 });
                    ui.add(
                        egui::Label::new(
                            RichText::new(plugin.description.trim())
                                .size(13.0)
                                .color(desc_color),
                        )
                        .wrap(),
                    );
                }
                if let Some(dir) = &plugin.dir {
                    ui.add(
                        egui::Label::new(
                            RichText::new(dir.display().to_string())
                                .small()
                                .monospace()
                                .color(text_color.gamma_multiply(0.48)),
                        )
                        .wrap(),
                    );
                }
            });

        action
    }

    fn show_uninstall_modal(&mut self, ctx: &Context) -> Option<PluginManagerAction> {
        let (id, name) = self.confirm_uninstall.clone()?;
        let message = format!(
            "确定卸载“{name}”吗？\n插件文件夹将被永久删除，其已保存设置也会清除。此操作无法撤销。"
        );
        let config = ModalConfig {
            id: "plugin_confirm_uninstall",
            title: "确认卸载插件",
            icon: egui_phosphor::regular::WARNING,
            icon_color: WARNING_ICON_COLOR,
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
                        .color(ERROR_TEXT_COLOR),
                )
                .clicked()
                {
                    confirmed = true;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if modal_button(ui, RichText::new("取消").strong()).clicked() {
                        cancelled = true;
                    }
                });
            });
        });

        if confirmed {
            self.confirm_uninstall = None;
            Some(PluginManagerAction::Uninstall(id))
        } else if cancelled || response.escape_pressed {
            self.confirm_uninstall = None;
            None
        } else {
            None
        }
    }
}

pub struct ExternalPluginSettings {
    pub id: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Default)]
pub struct ExternalPluginConfigWindow {
    open: bool,
    id: String,
    name: String,
    definitions: Vec<SettingDefinition>,
    values: BTreeMap<String, String>,
    show_passwords: bool,
    save_error: Option<String>,
}

impl ExternalPluginConfigWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(
        &mut self,
        plugin: &PluginMetadata,
        definitions: Vec<SettingDefinition>,
        saved: &BTreeMap<String, String>,
    ) {
        self.id = plugin.id.clone();
        self.name = plugin.name.clone();
        self.values = definitions
            .iter()
            .map(|definition| {
                (
                    definition.key.clone(),
                    saved
                        .get(&definition.key)
                        .cloned()
                        .unwrap_or_else(|| definition.default.clone()),
                )
            })
            .collect();
        self.definitions = definitions;
        self.show_passwords = false;
        self.save_error = None;
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.save_error = None;
    }

    pub fn notify_save_error(&mut self, error: String) {
        self.save_error = Some(error);
        self.open = true;
    }

    pub fn notify_saved(&mut self) {
        self.open = false;
        self.save_error = None;
    }

    pub fn show(&mut self, ctx: &Context) -> Option<ExternalPluginSettings> {
        if !self.open {
            return None;
        }

        let complete = self.definitions.iter().all(|definition| {
            !definition.required
                || self
                    .values
                    .get(&definition.key)
                    .is_some_and(|value| !value.trim().is_empty())
        });

        let mut save_clicked = false;
        let mut cancel_clicked = false;

        let mut open = self.open;
        let mut enter_pressed = false;
        let title = format!("插件设置 · {}", self.name);
        crate::ui::viewport::show_tool_window(
            ctx,
            "external_plugin_config_window",
            &title,
            [460.0, 520.0],
            &mut open,
            |ui| {
                let text_color = ui.visuals().text_color();
                let max_body_height = (ui.available_height() - 100.0).max(60.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    ui.spacing_mut().interact_size.y = 20.0;

                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(&self.id)
                                            .small()
                                            .monospace()
                                            .color(text_color.gamma_multiply(0.50)),
                                    );
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(egui_phosphor::regular::SLIDERS_HORIZONTAL)
                                                .size(14.0)
                                                .color(text_color.gamma_multiply(0.75)),
                                        );
                                        ui.add_space(4.0);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!("插件设置 · {}", self.name))
                                                    .strong()
                                                    .size(14.0)
                                                    .color(text_color.gamma_multiply(0.82)),
                                            )
                                            .truncate(),
                                        );
                                    });
                                });
                            });
                        });

                    hairline_separator(ui);

                    egui::ScrollArea::vertical()
                        .max_height(max_body_height)
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(12, 10))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 4.0;

                                    for definition in &self.definitions {
                                        let label = if definition.label.trim().is_empty() {
                                            definition.key.as_str()
                                        } else {
                                            definition.label.as_str()
                                        };
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            ui.label(
                                                RichText::new(label)
                                                    .size(13.0)
                                                    .color(text_color.gamma_multiply(0.82)),
                                            );
                                            if definition.required {
                                                ui.label(
                                                    RichText::new("*")
                                                        .size(13.0)
                                                        .color(ERROR_TEXT_COLOR),
                                                );
                                            }
                                        });

                                        let value =
                                            self.values.entry(definition.key.clone()).or_default();
                                        let is_password =
                                            definition.setting_type == SettingType::Password
                                                && !self.show_passwords;
                                        paper_text_edit(ui, value, is_password);

                                        if !definition.description.trim().is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&definition.description)
                                                        .small()
                                                        .color(text_color.gamma_multiply(0.56)),
                                                )
                                                .wrap(),
                                            );
                                        }
                                        ui.add_space(6.0);
                                    }

                                    if self.definitions.iter().any(|definition| {
                                        definition.setting_type == SettingType::Password
                                    }) {
                                        paper_checkbox(ui, &mut self.show_passwords, "显示密码");
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(
                                                    "密码会保存在应用配置文件中，不会写入运行日志。",
                                                )
                                                .small()
                                                .color(text_color.gamma_multiply(0.56)),
                                            )
                                            .wrap(),
                                        );
                                        ui.add_space(4.0);
                                    }

                                    if let Some(error) = &self.save_error {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(error)
                                                    .size(12.5)
                                                    .color(ERROR_TEXT_COLOR),
                                            )
                                            .wrap(),
                                        );
                                    }
                                });
                        });

                    hairline_separator(ui);

                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let save_btn = ui
                                        .add_enabled(
                                            complete,
                                            egui::Button::new(
                                                RichText::new(format!(
                                                    "{} 保存",
                                                    egui_phosphor::regular::CHECK
                                                ))
                                                .strong(),
                                            )
                                            .frame(false)
                                            .min_size(egui::vec2(0.0, 20.0)),
                                        )
                                        .on_disabled_hover_text("请先填写所有必填项");
                                    if save_btn.clicked() {
                                        save_clicked = true;
                                    }

                                    if modal_button(ui, "取消").clicked() {
                                        cancel_clicked = true;
                                    }
                                });
                            });
                        });
                });
                let ctx = ui.ctx();
                let ime_active =
                    ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))));
                enter_pressed = !ime_active
                    && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            },
        );

        if cancel_clicked || !open {
            self.close();
            None
        } else if save_clicked || (enter_pressed && complete) {
            Some(ExternalPluginSettings {
                id: self.id.clone(),
                values: self.values.clone(),
            })
        } else {
            None
        }
    }
}

fn hairline_separator(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    let color = ui.visuals().text_color().gamma_multiply(0.18);
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0_f32, color),
    );
}

fn quiet_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .frame(false)
            .min_size(egui::vec2(0.0, 20.0)),
    )
}

/// Gives checkboxes a crisp square paper box while global button backgrounds stay transparent.
fn paper_checkbox(
    ui: &mut Ui,
    checked: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.scope(|ui| {
        let text_color = ui.visuals().text_color();
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_fill = Color32::WHITE;
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.28));
        visuals.widgets.hovered.bg_fill = Color32::WHITE;
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.55));
        visuals.widgets.active.bg_fill = Color32::from_gray(242);
        visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.75));
        ui.checkbox(checked, label)
    })
    .inner
}

/// Gives single-line inputs a crisp 1px paper border inside frameless dialogs.
fn paper_text_edit(ui: &mut Ui, value: &mut String, password: bool) -> egui::Response {
    ui.scope(|ui| {
        let text_color = ui.visuals().text_color();
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.22));
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.45));
        visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.72));
        ui.add(
            egui::TextEdit::singleline(value)
                .password(password)
                .desired_width(f32::INFINITY),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_plugins() -> Vec<PluginMetadata> {
        vec![
            PluginMetadata {
                id: "print".to_string(),
                name: "打印当前文档".to_string(),
                description: "使用系统打印服务打印当前编辑器内容".to_string(),
                version: "0.1.5".to_string(),
                author: "Paper Shell".to_string(),
                source: PluginSource::Builtin,
                enabled: true,
                has_settings: false,
                dir: None,
            },
            PluginMetadata {
                id: "github_publish".to_string(),
                name: "GitHub 博客发布".to_string(),
                description: "通过 gh CLI 克隆博客仓库、创建分支并提交 PR".to_string(),
                version: "0.1.5".to_string(),
                author: "Paper Shell".to_string(),
                source: PluginSource::Builtin,
                enabled: false,
                has_settings: true,
                dir: None,
            },
            PluginMetadata {
                id: "word-count".to_string(),
                name: "字数统计".to_string(),
                description: "统计当前文档字数".to_string(),
                version: "0.2.0".to_string(),
                author: "writer".to_string(),
                source: PluginSource::External,
                enabled: true,
                has_settings: true,
                dir: Some(PathBuf::from("/tmp/plugins/word-count")),
            },
        ]
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

    fn key_press(key: egui::Key) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    fn exact_text_positions(output: &egui::FullOutput, target: &str) -> Vec<egui::Pos2> {
        let mut positions = Vec::new();
        fn collect(shape: &egui::Shape, target: &str, out: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == target => {
                    out.push(text.visual_bounding_rect().center());
                }
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect(s, target, out);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            collect(&clipped.shape, target, &mut positions);
        }
        positions
    }

    fn any_rect<F>(shape: &egui::Shape, predicate: &F) -> bool
    where
        F: Fn(&egui::epaint::RectShape) -> bool,
    {
        match shape {
            egui::Shape::Rect(rect) => predicate(rect),
            egui::Shape::Vec(shapes) => shapes.iter().any(|s| any_rect(s, predicate)),
            _ => false,
        }
    }

    #[test]
    fn native_settings_validate_save_and_dismiss_without_closing_manager() {
        use std::{cell::Cell, rc::Rc};
        let ctx = Context::default();
        ctx.set_embed_viewports(false);
        let stage = Rc::new(Cell::new(0));
        let current_stage = stage.clone();
        Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let mut input = egui::RawInput {
                viewport_id: viewport.ids.this,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    viewport.builder.inner_size.unwrap(),
                )),
                ..Default::default()
            };
            let info = input.viewports.entry(viewport.ids.this).or_default();
            info.parent = Some(viewport.ids.parent);
            if viewport.ids.this == egui::ViewportId::from_hash_of("external_plugin_config_window")
            {
                match current_stage.get() {
                    0 | 1 => input.events = key_press(egui::Key::Enter),
                    2 => input.events = key_press(egui::Key::Escape),
                    _ => info.events.push(egui::ViewportEvent::Close),
                }
            }
            let _ = ctx.run(input, |ctx| (viewport.viewport_ui_cb)(ctx));
        });
        let mut manager = PluginManagerWindow::new();
        manager.open(&ctx);
        let mut settings = ExternalPluginConfigWindow::new();
        let plugins = sample_plugins();
        settings.open(
            &plugins[2],
            vec![SettingDefinition {
                key: "token".into(),
                label: "令牌".into(),
                description: String::new(),
                setting_type: SettingType::Password,
                required: true,
                default: String::new(),
            }],
            &BTreeMap::new(),
        );
        let mut saved = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            saved = manager.show(ctx, &plugins, &[], &mut settings).1;
        });
        assert!(saved.is_none());
        assert!(settings.is_open());
        settings.values.insert("token".into(), "secret".into());
        stage.set(1);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            saved = manager.show(ctx, &plugins, &[], &mut settings).1;
        });
        assert_eq!(saved.unwrap().values["token"], "secret");
        // A failed persistence attempt keeps the draft in its own window.
        settings.notify_save_error("保存失败".into());
        stage.set(2);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!(manager.show(ctx, &plugins, &[], &mut settings).1.is_none());
        });
        assert!(!settings.is_open());
        assert!(manager.is_open());
        settings.notify_save_error("重试".into());
        stage.set(3);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!(manager.show(ctx, &plugins, &[], &mut settings).1.is_none());
        });
        assert!(!settings.is_open());
        assert!(manager.is_open());
    }

    #[test]
    fn standalone_window_uses_distinct_viewport_and_escape_priority() {
        let ctx = Context::default();
        let mut manager = PluginManagerWindow::new();
        let mut config_win = ExternalPluginConfigWindow::new();
        let plugins = sample_plugins();

        assert_ne!(manager.viewport_id(), egui::ViewportId::ROOT);
        assert!(!manager.is_open());

        manager.open(&ctx);
        assert!(manager.is_open());

        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 520.0));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                let _ = manager.show(ctx, &plugins, &[], &mut config_win);
            },
        );

        // Open external plugin settings modal; Escape closes modal first, keeping manager open.
        config_win.open(
            &plugins[2],
            vec![SettingDefinition {
                key: "api_key".into(),
                label: "API 密钥".into(),
                description: String::new(),
                setting_type: SettingType::Password,
                required: true,
                default: String::new(),
            }],
            &BTreeMap::new(),
        );
        assert!(config_win.is_open());

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events: key_press(egui::Key::Escape),
                ..Default::default()
            },
            |ctx| {
                let _ = manager.show(ctx, &plugins, &[], &mut config_win);
            },
        );
        assert!(
            !config_win.is_open(),
            "Escape should dismiss settings modal"
        );
        assert!(
            manager.is_open(),
            "Manager window must stay open when settings modal handles Escape"
        );

        // Trigger uninstall confirmation; Escape cancels confirmation first, keeping manager open.
        manager.confirm_uninstall = Some(("word-count".into(), "字数统计".into()));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events: key_press(egui::Key::Escape),
                ..Default::default()
            },
            |ctx| {
                let (action, _) = manager.show(ctx, &plugins, &[], &mut config_win);
                assert!(action.is_none());
            },
        );
        assert!(manager.confirm_uninstall.is_none());
        assert!(
            manager.is_open(),
            "Manager window must stay open when uninstall modal handles Escape"
        );

        // With no modals open, Escape closes the manager window itself.
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events: key_press(egui::Key::Escape),
                ..Default::default()
            },
            |ctx| {
                let _ = manager.show(ctx, &plugins, &[], &mut config_win);
            },
        );
        assert!(!manager.is_open());
    }

    #[test]
    fn plugin_manager_matches_app_palette_and_avoids_button_boxes_at_rest() {
        let ctx = Context::default();
        let mut manager = PluginManagerWindow::new();
        let mut config_win = ExternalPluginConfigWindow::new();
        let plugins = sample_plugins();
        manager.open(&ctx);

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(720.0, 520.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input.clone(), |ctx| {
            let _ = manager.show(ctx, &plugins, &[], &mut config_win);
        });
        let output = ctx.run(input, |ctx| {
            let _ = manager.show(ctx, &plugins, &[], &mut config_win);
        });

        let generic_button_fill = Color32::from_gray(230);
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| any_rect(&s.shape, &|r| r.fill == generic_button_fill)),
            "resting manager controls must not paint generic gray button chrome"
        );
    }

    #[test]
    fn plugin_manager_fits_minimum_width_with_long_names_and_load_errors() {
        for width in [360.0, MIN_MANAGER_WIDTH, DEFAULT_MANAGER_WIDTH] {
            let ctx = Context::default();
            let mut manager = PluginManagerWindow::new();
            let mut config_win = ExternalPluginConfigWindow::new();
            manager.open(&ctx);
            manager.set_status(
                false,
                format!("已打开插件目录：/tmp/{}", "very_long_segment_".repeat(12)),
            );

            let plugins = vec![PluginMetadata {
                id: "external-long-id".to_string(),
                name: "超长插件名称测试".repeat(25),
                description: "这是一段非常详细的插件说明文字，用于验证窄窗口下自动换行不会溢出。"
                    .repeat(6),
                version: "1.2.3".to_string(),
                author: "Long Author Name".to_string(),
                source: PluginSource::External,
                enabled: false,
                has_settings: true,
                dir: Some(PathBuf::from(format!(
                    "/tmp/plugins/{}",
                    "nested_folder_".repeat(10)
                ))),
            }];
            let errors = vec![PluginLoadError {
                path: PathBuf::from(format!("/tmp/plugins/{}/plugin.toml", "broken_".repeat(10))),
                id: Some("broken-plugin".into()),
                name: Some("损坏的外部插件".into()),
                error: "清单字段缺失：command 不能为空".repeat(4),
            }];

            for _ in 0..2 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, DEFAULT_MANAGER_HEIGHT),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        let _ = manager.show(ctx, &plugins, &errors, &mut config_win);
                    },
                );
                for clipped in &output.shapes {
                    if let egui::Shape::Text(text) = &clipped.shape {
                        assert!(
                            text.visual_bounding_rect().right() <= width + 2.0,
                            "text {:?} overflowed width {width}: {:?}",
                            text.galley.text(),
                            text.visual_bounding_rect()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn plugin_manager_dispatches_rescan_toggle_configure_uninstall_and_settings_save() {
        let ctx = Context::default();
        let mut manager = PluginManagerWindow::new();
        let mut config_win = ExternalPluginConfigWindow::new();
        let plugins = sample_plugins();
        manager.open(&ctx);

        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 520.0));
        let render = |manager: &mut PluginManagerWindow,
                      config_win: &mut ExternalPluginConfigWindow,
                      events: Vec<egui::Event>| {
            let mut result = (None, None);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    result = manager.show(ctx, &plugins, &[], config_win);
                },
            );
            (output, result.0, result.1)
        };

        let _ = render(&mut manager, &mut config_win, vec![]);
        let (output, _, _) = render(&mut manager, &mut config_win, vec![]);

        // Click Rescan in the title bar
        let rescan_label = format!("{} 重新扫描", egui_phosphor::regular::ARROW_CLOCKWISE);
        let rescan_pos = exact_text_positions(&output, &rescan_label)[0];
        let (_, action, _) = render(&mut manager, &mut config_win, click_events(rescan_pos));
        assert_eq!(action, Some(PluginManagerAction::Rescan));

        // Click OpenPluginsFolder in the title bar
        let open_dir_label = format!("{} 打开插件目录", egui_phosphor::regular::FOLDER_OPEN);
        let open_dir_pos = exact_text_positions(&output, &open_dir_label)[0];
        let (_, action, _) = render(&mut manager, &mut config_win, click_events(open_dir_pos));
        assert_eq!(action, Some(PluginManagerAction::OpenPluginsFolder));

        // Toggle enable checkbox on first plugin ("打印当前文档", currently enabled = true -> false)
        let (output, _, _) = render(&mut manager, &mut config_win, vec![]);
        let enable_pos = exact_text_positions(&output, "启用")[0];
        let (_, action, _) = render(&mut manager, &mut config_win, click_events(enable_pos));
        assert_eq!(
            action,
            Some(PluginManagerAction::SetEnabled {
                id: "print".to_string(),
                enabled: false,
            })
        );

        // Click Configure on first configurable plugin ("GitHub 博客发布")
        let (output, _, _) = render(&mut manager, &mut config_win, vec![]);
        let configure_label = format!("{} 设置", egui_phosphor::regular::SLIDERS_HORIZONTAL);
        let configure_pos = exact_text_positions(&output, &configure_label)[0];
        let (_, action, _) = render(&mut manager, &mut config_win, click_events(configure_pos));
        assert_eq!(
            action,
            Some(PluginManagerAction::Configure("github_publish".to_string()))
        );

        // Click Uninstall on external plugin -> opens modal -> confirm permanent delete
        let (output, _, _) = render(&mut manager, &mut config_win, vec![]);
        let uninstall_label = format!("{} 卸载", egui_phosphor::regular::TRASH);
        let uninstall_pos = exact_text_positions(&output, &uninstall_label)[0];
        let _ = render(&mut manager, &mut config_win, click_events(uninstall_pos));
        assert_eq!(
            manager.confirm_uninstall,
            Some(("word-count".to_string(), "字数统计".to_string()))
        );
        let _ = render(&mut manager, &mut config_win, vec![]);
        let (modal_output, _, _) = render(&mut manager, &mut config_win, vec![]);
        let confirm_label = format!("{} 永久删除", egui_phosphor::regular::TRASH);
        let confirm_pos = exact_text_positions(&modal_output, &confirm_label)[0];
        let (_, action, _) = render(&mut manager, &mut config_win, click_events(confirm_pos));
        assert_eq!(
            action,
            Some(PluginManagerAction::Uninstall("word-count".to_string()))
        );

        // External settings modal: required validation blocks empty save, filled value saves
        config_win.open(
            &plugins[2],
            vec![SettingDefinition {
                key: "token".into(),
                label: "访问令牌".into(),
                description: "必填凭证".into(),
                setting_type: SettingType::Password,
                required: true,
                default: String::new(),
            }],
            &BTreeMap::new(),
        );
        let (_, _, saved_empty) =
            render(&mut manager, &mut config_win, key_press(egui::Key::Enter));
        assert!(saved_empty.is_none());
        assert!(config_win.is_open());

        config_win
            .values
            .insert("token".into(), "secret-123".into());
        let (_, _, saved_ok) = render(&mut manager, &mut config_win, key_press(egui::Key::Enter));
        let saved = saved_ok.expect("filled required setting should save on Enter");
        assert_eq!(saved.id, "word-count");
        assert_eq!(
            saved.values.get("token").map(String::as_str),
            Some("secret-123")
        );
    }
}
