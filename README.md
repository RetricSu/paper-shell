# Paper Shell

Working in progress: A simple, compact and informative text editor for everyday writing.

## Build

```bash
cargo build --release
```

## Acceptance check

In ctx, set the acceptance command to run from the task worktree:

```bash
bash scripts/acceptance-check.sh
```

This runs the same formatting, Clippy (warnings are errors), and test checks as CI.
It stops on the first failure with a nonzero exit code and prints a success message
only when all checks pass. Cargo uses `Cargo.lock` without updating it.

Requires Rust/Cargo with the `rustfmt` and `clippy` components installed
(`rustup component add rustfmt clippy`). macOS matches the CI environment;
other platforms may need native GUI build dependencies. The first run downloads
dependencies and compiles the app, so allow extra time and network access.
These automated checks do not cover interactive desktop UI acceptance.

## Inline comments

Select any non-empty text range and use the floating **评论** button (or right-click
and choose **添加评论 / 备注**). Commented text has a pale amber highlight and an
underline. Click it to edit or delete the comment. Anchors follow later insertions,
deletions, formatting, search/replace, and history restores; partial deletion keeps
the comment on the remaining text, while deleting or replacing the entire range
removes it.

Comments are saved with the article (Cmd/Ctrl+S) as character-indexed metadata in
the app data directory's `marks` folder, leaving the original plain-text file
unchanged. Comment-only changes also trigger the unsaved-changes prompt when
switching articles or closing the window.

## Workspace files

Click the **纸壳** title or press **Cmd/Ctrl+Shift+E** to show or hide the file
tree. A tiny dot at the title's upper left is filled when the sidebar is visible
and hollow when hidden. The default workspace and all added folders appear as
peer roots; each expands independently.
Right-click blank space in the sidebar for **添加文件夹…** and **刷新**.
When the article list is hidden, the editor shows a non-interactive paragraph-dot
gutter. The focused paragraph's dot breathes gently; wrapped lines share one dot.
The gutter disappears while the article list is visible.
There is no separate toolbar. Each root has a small new-article icon at its right edge.
Click a folder row to expand it; indentation shows hierarchy without branch
lines or disclosure triangles. Files display only their names.
Folders are remembered, and duplicate additions are ignored. Right-click a custom
root and choose **从侧栏移除** to remove its entry without deleting any files.
Unavailable roots stay listed and show a read error so they can be retried or removed.

The default workspace keeps its existing location in the app data directory.
Internal `blobs`, `history`, `marks`, and `plugins` folders are hidden there.
Custom folders keep their original layout; articles are edited in place and
version history stays in the app data directory. Existing paragraph note files
are preserved, but the paragraph note feature has been removed.

Click a root's new-article icon (or right-click it and choose **在此新建文章**)
to start a blank article with the save dialog pointing to that folder. The tree supports `txt`, `md`,
`markdown`, `text`, `org`, and `rst`; hidden files (such as `.DS_Store`), hidden directories,
unsupported files, and symbolic links are ignored. Use the blank-space context menu to refresh after external folder changes. Switching
articles or closing the window offers save, discard, or cancel for unsaved changes.
Saving an empty article is supported.

## Plugins

Open **插件 → 管理插件…** to see every built-in and external plugin, its source,
version, author, enabled state, and any discovery errors. Print, Cloud Backup, and
GitHub Publish are built in. Cloud Backup and GitHub Publish have dedicated settings.
macOS Print uses the native system print panel; other platforms choose a printer and
margins for each print run.

To install an external plugin:

1. Choose **打开插件目录** in the manager.
2. Put the plugin in an immediate subdirectory of that folder. The subdirectory must
   contain a `plugin.toml` manifest and its command/script.
3. Return to Paper Shell and choose **重新扫描**. Restarting is not required.

The enable checkbox is saved immediately and applies to both built-in and external
plugins. Disabled plugins do not appear in the run menu and cannot be executed, but
their settings remain editable. **卸载** is available for external plugins only: after
confirmation it permanently deletes that plugin's direct child folder and its saved
settings. Invalid manifests, unreadable directories, reserved IDs, and duplicate IDs
are shown in the manager. All plugins sharing a duplicate ID are rejected rather than
running an arbitrary one. Built-in IDs (`print`, `cloud_backup`, `github_publish`) are
reserved.

Example `plugin.toml`:

```toml
id          = "word-count"
name        = "字数统计"
description = "Count words in the current document"
version     = "0.1.0"
author      = "you"
command     = "python3"
args        = ["main.py"]

[[settings]]
key         = "api_key"
label       = "API 密钥"
description = "Optional service credential"
type        = "password"
required    = true

[[settings]]
key         = "report_format"
label       = "报告格式"
type        = "text"
default     = "summary"
```

External settings are edited in the same manager. Text and password fields support
defaults and required validation. At runtime the resolved values are available as a
JSON object in `PAPER_SHELL_PLUGIN_SETTINGS`; for example, Python can read it with
`json.loads(os.environ["PAPER_SHELL_PLUGIN_SETTINGS"])`. Password values are masked in
the UI and are not logged by Paper Shell, but **they are stored unencrypted in the
application configuration file**.

The host also provides the document on stdin and sets `PAPER_SHELL_FILE`,
`PAPER_SHELL_DATA_DIR`, and `PAPER_SHELL_PLUGIN_DIR`. Standard output is shown as the
success message; a non-zero exit code and standard error are shown as an execution
failure. The command and its interpreter/runtime must already be installed.

**Trust boundary:** external plugins run local commands with the same permissions as
Paper Shell and are not sandboxed. Install only plugins you trust, and review the
manifest's `command` and `args` before enabling one.

## Printing (macOS)

Choose **打印当前文档** from the plugins menu to open the native macOS print panel.
The current editor text (including unsaved changes) is laid out with system fonts,
12 pt type, automatic wrapping and pagination, and equal 72 pt margins. The system
panel controls the printer, paper size, orientation, scale, copies, and page range.
Use its actual page preview to check the layout, or **PDF → Save as PDF** to inspect
or share the same print output before sending it to a printer. Text remains plain
text; Markdown and other source formats are not interpreted as markup.

macOS printing uses AppKit rather than submitting raw text to `lp`. Other platforms
retain their existing print command behavior. Automated macOS layout checks generate
PDFs for long Chinese/English paragraphs and unbroken URLs on A4, Letter, and
landscape paper without sending anything to a printer. Run them with
`cargo test --locked --test print_layout`; set `PAPER_SHELL_KEEP_PRINT_PDFS=1` to keep
the generated temporary PDFs for visual review (the test prints their paths).

## Bundle (macOS)

```bash
cargo install cargo-bundle
cargo bundle --release
```

Bundled `.app` will be created at `target/release/bundle/osx/Paper Shell.app`.

## Release

Pushing a version tag (e.g. `v0.1.0`) triggers GitHub Actions to build, package, and publish release assets:
- `Paper-Shell-macOS-universal.dmg` / `.zip` (Universal for Apple Silicon & Intel)
- `Paper-Shell-macOS-aarch64.dmg` / `.zip` (Apple Silicon)
- `Paper-Shell-macOS-x86_64.dmg` / `.zip` (Intel)
- `Paper-Shell-Linux-x86_64.tar.gz` (Linux x86_64)
- `Paper-Shell-Windows-x86_64.zip` (Windows x86_64)
- `SHA256SUMS.txt` (Checksums)

You can also trigger releases manually from the GitHub Actions tab via `workflow_dispatch`.

## TODOs

- [x] fix open-with on Mac and other system
- [x] search and replace
- [x] support copy-paste and right click context menu
- [x] select a text and high-light all the matched ones
- [ ] prevent leading punctuation for Chinese text, may require using [cosmic-text](https://github.com/pop-os/) to rewrite editor component for egui
- [ ] dictionary-lookup panel
- [ ] support narrative map with click-to-jump function
- [ ] informative data records on paragraph or full text
- [x] add a add-ons/plugin system for export
- [x] support print document (native macOS print preview and PDF export)
