use crate::plugin::external::{SettingDefinition, SettingType};
use crate::plugin::{PluginLoadError, PluginMetadata, PluginSource};
use egui::{Color32, Context, RichText};
use std::collections::BTreeMap;

pub enum PluginManagerAction {
    SetEnabled { id: String, enabled: bool },
    Rescan,
    OpenPluginsFolder,
    Configure(String),
    Uninstall(String),
}

#[derive(Default)]
pub struct PluginManagerWindow {
    open: bool,
    status: Option<(bool, String)>,
    confirm_uninstall: Option<(String, String)>,
}

impl PluginManagerWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.open = true;
        self.status = None;
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
    ) -> Option<PluginManagerAction> {
        if !self.open {
            return None;
        }

        let mut open = self.open;
        let mut action = None;
        egui::Window::new(format!(
            "{} 插件管理",
            egui_phosphor::regular::PUZZLE_PIECE
        ))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(format!(
                        "{} 重新扫描",
                        egui_phosphor::regular::ARROW_CLOCKWISE
                    ))
                    .on_hover_text("重新读取插件目录，加载新插件并显示配置错误")
                    .clicked()
                {
                    action = Some(PluginManagerAction::Rescan);
                }
                if ui
                    .button(format!(
                        "{} 打开插件目录",
                        egui_phosphor::regular::FOLDER_OPEN
                    ))
                    .clicked()
                {
                    action = Some(PluginManagerAction::OpenPluginsFolder);
                }
            });
            ui.label(
                RichText::new("外部插件可执行本机命令。只安装来源可信的插件；密码设置会保存在应用配置中。")
                    .small()
                    .weak(),
            );

            if let Some((is_error, message)) = &self.status {
                ui.add_space(6.0);
                ui.colored_label(
                    if *is_error {
                        Color32::from_rgb(200, 80, 80)
                    } else {
                        Color32::from_rgb(60, 140, 80)
                    },
                    message,
                );
            }

            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if !load_errors.is_empty() {
                        egui::CollapsingHeader::new(format!(
                            "加载问题（{}）",
                            load_errors.len()
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            for error in load_errors {
                                ui.group(|ui| {
                                    if let Some(name) = &error.name {
                                        ui.label(RichText::new(name).strong());
                                    }
                                    if let Some(id) = &error.id {
                                        ui.label(RichText::new(format!("ID：{id}")).small().weak());
                                    }
                                    ui.colored_label(
                                        Color32::from_rgb(200, 80, 80),
                                        &error.error,
                                    );
                                    ui.label(
                                        RichText::new(error.path.display().to_string())
                                            .small()
                                            .weak(),
                                    );
                                });
                            }
                        });
                        ui.add_space(8.0);
                    }

                    if plugins.is_empty() {
                        ui.label("尚未发现插件。打开插件目录放入插件后，点击“重新扫描”。");
                    }

                    for plugin in plugins {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&plugin.name).strong());
                                ui.label(
                                    RichText::new(format!(
                                        "{} · v{} · {}",
                                        plugin.source, plugin.version, plugin.id
                                    ))
                                    .small()
                                    .weak(),
                                );
                                if !plugin.enabled {
                                    ui.label(RichText::new("已禁用").small());
                                }
                            });
                            if !plugin.author.trim().is_empty() {
                                ui.label(RichText::new(format!("作者：{}", plugin.author)).small());
                            }
                            if !plugin.description.trim().is_empty() {
                                ui.label(&plugin.description);
                            }
                            ui.horizontal_wrapped(|ui| {
                                let mut enabled = plugin.enabled;
                                if ui.checkbox(&mut enabled, "启用").changed() {
                                    action = Some(PluginManagerAction::SetEnabled {
                                        id: plugin.id.clone(),
                                        enabled,
                                    });
                                }
                                if plugin.has_settings
                                    && ui
                                        .button(format!(
                                            "{} 设置",
                                            egui_phosphor::regular::SLIDERS_HORIZONTAL
                                        ))
                                        .clicked()
                                {
                                    action = Some(PluginManagerAction::Configure(
                                        plugin.id.clone(),
                                    ));
                                }
                                if plugin.source == PluginSource::External
                                    && ui
                                        .button(format!(
                                            "{} 卸载",
                                            egui_phosphor::regular::TRASH
                                        ))
                                        .clicked()
                                {
                                    self.confirm_uninstall =
                                        Some((plugin.id.clone(), plugin.name.clone()));
                                }
                            });
                        });
                    }
                });
        });

        self.open = open;
        if !self.open {
            self.confirm_uninstall = None;
        }

        if let Some((id, name)) = self.confirm_uninstall.clone() {
            let mut confirmed = false;
            let mut cancelled = false;
            egui::Window::new("确认卸载插件")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("确定卸载“{name}”吗？"));
                    ui.label("插件文件夹将被永久删除，其已保存设置也会清除。此操作无法撤销。");
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            cancelled = true;
                        }
                        if ui
                            .button(RichText::new("永久删除").color(Color32::from_rgb(200, 60, 60)))
                            .clicked()
                        {
                            confirmed = true;
                        }
                    });
                });
            if confirmed {
                self.confirm_uninstall = None;
                action = Some(PluginManagerAction::Uninstall(id));
            } else if cancelled {
                self.confirm_uninstall = None;
            }
        }

        action
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

        let mut open = self.open;
        let mut saved = None;
        egui::Window::new(format!("插件设置 · {}", self.name))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("ID：{}", self.id)).small().weak());
                ui.add_space(8.0);

                for definition in &self.definitions {
                    let label = if definition.label.trim().is_empty() {
                        definition.key.as_str()
                    } else {
                        definition.label.as_str()
                    };
                    ui.label(format!(
                        "{}{}",
                        label,
                        if definition.required { " *" } else { "" }
                    ));
                    let value = self.values.entry(definition.key.clone()).or_default();
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .password(
                                definition.setting_type == SettingType::Password
                                    && !self.show_passwords,
                            )
                            .desired_width(f32::INFINITY),
                    );
                    if !definition.description.trim().is_empty() {
                        ui.label(RichText::new(&definition.description).small().weak());
                    }
                    ui.add_space(6.0);
                }

                if self
                    .definitions
                    .iter()
                    .any(|definition| definition.setting_type == SettingType::Password)
                {
                    ui.checkbox(&mut self.show_passwords, "显示密码");
                    ui.label(
                        RichText::new("密码会保存在应用配置文件中，不会写入运行日志。").small(),
                    );
                }

                if let Some(error) = &self.save_error {
                    ui.colored_label(Color32::from_rgb(200, 80, 80), error);
                }

                let complete = self.definitions.iter().all(|definition| {
                    !definition.required
                        || self
                            .values
                            .get(&definition.key)
                            .is_some_and(|value| !value.trim().is_empty())
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(complete, egui::Button::new("保存"))
                        .on_disabled_hover_text("请先填写所有必填项")
                        .clicked()
                    {
                        saved = Some(ExternalPluginSettings {
                            id: self.id.clone(),
                            values: self.values.clone(),
                        });
                    }
                    if ui.button("取消").clicked() {
                        self.open = false;
                        self.save_error = None;
                    }
                });
            });

        self.open = self.open && open;
        saved
    }
}
