<div align="center">
  <img src="assets/app-icon.png" alt="Paper Shell app icon" width="112" />
  <h1>Paper Shell</h1>
  <p><strong>A quiet place to think, write, and revise.</strong></p>
  <p>
    A simple, compact, and informative desktop editor for everyday and long-form writing.
  </p>
  <p>
    <a href="https://github.com/RetricSu/paper-shell/releases/latest"><strong>Download</strong></a>
    ·
    <a href="#features">Features</a>
    ·
    <a href="#getting-started">Getting started</a>
    ·
    <a href="docs/development.md">Development</a>
  </p>
  <p>
    <a href="https://github.com/RetricSu/paper-shell/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/RetricSu/paper-shell?display_name=tag" /></a>
    <a href="https://github.com/RetricSu/paper-shell/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/RetricSu/paper-shell/actions/workflows/ci.yml/badge.svg" /></a>
  </p>
</div>

Paper Shell keeps the document at the center of the workspace. It combines plain-text
editing with practical tools for organizing files, annotating passages, exploring version
history, and understanding your writing habits—without turning the editor into a dashboard.

> Paper Shell is under active development. Download a packaged release below, or
> [build from source](docs/development.md#build-from-source).

## Download

Prebuilt packages are available for macOS, Linux, and Windows.

| Platform | Package | Notes |
| --- | --- | --- |
| macOS — Universal | [DMG](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-universal.dmg) · [ZIP](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-universal.zip) | Apple Silicon and Intel |
| macOS — Apple Silicon | [DMG](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-aarch64.dmg) · [ZIP](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-aarch64.zip) | `aarch64` |
| macOS — Intel | [DMG](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-x86_64.dmg) · [ZIP](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-macOS-x86_64.zip) | `x86_64` |
| Linux | [tar.gz](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-Linux-x86_64.tar.gz) | `x86_64` |
| Windows | [ZIP](https://github.com/RetricSu/paper-shell/releases/latest/download/Paper-Shell-Windows-x86_64.zip) | `x86_64` |

On macOS, open the DMG and drag **Paper Shell** into **Applications**. On Linux and
Windows, extract the archive and run the included executable. The current macOS packages
are ad-hoc signed rather than notarized, so macOS may display a Gatekeeper warning.

Every release includes [SHA-256 checksums](https://github.com/RetricSu/paper-shell/releases/latest/download/SHA256SUMS.txt).
See the [release page](https://github.com/RetricSu/paper-shell/releases/latest) for all
packages and release notes.

## Getting started

1. Open an existing document with **文件 → 打开文件…** (`Cmd/Ctrl+O`), or use the default
   workspace.
2. Press `Cmd/Ctrl+Shift+E`, or click the **纸壳** title, to show the workspace sidebar.
   Add a folder from the sidebar's context menu, or create an article from a folder's
   new-article action.
3. Write in plain text, then save with `Cmd/Ctrl+S`. Paper Shell supports `txt`, `md`,
   `markdown`, `text`, `org`, and `rst` files.
4. Open **文件 → 写作统计…**, **文件 → 版本历史…**, or **插件 → 管理插件…** to explore
   the tools around your document.

Use **文件 → 关于纸壳** to check the installed version and update the app.

### App updates

Paper Shell checks GitHub for a newer stable release in the background at startup. When
an update is available, **文件 → 关于纸壳 · 有新版本** opens the update controls. You can also
check manually in the About window. Choose **下载更新**, then **安装并重启** when ready;
the app verifies the package against the release's SHA-256 checksums before offering to
install it. Unsaved writing goes through the usual save/discard/cancel confirmation,
and the current saved file reopens after restart. Other open windows keep running.

Updates use the macOS Universal app bundle or the Linux/Windows x86_64 executable.
Install the app in a writable location first; macOS updates require a packaged `.app`
outside the DMG. Network errors can be retried from About, which also links to the
release page for manual installation. Drafts, prereleases, and older versions are skipped.

## Features

### Focused editing

- Search and replace, match highlighting, clipboard actions, and a selection toolbar.
- Adjustable Chinese fonts, font size, line spacing, and paragraph indentation.
- Optional Vim modal editing via **编辑 → Vim 模式 (Esc)**. Press `Esc` to switch between
  Insert and Normal modes; Visual mode and common motions, operators, and counts are supported.
- Unsaved-change confirmation when switching articles or closing a window.

### Workspace files

Keep the default workspace and any folders you add in one expandable file tree. Custom
folders stay where they are, and articles are edited in place. Duplicate folders are
ignored; removing a custom folder from the sidebar never deletes its files.

The tree hides unsupported files, hidden files and directories, and symbolic links. Use
the sidebar's refresh action after changing files outside Paper Shell.

### Inline comments

Select a non-empty range and release the mouse to add a comment, or right-click and choose
**添加评论 / 备注**. Commented text has a pale amber highlight and underline; click it to
edit or delete the comment.

Comments are saved as character-indexed metadata in the app data directory, leaving the
original text file unchanged. Anchors follow later edits and history restores. Deleting
the entire commented range also removes its comment.

### Branching version history

Open **文件 → 版本历史…** to inspect saved versions as a branching tree. Select a version
to see its timestamp, focus time, word changes, relationships, and content diff. Compare
against its parent, the current version, or a pinned base.

Rolling back restores the editor content without deleting later saves; continuing from an
older version creates a sibling branch. Versions can be labeled, pinned, or deleted with
confirmation. The current version and any branch containing it are protected from deletion.

### Writing statistics

Open **文件 → 写作统计…**, or click the title-bar word count, to see saved word totals,
additions and deletions, focus time, and active days. Compare the last 7 days, last 30 days,
this year, or all history, grouped by day, week, or calendar month.

The **趋势总览**, **时间明细**, and **文档统计** tabs separate trends, complete time-series
details, and a searchable per-document list. Statistics are computed in the background,
and saving updates an open window automatically. **刷新** picks up saves from other windows.

One Han character counts as one word; other non-whitespace runs count as one word each.
Statistics compare saved versions rather than keystrokes, so imports, pasted text, and
restored versions count as changes. Unsaved drafts are excluded. Focus time is the existing
editor focus timer, not a measure of active typing.

### Plugins and printing

Paper Shell includes **Print**, **Cloud Backup**, and **GitHub Publish** plugins. Open
**插件 → 管理插件…** to enable or configure built-ins, discover external plugins, or inspect
manifest errors. External plugins can be installed by placing a folder containing a
`plugin.toml` manifest and its command in the plugin directory, then choosing **重新扫描**.

On macOS, **打印当前文档** opens the native print panel with automatic wrapping, pagination,
and PDF export. Other platforms use their existing print-command flow.

> **Plugin security:** external plugins run local commands with the same permissions as
> Paper Shell and are not sandboxed. Install only plugins you trust and review their
> manifests before enabling them. External password settings are masked in the UI but are
> stored unencrypted in the application configuration file.

## Development

Build instructions, acceptance checks, packaging, the release process, and the project
roadmap live in [docs/development.md](docs/development.md).

## Feedback

Report bugs or request features through [GitHub Issues](https://github.com/RetricSu/paper-shell/issues).
