use crate::backend::editor_backend::EditorBackend;
use crate::statistics::{
    StatisticsGrouping, StatisticsPeriod, StatisticsReport, WritingActivity, WritingStatistics,
};
use crate::ui::dialog::{ModalConfig, modal_button, show_modal};
use chrono::{Local, NaiveDate};
use egui::{Color32, Context, RichText, Ui};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatisticsAction {
    OpenFile(PathBuf),
    RevealInFolder(PathBuf),
    DeleteDocument {
        doc_id: String,
        delete_local_file: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeleteDocument {
    pub doc_id: String,
    pub doc_name: String,
    pub file_path: Option<PathBuf>,
    pub delete_local_file: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum StatisticsView {
    #[default]
    Overview,
    Time,
    Documents,
}

#[derive(Default)]
pub struct StatisticsWindow {
    open: bool,
    refresh_requested: bool,
    receiver: Option<Receiver<Result<WritingStatistics, String>>>,
    data: Option<WritingStatistics>,
    error: Option<String>,
    period: StatisticsPeriod,
    grouping: StatisticsGrouping,
    view: StatisticsView,
    document_query: String,
    selected_doc_id: Option<String>,
    pending_delete: Option<PendingDeleteDocument>,
    pending_action: Option<StatisticsAction>,
    status_message: Option<String>,
}

impl StatisticsWindow {
    pub fn open(&mut self, ctx: &Context) {
        if self.open {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of("writing_statistics_window"),
                egui::ViewportCommand::Focus,
            );
        }
        self.open = true;
        self.pending_delete = None;
        self.invalidate();
    }

    pub fn invalidate(&mut self) {
        self.refresh_requested = true;
    }

    pub fn take_pending_action(&mut self) -> Option<StatisticsAction> {
        self.pending_action.take()
    }

    pub fn remove_document(&mut self, doc_id: &str) {
        if let Some(data) = &mut self.data {
            data.remove_document(doc_id);
        }
        if self.selected_doc_id.as_deref() == Some(doc_id) {
            self.selected_doc_id = None;
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn set_status_message(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn show(&mut self, ctx: &Context, backend: &Arc<EditorBackend>) {
        if !self.open {
            return;
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(data) => {
                            self.data = Some(data);
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                    self.receiver = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("统计任务意外结束，请重试".to_string());
                    self.receiver = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.refresh_requested && self.receiver.is_none() {
            let (sender, receiver) = channel();
            self.receiver = Some(receiver);
            self.refresh_requested = false;
            self.error = None;
            let backend = Arc::clone(backend);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = backend.load_writing_statistics().map_err(|e| e.to_string());
                let _ = sender.send(result);
                ctx.request_repaint();
            });
        }

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("writing_statistics_window"),
            egui::ViewportBuilder::default()
                .with_title("写作统计 · 纸壳")
                .with_inner_size([800.0, 720.0])
                .with_min_inner_size([600.0, 460.0])
                .with_resizable(true),
            |ctx, _class| {
                crate::style::configure_style(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(20))
                    .show(ctx, |ui| self.show_content(ui));
                if self.pending_delete.is_some() {
                    self.show_delete_modal(ctx);
                }
                if ctx.input(|i| i.viewport().close_requested())
                    || (self.pending_delete.is_none()
                        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
                {
                    self.open = false;
                }
            },
        );
    }

    fn show_delete_modal(&mut self, ctx: &Context) {
        let Some(pending) = &mut self.pending_delete else {
            return;
        };

        let file_exists = pending
            .file_path
            .as_ref()
            .is_some_and(|path| path.exists());

        let message = if file_exists {
            format!(
                "确定要将《{}》踢出统计吗？\n将删除该文章的历史版本记录与统计数据，该文章将不再计入总字数与写作趋势中。",
                pending.doc_name
            )
        } else {
            format!(
                "确定要将《{}》踢出统计吗？\n该文章的本地文件已不存在，踢出将永久清除其历史版本与统计记录。",
                pending.doc_name
            )
        };

        let config = ModalConfig {
            id: "statistics_confirm_delete_document",
            title: "确认踢出文章记录",
            icon: egui_phosphor::regular::TRASH,
            icon_color: Color32::from_rgb(180, 60, 60),
            message: &message,
            width: 420.0,
        };

        let mut confirmed = false;
        let mut cancelled = false;
        let mut delete_local_file = pending.delete_local_file;
        let doc_name = pending.doc_name.clone();
        let file_path_opt = pending.file_path.clone();

        let response = show_modal(ctx, config, |ui| {
            if file_exists {
                ui.checkbox(&mut delete_local_file, "同时删除磁盘上的本地文件");
                if delete_local_file {
                    if let Some(path) = &file_path_opt {
                        ui.colored_label(
                            Color32::from_rgb(200, 70, 70),
                            format!("⚠️ 警告：磁盘源文件将被永久删除！\n{}", path.display()),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new("说明：磁盘源文件将保留，仅删除历史记录与统计数据。")
                            .small()
                            .weak(),
                    );
                }
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                let delete_label = if delete_local_file {
                    format!("{} 确认踢出并删除文件", egui_phosphor::regular::TRASH)
                } else {
                    format!("{} 确认踢出统计", egui_phosphor::regular::TRASH)
                };
                if modal_button(
                    ui,
                    RichText::new(delete_label)
                        .color(Color32::from_rgb(180, 60, 60))
                        .strong(),
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

        if let Some(pending) = &mut self.pending_delete {
            pending.delete_local_file = delete_local_file;
        }

        if confirmed || response.enter_pressed {
            if let Some(pending) = self.pending_delete.take() {
                self.pending_action = Some(StatisticsAction::DeleteDocument {
                    doc_id: pending.doc_id.clone(),
                    delete_local_file: pending.delete_local_file,
                });
                self.remove_document(&pending.doc_id);
                self.status_message = Some(format!("已成功将《{}》踢出统计", doc_name));
            }
        } else if cancelled || response.escape_pressed {
            self.pending_delete = None;
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("写作统计");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(self.receiver.is_none(), egui::Button::new("刷新"))
                    .on_hover_text("重新读取所有文档的保存记录，包括其他窗口的保存")
                    .clicked()
                {
                    self.invalidate();
                    ui.ctx().request_repaint();
                }
                if self.receiver.is_some() && self.data.is_some() {
                    ui.spinner();
                    ui.label("更新中 · 显示上次结果");
                }
            });
        });
        ui.label(RichText::new("全部已记录文档 · 基于已保存版本").weak());
        if let Some(error) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("无法更新统计：{error}"),
            );
            if self.data.is_some() {
                ui.label("以下仍为上次成功读取的数据。");
            }
        }
        let Some(data) = &self.data else {
            if self.receiver.is_some() {
                ui.add_space(80.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new("正在加载写作统计…").strong());
                    ui.label(
                        RichText::new("可能需要几秒钟，完成后自动显示。")
                            .small()
                            .weak(),
                    );
                });
            } else {
                ui.add_space(24.0);
                ui.label("点击刷新重试。");
            }
            return;
        };
        ui.add_space(16.0);
        if !data.warnings.is_empty() {
            egui::CollapsingHeader::new(format!(
                "{} 条记录未能完整读取，统计可能偏低",
                data.warnings.len()
            ))
            .show(ui, |ui| {
                for warning in &data.warnings {
                    ui.label(RichText::new(warning).small());
                }
            });
        }
        if data.documents.is_empty() {
            if data.warnings.is_empty() {
                ui.label(RichText::new("还没有保存记录").strong());
                ui.label("保存第一篇文章后，即可查看总字数与写作趋势。");
            } else {
                ui.label("没有可用的保存记录，请检查上方错误后刷新重试。");
            }
            return;
        }

        let today = Local::now().date_naive();
        let lifetime = data.report(StatisticsPeriod::AllTime, StatisticsGrouping::Month, today);
        egui::Grid::new("statistics_totals")
            .num_columns(4)
            .spacing([24.0, 10.0])
            .show(ui, |ui| {
                ui.label("最新保存总字数").on_hover_text(
                    "每篇已记录文档的最新保存版本之和，不叠加历史版本。保留历史的已移除文档也在内。",
                );
                ui.strong(format!("{} 字", number(data.total_words())));
                ui.label("文档数");
                ui.strong(number(data.documents.len()));
                ui.end_row();
                ui.label("累计新增");
                ui.strong(format!("{} 字", number(lifetime.activity.added)));
                ui.label("累计删除");
                ui.strong(format!("{} 字", number(lifetime.activity.removed)));
                ui.end_row();
                ui.label("累计写作时长");
                ui.strong(duration(lifetime.activity.seconds));
                ui.label("活跃天数").on_hover_text("有新增或删除字词的日期，跨文档不重复计天");
                ui.strong(format!("{} 天", number(lifetime.active_days)));
                ui.end_row();
            });
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (view, label) in [
                (StatisticsView::Overview, "趋势总览"),
                (StatisticsView::Time, "时间明细"),
                (StatisticsView::Documents, "文档统计"),
            ] {
                ui.selectable_value(&mut self.view, view, label);
            }
        });
        ui.add_space(8.0);

        match self.view {
            StatisticsView::Overview => {
                show_overview(ui, data, &mut self.period, &mut self.grouping, today)
            }
            StatisticsView::Time => {
                show_time_details(ui, data, &mut self.period, &mut self.grouping, today)
            }
            StatisticsView::Documents => {
                show_documents(
                    ui,
                    data,
                    &mut self.document_query,
                    &mut self.selected_doc_id,
                    &mut self.pending_delete,
                    &mut self.pending_action,
                );
            }
        }
    }
}

fn show_overview(
    ui: &mut Ui,
    data: &WritingStatistics,
    period: &mut StatisticsPeriod,
    grouping: &mut StatisticsGrouping,
    today: NaiveDate,
) {
    let report = show_report_controls(ui, data, period, grouping, today);
    egui::ScrollArea::vertical()
        .id_salt("statistics_overview")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_chart(ui, &report.buckets, *grouping);
            if !report.activity.is_active() {
                ui.label(RichText::new("这段时间还没有已保存的字数变化。").weak());
            }
            ui.add_space(8.0);
            egui::CollapsingHeader::new("统计口径与说明").show(ui, |ui| {
                ui.label(RichText::new("汉字每字计 1，其他连续非空白字符计 1 个词，与编辑器一致。首次保存的全文计入新增；导入、粘贴和版本恢复也计入，不代表实际键入量。保存间的改写按字词比较，未保存的改动不计入。写作时长沿用已保存的编辑器聚焦时间，按保存日期归档。").small().weak());
            });
        });
}

fn show_time_details(
    ui: &mut Ui,
    data: &WritingStatistics,
    period: &mut StatisticsPeriod,
    grouping: &mut StatisticsGrouping,
    today: NaiveDate,
) {
    let report = show_report_controls(ui, data, period, grouping, today);
    egui::ScrollArea::vertical()
        .id_salt("statistics_time_rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(
                RichText::new("按周以周一为起点，首尾仅包含所选日期范围内的记录。")
                    .small()
                    .weak(),
            );
            ui.add_space(4.0);
            egui::Grid::new("statistics_time_table")
                .num_columns(5)
                .striped(true)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    for heading in ["日期", "新增", "删除", "净增", "写作时长"] {
                        ui.strong(heading);
                    }
                    ui.end_row();
                    for (date, activity) in report.buckets.iter().rev() {
                        ui.label(bucket_label(*date, *grouping));
                        ui.label(format!("+{}", number(activity.added)));
                        ui.label(format!("−{}", number(activity.removed)));
                        ui.label(format!("{:+}", activity.net()));
                        ui.label(duration(activity.seconds));
                        ui.end_row();
                    }
                });
        });
}

fn show_report_controls(
    ui: &mut Ui,
    data: &WritingStatistics,
    period: &mut StatisticsPeriod,
    grouping: &mut StatisticsGrouping,
    today: NaiveDate,
) -> StatisticsReport {
    ui.horizontal_wrapped(|ui| {
        for (candidate, label) in [
            (StatisticsPeriod::Last7Days, "近 7 天"),
            (StatisticsPeriod::Last30Days, "近 30 天"),
            (StatisticsPeriod::ThisYear, "今年"),
            (StatisticsPeriod::AllTime, "全部"),
        ] {
            if ui
                .selectable_value(&mut *period, candidate, label)
                .clicked()
            {
                *grouping = match candidate {
                    StatisticsPeriod::Last7Days | StatisticsPeriod::Last30Days => {
                        StatisticsGrouping::Day
                    }
                    StatisticsPeriod::ThisYear | StatisticsPeriod::AllTime => {
                        StatisticsGrouping::Month
                    }
                };
            }
        }
        ui.separator();
        egui::ComboBox::from_id_salt("statistics_grouping")
            .width(80.0)
            .selected_text(grouping_label(*grouping))
            .show_ui(ui, |ui| {
                for candidate in [
                    StatisticsGrouping::Day,
                    StatisticsGrouping::Week,
                    StatisticsGrouping::Month,
                ] {
                    ui.selectable_value(&mut *grouping, candidate, grouping_label(candidate));
                }
            });
    });
    let report = data.report(*period, *grouping, today);
    ui.label(
        RichText::new(format!("{} 至 {} · 本地日期", report.start, today))
            .small()
            .weak(),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("新增 {}", number(report.activity.added)));
        ui.label(format!("删除 {}", number(report.activity.removed)));
        ui.label(format!("净增 {:+}", report.activity.net()));
        ui.label(format!("活跃 {} 天", report.active_days));
        ui.label(format!("写作 {}", duration(report.activity.seconds)));
    });
    ui.add_space(8.0);
    report
}

fn show_documents(
    ui: &mut Ui,
    data: &WritingStatistics,
    query: &mut String,
    selected_doc_id: &mut Option<String>,
    pending_delete: &mut Option<PendingDeleteDocument>,
    pending_action: &mut Option<StatisticsAction>,
) {
    let matching: Vec<usize> = data
        .documents
        .iter()
        .enumerate()
        .filter(|(_, doc)| document_matches(doc, query))
        .map(|(index, _)| index)
        .collect();

    ui.horizontal(|ui| {
        if query.trim().is_empty() {
            ui.strong(format!("{} 篇文档", number(data.documents.len())));
        } else {
            ui.strong(format!(
                "匹配 {} / {} 篇",
                number(matching.len()),
                number(data.documents.len())
            ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let width = ui.available_width().clamp(120.0, 280.0);
            ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("搜索文件名或路径")
                    .desired_width(width),
            );
        });
    });
    ui.label(
        RichText::new("点击文档可展开管理详情 · 可直接在列表右侧踢出记录不再统计")
            .small()
            .weak(),
    );
    ui.add_space(4.0);

    if matching.is_empty() {
        ui.add_space(32.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("没有匹配的文档").strong());
            ui.label(RichText::new("试试更短的文件名或路径关键词。").weak());
        });
        return;
    }

    let (name_width, count_width, action_width) = document_column_widths(ui);
    ui.horizontal(|ui| {
        aligned_label(
            ui,
            [name_width, 20.0].into(),
            RichText::new("文档（点击查看详情）").small().weak(),
            egui::Align::Min,
        );
        aligned_label(
            ui,
            [count_width, 20.0].into(),
            RichText::new("最新保存字数").small().weak(),
            egui::Align::Max,
        );
        aligned_label(
            ui,
            [action_width, 20.0].into(),
            RichText::new("操作").small().weak(),
            egui::Align::Max,
        );
    });
    ui.separator();
    let scroll_height = if selected_doc_id.is_some() {
        (ui.available_height() - 170.0).max(120.0)
    } else {
        ui.available_height()
    };
    egui::ScrollArea::vertical()
        .id_salt("statistics_documents")
        .max_height(scroll_height)
        .auto_shrink([false, false])
        .show_rows(ui, 32.0, matching.len(), |ui, rows| {
            let (name_width, count_width, action_width) = document_column_widths(ui);
            for index in rows {
                let doc = &data.documents[matching[index]];
                let (name, path) = document_labels(doc);
                let file_exists = doc.path.as_ref().is_some_and(|p| p.exists());
                let is_selected = selected_doc_id.as_deref() == Some(&doc.id);
                ui.horizontal(|ui| {
                    let (name_rect, name_resp) =
                        ui.allocate_exact_size([name_width, 32.0].into(), egui::Sense::click());
                    let mut text = RichText::new(&name);
                    if is_selected {
                        text = text.strong().underline();
                    }
                    if !file_exists {
                        text = text.weak();
                    }
                    let name_label = ui
                        .new_child(
                            egui::UiBuilder::new()
                                .max_rect(name_rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        )
                        .add(egui::Label::new(text).truncate());
                    let hover_hint = if file_exists {
                        format!("{}\n点击选择此文章查看详情", path)
                    } else {
                        format!("{}\n[本地文件已不存在]\n点击选择此文章查看详情", path)
                    };
                    if (name_resp | name_label).on_hover_text(hover_hint).clicked() {
                        *selected_doc_id = if is_selected {
                            None
                        } else {
                            Some(doc.id.clone())
                        };
                    }

                    aligned_label(
                        ui,
                        [count_width, 32.0].into(),
                        doc.words.map_or_else(
                            || "无法读取".to_string(),
                            |words| format!("{} 字", number(words)),
                        ),
                        egui::Align::Max,
                    );

                    let (action_rect, _) =
                        ui.allocate_exact_size([action_width, 32.0].into(), egui::Sense::hover());
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(action_rect)
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    );
                    child.spacing_mut().item_spacing.x = 4.0;
                    if child
                        .button(RichText::new("踢出").color(Color32::from_rgb(180, 60, 60)).small())
                        .on_hover_text("将此文章踢出统计，不再记录字数与写作趋势")
                        .clicked()
                    {
                        *pending_delete = Some(PendingDeleteDocument {
                            doc_id: doc.id.clone(),
                            doc_name: name.clone(),
                            file_path: doc.path.clone(),
                            delete_local_file: false,
                        });
                    }
                    if file_exists
                        && let Some(doc_path) = &doc.path
                        && child
                            .button(RichText::new("打开").small())
                            .on_hover_text("在编辑器中打开此文章")
                            .clicked()
                    {
                        *pending_action = Some(StatisticsAction::OpenFile(doc_path.clone()));
                    }
                });
            }
        });

    if let Some(selected_id) = selected_doc_id.as_deref()
        && let Some(doc) = data.documents.iter().find(|d| d.id == selected_id)
    {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        let (name, path) = document_labels(doc);
        let file_exists = doc.path.as_ref().is_some_and(|p| p.exists());
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("文章详情 · 《{}》", name));
                    if file_exists {
                        ui.colored_label(Color32::from_rgb(60, 140, 70), "● 本地文件存在");
                    } else {
                        ui.colored_label(Color32::from_rgb(190, 120, 40), "⚠️ 本地源文件已不存在（仅保留统计历史）");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕ 关闭详情").clicked() {
                            *selected_doc_id = None;
                        }
                    });
                });
                ui.add_space(2.0);
                ui.label(RichText::new(format!("路径：{}", path)).small().weak());
                ui.add_space(6.0);
                egui::Grid::new("statistics_doc_detail_grid")
                    .num_columns(4)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("最新保存字数");
                        ui.strong(doc.words.map_or_else(|| "无法读取".to_string(), |w| format!("{} 字", number(w))));
                        ui.label("保存版本数");
                        ui.strong(format!("{} 次", doc.version_count));
                        ui.end_row();
                        ui.label("累计新增");
                        ui.strong(format!("+{} 字", number(doc.total_activity.added)));
                        ui.label("累计删除");
                        ui.strong(format!("−{} 字", number(doc.total_activity.removed)));
                        ui.end_row();
                        ui.label("累计写作时长");
                        ui.strong(duration(doc.total_activity.seconds));
                        ui.label("最后保存时间");
                        ui.strong(doc.last_saved.map_or_else(|| "未知".to_string(), |dt| dt.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()));
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if file_exists
                        && let Some(doc_path) = &doc.path
                    {
                        if ui.button("在编辑器中打开").clicked() {
                            *pending_action = Some(StatisticsAction::OpenFile(doc_path.clone()));
                        }
                        if ui.button("在访达/资源管理器中显示").clicked() {
                            *pending_action = Some(StatisticsAction::RevealInFolder(doc_path.clone()));
                        }
                    }
                    if ui
                        .button(
                            RichText::new(format!("{} 踢出统计并删除记录", egui_phosphor::regular::TRASH))
                                .color(Color32::from_rgb(180, 60, 60)),
                        )
                        .on_hover_text("从纸壳中彻底清除该文章的记录与统计")
                        .clicked()
                    {
                        *pending_delete = Some(PendingDeleteDocument {
                            doc_id: doc.id.clone(),
                            doc_name: name.clone(),
                            file_path: doc.path.clone(),
                            delete_local_file: false,
                        });
                    }
                });
            });
    }
}

fn aligned_label(
    ui: &mut Ui,
    size: egui::Vec2,
    text: impl Into<egui::WidgetText>,
    align: egui::Align,
) -> egui::Response {
    let layout = if align == egui::Align::Max {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    let (cell_rect, cell_response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let label_response = ui
        .new_child(egui::UiBuilder::new().max_rect(cell_rect).layout(layout))
        .add(egui::Label::new(text).truncate().halign(align));
    cell_response | label_response
}

fn document_column_widths(ui: &Ui) -> (f32, f32, f32) {
    let count_width = 100.0;
    let action_width = 110.0;
    let spacing = ui.spacing().item_spacing.x * 2.0;
    let name_width = (ui.available_width() - count_width - action_width - spacing).max(80.0);
    (name_width, count_width, action_width)
}

pub fn reveal_path_in_file_manager(path: &Path) {
    if !path.exists() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,\"{}\"", path.display()))
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let _ = std::process::Command::new("xdg-open")
            .arg(target)
            .spawn();
    }
}

fn document_labels(doc: &crate::statistics::DocumentStatistics) -> (String, String) {
    let path = doc
        .path
        .as_ref()
        .map_or_else(|| doc.id.clone(), |path| path.display().to_string());
    let name = doc
        .path
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || doc.id.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
    (name, path)
}

fn document_matches(doc: &crate::statistics::DocumentStatistics, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let (name, path) = document_labels(doc);
    name.to_lowercase().contains(&query) || path.to_lowercase().contains(&query)
}

fn grouping_label(grouping: StatisticsGrouping) -> &'static str {
    match grouping {
        StatisticsGrouping::Day => "按日",
        StatisticsGrouping::Week => "按周",
        StatisticsGrouping::Month => "按月",
    }
}

fn bucket_label(date: NaiveDate, grouping: StatisticsGrouping) -> String {
    match grouping {
        StatisticsGrouping::Day => date.format("%Y-%m-%d").to_string(),
        StatisticsGrouping::Week => format!("{} 周", date.format("%Y-%m-%d")),
        StatisticsGrouping::Month => date.format("%Y-%m").to_string(),
    }
}

fn show_chart(ui: &mut Ui, buckets: &[(NaiveDate, WritingActivity)], grouping: StatisticsGrouping) {
    let added_color = Color32::from_rgb(68, 121, 81);
    let removed_color = Color32::from_rgb(156, 82, 76);
    ui.horizontal(|ui| {
        ui.colored_label(added_color, "↑ 新增");
        ui.colored_label(removed_color, "↓ 删除");
        if buckets.len() > 60 {
            ui.label(
                RichText::new("图中显示最近 60 个时间段，完整数据见时间明细")
                    .small()
                    .weak(),
            );
        }
    });
    let visible = &buckets[buckets.len().saturating_sub(60)..];
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 220.0),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(6.0, 20.0));
    let baseline = plot.center().y;
    let max = visible
        .iter()
        .map(|(_, a)| a.added.max(a.removed))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let scale = (plot.height() / 2.0 - 4.0) / max;
    let step = plot.width() / visible.len().max(1) as f32;
    ui.painter().hline(
        plot.x_range(),
        baseline,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    for (index, (date, activity)) in visible.iter().enumerate() {
        let x = plot.left() + step * index as f32;
        let width = (step * 0.65).clamp(1.0, 32.0);
        let left = x + (step - width) / 2.0;
        for (value, color, above) in [
            (activity.added, added_color, true),
            (activity.removed, removed_color, false),
        ] {
            if value > 0 {
                let height = (value as f32 * scale).max(1.0);
                let top = if above { baseline - height } else { baseline };
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height)),
                    0.0,
                    color,
                );
            }
        }
        ui.interact(
            egui::Rect::from_min_max(
                egui::pos2(x, plot.top()),
                egui::pos2(x + step, plot.bottom()),
            ),
            ui.id().with(("statistics_bar", index)),
            egui::Sense::hover(),
        )
        .on_hover_text(format!(
            "{}\n新增 +{}\n删除 −{}\n净增 {:+}\n写作 {}",
            bucket_label(*date, grouping),
            number(activity.added),
            number(activity.removed),
            activity.net(),
            duration(activity.seconds)
        ));
        if index == 0 || index == visible.len() / 2 || index + 1 == visible.len() {
            let align = if index == 0 {
                egui::Align2::LEFT_TOP
            } else if index + 1 == visible.len() {
                egui::Align2::RIGHT_TOP
            } else {
                egui::Align2::CENTER_TOP
            };
            let x = if index == 0 {
                plot.left()
            } else if index + 1 == visible.len() {
                plot.right()
            } else {
                x + step / 2.0
            };
            ui.painter().text(
                egui::pos2(x, plot.bottom() + 4.0),
                align,
                date.format(if grouping == StatisticsGrouping::Month {
                    "%Y-%m"
                } else {
                    "%m-%d"
                })
                .to_string(),
                egui::FontId::proportional(11.0),
                ui.visuals().text_color(),
            );
        }
    }
}

fn number(value: usize) -> String {
    let digits = value.to_string();
    let mut result = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

fn duration(seconds: u64) -> String {
    format!("{} 小时 {} 分", seconds / 3600, seconds % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statistics::DocumentStatistics;

    #[test]
    fn statistics_views_fit_minimum_width_with_large_totals_and_long_names() {
        for view in [
            StatisticsView::Overview,
            StatisticsView::Time,
            StatisticsView::Documents,
        ] {
            for (width, height) in [(600.0, 460.0), (800.0, 720.0)] {
                let ctx = Context::default();
                let today = Local::now().date_naive();
                let mut window = StatisticsWindow {
                    data: Some(WritingStatistics {
                        documents: vec![DocumentStatistics {
                            path: Some(format!("/tmp/{}.md", "很长的文章名称".repeat(100)).into()),
                            words: Some(1_234_567),
                            days: [(
                                today,
                                WritingActivity {
                                    added: 1_234_567,
                                    removed: 123,
                                    seconds: 360_000,
                                },
                            )]
                            .into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    view,
                    ..Default::default()
                };
                for _ in 0..2 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, height),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default()
                                .frame(egui::Frame::NONE.inner_margin(20))
                                .show(ctx, |ui| {
                                    let available = ui.available_width();
                                    window.show_content(ui);
                                    assert!(
                                        ui.min_rect().width() <= available + 1.0,
                                        "{view:?} overflow at {width}x{height}: {:?}",
                                        ui.min_rect()
                                    );
                                });
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn document_search_matches_names_and_paths_case_insensitively() {
        let doc = DocumentStatistics {
            path: Some("/tmp/Writing/Chapter One.md".into()),
            ..Default::default()
        };
        assert!(document_matches(&doc, ""));
        assert!(document_matches(&doc, "chapter"));
        assert!(document_matches(&doc, "WRITING"));
        assert!(document_matches(&doc, " one "));
        assert!(!document_matches(&doc, "missing"));
    }

    #[test]
    fn test_take_pending_action_and_remove_document() {
        let mut window = StatisticsWindow {
            data: Some(WritingStatistics {
                documents: vec![
                    DocumentStatistics {
                        id: "doc-1".to_string(),
                        words: Some(100),
                        ..Default::default()
                    },
                    DocumentStatistics {
                        id: "doc-2".to_string(),
                        words: Some(200),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            selected_doc_id: Some("doc-1".to_string()),
            ..Default::default()
        };
        assert!(window.take_pending_action().is_none());

        window.remove_document("doc-1");
        assert_eq!(window.data.as_ref().unwrap().documents.len(), 1);
        assert_eq!(window.data.as_ref().unwrap().documents[0].id, "doc-2");
        assert_eq!(window.selected_doc_id, None);

        window.pending_action = Some(StatisticsAction::DeleteDocument {
            doc_id: "doc-2".into(),
            delete_local_file: false,
        });
        assert_eq!(
            window.take_pending_action(),
            Some(StatisticsAction::DeleteDocument {
                doc_id: "doc-2".into(),
                delete_local_file: false,
            })
        );
        assert!(window.take_pending_action().is_none());
    }

    #[test]
    fn test_delete_modal_escape_cancels_and_enter_confirms() {
        let ctx = Context::default();
        let mut window = StatisticsWindow {
            data: Some(WritingStatistics {
                documents: vec![DocumentStatistics {
                    id: "doc-to-delete".to_string(),
                    path: Some("/tmp/test_article.md".into()),
                    words: Some(500),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            pending_delete: Some(PendingDeleteDocument {
                doc_id: "doc-to-delete".into(),
                doc_name: "test_article.md".into(),
                file_path: Some("/tmp/test_article.md".into()),
                delete_local_file: false,
            }),
            open: true,
            ..Default::default()
        };

        // 1. Pressing Escape cancels the pending delete modal without closing window
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                window.show_delete_modal(ctx);
            },
        );
        assert!(window.pending_delete.is_none());
        assert!(window.pending_action.is_none());

        // 2. Setting pending delete again and pressing Enter confirms
        window.pending_delete = Some(PendingDeleteDocument {
            doc_id: "doc-to-delete".into(),
            doc_name: "test_article.md".into(),
            file_path: Some("/tmp/test_article.md".into()),
            delete_local_file: false,
        });
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                window.show_delete_modal(ctx);
            },
        );
        assert!(window.pending_delete.is_none());
        assert_eq!(
            window.take_pending_action(),
            Some(StatisticsAction::DeleteDocument {
                doc_id: "doc-to-delete".into(),
                delete_local_file: false,
            })
        );
        // Document was removed from window data
        assert!(window.data.as_ref().unwrap().documents.is_empty());
        assert!(window.status_message.is_some());
    }

    #[test]
    fn test_documents_view_with_selected_doc_details_renders_and_fits() {
        let ctx = Context::default();
        let mut window = StatisticsWindow {
            data: Some(WritingStatistics {
                documents: vec![DocumentStatistics {
                    id: "doc-selected".to_string(),
                    path: Some("/tmp/my_novel.md".into()),
                    words: Some(5000),
                    version_count: 10,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            view: StatisticsView::Documents,
            selected_doc_id: Some("doc-selected".to_string()),
            ..Default::default()
        };

        for (width, height) in [(600.0, 460.0), (800.0, 720.0)] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let available = ui.available_width();
                        window.show_content(ui);
                        assert!(ui.min_rect().width() <= available + 1.0);
                    });
                },
            );
        }
    }
}
