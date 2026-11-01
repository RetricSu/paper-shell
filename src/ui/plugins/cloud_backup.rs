//! Interactive cloud-backup window.
//!
//! Every background operation is bound to an operation id, document id, and
//! target URL. Responses that no longer match the active operation are ignored.

use crate::plugin::builtin::cloud_backup::{
    BackupPayload, CloudBackupConfig, DocumentIdentity, download_backup, resolve_target_url,
    test_connection, upload_backup,
};
use egui::{Color32, Context, RichText};
use std::sync::mpsc::{Receiver, Sender, channel};

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

    pub fn open(&mut self, config: &CloudBackupConfig) {
        self.config = config.clone();
        self.open = true;
        if self.active_operation.is_none() {
            self.status_message = config
                .last_backup_status
                .as_ref()
                .map(|status| (false, status.clone()));
        }
    }

    pub fn set_status(&mut self, is_error: bool, message: String) {
        self.status_message = Some((is_error, message));
    }

    fn sync_document(&mut self, identity: &DocumentIdentity) {
        if self.current_doc_id.as_deref() == Some(identity.doc_id.as_str()) {
            return;
        }

        let invalidated = self.active_operation.is_some() || self.restored.is_some();
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
                    }
                    Err(error) => self.set_status(true, error),
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

    pub fn show(
        &mut self,
        ctx: &Context,
        identity: &DocumentIdentity,
        current_content: &str,
    ) -> Option<CloudBackupAction> {
        self.sync_document(identity);
        self.process_responses();

        if !self.open {
            return None;
        }

        let mut open = self.open;
        let mut action = None;

        egui::Window::new(format!(
            "{} 云端备份与同步",
            egui_phosphor::regular::CLOUD
        ))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("当前文档：");
                ui.label(RichText::new(&identity.filename).strong());
                ui.label(format!("({} 字符)", current_content.chars().count()));
            });
            ui.label(RichText::new(format!("文档标识：{}", identity.doc_id)).small().weak());
            ui.add_space(8.0);

            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("自建云端配置").strong());
                    ui.label(
                        RichText::new("(支持 WebDAV / Nextcloud / Alist / NAS / HTTP)").small(),
                    );
                });
                ui.add_space(4.0);

                ui.label("云端服务目录 URL");
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.server_url)
                        .hint_text("例如: https://dav.example.com/papershell/"),
                );
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
                            egui::TextEdit::singleline(&mut self.config.password)
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
                            egui::TextEdit::singleline(&mut self.config.encryption_password)
                                .password(!self.show_enc_password)
                                .hint_text("仅保留在本次会话"),
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
                            "加密密码不明文写入配置文件，重启后需重新输入。请牢记密码，丢失无法解密。",
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
                    self.set_status(false, "云端备份配置已保存".to_string());
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
                    .stroke(egui::Stroke::new(1.0, color))
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
                    egui::ScrollArea::vertical().max_height(90.0).show(ui, |ui| {
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
                            RichText::new("恢复前会先将当前正文保存到本机恢复快照，避免不可撤销的丢失。")
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
        });

        self.open = self.open && open;
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
    fn reopening_keeps_busy_state_and_stale_results_are_ignored() {
        let mut dialog = CloudBackupDialog::new();
        let doc = identity("doc-a");
        dialog.sync_document(&doc);
        let first_id = dialog.begin_operation(&doc, "https://example/a".into(), "running");

        dialog.open(&CloudBackupConfig::default());
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
