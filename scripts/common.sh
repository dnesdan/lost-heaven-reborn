#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# Native hosts can override the tool/cache location without modifying HOME.
export LHR_TOOLS_DIR="${LHR_TOOLS_DIR:-${ROOT%/*}/lhr-tools}"
export CARGO_HOME="${CARGO_HOME:-$LHR_TOOLS_DIR/rust/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$LHR_TOOLS_DIR/rust/rustup}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$LHR_TOOLS_DIR/rust/target}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL=0
export PATH="$CARGO_HOME/bin:$PATH"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$LHR_TOOLS_DIR/user-data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$LHR_TOOLS_DIR/user-cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$LHR_TOOLS_DIR/user-data/config}"
export LHR_PRIVATE_DIR="${LHR_PRIVATE_DIR:-$LHR_TOOLS_DIR/private}"
runtime_root="$LHR_TOOLS_DIR/runtime/rootfs"
if [[ "$(uname -s)" == Linux && -d "$runtime_root" ]]; then
    export PATH="$runtime_root/usr/bin:$PATH"
    export LD_LIBRARY_PATH="$runtime_root/usr/lib/x86_64-linux-gnu:$runtime_root/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    if [[ "${LHR_SOFTWARE_VULKAN:-auto}" == 1 || ( "${LHR_SOFTWARE_VULKAN:-auto}" == auto && ! -d /dev/dri ) ]]; then
        export VK_DRIVER_FILES="$runtime_root/usr/share/vulkan/icd.d/lvp_icd.json"
        export VK_ICD_FILENAMES="$VK_DRIVER_FILES"
    fi
fi
require_rust() {
    command -v cargo >/dev/null || { echo 'Rust unavailable. Run scripts/setup.sh.' >&2; return 1; }
    [[ "$(rustc --version)" == 'rustc 1.99.1 '* ]] || { echo 'Rust 1.99.1 is required; run scripts/setup.sh.' >&2; return 1; }
}
