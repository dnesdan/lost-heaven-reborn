#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_rust
cd "$ROOT"
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 -m unittest discover -s tests -p 'test_binary_analysis.py' -v
python3 -m unittest discover -s tests -p 'test_progress.py' -v
python3 -m unittest discover -s tests -p 'test_rust_boundaries.py' -v
python3 -m unittest discover -s tests -p 'test_capture_lab.py' -v
python3 scripts/progress.py --check
