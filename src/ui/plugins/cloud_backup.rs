//! Interactive cloud-backup window.
//!
//! Every background operation is bound to an operation id, document id, and
//! target URL. Responses that no longer match the active operation are ignored.

use crate::plugin::builtin::cloud_backup::{
    BackupPayload, CloudBackupConfig, DocumentIdentity, download_backup, insecure_http_endpoint,
    resolve_target_url, test_connection, upload_backup,
};
use egui::{Color32, Context, RichText, Ui, Vec2};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackupStatus {
    #[default]
    Idle,
    Running,
    Succeeded,
}

pub enum CloudBackupAction {
    SaveConfig(CloudBackupConfig),
    ReplaceContent {
        content: String,
        expected_doc_id: String,
        source_name: String,
    },
    OpenNewDocument {
        filename: String,
        content: String,
    },
}

enum CloudOpResult {
    Test(Result<String, String>),
    Upload(Result<String, String>, String),
    Download(Result<BackupPayload, String>),
}

struct OperationResponse {
    operation_id: u64,
    doc_id: String,
    target_url: String,
    result: CloudOpResult,
}

struct ActiveOperation {
    operation_id: u64,
    doc_id: String,
    target_url: String,
    description: &'static str,
}

struct RestoredBackup {
    requested_doc_id: String,
    target_url: String,
    payload: BackupPayload,
}

pub struct CloudBackupDialog {
    open: bool,
    inline_upload: bool,
    success_until: Option<Instant>,
    viewport_id: egui::ViewportId,
    config: CloudBackupConfig,
    show_password: bool,
    show_enc_password: bool,
    current_doc_id: Option<String>,
    active_operation: Option<ActiveOperation>,
    next_operation_id: u64,
    status_message: Option<(bool, String)>,
    restored: Option<RestoredBackup>,
    tx: Sender<OperationResponse>,
    rx: Receiver<OperationResponse>,
}

impl Default for CloudBackupDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudBackupDialog {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            open: false,
            inline_upload: false,
            success_until: None,
            viewport_id: egui::ViewportId::from_hash_of("cloud_backup_window"),
            config: CloudBackupConfig::default(),
            show_password: false,
            show_enc_password: false,
            current_doc_id: None,
            active_operation: None,
            next_operation_id: 0,
            status_message: None,
            restored: None,
            tx,
            rx,
        }
    }

    pub fn backup_status(&self) -> BackupStatus {
        if self.inline_upload && self.active_operation.is_some() {
            BackupStatus::Running
        } else if self
            .success_until
            .is_some_and(|until| Instant::now() < until)
        {
            BackupStatus::Succeeded
        } else {
            BackupStatus::Idle
        }
    }

    pub fn backup_now(
        &mut self,
        ctx: &Context,
        config: &CloudBackupConfig,
        identity: &DocumentIdentity,
        content: &str,
    ) {
        self.sync_document(identity);
        if self.active_operation.is_some() {
            return;
        }
        self.config = config.clone();
        self.success_until = None;
        if let Some(error) = &config.credential_error {
            self.open(ctx, config);
            self.set_status(true, error.clone());
            return;
        }
        if self.config.server_url.trim().is_empty() {
            self.open(ctx, config);
            self.set_status(true, "请先配置云端地址并保存，再执行备份".into());
            return;
        }
        self.inline_upload = true;
        self.spawn_upload(identity, content);
        if self.active_operation.is_none() {
            self.inline_upload = false;
            self.open = true;
        }
        ctx.request_repaint();
    }

    pub fn viewport_id(&self) -> egui::ViewportId {
        self.viewport_id
    }

    pub fn open(&mut self, ctx: &Context, config: &CloudBackupConfig) {
        self.config = config.clone();
        if self.open {
            ctx.send_viewport_cmd_to(self.viewport_id, egui::ViewportCommand::Focus);
        } else {
            self.open = true;
        }
        if self.active_operation.is_none() {
            self.status_message = config
                .credential_error
                .as_ref()
                .map(|error| (true, error.clone()))
                .or_else(|| {
                    config
                        .last_backup_status
                        .as_ref()
                        .map(|status| (false, status.clone()))
                });
        }
        ctx.request_repaint();
    }

    pub fn set_status(&mut self, is_error: bool, message: String) {
        self.status_message = Some((is_error, message));
    }

    fn sync_document(&mut self, identity: &DocumentIdentity) {
        if self.current_doc_id.as_deref() == Some(identity.doc_id.as_str()) {
            return;
        }

        let invalidated = self.active_operation.is_some() || self.restored.is_some();
        self.inline_upload = false;
        self.success_until = None;
        self.current_doc_id = Some(identity.doc_id.clone());
        self.active_operation = None;
        self.restored = None;
        if invalidated {
            self.status_message = Some((
                true,
                "当前文档已切换，上一文档的云端操作结果将被忽略".to_string(),
            ));
        }
    }

    fn begin_operation(
        &mut self,
        identity: &DocumentIdentity,
        target_url: String,
        description: &'static str,
    ) -> u64 {
        self.next_operation_id = self.next_operation_id.wrapping_add(1);
        let operation_id = self.next_operation_id;
        self.active_operation = Some(ActiveOperation {
            operation_id,
            doc_id: identity.doc_id.clone(),
            target_url,
            description,
        });
        self.status_message = None;
        operation_id
    }

    fn process_responses(&mut self) {
        while let Ok(response) = self.rx.try_recv() {
            let matches_active = self.active_operation.as_ref().is_some_and(|active| {
                active.operation_id == response.operation_id
                    && active.doc_id == response.doc_id
                    && active.target_url == response.target_url
            }) && self.current_doc_id.as_deref()
                == Some(response.doc_id.as_str());

            if !matches_active {
                continue;
            }
            self.active_operation = None;

            match response.result {
                CloudOpResult::Test(result) => match result {
                    Ok(message) => self.set_status(false, message),
                    Err(error) => self.set_status(true, error),
                },
                CloudOpResult::Upload(result, timestamp) => match result {
                    Ok(message) => {
                        self.config.last_backup_time = Some(timestamp);
                        self.config.last_backup_status = Some(message.clone());
                        self.set_status(false, message);
                        if self.inline_upload {
                            self.success_until = Some(Instant::now() + Duration::from_secs(2));
                        }
                        self.inline_upload = false;
                    }
                    Err(error) => {
                        if self.inline_upload {
                            self.open = true;
                        }
                        self.inline_upload = false;
                        self.set_status(true, error);
                    }
                },
                CloudOpResult::Download(result) => match result {
                    Ok(payload) if payload.doc_id != response.doc_id => {
                        self.set_status(
                            true,
                            format!(
                                "云端备份属于其他文档（{}），已拒绝载入到当前文档",
                                payload.doc_id
                            ),
                        );
                    }
                    Ok(payload) => {
                        self.set_status(
                            false,
                            format!(
                                "成功从云端获取备份：\n文档：{}\n备份时间：{}\n字数：{} 字",
                                payload.filename, payload.timestamp, payload.word_count
                            ),
                        );
                        self.restored = Some(RestoredBackup {
                            requested_doc_id: response.doc_id,
                            target_url: response.target_url,
                            payload,
                        });
                    }
                    Err(error) => self.set_status(true, error),
                },
            }
        }
    }

    fn spawn_test(&mut self, identity: &DocumentIdentity) {
        let target_url = self.config.server_url.trim().to_string();
        let operation_id = self.begin_operation(identity, target_url, "正在测试连接…");
        let config = self.config.clone();
        let tx = self.tx.clone();
        let doc_id = identity.doc_id.clone();
        std::thread::spawn(move || {
            let result = test_connection(&config).map_err(|error| error.to_string());
            let _ = tx.send(OperationResponse {
                operation_id,
                doc_id,
                target_url: config.server_url.trim().to_string(),
                result: CloudOpResult::Test(result),
            });
        });
    }

    fn spawn_upload(&mut self, identity: &DocumentIdentity, content: &str) {
        let target_url = match resolve_target_url(
            &self.config.server_url,
            &self.config.remote_filename,
            &identity.stem,
            &identity.doc_id,
            &identity.filename,
        ) {
            Ok(url) => url,
            Err(error) => {
                self.set_status(true, error.to_string());
                return;
            }
        };
        let operation_id = self.begin_operation(identity, target_url.clone(), "正在备份到云端…");
        let config = self.config.clone();
        let tx = self.tx.clone();
        let identity = identity.clone();
        let content = content.to_string();
        std::thread::spawn(move || {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let result = upload_backup(&config, &target_url, &identity, &content)
                .map_err(|error| error.to_string());
            let _ = tx.send(OperationResponse {
                operation_id,
                doc_id: identity.doc_id.clone(),
                target_url,
                result: CloudOpResult::Upload(result, timestamp),
            });
        });
    }

    fn spawn_download(&mut self, identity: &DocumentIdentity) {
        let target_url = match resolve_target_url(
            &self.config.server_url,
            &self.config.remote_filename,
            &identity.stem,
            &identity.doc_id,
            &identity.filename,
        ) {
            Ok(url) => url,
            Err(error) => {
                self.set_status(true, error.to_string());
                return;
            }
        };
        let operation_id =
            self.begin_operation(identity, target_url.clone(), "正在从云端拉取备份…");
        self.restored = None;
        let config = self.config.clone();
        let tx = self.tx.clone();
        let doc_id = identity.doc_id.clone();
        std::thread::spawn(move || {
            let result = download_backup(&config, &target_url).map_err(|error| error.to_string());
            let _ = tx.send(OperationResponse {
                operation_id,
                doc_id,
                target_url,
                result: CloudOpResult::Download(result),
            });
        });
    }

    pub fn notify_save_success(&mut self, config: &CloudBackupConfig) {
        self.config = config.clone();
        self.set_status(
            false,
            if config.credential_id.is_some() {
                "配置已保存；密码已交由系统凭据库保存，重启后自动读取（清空密码并保存可移除）"
            } else {
                "配置已保存；未记住任何密码"
            }
            .to_string(),
        );
    }

    pub fn notify_save_error(&mut self, error: String) {
        self.set_status(true, error);
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        identity: &DocumentIdentity,
        current_content: &str,
    ) -> Option<CloudBackupAction> {
        self.sync_document(identity);
        let previous_status = self.backup_status();
        self.process_responses();
        if self.backup_status() != previous_status {
            ctx.request_repaint();
        }
        if self.active_operation.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(until) = self.success_until {
            if let Some(remaining) = until.checked_duration_since(Instant::now()) {
                ctx.request_repaint_after(remaining);
            } else {
                self.success_until = None;
                ctx.request_repaint();
            }
        }

        if !self.open {
            return None;
        }

        let mut action = None;

        ctx.show_viewport_immediate(
            self.viewport_id,
            egui::ViewportBuilder::default()
                .with_title(format!(
                    "{} 云端备份与同步 · {}",
                    egui_phosphor::regular::CLOUD,
                    crate::constant::DEFAULT_WINDOW_TITLE
                ))
                .with_inner_size(Vec2::new(560.0, 700.0))
                .with_min_inner_size(Vec2::new(440.0, 420.0))
                .with_decorations(false)
                .with_transparent(true)
                .with_resizable(true),
            |ctx, _class| {
                crate::style::configure_style(ctx);
                let title_bar_panel = egui::TopBottomPanel::top("cloud_backup_title_bar")
                    .show_separator_line(false)
                    .show(ctx, |ui| self.show_title_bar(ui));
                crate::ui::title_bar::TitleBar::paint_bottom_border(
                    ctx,
                    title_bar_panel.response.rect,
                );
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(16))
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                action = self.show_content(ui, identity, current_content);
                            });
                    });

                let close_requested = ctx.input(|input| input.viewport().close_requested())
                    || ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                if close_requested {
                    self.open = false;
                }
                if !self.open {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );

        action
    }

    fn show_title_bar(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 20.0;

        let drag = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("cloud_backup_title_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let is_maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        egui::MenuBar::new().ui(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized(
                        [22.0, 20.0],
                        egui::Button::new(egui_phosphor::regular::X).frame(false),
                    )
                    .on_hover_text("关闭窗口")
                    .clicked()
                {
                    self.open = false;
                }

                let is_maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
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

                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} 云端备份与同步", egui_phosphor::regular::CLOUD))
                            .strong()
                            .color(ui.visuals().text_color().gamma_multiply(0.82)),
                    );
                });
            });
        });
    }

    fn show_content(
        &mut self,
        ui: &mut Ui,
        identity: &DocumentIdentity,
        current_content: &str,
    ) -> Option<CloudBackupAction> {
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("当前文档：");
            ui.label(RichText::new(&identity.filename).strong());
            ui.label(format!("({} 字符)", current_content.chars().count()));
        });
        ui.label(
            RichText::new(format!("文档标识：{}", identity.doc_id))
                .small()
                .weak(),
        );
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("自建云端配置").strong());
                ui.label(RichText::new("(支持 WebDAV / Nextcloud / Alist / NAS / HTTP)").small());
            });
            ui.add_space(4.0);

            ui.label("云端服务目录 URL");
            ui.add(
                egui::TextEdit::singleline(&mut self.config.server_url)
                    .hint_text("例如: https://dav.example.com/papershell/"),
            );
            if insecure_http_endpoint(&self.config.server_url) {
                ui.colored_label(Color32::from_rgb(180, 100, 40),
                    "安全风险：HTTP 会明文传输登录凭据及未加密正文。即使启用端到端加密也无法保护登录凭据，请使用 HTTPS。");
            }
            ui.label(RichText::new("点击保存配置后，密码 / Token 和加密密码保存在系统凭据库，重启后自动读取；不写入配置文件。清空后保存可移除已记住的密码。").small().weak());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("用户名 (可选)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.username)
                            .hint_text("Basic Auth 用户名"),
                    );
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("密码 / Token (可选)");
                        if ui
                            .button(if self.show_password {
                                egui_phosphor::regular::EYE_SLASH
                            } else {
                                egui_phosphor::regular::EYE
                            })
                            .on_hover_text("显示/隐藏密码")
                            .clicked()
                        {
                            self.show_password = !self.show_password;
                        }
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut *self.config.password)
                            .password(!self.show_password)
                            .hint_text("密码或 Bearer Token"),
                    );
                });
            });
            ui.add_space(4.0);

            ui.label("远程文件名模板（建议保留 {doc_id} 防止同名文档冲突）");
            ui.add(
                egui::TextEdit::singleline(&mut self.config.remote_filename)
                    .hint_text("{stem}_{doc_id}.backup"),
            );
            ui.label(RichText::new("同一文档备份到同一远程文件，后续备份覆盖上一份，不自动保留历史版本。").small().weak());
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("安全与加密").strong());
                ui.checkbox(
                    &mut self.config.enable_encryption,
                    "启用端到端加密 (AES-256-GCM)",
                );
            });

            if self.config.enable_encryption {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("加密密码：");
                    ui.add(
                        egui::TextEdit::singleline(&mut *self.config.encryption_password)
                            .password(!self.show_enc_password)
                            .hint_text("输入原密码，保存后自动记住"),
                    );
                    if ui
                        .button(if self.show_enc_password {
                            egui_phosphor::regular::EYE_SLASH
                        } else {
                            egui_phosphor::regular::EYE
                        })
                        .on_hover_text("显示/隐藏加密密码")
                        .clicked()
                    {
                        self.show_enc_password = !self.show_enc_password;
                    }
                });
                ui.label(
                    RichText::new(
                        "保存配置后由系统凭据库记住。已有备份请使用原密码，并另行妥善保管，以便换机或恢复。",
                    )
                    .small()
                    .color(Color32::from_rgb(180, 140, 60)),
                );
            }
        });

        ui.add_space(10.0);
        let is_busy = self.active_operation.is_some();
        let has_url = !self.config.server_url.trim().is_empty();

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    has_url && !is_busy,
                    egui::Button::new(format!("{} 测试连接", egui_phosphor::regular::PLUG)),
                )
                .clicked()
            {
                self.spawn_test(identity);
            }

            if ui
                .add_enabled(
                    has_url && !is_busy,
                    egui::Button::new(format!(
                        "{} 立即备份到云端",
                        egui_phosphor::regular::CLOUD_ARROW_UP
                    )),
                )
                .clicked()
            {
                self.spawn_upload(identity, current_content);
            }

            if ui
                .add_enabled(
                    has_url && !is_busy,
                    egui::Button::new(format!(
                        "{} 从云端同步拉取",
                        egui_phosphor::regular::CLOUD_ARROW_DOWN
                    )),
                )
                .clicked()
            {
                self.spawn_download(identity);
            }

            if ui
                .add_enabled(
                    !is_busy,
                    egui::Button::new(format!("{} 保存配置", egui_phosphor::regular::CHECK)),
                )
                .clicked()
            {
                action = Some(CloudBackupAction::SaveConfig(self.config.clone()));
            }
        });

        if let Some(active) = &self.active_operation {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(active.description);
            });
        }

        if let Some((is_error, message)) = &self.status_message {
            ui.add_space(8.0);
            let (icon, color) = if *is_error {
                (
                    egui_phosphor::regular::WARNING_CIRCLE,
                    Color32::from_rgb(210, 80, 80),
                )
            } else {
                (
                    egui_phosphor::regular::CHECK_CIRCLE,
                    Color32::from_rgb(60, 160, 90),
                )
            };
            egui::Frame::new()
                .fill(if *is_error {
                    Color32::from_rgba_premultiplied(220, 50, 50, 18)
                } else {
                    Color32::from_rgba_premultiplied(50, 180, 80, 18)
                })
                .stroke(egui::Stroke::new(1.0_f32, color))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(icon).color(color));
                        ui.label(message);
                    });
                });
        }

        let mut restore_action = None;
        let mut discard_restored = false;
        if let Some(restored) = &self.restored {
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(RichText::new("云端内容准备就绪").strong());
                ui.label(format!(
                    "文档：{} · {} 字 · 来源：{}",
                    restored.payload.filename, restored.payload.word_count, restored.target_url
                ));
                let preview: String = restored.payload.content.chars().take(160).collect();
                egui::ScrollArea::vertical()
                    .max_height(90.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(preview).weak());
                    });
                ui.add_space(6.0);

                if restored.requested_doc_id != identity.doc_id {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        "当前文档已变化，不能应用这份备份",
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("备份当前内容并替换").clicked() {
                            restore_action = Some(CloudBackupAction::ReplaceContent {
                                content: restored.payload.content.clone(),
                                expected_doc_id: identity.doc_id.clone(),
                                source_name: restored.payload.filename.clone(),
                            });
                        }
                        if ui.button("作为新文档打开").clicked() {
                            restore_action = Some(CloudBackupAction::OpenNewDocument {
                                filename: restored.payload.filename.clone(),
                                content: restored.payload.content.clone(),
                            });
                        }
                    });
                    ui.label(
                        RichText::new(
                            "恢复前会先将当前正文保存到本机恢复快照，避免不可撤销的丢失。",
                        )
                        .small(),
                    );
                }

                if ui.button("放弃本次同步").clicked() {
                    discard_restored = true;
                }
            });
        }

        if restore_action.is_some() || discard_restored {
            self.restored = None;
        }
        if restore_action.is_some() {
            action = restore_action;
        }

        ui.add_space(10.0);
        ui.separator();
        if ui.button("关闭").clicked() {
            self.open = false;
        }

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: &str) -> DocumentIdentity {
        DocumentIdentity {
            filename: format!("{id}.md"),
            stem: id.to_string(),
            doc_id: id.to_string(),
            source_path: Some(format!("/tmp/{id}.md")),
        }
    }

    #[test]
    fn unavailable_credentials_open_settings_and_save_refreshes_reference() {
        let ctx = Context::default();
        let mut dialog = CloudBackupDialog::new();
        let mut config = CloudBackupConfig {
            server_url: "https://example.com/dav/".into(),
            credential_error: Some("请解锁系统凭据库".into()),
            ..Default::default()
        };
        dialog.backup_now(&ctx, &config, &identity("doc-a"), "text");
        assert!(dialog.open);
        assert!(dialog.active_operation.is_none());
        assert_eq!(
            dialog.status_message,
            Some((true, "请解锁系统凭据库".into()))
        );
        config.credential_id = Some("new-reference".into());
        config.credential_error = None;
        dialog.notify_save_success(&config);
        assert_eq!(
            dialog.config.credential_id.as_deref(),
            Some("new-reference")
        );
        assert!(dialog.config.credential_error.is_none());
    }

    #[test]
    fn inline_backup_completion_is_processed_while_closed_and_expires() {
        let ctx = Context::default();
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        dialog.sync_document(&doc);
        dialog.inline_upload = true;
        let operation_id = dialog.begin_operation(&doc, "https://example/a".into(), "upload");
        assert_eq!(dialog.backup_status(), BackupStatus::Running);

        // Repeated clicks cannot replace the in-flight upload.
        dialog.backup_now(&ctx, &CloudBackupConfig::default(), &doc, "new content");
        assert_eq!(
            dialog.active_operation.as_ref().unwrap().operation_id,
            operation_id
        );
        dialog
            .tx
            .send(OperationResponse {
                operation_id,
                doc_id: doc.doc_id.clone(),
                target_url: "https://example/a".into(),
                result: CloudOpResult::Upload(Ok("uploaded".into()), "now".into()),
            })
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!(dialog.show(ctx, &doc, "content").is_none());
        });
        assert_eq!(dialog.backup_status(), BackupStatus::Succeeded);
        assert_eq!(dialog.config.last_backup_time.as_deref(), Some("now"));
        assert_eq!(dialog.status_message, Some((false, "uploaded".into())));

        dialog.success_until = Some(Instant::now() - Duration::from_millis(1));
        assert_eq!(dialog.backup_status(), BackupStatus::Idle);
    }

    #[test]
    fn inline_backup_failure_shows_actionable_error() {
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        dialog.sync_document(&doc);
        dialog.inline_upload = true;
        let operation_id = dialog.begin_operation(&doc, "https://example/a".into(), "upload");
        dialog
            .tx
            .send(OperationResponse {
                operation_id,
                doc_id: doc.doc_id.clone(),
                target_url: "https://example/a".into(),
                result: CloudOpResult::Upload(Err("服务器拒绝访问".into()), "now".into()),
            })
            .unwrap();
        dialog.process_responses();
        assert!(dialog.open);
        assert_eq!(dialog.backup_status(), BackupStatus::Idle);
        assert_eq!(dialog.status_message, Some((true, "服务器拒绝访问".into())));
    }

    #[test]
    fn inline_backup_requires_configuration_and_resets_on_document_switch() {
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        dialog.backup_now(
            &Context::default(),
            &CloudBackupConfig::default(),
            &doc,
            "content",
        );
        assert!(dialog.open);
        assert!(
            dialog
                .status_message
                .as_ref()
                .unwrap()
                .1
                .contains("请先配置")
        );

        dialog.inline_upload = true;
        let operation_id = dialog.begin_operation(&doc, "https://example/a".into(), "upload");
        dialog.sync_document(&identity("doc-b"));
        dialog
            .tx
            .send(OperationResponse {
                operation_id,
                doc_id: doc.doc_id.clone(),
                target_url: "https://example/a".into(),
                result: CloudOpResult::Upload(Ok("stale upload".into()), "now".into()),
            })
            .unwrap();
        dialog.process_responses();
        assert_eq!(dialog.backup_status(), BackupStatus::Idle);
        assert!(
            dialog
                .status_message
                .as_ref()
                .unwrap()
                .1
                .contains("当前文档已切换")
        );
    }

    #[test]
    fn cloud_backup_uses_a_standalone_closable_viewport() {
        let ctx = Context::default();
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        assert_ne!(dialog.viewport_id(), egui::ViewportId::ROOT);

        dialog.open(&ctx, &CloudBackupConfig::default());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(560.0, 700.0),
            )),
            ..Default::default()
        };
        let output = ctx.run(input.clone(), |ctx| {
            assert!(dialog.show(ctx, &doc, "content").is_none());
        });
        assert!(output.shapes.iter().any(|clipped| {
            matches!(
                &clipped.shape,
                egui::Shape::Text(text) if text.galley.text().contains("云端备份与同步")
            )
        }));

        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..input
            },
            |ctx| {
                assert!(dialog.show(ctx, &doc, "content").is_none());
            },
        );
        assert!(!dialog.open);
    }

    #[test]
    fn reopening_keeps_busy_state_and_stale_results_are_ignored() {
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        dialog.sync_document(&doc);
        let first_id = dialog.begin_operation(&doc, "https://example/a".into(), "running");

        dialog.open(&Context::default(), &CloudBackupConfig::default());
        assert!(dialog.active_operation.is_some());

        let second_id = dialog.begin_operation(&doc, "https://example/b".into(), "running");
        dialog
            .tx
            .send(OperationResponse {
                operation_id: first_id,
                doc_id: doc.doc_id.clone(),
                target_url: "https://example/a".into(),
                result: CloudOpResult::Test(Ok("stale".into())),
            })
            .unwrap();
        dialog.process_responses();
        assert_eq!(
            dialog.active_operation.as_ref().map(|op| op.operation_id),
            Some(second_id)
        );

        dialog
            .tx
            .send(OperationResponse {
                operation_id: second_id,
                doc_id: doc.doc_id.clone(),
                target_url: "https://example/b".into(),
                result: CloudOpResult::Test(Ok("current".into())),
            })
            .unwrap();
        dialog.process_responses();
        assert!(dialog.active_operation.is_none());
        assert_eq!(dialog.status_message, Some((false, "current".to_string())));
    }

    #[test]
    fn switching_document_discards_late_download() {
        let mut dialog = CloudBackupDialog::new();
        let doc_a = identity("doc-a");
        let doc_b = identity("doc-b");
        dialog.sync_document(&doc_a);
        let operation_id = dialog.begin_operation(&doc_a, "https://example/a".into(), "running");

        dialog.sync_document(&doc_b);
        dialog
            .tx
            .send(OperationResponse {
                operation_id,
                doc_id: doc_a.doc_id.clone(),
                target_url: "https://example/a".into(),
                result: CloudOpResult::Download(Ok(BackupPayload {
                    version: 1,
                    doc_id: doc_a.doc_id,
                    source_path: None,
                    filename: "a.md".into(),
                    content: "old content".into(),
                    timestamp: "now".into(),
                    word_count: 2,
                })),
            })
            .unwrap();
        dialog.process_responses();
        assert!(dialog.restored.is_none());
        assert!(dialog.active_operation.is_none());
    }
}
