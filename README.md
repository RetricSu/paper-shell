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

## Workspace files

Click the **纸壳** title or use **文件 → 显示文章列表 / 隐藏文章列表**
(Cmd/Ctrl+Shift+E) to show or hide the file tree. A tiny dot at the title's upper
left is filled when the sidebar is visible and hollow when hidden. This command sits below a separator at the bottom of the File menu. The default
workspace and all added folders appear as peer roots; each expands independently.
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
`markdown`, `text`, `org`, and `rst`; other files are shown disabled, and symbolic
links are not followed. Use the blank-space context menu to refresh after external folder changes. Switching
articles or closing the window offers save, discard, or cancel for unsaved changes.
Saving an empty article is supported.

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
- [ ] add a add-ons/plugin system for export
- [ ] support print document
