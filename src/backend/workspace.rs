//! Read workspace folders, ignoring hidden system files (like `.DS_Store`), unopenable files, and symlinks.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub editable: bool,
}

pub fn read_folder(path: &Path, default_root: &Path) -> io::Result<Vec<WorkspaceEntry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Ignore hidden files and directories (e.g. .DS_Store, .git)
        if name_str.starts_with('.') {
            continue;
        }

        let kind = entry.file_type()?;
        if kind.is_dir() {
            if path == default_root
                && matches!(name_str.as_ref(), "blobs" | "history" | "marks" | "plugins")
            {
                continue;
            }
            entries.push(WorkspaceEntry {
                path: entry.path(),
                is_dir: true,
                editable: false,
            });
        } else if kind.is_file() {
            let path = entry.path();
            if is_text_file(&path) {
                entries.push(WorkspaceEntry {
                    path,
                    is_dir: false,
                    editable: true,
                });
            }
        }
        // Non-regular files, unsupported file formats, and symbolic links are ignored.
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    Ok(entries)
}

fn is_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "txt" | "md" | "markdown" | "text" | "org" | "rst"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_hidden_and_unsupported_files_ignored() {
        let root = std::env::temp_dir().join(format!("workspace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("chapters")).unwrap();
        fs::write(root.join("draft.MD"), "hello").unwrap();
        fs::write(root.join(".DS_Store"), [0, 0, 0, 1]).unwrap();
        fs::write(root.join(".gitignore"), "target/").unwrap();
        fs::write(root.join("image.png"), [0, 255]).unwrap();
        fs::write(root.join("document.pdf"), [37, 80, 68, 70]).unwrap();

        let entries = read_folder(&root, &root).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, root.join("chapters"));
        assert!(!entries[0].editable);
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].path, root.join("draft.MD"));
        assert!(entries[1].editable);

        // When reading as a custom root (not default_root), blobs is kept, but .git and unopenable files are still ignored.
        let custom_entries = read_folder(&root, &root.join("other")).unwrap();
        assert_eq!(custom_entries.len(), 3);
        assert_eq!(custom_entries[0].path, root.join("blobs"));
        assert_eq!(custom_entries[1].path, root.join("chapters"));
        assert_eq!(custom_entries[2].path, root.join("draft.MD"));

        assert!(read_folder(&root.join("missing"), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_ignored() {
        let root = std::env::temp_dir().join(format!("workspace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let text_file = root.join("real.txt");
        fs::write(&text_file, "real").unwrap();
        std::os::unix::fs::symlink(&text_file, root.join("link.txt")).unwrap();

        let entries = read_folder(&root, &root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, text_file);
        fs::remove_dir_all(root).unwrap();
    }
}
