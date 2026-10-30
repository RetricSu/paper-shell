#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo '==> Checking formatting'
cargo fmt --all --check

echo '==> Running Clippy'
cargo clippy --locked --all-targets --all-features -- -D warnings

echo '==> Running tests'
cargo test --locked --all-targets --all-features

echo '==> Acceptance checks passed'
