use crate::backend::editor_backend::EditorBackend;
use crate::statistics::{StatisticsGrouping, StatisticsPeriod, WritingActivity, WritingStatistics};
use chrono::{Local, NaiveDate};
use egui::{Color32, Context, RichText, Ui};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Default)]
pub struct StatisticsWindow {
    open: bool,
    refresh_requested: bool,
    receiver: Option<Receiver<Result<WritingStatistics, String>>>,
    data: Option<WritingStatistics>,
    error: Option<String>,
    period: StatisticsPeriod,
    grouping: StatisticsGrouping,
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
        self.invalidate();
    }

    pub fn invalidate(&mut self) {
        self.refresh_requested = true;
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
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| self.show_content(ui));
                    });
                if ctx.input(|i| i.viewport().close_requested())
                    || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.open = false;
                }
            },
        );
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
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (period, label) in [
                (StatisticsPeriod::Last7Days, "近 7 天"),
                (StatisticsPeriod::Last30Days, "近 30 天"),
                (StatisticsPeriod::ThisYear, "今年"),
                (StatisticsPeriod::AllTime, "全部"),
            ] {
                if ui
                    .selectable_value(&mut self.period, period, label)
                    .clicked()
                {
                    self.grouping = match period {
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
                .selected_text(grouping_label(self.grouping))
                .show_ui(ui, |ui| {
                    for grouping in [
                        StatisticsGrouping::Day,
                        StatisticsGrouping::Week,
                        StatisticsGrouping::Month,
                    ] {
                        ui.selectable_value(&mut self.grouping, grouping, grouping_label(grouping));
                    }
                });
        });
        let report = data.report(self.period, self.grouping, today);
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
        show_chart(ui, &report.buckets, self.grouping);
        if !report.activity.is_active() {
            ui.label(RichText::new("这段时间还没有已保存的字数变化。").weak());
        }
        egui::CollapsingHeader::new("时间明细").show(ui, |ui| {
            ui.label(
                RichText::new("按周以周一为起点，首尾仅包含所选日期范围内的记录。")
                    .small()
                    .weak(),
            );
            egui::ScrollArea::vertical()
                .id_salt("statistics_time_rows")
                .max_height(240.0)
                .show(ui, |ui| {
                    egui::Grid::new("statistics_time_table")
                        .num_columns(5)
                        .striped(true)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            for heading in ["日期", "新增", "删除", "净增", "写作时长"]
                            {
                                ui.strong(heading);
                            }
                            ui.end_row();
                            for (date, activity) in report.buckets.iter().rev() {
                                ui.label(bucket_label(*date, self.grouping));
                                ui.label(format!("+{}", number(activity.added)));
                                ui.label(format!("−{}", number(activity.removed)));
                                ui.label(format!("{:+}", activity.net()));
                                ui.label(duration(activity.seconds));
                                ui.end_row();
                            }
                        });
                });
        });
        ui.add_space(8.0);
        egui::CollapsingHeader::new("各文档 · 最新保存字数")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("statistics_documents")
                    .max_height(180.0)
                    .show_rows(ui, 20.0, data.documents.len(), |ui, rows| {
                        for index in rows {
                            let doc = &data.documents[index];
                            let path = doc
                                .path
                                .as_ref()
                                .map_or_else(|| doc.id.clone(), |p| p.display().to_string());
                            let name = doc.path.as_ref().and_then(|p| p.file_name()).map_or_else(
                                || doc.id.clone(),
                                |n| n.to_string_lossy().into_owned(),
                            );
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 100.0).max(100.0);
                                ui.add_sized(
                                    [width, 20.0],
                                    egui::Label::new(name).truncate().halign(egui::Align::Min),
                                )
                                .on_hover_text(path);
                                ui.label(doc.words.map_or_else(
                                    || "无法读取".to_string(),
                                    |n| format!("{} 字", number(n)),
                                ));
                            });
                        }
                    });
            });
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("统计口径：汉字每字计 1，其他连续非空白字符计 1 个词，与编辑器一致。首次保存的全文计入新增；导入、粘贴和版本恢复也计入，不代表实际键入量。保存间的改写按字词比较，未保存的改动不计入。写作时长沿用已保存的编辑器聚焦时间，按保存日期归档。").small().weak());
    }
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
        egui::vec2(ui.available_width(), 164.0),
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
    fn statistics_content_fits_minimum_width_with_large_totals_and_long_names() {
        for width in [600.0, 800.0] {
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
                ..Default::default()
            };
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 720.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE.inner_margin(20))
                            .show(ctx, |ui| {
                                let available = ui.available_width();
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    window.show_content(ui);
                                    assert!(
                                        ui.min_rect().width() <= available + 1.0,
                                        "statistics overflow at {width}: {:?}",
                                        ui.min_rect()
                                    );
                                });
                            });
                    },
                );
            }
        }
    }
}
