# Paper Shell development and maintenance

This document contains contributor and maintainer instructions. For installation and
product usage, see the [README](../README.md).

## Build from source

Install a current stable Rust toolchain, then build the release binary:

```bash
cargo build --release
```

macOS matches the primary CI environment. Other platforms may require native GUI build
dependencies; the exact packages used by Linux CI are listed in
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Acceptance check

Run the complete local check from the repository root:

```bash
bash scripts/acceptance-check.sh
```

This runs the same formatting, Clippy (warnings are errors), and test checks as CI. It
stops on the first failure with a nonzero exit code and prints a success message only
when all checks pass. Cargo uses `Cargo.lock` without updating it.

The script requires Rust/Cargo with the `rustfmt` and `clippy` components installed:

```bash
rustup component add rustfmt clippy
```

The first run downloads dependencies and compiles the app, so allow extra time and network
access. These automated checks do not cover interactive desktop UI acceptance.

To run only the macOS print-layout checks:

```bash
cargo test --locked --test print_layout
```

The test generates PDFs for long Chinese/English paragraphs and unbroken URLs on A4,
Letter, and landscape paper without sending anything to a printer. Set
`PAPER_SHELL_KEEP_PRINT_PDFS=1` to keep the generated temporary PDFs for visual review;
the test prints their paths.

## Bundle on macOS

Install the same `cargo-bundle` version used by the release workflow, then create the app:

```bash
cargo install cargo-bundle --version 0.9.0
cargo bundle --release
```

The bundled app is created at:

```text
target/release/bundle/osx/Paper Shell.app
```

## Release

Pushing a version tag such as `v0.1.9` triggers
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds, packages,
and publishes a GitHub Release.

Before tagging:

1. Update `package.version` in `Cargo.toml` and the root `paper-shell` package version in
   `Cargo.lock` to the same value.
2. Update user-facing documentation and release notes as needed.
3. Run `bash scripts/acceptance-check.sh` and verify that the working tree is clean.
4. Stage the intended files, commit the release changes, push `master`, create an
   annotated tag, and push the tag:

   ```bash
   git add Cargo.toml Cargo.lock README.md docs/development.md
   git commit -m "chore: bump version to X.Y.Z"
   git push origin master
   git tag -a "vX.Y.Z" -m "Paper Shell vX.Y.Z"
   git push origin "vX.Y.Z"
   ```

The release workflow generates:

- `Paper-Shell-macOS-universal.dmg` / `.zip` (Apple Silicon and Intel)
- `Paper-Shell-macOS-aarch64.dmg` / `.zip` (Apple Silicon)
- `Paper-Shell-macOS-x86_64.dmg` / `.zip` (Intel)
- `Paper-Shell-Linux-x86_64.tar.gz` (Linux x86_64)
- `Paper-Shell-Windows-x86_64.zip` (Windows x86_64)
- `SHA256SUMS.txt` (checksums for every package)

You can also trigger releases manually from the GitHub Actions tab via `workflow_dispatch`.
The workflow accepts an optional tag name and controls for publishing a release, creating
a draft, or marking it as a prerelease.

To monitor a tagged release and inspect the resulting assets with the GitHub CLI:

```bash
gh run list --workflow=release.yml --limit=5
gh run watch <run-id>
gh release view "vX.Y.Z"
```

## External plugin contract

External plugins receive the document on standard input. Paper Shell also sets:

- `PAPER_SHELL_FILE`
- `PAPER_SHELL_DATA_DIR`
- `PAPER_SHELL_PLUGIN_DIR`
- `PAPER_SHELL_PLUGIN_SETTINGS` (a JSON object containing resolved setting values)

Standard output is shown as the success message. A nonzero exit code and standard error
are shown as an execution failure. The command and its interpreter/runtime must already
be installed.

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

Text and password fields support defaults and required validation. For example, Python
can read resolved settings with `json.loads(os.environ["PAPER_SHELL_PLUGIN_SETTINGS"])`.
Password values are masked in the UI and are not logged by Paper Shell, but they are stored
unencrypted in the application configuration file.

Built-in IDs (`print`, `cloud_backup`, `github_publish`) are reserved. Invalid manifests,
unreadable directories, reserved IDs, and duplicate IDs are shown in the manager. All
plugins sharing a duplicate ID are rejected rather than running an arbitrary one.
External plugins are not sandboxed; review their commands and arguments before enabling
them.

## Roadmap

- [x] Fix open-with on macOS and other systems
- [x] Search and replace
- [x] Copy/paste and right-click context menu
- [x] Highlight matches for selected text
- [x] Add an add-ons/plugin system for export
- [x] Support document printing (native macOS print preview and PDF export)
- [ ] Prevent leading punctuation for Chinese text; this may require using
      [cosmic-text](https://github.com/pop-os/cosmic-text) to rewrite the egui editor component
- [ ] Dictionary-lookup panel
- [ ] Narrative map with click-to-jump navigation
- [ ] Informative data records on paragraphs or full text
