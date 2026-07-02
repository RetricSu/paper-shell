//! UI for displaying plugin execution status and results.
//!
//! Plugins run on a background thread, so this window has three states: hidden,
//! running (spinner), and finished (success or error text). The app drives it
//! by calling [`PluginOutputWindow::start`] when launching a plugin and
//! [`PluginOutputWindow::finish`] when the result arrives.

use egui::{Color32, Context, RichText, Vec2};

#[derive(Default)]
pub struct PluginOutputWindow {
    open: bool,
    running: bool,
    is_error: bool,
    title: String,
    body: String,
}

impl PluginOutputWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows the window in its "running" state for the given plugin name.
    pub fn start(&mut self, plugin_name: &str) {
        self.open = true;
        self.running = true;
        self.is_error = false;
        self.title = plugin_name.to_string();
        self.body.clear();
    }

    /// Updates the window with the plugin's result.
    pub fn finish(&mut self, plugin_name: String, result: Result<String, String>) {
        self.open = true;
        self.running = false;
        self.title = plugin_name;
        match result {
            Ok(message) => {
                self.is_error = false;
                self.body = if message.trim().is_empty() {
                    "完成".to_string()
                } else {
                    message
                };
            }
            Err(error) => {
                self.is_error = true;
                self.body = error;
            }
        }
    }

    /// Renders the window if visible.
    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new(format!("插件 · {}", self.title))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                if self.running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("正在运行…");
                    });
                    return;
                }

                let color = if self.is_error {
                    Color32::from_rgb(200, 80, 80)
                } else {
                    ui.visuals().text_color()
                };

                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(&self.body).color(color));
                    });
            });

        self.open = open;
    }
}

pub struct GithubPublishConfigWindow {
    open: bool,
    repo: String,
    base_branch: String,
    commit_message: String,
    pr_title: String,
    base_dir: String,
    collections: Vec<crate::plugin::builtin::github_publish::CollectionConfig>,
    show_hint: bool,
}

impl GithubPublishConfigWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            repo: String::new(),
            base_branch: String::new(),
            commit_message: String::new(),
            pr_title: String::new(),
            base_dir: String::new(),
            collections: Vec::new(),
            show_hint: false,
        }
    }

    pub fn open(
        &mut self,
        config: &crate::plugin::builtin::github_publish::GithubPublishConfig,
        show_hint: bool,
    ) {
        self.repo = config.repo.clone();
        self.base_branch = config.base_branch.clone();
        self.commit_message = config.commit_message.clone();
        self.pr_title = config.pr_title.clone();
        self.base_dir = config.base_dir.clone();
        self.collections = config.collections.clone();
        self.open = true;
        self.show_hint = show_hint;
    }

    pub fn show(
        &mut self,
        ctx: &Context,
    ) -> Option<crate::plugin::builtin::github_publish::GithubPublishConfig> {
        if !self.open {
            return None;
        }

        let mut open = self.open;
        let mut saved = None;

        egui::Window::new("配置 GitHub 发布")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                if self.show_hint {
                    ui.label(
                        RichText::new(
                            "请先配置博客仓库地址（格式：owner/repo，例如 RetricSu/blog）",
                        )
                        .color(Color32::YELLOW),
                    );
                    ui.add_space(8.0);
                }

                ui.label("仓库地址 (owner/repo)");
                ui.add(egui::TextEdit::singleline(&mut self.repo));
                ui.add_space(8.0);

                ui.label("目标分支");
                ui.add(egui::TextEdit::singleline(&mut self.base_branch));
                ui.add_space(8.0);

                ui.label("提交信息模板 ({filename} 会被替换)");
                ui.add(egui::TextEdit::singleline(&mut self.commit_message));
                ui.add_space(8.0);

                ui.label("PR 标题模板 ({filename} 会被替换)");
                ui.add(egui::TextEdit::singleline(&mut self.pr_title));
                ui.add_space(8.0);

                ui.label("默认目录（无分类时使用，空 = 仓库根目录）");
                ui.add(egui::TextEdit::singleline(&mut self.base_dir));
                ui.add_space(8.0);

                ui.label("分类目录（可选）");
                ui.add_space(4.0);
                let mut remove_idx = None;
                for (i, entry) in self.collections.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut entry.label);
                        ui.text_edit_singleline(&mut entry.dir);
                        if ui.button("✕").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(i) = remove_idx {
                    self.collections.remove(i);
                }
                if ui.button("+ 添加分类").clicked() {
                    self.collections.push(
                        crate::plugin::builtin::github_publish::CollectionConfig {
                            label: String::new(),
                            dir: String::new(),
                        },
                    );
                }
                ui.add_space(8.0);

                ui.separator();

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.repo.trim().is_empty(),
                            egui::Button::new("保存"),
                        )
                        .clicked()
                    {
                        saved = Some(
                            crate::plugin::builtin::github_publish::GithubPublishConfig {
                                repo: self.repo.clone(),
                                base_branch: self.base_branch.clone(),
                                commit_message: self.commit_message.clone(),
                                pr_title: self.pr_title.clone(),
                                base_dir: self.base_dir.clone(),
                                collections: self.collections.clone(),
                            },
                        );
                        self.open = false;
                    }

                    if ui.button("取消").clicked() {
                        self.open = false;
                    }
                });
            });

        self.open = self.open && open;
        saved
    }
}

pub struct PublishParams {
    pub title: String,
    pub description: Option<String>,
    pub collection_dir: String,
}

pub struct PrintParams {
    pub printer: Option<String>,
    pub margin_points: u16,
}

pub struct PrintDialog {
    open: bool,
    document_name: String,
    preview: String,
    printers: Vec<String>,
    selected_printer: usize,
    margin_points: u16,
    viewport_id: egui::ViewportId,
}

impl PrintDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            document_name: String::new(),
            preview: String::new(),
            printers: Vec::new(),
            selected_printer: 0,
            margin_points: 72,
            viewport_id: egui::ViewportId::from_hash_of("print_preview_window"),
        }
    }

    pub fn open(&mut self, document_name: String, preview: String) {
        self.document_name = document_name;
        self.preview = preview;
        self.printers = crate::plugin::builtin::print::available_printers();
        self.selected_printer = 0;
        self.margin_points = 72;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &Context) -> Option<PrintParams> {
        if !self.open {
            return None;
        }

        let mut submitted = None;

        ctx.show_viewport_immediate(
            self.viewport_id,
            egui::ViewportBuilder::default()
                .with_title("打印预览")
                .with_inner_size(Vec2::new(620.0, 640.0))
                .with_min_inner_size(Vec2::new(520.0, 480.0))
                .with_resizable(true),
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("打印机");
                        egui::ComboBox::from_id_salt("print_printer")
                            .selected_text(self.selected_printer_label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_printer, 0, "默认打印机");
                                for (index, printer) in self.printers.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_printer,
                                        index + 1,
                                        printer,
                                    );
                                }
                            });

                        ui.add_space(12.0);
                        ui.label("边距");
                        egui::ComboBox::from_id_salt("print_margin")
                            .selected_text(format!("{} pt", self.margin_points))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.margin_points, 54, "54 pt");
                                ui.selectable_value(&mut self.margin_points, 72, "72 pt");
                                ui.selectable_value(&mut self.margin_points, 90, "90 pt");
                            });
                    });

                    if self.printers.is_empty() {
                        ui.label(RichText::new("未读取到打印机，将使用系统默认打印机").small());
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    let margin = self.margin_points as f32 / 3.0;
                    egui::Frame::new()
                        .fill(Color32::from_rgb(248, 247, 244))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(205, 202, 194)))
                        .inner_margin(egui::Margin::same(margin as i8))
                        .show(ui, |ui| {
                            ui.set_min_height(420.0);
                            ui.label(RichText::new(&self.document_name).strong());
                            ui.add_space(8.0);
                            egui::ScrollArea::vertical()
                                .max_height(460.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&self.preview).monospace());
                                });
                        });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("打印").clicked() {
                            submitted = Some(PrintParams {
                                printer: self.selected_printer_name(),
                                margin_points: self.margin_points,
                            });
                            self.open = false;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        if ui.button("取消").clicked() {
                            self.open = false;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
            },
        );

        submitted
    }

    fn selected_printer_label(&self) -> &str {
        if self.selected_printer == 0 {
            "默认打印机"
        } else {
            self.printers
                .get(self.selected_printer - 1)
                .map(String::as_str)
                .unwrap_or("默认打印机")
        }
    }

    fn selected_printer_name(&self) -> Option<String> {
        if self.selected_printer == 0 {
            None
        } else {
            self.printers.get(self.selected_printer - 1).cloned()
        }
    }
}

pub struct PublishDialog {
    open: bool,
    title: String,
    description: String,
    collection: usize,
    options: Vec<(String, String)>,
}

impl PublishDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            title: String::new(),
            description: String::new(),
            collection: 0,
            options: Vec::new(),
        }
    }

    pub fn open(
        &mut self,
        config: &crate::plugin::builtin::github_publish::GithubPublishConfig,
    ) {
        self.title.clear();
        self.description.clear();
        self.collection = 0;
        self.options = if config.collections.is_empty() {
            vec![("默认".to_string(), config.base_dir.clone())]
        } else {
            config
                .collections
                .iter()
                .map(|c| (c.label.clone(), c.dir.clone()))
                .collect()
        };
        self.open = true;
    }

    pub fn show(&mut self, ctx: &Context) -> Option<PublishParams> {
        if !self.open {
            return None;
        }

        let mut open = self.open;
        let mut published = None;

        egui::Window::new("发布到博客")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label("标题（必填）");
                ui.add(egui::TextEdit::singleline(&mut self.title));
                ui.add_space(8.0);

                ui.label("描述（可选）");
                ui.add(egui::TextEdit::singleline(&mut self.description));
                ui.add_space(8.0);

                ui.label("分类");
                egui::ComboBox::from_label("")
                    .selected_text(self.options[self.collection].0.as_str())
                    .show_ui(ui, |ui| {
                        for (index, (label, _)) in self.options.iter().enumerate() {
                            ui.selectable_value(&mut self.collection, index, label.as_str());
                        }
                    });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.title.trim().is_empty(),
                            egui::Button::new("发布"),
                        )
                        .clicked()
                    {
                        published = Some(PublishParams {
                            title: self.title.trim().to_string(),
                            description: if self.description.trim().is_empty() {
                                None
                            } else {
                                Some(self.description.trim().to_string())
                            },
                            collection_dir: self.options[self.collection].1.clone(),
                        });
                        self.open = false;
                    }

                    if ui.button("取消").clicked() {
                        self.open = false;
                    }
                });
            });

        self.open = self.open && open;
        published
    }
}
