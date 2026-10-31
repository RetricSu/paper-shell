use crate::backend::workspace::{WorkspaceEntry, read_folder};
use egui::Ui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub enum SidebarAction {
    OpenFile(PathBuf),
    OpenFolder,
    RemoveFolder(PathBuf),
    NewDocument(Option<PathBuf>),
}

/// Directory contents are read only on first expansion or explicit refresh.
#[derive(Default)]
pub struct Sidebar {
    expanded: bool,
    folders: HashMap<PathBuf, Result<Vec<WorkspaceEntry>, String>>,
    open_folders: HashMap<PathBuf, bool>,
}

impl Sidebar {
    pub fn refresh(&mut self) {
        self.folders.clear();
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn expand(&mut self) {
        self.expanded = true;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        default_root: &Path,
        custom_roots: &[PathBuf],
        current_file: Option<&Path>,
    ) -> Option<SidebarAction> {
        if !self.expanded {
            return None;
        }
        let mut action = None;
        let max_width = (ctx.content_rect().width() * 0.45).clamp(140.0, 400.0);
        egui::SidePanel::left("workspace_tree")
            .default_width(230.0)
            .width_range(140.0..=max_width)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                ui.spacing_mut().indent = 12.0;
                ui.visuals_mut().indent_has_left_vline = false;
                // Register behind the rows so their own interactions take precedence.
                let blank_area = ui.interact(
                    ui.max_rect(),
                    ui.id().with("workspace_blank"),
                    egui::Sense::click(),
                );
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("workspace_files")
                    .show(ui, |ui| {
                        for root in std::iter::once(default_root)
                            .chain(custom_roots.iter().map(PathBuf::as_path))
                        {
                            let is_default = root == default_root;
                            let name = if is_default {
                                "默认工作区".to_string()
                            } else {
                                root.file_name()
                                    .unwrap_or(root.as_os_str())
                                    .to_string_lossy()
                                    .into_owned()
                            };
                            let (response, open) = ui
                                .horizontal(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized(
                                                    [22.0, 22.0],
                                                    egui::Button::new(
                                                        egui_phosphor::regular::FILE_PLUS,
                                                    )
                                                    .frame(false),
                                                )
                                                .on_hover_text(format!("在 {} 中新建文章", name))
                                                .clicked()
                                            {
                                                action = Some(SidebarAction::NewDocument(Some(
                                                    root.to_path_buf(),
                                                )));
                                            }
                                            ui.with_layout(
                                                egui::Layout::top_down(egui::Align::Min),
                                                |ui| self.folder_row(ui, root, &name, true),
                                            )
                                            .inner
                                        },
                                    )
                                    .inner
                                })
                                .inner;
                            response
                                .on_hover_text(root.display().to_string())
                                .context_menu(|ui| {
                                    if ui.button("在此新建文章").clicked() {
                                        action = Some(SidebarAction::NewDocument(Some(
                                            root.to_path_buf(),
                                        )));
                                        ui.close();
                                    }
                                    if !is_default
                                        && ui
                                            .button("从侧栏移除")
                                            .on_hover_text("仅移除入口，保留磁盘文件")
                                            .clicked()
                                    {
                                        action =
                                            Some(SidebarAction::RemoveFolder(root.to_path_buf()));
                                        ui.close();
                                    }
                                });
                            if open {
                                ui.indent(("workspace_children", root), |ui| {
                                    self.show_folder(
                                        ui,
                                        root,
                                        default_root,
                                        current_file,
                                        0,
                                        &mut action,
                                    );
                                });
                            }
                        }
                    });
                blank_area.context_menu(|ui| {
                    if ui.button("添加文件夹…").clicked() {
                        action = Some(SidebarAction::OpenFolder);
                        ui.close();
                    }
                    if ui.button("刷新").clicked() {
                        self.refresh();
                        ui.close();
                    }
                });
            });
        action
    }

    /// Folder rows toggle directly, without disclosure triangles or branch lines.
    fn folder_row(
        &mut self,
        ui: &mut Ui,
        path: &Path,
        name: &str,
        root: bool,
    ) -> (egui::Response, bool) {
        let open = self.open_folders.entry(path.to_path_buf()).or_insert(root);
        let icon = if *open {
            egui_phosphor::regular::FOLDER_OPEN
        } else {
            egui_phosphor::regular::FOLDER_SIMPLE
        };
        let label = egui::RichText::new(format!("{icon} {name}"));
        let label = if root { label.strong() } else { label };
        let response = ui
            .push_id(path, |ui| {
                ui.add(egui::Button::new(label).frame(false).truncate())
                    .on_hover_text(if *open {
                        "点击收起"
                    } else {
                        "点击展开"
                    })
            })
            .inner;
        if response.clicked() {
            *open = !*open;
        }
        (response, *open)
    }

    fn show_folder(
        &mut self,
        ui: &mut Ui,
        path: &Path,
        default_root: &Path,
        current_file: Option<&Path>,
        depth: usize,
        action: &mut Option<SidebarAction>,
    ) {
        if depth >= 32 {
            ui.weak("目录层级过深");
            return;
        }
        let entries = self
            .folders
            .entry(path.to_path_buf())
            .or_insert_with(|| read_folder(path, default_root).map_err(|e| e.to_string()))
            .clone();
        match entries {
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, "无法读取文件夹")
                    .on_hover_text(error);
            }
            Ok(entries) => {
                if entries.is_empty() {
                    ui.weak(if depth == 0 {
                        "空文件夹，可右键新建文章"
                    } else {
                        "空文件夹"
                    });
                }
                for entry in entries {
                    let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
                    if entry.is_dir {
                        let (response, open) = self.folder_row(ui, &entry.path, &name, false);
                        response.on_hover_text(entry.path.display().to_string());
                        if open {
                            ui.indent(&entry.path, |ui| {
                                self.show_folder(
                                    ui,
                                    &entry.path,
                                    default_root,
                                    current_file,
                                    depth + 1,
                                    action,
                                );
                            });
                        }
                    } else {
                        let response = ui
                            .add_enabled(
                                entry.editable,
                                egui::Button::selectable(
                                    current_file == Some(entry.path.as_path()),
                                    name.as_ref(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(entry.path.display().to_string())
                            .on_disabled_hover_text(
                                "支持 txt、md、markdown、text、org、rst 文本文件；不跟随符号链接",
                            );
                        if response.clicked() {
                            *action = Some(SidebarAction::OpenFile(entry.path));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_position(output: &egui::FullOutput, needle: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text().contains(needle)
                {
                    return Some(text.pos + text.galley.size() * 0.5);
                }
                None
            })
            .unwrap_or_else(|| panic!("Missing text: {needle}"))
    }

    fn click(position: egui::Pos2) -> Vec<egui::Event> {
        pointer_click(position, egui::PointerButton::Primary)
    }

    fn pointer_click(position: egui::Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: position,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn collapsed_sidebar_reserves_no_space_and_reads_no_folders() {
        let ctx = egui::Context::default();
        let mut sidebar = Sidebar::default();
        let _ = ctx.run(Default::default(), |ctx| {
            let before = ctx.available_rect();
            assert!(sidebar.show(ctx, Path::new("missing"), &[], None).is_none());
            assert_eq!(ctx.available_rect(), before);
        });
        assert!(sidebar.folders.is_empty());
    }

    #[test]
    fn expand_navigate_open_refresh_and_collapse() {
        let root = std::env::temp_dir().join(format!("sidebar-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("chapters")).unwrap();
        let custom = root.with_extension("custom");
        std::fs::create_dir_all(&custom).unwrap();
        let custom_article = custom.join("custom-article.txt");
        std::fs::write(&custom_article, "custom").unwrap();
        let article = root.join("chapters/draft.md");
        std::fs::write(&article, "hello").unwrap();
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut sidebar = Sidebar::default();
        let mut render = |events| {
            let mut action = None;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(720.0, 468.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                        ui.menu_button("文件", |ui| {
                            if ui
                                .button(if sidebar.is_expanded() {
                                    "隐藏文章列表"
                                } else {
                                    "显示文章列表"
                                })
                                .clicked()
                            {
                                sidebar.toggle();
                                ui.close();
                            }
                        });
                    });
                    action =
                        sidebar.show(ctx, &root, std::slice::from_ref(&custom), Some(&article));
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("Editor");
                    });
                },
            );
            (output, action)
        };
        let (output, _) = render(vec![]);
        render(click(text_position(&output, "文件")));
        let (output, _) = render(vec![]);
        render(click(text_position(&output, "显示文章列表")));
        let (output, _) = render(vec![]);
        // Both roots and their contents coexist, without changing a selection.
        text_position(&output, "默认工作区");
        text_position(&output, &custom.file_name().unwrap().to_string_lossy());
        let new_buttons: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == egui_phosphor::regular::FILE_PLUS
                {
                    Some(text.pos + text.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(new_buttons.len(), 2);
        assert!((new_buttons[0].x - new_buttons[1].x).abs() < 1.0);
        assert!(
            new_buttons[0].x > 200.0,
            "new article buttons must stay at the panel's right edge"
        );
        let (_, action) = render(click(new_buttons[0]));
        assert!(matches!(action, Some(SidebarAction::NewDocument(Some(path))) if path == root));
        let (_, action) = render(click(new_buttons[1]));
        assert!(matches!(action, Some(SidebarAction::NewDocument(Some(path))) if path == custom));
        let (output, _) = render(vec![]);
        let custom_position = text_position(&output, "custom-article.txt");
        let (_, custom_action) = render(click(custom_position));
        assert!(
            matches!(custom_action, Some(SidebarAction::OpenFile(path)) if path == custom_article)
        );
        let (output, _) = render(vec![]);
        let position = text_position(&output, "chapters");
        render(click(position));
        let (output, _) = render(vec![]);
        let position = text_position(&output, "draft.md");
        let (_, action) = render(click(position));
        assert!(matches!(action, Some(SidebarAction::OpenFile(path)) if path == article));
        std::fs::write(root.join("new.txt"), "new").unwrap();
        render(pointer_click(
            egui::pos2(100.0, 420.0),
            egui::PointerButton::Secondary,
        ));
        let (output, _) = render(vec![]);
        text_position(&output, "添加文件夹…");
        render(click(text_position(&output, "刷新")));
        let (output, _) = render(vec![]);
        text_position(&output, "new.txt");
        render(click(text_position(&output, "文件")));
        let (output, _) = render(vec![]);
        render(click(text_position(&output, "隐藏文章列表")));
        render(vec![]);
        assert!(!sidebar.expanded);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(custom).unwrap();
    }
}
