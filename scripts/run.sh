#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_rust
cd "$ROOT"
exec cargo run --locked -p lhr-engine -- "$@"
