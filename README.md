# Paper Shell

Working in progress: A simple, compact and informative text editor for everyday writing.

## Build

```bash
cargo build --release
```

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
