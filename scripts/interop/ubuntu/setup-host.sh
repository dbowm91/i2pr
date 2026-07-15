#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/../lib/common.sh"

require_file /etc/os-release
source /etc/os-release
[[ "${ID:-}" == "ubuntu" ]] || die "setup-host.sh refuses to run apt outside Ubuntu"
[[ "$(uname -m)" == "x86_64" || "$(uname -m)" == "amd64" ]] \
  || die "Plan 038 requires amd64/x86_64"

if [[ "$EUID" -eq 0 ]]; then
  root_prefix=()
else
  require_command sudo
  sudo -v
  root_prefix=(sudo)
fi

packages=(
  ca-certificates curl git wget xz-utils unzip zip coreutils findutils procps
  util-linux iproute2 nftables python3 python3-venv
  openjdk-17-jdk-headless ant gettext
  build-essential cmake pkg-config libboost-all-dev libssl-dev zlib1g-dev
)

export DEBIAN_FRONTEND=noninteractive
"${root_prefix[@]}" apt-get update
"${root_prefix[@]}" apt-get install --no-install-recommends -y "${packages[@]}"
"$script_dir/check-host.sh" --post-install
printf 'Plan 038 host setup complete: Ubuntu %s, amd64, declared packages installed; no router service was enabled.\n' "${VERSION_ID:-unknown}"
