mod diff;
mod stats;
pub(crate) mod tree;
mod types;
mod ui;

use crate::backend::editor_backend::{EditorBackend, HistoryEntry};
use chrono::Local;
use egui::{Color32, Context, RichText, ScrollArea, Ui, Vec2};
use std::borrow::Cow;
use std::rc::Rc;

use types::{BranchFilter, DiffCompareMode, GraphEdge, HistoryTree};
pub use types::{DiffLine, DiffLineType, HistoryVersionData};

const ROW_HEIGHT: f32 = 44.0;
const BRANCH_COLORS: [Color32; 6] = [
    Color32::from_rgb(46, 151, 83),
    Color32::from_rgb(78, 110, 168),
    Color32::from_rgb(173, 120, 24),
    Color32::from_rgb(128, 90, 150),
    Color32::from_rgb(45, 125, 135),
    Color32::from_rgb(156, 82, 76),
];

#[derive(Debug, PartialEq, Eq)]
pub enum HistoryAction {
    RollbackToVersion(String),
    UpdateVersionLabel {
        version_id: String,
        label: Option<String>,
    },
    DeleteVersion {
        version_id: String,
        delete_branch: bool,
    },
}

#[derive(Debug, Clone)]
struct PendingDelete {
    version_id: String,
    delete_branch: bool,
    subtree_size: usize,
}

#[derive(Debug, Clone)]
enum RowAction {
    Select(String),
    Rollback(String),
    EditLabel(String),
    Pin(String),
    Delete(String, bool, usize),
}

pub struct HistoryWindow {
    open: bool,
    history_data: Option<Rc<HistoryTree>>,
    layout: types::HistoryLayout,
    layout_filter: BranchFilter,
    error_message: Option<String>,
    selected_id: Option<String>,
    current_version_id: Option<String>,
    viewport_id: egui::ViewportId,
    pending_action: Option<HistoryAction>,
    pub(crate) confirm_rollback: Option<String>,
    branch_filter: BranchFilter,
    compare_mode: DiffCompareMode,
    pinned_base_id: Option<String>,
    editing_label: Option<(String, String)>,
    pending_delete: Option<PendingDelete>,
}

impl Default for HistoryWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            history_data: None,
            layout: types::HistoryLayout::default(),
            layout_filter: BranchFilter::All,
            error_message: None,
            selected_id: None,
            current_version_id: None,
            viewport_id: egui::ViewportId::from_hash_of("history_window"),
            pending_action: None,
            confirm_rollback: None,
            branch_filter: BranchFilter::All,
            compare_mode: DiffCompareMode::Parent,
            pinned_base_id: None,
            editing_label: None,
            pending_delete: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_current_version_id(&mut self, id: Option<String>) {
        self.current_version_id = id;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.pending_action = None;
        self.confirm_rollback = None;
        self.pending_delete = None;
    }

    pub fn open(&mut self) {
        let already_open = self.open;
        self.open = true;
        self.error_message = None;
        self.confirm_rollback = None;
        self.pending_delete = None;
        if !already_open {
            self.history_data = None;
            self.layout = types::HistoryLayout::default();
            self.layout_filter = BranchFilter::All;
            self.selected_id = None;
            self.branch_filter = BranchFilter::All;
            self.compare_mode = DiffCompareMode::Parent;
            self.pinned_base_id = None;
            self.editing_label = None;
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
        self.history_data = None;
    }

    pub fn set_history(
        &mut self,
        entries: Vec<HistoryEntry>,
        backend: &EditorBackend,
    ) -> Result<(), String> {
        let tree = tree::build_version_tree(entries, backend, self.current_version_id.as_deref())?;
        if let BranchFilter::Branch(id) = self.branch_filter
            && !tree.branches.iter().any(|branch| branch.id == id)
        {
            self.branch_filter = BranchFilter::All;
        }
        if self
            .pinned_base_id
            .as_deref()
            .is_some_and(|id| !tree.nodes.iter().any(|node| node.id == id))
        {
            self.pinned_base_id = None;
            self.compare_mode = DiffCompareMode::Parent;
        }
        let selected = self
            .selected_id
            .as_deref()
            .and_then(|id| tree.nodes.iter().find(|node| node.id == id))
            .or_else(|| tree.nodes.iter().find(|node| node.is_current))
            .or_else(|| tree.nodes.last())
            .map(|node| node.id.clone());
        self.selected_id = selected;
        self.layout = tree::compute_display_layout(&tree, &self.branch_filter);
        self.layout_filter = self.branch_filter;
        self.history_data = Some(Rc::new(tree));
        self.error_message = None;
        Ok(())
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let viewport_id = self.viewport_id;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(format!(
                    "{} 版本历史",
                    egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                ))
                .with_inner_size(Vec2::new(980.0, 640.0))
                .with_min_inner_size(Vec2::new(600.0, 420.0))
                .with_decorations(false)
                .with_resizable(true)
                .with_transparent(true),
            |ctx, _class| {
                crate::style::configure_style(ctx);
                let title_bar_panel = egui::TopBottomPanel::top("history_title_bar")
                    .show_separator_line(false)
                    .show(ctx, |ui| self.show_title_bar(ui));
                crate::ui::title_bar::TitleBar::paint_bottom_border(
                    ctx,
                    title_bar_panel.response.rect,
                );
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::ZERO),
                    )
                    .show(ctx, |ui| self.show_content(ui));
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
                if self.confirm_rollback.is_none()
                    && self.pending_delete.is_none()
                    && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.open = false;
                }
            },
        );
    }

    pub fn take_pending_action(&mut self) -> Option<HistoryAction> {
        self.pending_action.take()
    }

    fn show_title_bar(&mut self, ui: &mut Ui) {
        let title_bar_rect = ui.available_rect_before_wrap();
        let interact = ui.interact(
            title_bar_rect,
            ui.id().with("history_title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if interact.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if interact.double_clicked() {
            let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 20.0;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} 版本历史",
                        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                    ))
                    .color(ui.visuals().text_color().gamma_multiply(0.82)),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .add_sized(
                        [22.0, 20.0],
                        egui::Button::new(egui_phosphor::regular::X).frame(false),
                    )
                    .on_hover_text("关闭")
                    .clicked()
                {
                    self.open = false;
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
            });
        });
    }

    fn show_content(&mut self, ui: &mut Ui) {
        if let Some(error) = &self.error_message {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(
                    RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                        .size(32.0)
                        .color(Color32::from_rgb(200, 80, 80)),
                );
                ui.add_space(8.0);
                ui.heading("无法加载版本历史");
                ui.add_space(8.0);
                ui.label(RichText::new(error).color(Color32::from_rgb(200, 80, 80)));
            });
            return;
        }
        let Some(tree) = self.history_data.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("正在加载历史版本…");
                ui.add_space(12.0);
                ui.spinner();
            });
            return;
        };
        if tree.nodes.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.heading("暂无历史版本");
                ui.add_space(8.0);
                ui.label(
                    RichText::new("编辑并保存文档后，即可在此查看版本历史。")
                        .color(Color32::from_gray(120)),
                );
            });
            return;
        }

        if self.layout_filter != self.branch_filter {
            self.layout = tree::compute_display_layout(&tree, &self.branch_filter);
            self.layout_filter = self.branch_filter;
        }
        let layout = self.layout.clone();
        let mut row_action = None;
        let panel = egui::SidePanel::left("version_tree_panel")
            .resizable(true)
            .default_width(300.0)
            .width_range(200.0..=420.0)
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} 个版本 · {} 条分支",
                            tree.nodes.len(),
                            tree.branches.len()
                        ))
                        .small(),
                    );
                });
                if tree.branches.len() > 1 {
                    let selected_text = match &self.branch_filter {
                        BranchFilter::All => "全部分叉".to_string(),
                        BranchFilter::CurrentLineage => "当前主线".to_string(),
                        BranchFilter::Branch(id) => tree
                            .branches
                            .iter()
                            .find(|branch| branch.id == *id)
                            .map(|branch| branch_label(&tree, branch.id))
                            .unwrap_or_else(|| "全部分叉".into()),
                    };
                    egui::ComboBox::from_id_salt("history_branch_filter")
                        .selected_text(selected_text)
                        .width(ui.available_width() - 8.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.branch_filter,
                                BranchFilter::All,
                                "全部分叉",
                            );
                            ui.selectable_value(
                                &mut self.branch_filter,
                                BranchFilter::CurrentLineage,
                                "当前主线",
                            );
                            for branch in &tree.branches {
                                ui.selectable_value(
                                    &mut self.branch_filter,
                                    BranchFilter::Branch(branch.id),
                                    format!(
                                        "{}（{}）",
                                        branch_label(&tree, branch.id),
                                        branch.node_count
                                    ),
                                );
                            }
                        });
                }
                ui.label(RichText::new("● 当前  ◎ 分叉点  ○ 分支末端").small().weak());
                ui.separator();
                ui.spacing_mut().item_spacing.y = 0.0;
                ScrollArea::vertical()
                    .id_salt("history_version_tree")
                    .auto_shrink([false, false])
                    .show_rows(ui, ROW_HEIGHT, layout.display_order.len(), |ui, rows| {
                        for row in rows {
                            let index = layout.display_order[row];
                            if let Some(action) = show_version_row(
                                ui,
                                &tree.nodes[index],
                                row,
                                layout.lane_count,
                                &layout.edges,
                                self.selected_id.as_deref() == Some(tree.nodes[index].id.as_str()),
                            ) {
                                row_action = Some(action);
                            }
                        }
                    });
            });
        ui.painter().vline(
            panel.response.rect.right() - 0.5,
            panel.response.rect.y_range(),
            egui::Stroke::new(1.0_f32, ui.visuals().text_color().gamma_multiply(0.18)),
        );

        if let Some(action) = row_action {
            self.apply_row_action(action);
        }

        let selected = self
            .selected_id
            .as_deref()
            .and_then(|id| tree.nodes.iter().find(|node| node.id == id));
        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::same(12)))
            .show_inside(ui, |ui| {
                if let Some(version) = selected {
                    self.show_version_details(ui, &tree, version);
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading("请选择版本");
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("从左侧树图中选择版本，查看内容与管理操作。")
                                .color(Color32::from_gray(120)),
                        );
                    });
                }
            });

        self.show_rollback_modal(ui);
        self.show_delete_modal(ui);
    }

    fn apply_row_action(&mut self, action: RowAction) {
        match action {
            RowAction::Select(id) => self.selected_id = Some(id),
            RowAction::Rollback(id) => self.confirm_rollback = Some(id),
            RowAction::EditLabel(id) => {
                let label = self
                    .history_data
                    .as_ref()
                    .and_then(|tree| tree.nodes.iter().find(|node| node.id == id))
                    .and_then(|node| node.entry.label.clone())
                    .unwrap_or_default();
                self.editing_label = Some((id, label));
            }
            RowAction::Pin(id) => {
                self.pinned_base_id = Some(id);
                self.compare_mode = DiffCompareMode::Pinned;
            }
            RowAction::Delete(id, branch, size) => {
                self.pending_delete = Some(PendingDelete {
                    version_id: id,
                    delete_branch: branch,
                    subtree_size: size,
                });
            }
        }
    }

    fn show_version_details(
        &mut self,
        ui: &mut Ui,
        tree: &HistoryTree,
        version: &HistoryVersionData,
    ) {
        let muted = ui.visuals().text_color().gamma_multiply(0.65);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(&version.branch_name)
                    .strong()
                    .color(branch_color(version.branch_id)),
            );
            if version.is_current {
                ui.label(RichText::new("当前版本").color(crate::style::STATUS_GREEN));
            }
            if version.is_fork_point {
                ui.label(RichText::new("分叉点").color(Color32::from_rgb(173, 120, 24)));
            }
            if version.is_branch_head {
                ui.label(RichText::new("分支末端").small().color(muted));
            }
            ui.label(
                RichText::new(format!("+{}", version.added_count))
                    .color(crate::style::STATUS_GREEN),
            );
            ui.label(
                RichText::new(format!("-{}", version.removed_count))
                    .color(Color32::from_rgb(145, 64, 64)),
            );
            let seconds = version.entry.time_spent.unwrap_or(0);
            ui.label(
                RichText::new(format!(
                    "{} {:02}:{:02}",
                    egui_phosphor::regular::CLOCK,
                    seconds / 60,
                    seconds % 60
                ))
                .small()
                .color(muted),
            );
            ui.label(
                RichText::new(format!("#{}", version.entry.hash))
                    .monospace()
                    .small()
                    .color(muted),
            );
        });
        ui.label(
            RichText::new(
                version
                    .entry
                    .timestamp
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            )
            .small()
            .color(muted),
        );
        ui.add_space(4.0);

        if self
            .editing_label
            .as_ref()
            .is_some_and(|(id, _)| id == &version.id)
        {
            let mut save = false;
            let mut cancel = false;
            let mut clear = false;
            if let Some((_, text)) = &mut self.editing_label {
                ui.horizontal(|ui| {
                    ui.label("备注");
                    let response = ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(240.0)
                            .hint_text("例如：人物线、第三章初稿"),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                    if ui.button("保存").clicked() {
                        save = true;
                    }
                    if ui.button("清除").clicked() {
                        clear = true;
                    }
                    if ui.button("取消").clicked() {
                        cancel = true;
                    }
                });
            }
            if save || clear {
                let label = if clear {
                    None
                } else {
                    self.editing_label
                        .as_ref()
                        .map(|(_, text)| text.trim().to_string())
                        .filter(|text| !text.is_empty())
                };
                self.pending_action = Some(HistoryAction::UpdateVersionLabel {
                    version_id: version.id.clone(),
                    label,
                });
                self.editing_label = None;
            } else if cancel {
                self.editing_label = None;
            }
        } else {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(version.label().map_or("未添加备注", |label| label)).color(
                        if version.label().is_some() {
                            ui.visuals().text_color()
                        } else {
                            muted
                        },
                    ),
                );
                if ui
                    .button(format!(
                        "{} 编辑备注",
                        egui_phosphor::regular::PENCIL_SIMPLE
                    ))
                    .clicked()
                {
                    self.editing_label =
                        Some((version.id.clone(), version.label().unwrap_or("").into()));
                }
            });
        }

        ui.horizontal_wrapped(|ui| {
            if let Some(parent) = &version.parent_id
                && ui.small_button("↑ 父版本").clicked()
            {
                self.selected_id = Some(parent.clone());
            }
            for child in &version.children_ids {
                if ui.small_button("↓ 子版本").on_hover_text(child).clicked() {
                    self.selected_id = Some(child.clone());
                }
            }
            if let Some(fork_parent) = tree
                .branches
                .iter()
                .find(|branch| branch.id == version.branch_id)
                .and_then(|branch| branch.fork_parent_id.as_ref())
                && ui
                    .small_button("分叉起点")
                    .on_hover_text(fork_parent)
                    .clicked()
            {
                self.selected_id = Some(fork_parent.clone());
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!version.is_current, |ui| {
                if ui
                    .button(format!(
                        "{} 回滚到此版本",
                        egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                    ))
                    .clicked()
                {
                    self.confirm_rollback = Some(version.id.clone());
                }
            });
            if ui.button("设为对比基准").clicked() {
                self.pinned_base_id = Some(version.id.clone());
                self.compare_mode = DiffCompareMode::Pinned;
            }
            ui.add_enabled_ui(!version.is_current, |ui| {
                if ui.button("删除版本").clicked() {
                    self.pending_delete = Some(PendingDelete {
                        version_id: version.id.clone(),
                        delete_branch: false,
                        subtree_size: version.subtree_size,
                    });
                }
            });
            ui.add_enabled_ui(version.can_delete_branch, |ui| {
                if ui
                    .button("删除此分叉")
                    .on_hover_text(format!(
                        "删除此节点及其 {} 个后代版本",
                        version.subtree_size - 1
                    ))
                    .clicked()
                {
                    self.pending_delete = Some(PendingDelete {
                        version_id: version.id.clone(),
                        delete_branch: true,
                        subtree_size: version.subtree_size,
                    });
                }
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.compare_mode,
                DiffCompareMode::Parent,
                "相对父版本",
            );
            ui.selectable_value(
                &mut self.compare_mode,
                DiffCompareMode::Current,
                "相对当前版本",
            );
            ui.selectable_value(&mut self.compare_mode, DiffCompareMode::Pinned, "自选基准");
            if self.compare_mode == DiffCompareMode::Pinned {
                let selected_text = self
                    .pinned_base_id
                    .as_deref()
                    .and_then(|id| tree.nodes.iter().find(|node| node.id == id))
                    .map(short_node_name)
                    .unwrap_or_else(|| "选择基准版本".into());
                egui::ComboBox::from_id_salt("history_compare_base")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for node in &tree.nodes {
                            if ui
                                .selectable_label(
                                    self.pinned_base_id.as_deref() == Some(node.id.as_str()),
                                    short_node_name(node),
                                )
                                .clicked()
                            {
                                self.pinned_base_id = Some(node.id.clone());
                            }
                        }
                    });
            }
        });

        let (diff_lines, description) = comparison_diff(
            tree,
            version,
            self.compare_mode,
            self.pinned_base_id.as_deref(),
        );
        ui.label(RichText::new(description).small().weak());
        if self.compare_mode != DiffCompareMode::Parent {
            let stats = stats::calculate_stats(&diff::group_into_rows(&diff_lines));
            ui.label(
                RichText::new(format!("+{}  -{}", stats.added_count, stats.removed_count)).small(),
            );
        }
        ui.add_space(4.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| ui::render_diff_view(ui, &diff_lines));
    }

    fn show_rollback_modal(&mut self, ui: &mut Ui) {
        let Some(id) = self.confirm_rollback.clone() else {
            return;
        };
        let config = crate::ui::dialog::ModalConfig {
            id: "history_confirm_rollback",
            title: "确认回滚到此版本",
            icon: egui_phosphor::regular::WARNING,
            icon_color: Color32::from_rgb(173, 120, 24),
            message: "确定要将文档回滚到此版本吗？\n当前编辑器内容将被替换；后续保存会从该版本分叉，已有版本不会丢失。",
            width: 380.0,
        };
        let mut confirm = false;
        let mut cancel = false;
        let response = crate::ui::dialog::show_modal(ui.ctx(), config, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::dialog::modal_button(
                        ui,
                        RichText::new(format!(
                            "{} 确认回滚",
                            egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                        ))
                        .strong(),
                    )
                    .clicked()
                    {
                        confirm = true;
                    }
                    if crate::ui::dialog::modal_button(ui, "取消").clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if confirm || response.enter_pressed {
            self.pending_action = Some(HistoryAction::RollbackToVersion(id));
            self.confirm_rollback = None;
            self.open = false;
        } else if cancel || response.escape_pressed {
            self.confirm_rollback = None;
        }
    }

    fn show_delete_modal(&mut self, ui: &mut Ui) {
        let Some(pending) = self.pending_delete.clone() else {
            return;
        };
        let branch = pending.delete_branch;
        let config = crate::ui::dialog::ModalConfig {
            id: "history_confirm_delete",
            title: if branch {
                "确认删除此分叉"
            } else {
                "确认删除此版本"
            },
            icon: egui_phosphor::regular::WARNING,
            icon_color: Color32::from_rgb(145, 64, 64),
            message: if branch {
                "将删除此节点及其全部后代版本，其他分叉不受影响。\n此操作不可撤销。"
            } else {
                "将删除此版本记录，子版本会连接到它的父版本。\n文章正文与其他分支保留。此操作不可撤销。"
            },
            width: 380.0,
        };
        let mut confirm = false;
        let mut cancel = false;
        let response = crate::ui::dialog::show_modal(ui.ctx(), config, |ui| {
            ui.label(format!(
                "将影响 {} 个树节点。",
                if branch { pending.subtree_size } else { 1 }
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::dialog::modal_button(
                        ui,
                        RichText::new(if branch {
                            "删除分叉"
                        } else {
                            "删除版本"
                        })
                        .color(Color32::from_rgb(145, 64, 64))
                        .strong(),
                    )
                    .clicked()
                    {
                        confirm = true;
                    }
                    if crate::ui::dialog::modal_button(ui, "取消").clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if confirm {
            self.pending_action = Some(HistoryAction::DeleteVersion {
                version_id: pending.version_id,
                delete_branch: pending.delete_branch,
            });
            self.pending_delete = None;
        } else if cancel || response.escape_pressed {
            self.pending_delete = None;
        }
    }
}

fn comparison_diff<'a>(
    tree: &'a HistoryTree,
    version: &'a HistoryVersionData,
    mode: DiffCompareMode,
    pinned_id: Option<&str>,
) -> (Cow<'a, [DiffLine]>, String) {
    match mode {
        DiffCompareMode::Parent => (
            Cow::Borrowed(version.diff_lines.as_slice()),
            if version.parent_id.is_some() {
                "父版本 → 所选版本（只显示本版本的改动）".into()
            } else {
                "根版本完整内容".into()
            },
        ),
        DiffCompareMode::Current => {
            let current = tree.nodes.iter().find(|node| node.is_current);
            if let Some(current) = current {
                (
                    Cow::Owned(diff::compute_diff(&current.content, &version.content)),
                    format!("当前版本 → 所选版本（{}）", short_node_name(current)),
                )
            } else {
                (
                    Cow::Borrowed(version.diff_lines.as_slice()),
                    "当前版本不可用".into(),
                )
            }
        }
        DiffCompareMode::Pinned => {
            if let Some(base) =
                pinned_id.and_then(|id| tree.nodes.iter().find(|node| node.id == id))
            {
                (
                    Cow::Owned(diff::compute_diff(&base.content, &version.content)),
                    format!("基准 {} → 所选版本", short_node_name(base)),
                )
            } else {
                (
                    Cow::Borrowed(version.diff_lines.as_slice()),
                    "请先选择一个对比基准".into(),
                )
            }
        }
    }
}

fn short_node_name(node: &HistoryVersionData) -> String {
    node.label().map_or_else(
        || {
            node.entry
                .timestamp
                .with_timezone(&Local)
                .format("%m-%d %H:%M")
                .to_string()
        },
        |label| label.to_string(),
    )
}

fn branch_label(tree: &HistoryTree, branch_id: usize) -> String {
    tree.branches
        .iter()
        .find(|branch| branch.id == branch_id)
        .and_then(|branch| tree.nodes.iter().find(|node| node.id == branch.head_id))
        .and_then(|head| head.label())
        .map_or_else(
            || {
                tree.branches
                    .iter()
                    .find(|branch| branch.id == branch_id)
                    .map(|branch| branch.name.clone())
                    .unwrap_or_else(|| format!("分叉 {branch_id}"))
            },
            |label| label.to_string(),
        )
}

fn branch_color(branch_id: usize) -> Color32 {
    BRANCH_COLORS[branch_id % BRANCH_COLORS.len()]
}

fn show_version_row(
    ui: &mut Ui,
    node: &HistoryVersionData,
    row: usize,
    lane_count: usize,
    edges: &[GraphEdge],
    selected: bool,
) -> Option<RowAction> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::hover());
    let accessible_label = format!(
        "{}，{}，{}",
        node.entry
            .timestamp
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S"),
        node.branch_name,
        node.label().unwrap_or("无备注")
    );
    // Keep the custom graph row keyboard-focusable; its accessible text is
    // painted separately below so the graph and typography stay aligned.
    let response = ui.put(
        rect,
        egui::Button::new(RichText::new(accessible_label).color(Color32::TRANSPARENT)).frame(false),
    );
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(240));
    }
    if selected {
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(232));
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, ui.visuals().text_color()),
            egui::StrokeKind::Inside,
        );
    } else if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, ui.visuals().text_color().gamma_multiply(0.65)),
            egui::StrokeKind::Inside,
        );
    }
    let graph_width = ((lane_count as f32 * 14.0) + 12.0).min(width * 0.45);
    let lane_step = if lane_count > 0 {
        (graph_width - 12.0) / lane_count as f32
    } else {
        14.0
    };
    let lane_x = |lane: usize| rect.left() + 6.0 + lane_step * lane as f32 + lane_step / 2.0;
    for edge in edges
        .iter()
        .filter(|edge| edge.child_row <= row && edge.parent_row >= row)
    {
        let color = branch_color(edge.branch_id).gamma_multiply(0.72);
        let stroke = egui::Stroke::new(1.35_f32, color);
        if edge.child_lane == edge.parent_lane {
            let x = lane_x(edge.child_lane);
            let start_y = if edge.child_row == row {
                rect.center().y
            } else {
                rect.top()
            };
            let end_y = if edge.parent_row == row {
                rect.center().y
            } else {
                rect.bottom()
            };
            ui.painter()
                .line_segment([egui::pos2(x, start_y), egui::pos2(x, end_y)], stroke);
        } else {
            let child_x = lane_x(edge.child_lane);
            if row < edge.parent_row {
                let start_y = if edge.child_row == row {
                    rect.center().y
                } else {
                    rect.top()
                };
                ui.painter().line_segment(
                    [
                        egui::pos2(child_x, start_y),
                        egui::pos2(child_x, rect.bottom()),
                    ],
                    stroke,
                );
            } else {
                let parent_x = lane_x(edge.parent_lane);
                let from = egui::pos2(child_x, rect.top());
                let to = egui::pos2(parent_x, rect.center().y);
                let control_1 = egui::pos2(child_x, rect.top() + ROW_HEIGHT * 0.28);
                let control_2 = egui::pos2(parent_x, rect.center().y - ROW_HEIGHT * 0.28);
                let points: Vec<egui::Pos2> = (0..=12)
                    .map(|step| {
                        let t = step as f32 / 12.0;
                        let u = 1.0 - t;
                        egui::pos2(
                            u * u * u * from.x
                                + 3.0 * u * u * t * control_1.x
                                + 3.0 * u * t * t * control_2.x
                                + t * t * t * to.x,
                            u * u * u * from.y
                                + 3.0 * u * u * t * control_1.y
                                + 3.0 * u * t * t * control_2.y
                                + t * t * t * to.y,
                        )
                    })
                    .collect();
                ui.painter().line(points, stroke);
            }
        }
    }
    let center = egui::pos2(lane_x(node.lane), rect.center().y);
    let color = branch_color(node.branch_id);
    let panel_fill = ui.visuals().panel_fill;
    ui.painter().circle_filled(center, 7.0, panel_fill);
    if node.is_current {
        ui.painter()
            .circle_stroke(center, 6.0, egui::Stroke::new(1.5_f32, color));
        ui.painter().circle_filled(center, 3.5, color);
    } else if node.is_fork_point {
        ui.painter()
            .circle_stroke(center, 5.0, egui::Stroke::new(1.5_f32, color));
        ui.painter().circle_filled(center, 2.2, color);
    } else if node.is_branch_head {
        ui.painter()
            .circle_stroke(center, 4.5, egui::Stroke::new(1.5_f32, color));
        ui.painter().circle_filled(center, 1.5, color);
    } else {
        ui.painter().circle_filled(center, 3.2, color);
    }

    let text_x = rect.left() + graph_width + 4.0;
    let text_color = ui.visuals().text_color();
    let painter = ui.painter().with_clip_rect(egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top()),
        rect.right_bottom(),
    ));
    painter.text(
        egui::pos2(text_x, rect.top() + 5.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}{}",
            node.entry
                .timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S"),
            if node.is_current { "  当前" } else { "" }
        ),
        egui::FontId::proportional(11.5),
        text_color.gamma_multiply(if selected { 1.0 } else { 0.82 }),
    );
    let secondary = node
        .label()
        .map_or_else(|| node.branch_name.clone(), |label| label.to_string());
    painter.text(
        egui::pos2(text_x, rect.top() + 24.0),
        egui::Align2::LEFT_TOP,
        format!("{secondary}  +{} -{}", node.added_count, node.removed_count),
        egui::FontId::proportional(11.0),
        if node.label().is_some() {
            text_color
        } else {
            color
        },
    );
    response.clone().on_hover_text(format!(
        "{}\n{}\n#{}\n版本 ID: {}{}",
        node.entry
            .timestamp
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S"),
        node.branch_name,
        node.entry.hash,
        node.id,
        node.label()
            .map_or(String::new(), |label| format!("\n备注：{label}")),
    ));

    let mut action = response
        .clicked()
        .then(|| RowAction::Select(node.id.clone()));
    response.context_menu(|ui| {
        if ui
            .add_enabled(!node.is_current, egui::Button::new("回滚到此版本"))
            .clicked()
        {
            action = Some(RowAction::Rollback(node.id.clone()));
            ui.close();
        }
        if ui.button("编辑备注").clicked() {
            action = Some(RowAction::EditLabel(node.id.clone()));
            ui.close();
        }
        if ui.button("设为对比基准").clicked() {
            action = Some(RowAction::Pin(node.id.clone()));
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!node.is_current, egui::Button::new("删除此版本"))
            .clicked()
        {
            action = Some(RowAction::Delete(node.id.clone(), false, node.subtree_size));
            ui.close();
        }
        if ui
            .add_enabled(node.can_delete_branch, egui::Button::new("删除此分叉"))
            .on_hover_text("当前版本所在的分叉不能删除")
            .clicked()
        {
            action = Some(RowAction::Delete(node.id.clone(), true, node.subtree_size));
            ui.close();
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_window_open_resets_transient_state() {
        let mut window = HistoryWindow::new();
        assert!(!window.is_open());
        window.confirm_rollback = Some("abc".into());
        window.error_message = Some("err".into());
        window.open();
        assert!(window.is_open());
        assert!(window.confirm_rollback.is_none());
        assert!(window.error_message.is_none());
        assert!(window.history_data.is_none());
    }

    #[test]
    fn history_window_set_error() {
        let mut window = HistoryWindow::new();
        window.set_error("failed to read".into());
        assert_eq!(window.error_message, Some("failed to read".into()));
        assert!(window.history_data.is_none());
    }

    #[test]
    fn history_window_take_pending_action() {
        let mut window = HistoryWindow::new();
        assert!(window.take_pending_action().is_none());
        window.pending_action = Some(HistoryAction::RollbackToVersion("commit_hash".into()));
        assert_eq!(
            window.take_pending_action(),
            Some(HistoryAction::RollbackToVersion("commit_hash".into()))
        );
        assert!(window.take_pending_action().is_none());
    }
}
