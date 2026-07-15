#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

create_namespace_topology() {
  local run_id=$1
  local enable_ipv6=${2:-0}
  local i2pr_ns="i2pr-$run_id"
  local ref_ns="ref-$run_id"
  local i2pr_if="veth-i2pr-$run_id"
  local ref_if="veth-ref-$run_id"
  [[ "$run_id" =~ ^[a-zA-Z0-9-]{1,40}$ ]] || die "invalid namespace run id"
  root_run ip netns add "$i2pr_ns"
  root_run ip netns add "$ref_ns"
  root_run ip link add "$i2pr_if" type veth peer name "$ref_if"
  root_run ip link set "$i2pr_if" netns "$i2pr_ns"
  root_run ip link set "$ref_if" netns "$ref_ns"
  root_run ip -n "$i2pr_ns" link set lo up
  root_run ip -n "$ref_ns" link set lo up
  root_run ip -n "$i2pr_ns" link set "$i2pr_if" name peer0
  root_run ip -n "$ref_ns" link set "$ref_if" name peer0
  root_run ip -n "$i2pr_ns" link set peer0 up
  root_run ip -n "$ref_ns" link set peer0 up
  root_run ip -n "$i2pr_ns" addr add 192.0.2.1/30 dev peer0
  root_run ip -n "$ref_ns" addr add 192.0.2.2/30 dev peer0
  if [[ "$enable_ipv6" == "1" ]]; then
    root_run ip -n "$i2pr_ns" -6 addr add 2001:db8:36::1/64 dev peer0
    root_run ip -n "$ref_ns" -6 addr add 2001:db8:36::2/64 dev peer0
  fi
  install_namespace_firewall "$i2pr_ns" 192.0.2.2 "$enable_ipv6"
  install_namespace_firewall "$ref_ns" 192.0.2.1 "$enable_ipv6"
  printf '%s\n%s\n' "$i2pr_ns" "$ref_ns"
}

install_namespace_firewall() {
  local namespace=$1
  local peer_ipv4=$2
  local enable_ipv6=$3
  root_run ip netns exec "$namespace" nft -f - <<EOF
flush ruleset
table inet i2pr_interop {
  chain output {
    type filter hook output priority 0; policy drop;
    oifname "lo" accept
    ct state established,related accept
    ip daddr $peer_ipv4 tcp dport { 45678, 45679 } accept
    ip daddr $peer_ipv4 icmp type echo-request accept
$(if [[ "$enable_ipv6" == "1" ]]; then printf '    ip6 daddr 2001:db8:36::/64 accept\n'; fi)
  }
  chain input {
    type filter hook input priority 0; policy drop;
    iifname "lo" accept
    ct state established,related accept
    ip saddr $peer_ipv4 tcp sport { 45678, 45679 } accept
$(if [[ "$enable_ipv6" == "1" ]]; then printf '    ip6 saddr 2001:db8:36::/64 accept\n'; fi)
  }
}
EOF
}

destroy_namespace_topology() {
  local run_id=$1
  root_run ip netns del "i2pr-$run_id" >/dev/null 2>&1 || true
  root_run ip netns del "ref-$run_id" >/dev/null 2>&1 || true
}
