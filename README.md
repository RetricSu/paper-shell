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
