#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export XDG_DATA_HOME="${XDG_DATA_HOME:-/workspace/lhr-tools/godot-data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/workspace/.cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/workspace/.config}"
export LHR_PRIVATE_DIR="${LHR_PRIVATE_DIR:-$XDG_DATA_HOME/lost-heaven-reborn/private}"
runtime_root="${LHR_TOOLS_DIR:-/workspace/lhr-tools}/runtime/rootfs"
if [[ -d "$runtime_root" ]]; then
    export PATH="$runtime_root/usr/bin:$PATH"
    export LD_LIBRARY_PATH="$runtime_root/usr/lib/x86_64-linux-gnu:$runtime_root/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    if [[ "${LHR_SOFTWARE_VULKAN:-auto}" == 1 || ( "${LHR_SOFTWARE_VULKAN:-auto}" == auto && ! -d /dev/dri ) ]]; then
        export VK_DRIVER_FILES="$runtime_root/usr/share/vulkan/icd.d/lvp_icd.json"
        export VK_ICD_FILENAMES="$VK_DRIVER_FILES"
    fi
fi
GODOT="${GODOT:-$(command -v godot || true)}"
if [[ -z "$GODOT" ]]; then
    GODOT="${LHR_TOOLS_DIR:-/workspace/lhr-tools}/godot"
fi
require_godot() {
    if [[ ! -x "$GODOT" ]]; then
        echo 'Godot unavailable. Run scripts/setup.sh or set GODOT to an executable.' >&2
        exit 1
    fi
    if [[ "$("$GODOT" --version)" != 4.6.3.stable* ]]; then
        echo 'Godot 4.6.3 stable is required.' >&2
        exit 1
    fi
}
