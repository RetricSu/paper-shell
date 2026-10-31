//! Read workspace folders without exposing application storage or following symlinks.
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
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if path == default_root
            && kind.is_dir()
            && matches!(
                name.to_str(),
                Some("blobs" | "history" | "marks" | "plugins")
            )
        {
            continue;
        }
        let path = entry.path();
        let editable = kind.is_file() && is_text_file(&path);
        entries.push(WorkspaceEntry {
            path,
            is_dir: kind.is_dir(),
            editable,
        });
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
    fn default_storage_is_hidden_but_custom_folders_are_preserved() {
        let root = std::env::temp_dir().join(format!("workspace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("chapters")).unwrap();
        fs::write(root.join("draft.MD"), "hello").unwrap();
        fs::write(root.join("image.png"), [0, 255]).unwrap();
        let entries = read_folder(&root, &root).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, root.join("chapters"));
        assert!(entries[1].editable);
        assert!(!entries[2].editable);
        assert_eq!(read_folder(&root, &root.join("other")).unwrap().len(), 4);
        assert!(read_folder(&root.join("missing"), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_traversed_or_edited() {
        let root = std::env::temp_dir().join(format!("workspace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let entries = read_folder(&root, &root).unwrap();
        assert!(!entries[0].is_dir);
        assert!(!entries[0].editable);
        fs::remove_dir_all(root).unwrap();
    }
}
