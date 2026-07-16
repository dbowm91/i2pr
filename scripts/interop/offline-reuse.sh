#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"
require_command unshare
require_command ip
require_command cargo

repo_root=$REPO_ROOT
root_run unshare --net --mount-proc -- bash -s -- "$repo_root" <<'EOF'
set -euo pipefail
repo_root=$1
ip link set lo up
[[ -z "$(ip route show)" ]] || { echo "offline reuse namespace has an IPv4 route" >&2; exit 1; }
[[ -z "$(ip -6 route show)" ]] || { echo "offline reuse namespace has an IPv6 route" >&2; exit 1; }
export GIT_TERMINAL_PROMPT=0
export CARGO_NET_OFFLINE=true
export CARGO_HTTP_DEBUG=false
export RUSTUP_TOOLCHAIN=1.95.0
export RUSTUP_AUTO_INSTALL=0
cd "$repo_root"
bash "$repo_root/scripts/interop/build-references.sh" --offline
cargo build --locked --package i2pr-interop
EOF

python3 "$script_dir/cache-manifest.py" --verify
printf 'offline reference cache reuse and current-checkout launcher build verified\n'
