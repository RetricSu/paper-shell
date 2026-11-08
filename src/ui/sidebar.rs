use crate::backend::workspace::{WorkspaceEntry, read_folder};
use egui::Ui;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub enum SidebarAction {
    OpenFile(PathBuf),
    OpenFolder,
    RemoveFolder(PathBuf),
    NewDocument(Option<PathBuf>),
}

/// Persistent workspace UI state (session state) decoupled from user settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceState {
    #[serde(default)]
    pub sidebar_expanded: bool,
    #[serde(default)]
    pub expanded_folders: BTreeSet<PathBuf>,
    #[serde(default)]
    pub collapsed_roots: BTreeSet<PathBuf>,
}

impl WorkspaceState {
    pub const FILE_NAME: &'static str = "workspace_state.json";

    /// Load persistent workspace state from data directory, sanitizing stale paths.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(Self::FILE_NAME);
        if let Ok(file) = std::fs::File::open(&path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(mut state) = serde_json::from_reader::<_, Self>(reader) {
                state.sanitize();
                return state;
            }
        }
        Self::default()
    }

    /// Atomically save workspace state to data directory.
    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let path = data_dir.join(Self::FILE_NAME);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp_path = path.with_extension("tmp");
        {
            let file = std::fs::File::create(&temp_path)?;
            let writer = std::io::BufWriter::new(file);
            serde_json::to_writer_pretty(writer, self)?;
        }
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Check whether a folder is open. Roots default to open, subfolders default to closed.
    pub fn is_open(&self, path: &Path, is_root: bool) -> bool {
        if is_root {
            !self.collapsed_roots.contains(path)
        } else {
            self.expanded_folders.contains(path)
        }
    }

    /// Update open state for a root or subfolder.
    pub fn set_open(&mut self, path: &Path, is_root: bool, open: bool) {
        if is_root {
            if open {
                self.collapsed_roots.remove(path);
            } else {
                self.collapsed_roots.insert(path.to_path_buf());
            }
        } else if open {
            self.expanded_folders.insert(path.to_path_buf());
        } else {
            self.expanded_folders.remove(path);
        }
    }

    /// Purge paths that no longer exist on disk to prevent zombie paths from accumulating.
    pub fn sanitize(&mut self) {
        self.expanded_folders.retain(|p| p.is_dir());
        self.collapsed_roots.retain(|p| p.is_dir());
    }
}

/// Directory contents are read only on first expansion or explicit refresh.
pub struct Sidebar {
    state: WorkspaceState,
    state_dirty: bool,
    folders: HashMap<PathBuf, Result<Vec<WorkspaceEntry>, String>>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new(WorkspaceState::default())
    }
}

impl Sidebar {
    pub fn new(state: WorkspaceState) -> Self {
        Self {
            state,
            state_dirty: false,
            folders: HashMap::new(),
        }
    }

    pub fn state(&self) -> &WorkspaceState {
        &self.state
    }

    pub fn is_dirty(&self) -> bool {
        self.state_dirty
    }

    pub fn save_if_dirty(&mut self, data_dir: &Path) {
        if self.state_dirty {
            if let Err(error) = self.state.save(data_dir) {
                tracing::warn!("Failed to save workspace state: {}", error);
            }
            self.state_dirty = false;
        }
    }

    pub fn save_state(&mut self, data_dir: &Path) -> std::io::Result<()> {
        self.state_dirty = false;
        self.state.save(data_dir)
    }

    pub fn on_root_removed(&mut self, root: &Path) {
        self.state.collapsed_roots.remove(root);
        self.state.expanded_folders.retain(|p| !p.starts_with(root));
        self.state_dirty = true;
    }

    pub fn refresh(&mut self) {
        self.folders.clear();
        self.state.sanitize();
    }

    pub fn is_expanded(&self) -> bool {
        self.state.sidebar_expanded
    }

    pub fn toggle(&mut self) {
        self.state.sidebar_expanded = !self.state.sidebar_expanded;
        self.state_dirty = true;
    }

    pub fn expand(&mut self) {
        if !self.state.sidebar_expanded {
            self.state.sidebar_expanded = true;
            self.state_dirty = true;
        }
    }

    pub fn collapse(&mut self) {
        if self.state.sidebar_expanded {
            self.state.sidebar_expanded = false;
            self.state_dirty = true;
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        default_root: &Path,
        custom_roots: &[PathBuf],
        current_file: Option<&Path>,
    ) -> Option<SidebarAction> {
        if !self.state.sidebar_expanded {
            return None;
        }
        let mut action = None;
        let max_width = (ctx.content_rect().width() * 0.45).clamp(140.0, 400.0);
        let mut side_panel = egui::SidePanel::left("workspace_tree")
            .default_width(230.0)
            .width_range(140.0..=max_width);
        if let Some(skin) = crate::style::get_active_skin()
            && let Some(color) = skin
                .colors
                .sidebar_background_color
                .as_deref()
                .and_then(crate::config::parse_hex_color)
        {
            side_panel = side_panel.frame(egui::Frame::side_top_panel(&ctx.style()).fill(color));
        }
        side_panel.show(ctx, |ui| {
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
                                    action =
                                        Some(SidebarAction::NewDocument(Some(root.to_path_buf())));
                                    ui.close();
                                }
                                if !is_default
                                    && ui
                                        .button("从侧栏移除")
                                        .on_hover_text("仅移除入口，保留磁盘文件")
                                        .clicked()
                                {
                                    action = Some(SidebarAction::RemoveFolder(root.to_path_buf()));
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
        let is_open = self.state.is_open(path, root);
        let icon = if is_open {
            egui_phosphor::regular::FOLDER_OPEN
        } else {
            egui_phosphor::regular::FOLDER_SIMPLE
        };
        let label = egui::RichText::new(format!("{icon} {name}"));
        let label = if root { label.strong() } else { label };
        let response = ui
            .push_id(path, |ui| {
                ui.add(egui::Button::new(label).frame(false).truncate())
                    .on_hover_text(if is_open {
                        "点击收起"
                    } else {
                        "点击展开"
                    })
            })
            .inner;
        let mut new_open = is_open;
        if response.clicked() {
            new_open = !is_open;
            self.state.set_open(path, root, new_open);
            self.state_dirty = true;
        }
        (response, new_open)
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
        assert!(!sidebar.is_expanded());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(custom).unwrap();
    }

    #[test]
    fn workspace_state_save_load_and_sanitize() {
        let temp_dir = std::env::temp_dir().join(format!("ws-state-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let sub1 = temp_dir.join("sub1");
        let sub2 = temp_dir.join("sub2");
        let deleted = temp_dir.join("deleted");
        std::fs::create_dir_all(&sub1).unwrap();
        std::fs::create_dir_all(&sub2).unwrap();
        std::fs::create_dir_all(&deleted).unwrap();

        let mut state = WorkspaceState {
            sidebar_expanded: true,
            ..Default::default()
        };
        state.set_open(&temp_dir, true, false); // collapse root
        state.set_open(&sub1, false, true); // expand sub1
        state.set_open(&deleted, false, true); // expand deleted

        assert!(!state.is_open(&temp_dir, true));
        assert!(state.is_open(&sub1, false));
        assert!(!state.is_open(&sub2, false));
        assert!(state.is_open(&deleted, false));

        // Save state to disk
        state.save(&temp_dir).unwrap();

        // Remove deleted directory before loading
        std::fs::remove_dir_all(&deleted).unwrap();

        // Load back and verify sanitize pruned deleted directory
        let loaded = WorkspaceState::load(&temp_dir);
        assert!(loaded.sidebar_expanded);
        assert!(!loaded.is_open(&temp_dir, true));
        assert!(loaded.is_open(&sub1, false));
        assert!(!loaded.is_open(&deleted, false));

        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn sidebar_folder_toggle_persists_across_instances() {
        let root = std::env::temp_dir().join(format!("sidebar-toggle-{}", uuid::Uuid::new_v4()));
        let sub = root.join("nested");
        std::fs::create_dir_all(&sub).unwrap();
        let doc = sub.join("doc.md");
        std::fs::write(&doc, "test").unwrap();

        let data_dir = root.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let mut sidebar = Sidebar::new(WorkspaceState {
            sidebar_expanded: true,
            ..Default::default()
        });

        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.0);

        let render = |sidebar: &mut Sidebar, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(720.0, 468.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    sidebar.show(ctx, &root, &[], None);
                },
            )
        };

        // First frame: root is displayed, nested subfolder is collapsed by default
        let output = render(&mut sidebar, vec![]);
        let nested_pos = text_position(&output, "nested");

        // Click to expand nested folder
        render(&mut sidebar, click(nested_pos));
        assert!(sidebar.is_dirty());
        assert!(sidebar.state().is_open(&sub, false));

        // Save state to data_dir
        sidebar.save_if_dirty(&data_dir);
        assert!(!sidebar.is_dirty());

        // Create a new sidebar instance loading from data_dir
        let loaded_state = WorkspaceState::load(&data_dir);
        let mut new_sidebar = Sidebar::new(loaded_state);
        assert!(new_sidebar.state().is_open(&sub, false));

        // Render new sidebar: doc.md is already visible without clicking
        let output = render(&mut new_sidebar, vec![]);
        text_position(&output, "doc.md");

        // Clean up root
        new_sidebar.on_root_removed(&root);
        assert!(!new_sidebar.state().is_open(&sub, false));

        std::fs::remove_dir_all(root).unwrap();
    }
}
