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

## Bundle and install on macOS

To build, bundle, and install/replace `Paper Shell.app` locally on macOS in one command:

```bash
./scripts/install-mac.sh
```

This bundles `--format osx` directly, ad-hoc signs the `.app` bundle, safely replaces
the existing `/Applications/Paper Shell.app`, clears quarantine flags, and touches the
bundle to update Finder and LaunchServices.

Common options:

- `./scripts/install-mac.sh -r`: Gracefully quit running app, replace, and reopen.
- `./scripts/install-mac.sh --debug -r`: Debug build for fastest local compile iteration.
- `./scripts/install-mac.sh --user`: Install to `~/Applications/Paper Shell.app`.
- `./scripts/install-mac.sh --skip-build`: Re-install an existing bundle without rebuilding.

To create only the bundle without installing:

```bash
cargo bundle --release --format osx
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

### In-app update contract

`src/updater.rs` reads the public GitHub Releases `latest` endpoint for
`RetricSu/paper-shell`; no token, update server, or signing secret is required. It compares
semantic versions and only offers newer stable versions. Package filenames above are an
interface shared with the updater: macOS uses the Universal ZIP and replaces the entire
bundle, Windows uses its ZIP, and Linux uses its tar.gz. Keep these names and archive
layouts stable. All archives and `SHA256SUMS.txt` must belong to the same release tag.

The workflow validates the tag against both Cargo manifests before building and refuses
to publish an existing tag from a different commit. Manual runs must build the intended
tag/commit with a matching version; a missing tag is created at the workflow's commit.
Versions containing a prerelease suffix are automatically marked as prereleases.
`scripts/check-update-assets.py` checks archive layouts and macOS bundle versions before
checksums and publication. Every build uses the lockfile. Never replace a published
version's packages with a different build; publish a new version instead.

Downloads stream to a temporary directory, with size limits, checksum verification,
and rejection of archive traversal and symlinks. On macOS, the bundle identifier,
bundle version, and code signature are also checked; the verified replacement's quarantine
attribute is removed before installation so ad-hoc signed updates do not repeat the
browser-download Gatekeeper prompt. Installation stages beside the current app and restores
the old bundle if the final rename fails. Other platforms
use `self-replace` for executable replacement. Installing happens after the document
save/discard/cancel flow, then eframe closes and restarts the updated executable with
the current saved file. The update affects the installation shared by all windows;
only the window initiating the update restarts, and others continue with their loaded
version until reopened.

SHA-256 checksums protect download integrity over HTTPS. They are distributed by the
same GitHub release and are not an independent publisher signature. The existing macOS
ad-hoc signing/Gatekeeper limitations still apply. Developer ID signing/notarization
and independently signed update metadata can be added separately.

For end-to-end release acceptance, use disposable copies of two packaged versions:
publish the newer version through the workflow, launch the older updater-enabled copy,
check/download/install from About, and verify the new version and reopened document.
Also exercise cancel/save failure, offline/rate-limit errors, and a read-only installation.
Run this on macOS (including an app path containing spaces), Windows, and Linux. Unit
tests cover version/asset selection, checksum failures, unsafe archives, bundle rollback,
and cancellation with unsaved writing; they do not replace a real packaged upgrade test.
An older release without this updater needs one manual upgrade to an updater-enabled build.

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
