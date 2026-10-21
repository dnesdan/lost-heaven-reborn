#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
source "$SCRIPT_DIR/common.sh"
[[ "$(uname -m)" == x86_64 ]] || { echo 'Local Debian graphics bootstrap requires x86_64.' >&2; exit 1; }
source /etc/os-release
[[ "$ID" == debian && "$VERSION_ID" == 13 ]] || { echo 'Local graphics bootstrap requires Debian 13; install host-native Vulkan/X11 packages instead.' >&2; exit 1; }
TOOLS_DIR="${LHR_TOOLS_DIR:-/workspace/lhr-tools}"
APT_DIR="$TOOLS_DIR/apt"
RUNTIME_DIR="$TOOLS_DIR/runtime"
ROOTFS="$RUNTIME_DIR/rootfs"
KEYRING=/usr/share/keyrings/debian-archive-keyring.gpg

if [[ ! -r "$KEYRING" ]]; then
  echo "Missing Debian archive keyring: $KEYRING" >&2
  exit 1
fi
mkdir -p "$APT_DIR/lists/partial" "$APT_DIR/cache/archives/partial" "$ROOTFS"
printf '%s\n' "deb [signed-by=$KEYRING] https://deb.debian.org/debian trixie main" > "$APT_DIR/sources.list"

APT=(/usr/bin/apt-get -c /dev/null
  -o "Dir::Etc::sourcelist=$APT_DIR/sources.list"
  -o Dir::Etc::sourceparts=-
  -o "Dir::State::lists=$APT_DIR/lists"
  -o Dir::State::status=/var/lib/dpkg/status
  -o "Dir::Cache=$APT_DIR/cache"
  -o "Dir::Cache::archives=$APT_DIR/cache/archives"
  -o "Dir::Cache::archives::partial=$APT_DIR/cache/archives/partial"
  -o 'APT::Update::Post-Invoke::='
  -o Debug::NoLocking=1)

# APT verifies the signed Release/InRelease and package hashes against the Debian keyring.
"${APT[@]}" update
cd "$APT_DIR/cache/archives"
# Unlike install --download-only, download fetches even already-installed packages.
"${APT[@]}" download \
  libxdo3=1:3.20160805.1-5.1 \
  mesa-vulkan-drivers=25.0.7-2+deb13u1 \
  vulkan-tools=1.4.304.0+dfsg1-1 \
  xvfb=2:21.1.16-1.3+deb13u3 \
  xdotool=1:3.20160805.1-5.1

sha256sum --check <<'HASHES'
d634e25d2b50af140ee9f44bab507fcfb665f3556fd483faefdb56e85598cc12  libxdo3_1%3a3.20160805.1-5.1_amd64.deb
caf2f7d296b7522efe74e40a74bce762a3ce84c33b53b5e54e1248ac2e0a13a5  mesa-vulkan-drivers_25.0.7-2+deb13u1_amd64.deb
742d1cd78c333a94b8214e21ba455d47884dc32f110016ffe6998cf88f2a2f94  vulkan-tools_1.4.304.0+dfsg1-1_amd64.deb
98ac8681533b01020c7288fb2e9de403d586747dab48cfd58e04cb00d0c44fb1  xdotool_1%3a3.20160805.1-5.1_amd64.deb
365da2b6c93339f34337c434a9489bb6411a240fa47b9f49693d3091745f28c2  xvfb_2%3a21.1.16-1.3+deb13u3_amd64.deb
HASHES
sha256sum *.deb > "$RUNTIME_DIR/debian-packages.sha256"

for package in ./*.deb; do
  dpkg-deb -x "$package" "$ROOTFS"
done

cat > "$RUNTIME_DIR/activate.sh" <<'ACTIVATE'
#!/usr/bin/env bash
# Source this file to activate the unpacked graphics runtime.
RUNTIME_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
TOOLS_DIR="$(cd -- "$RUNTIME_SCRIPT_DIR/.." && pwd -P)"
RUNTIME_ROOT="$RUNTIME_SCRIPT_DIR/rootfs"
export PATH="$RUNTIME_ROOT/usr/bin:$PATH"
export LD_LIBRARY_PATH="$RUNTIME_ROOT/usr/lib/x86_64-linux-gnu:$RUNTIME_ROOT/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export VK_DRIVER_FILES="$RUNTIME_ROOT/usr/share/vulkan/icd.d/lvp_icd.json"
export VK_ICD_FILENAMES="$VK_DRIVER_FILES"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$TOOLS_DIR/user-data}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$TOOLS_DIR/user-data/config}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$TOOLS_DIR/user-cache}"
ACTIVATE
chmod +x "$RUNTIME_DIR/activate.sh"
