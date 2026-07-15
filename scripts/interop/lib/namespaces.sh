#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

topology_token_for() {
  local run_id=$1
  printf 'synthetic-private-036\0%s\n' "$run_id" | sha256sum | awk '{print substr($1, 1, 8)}'
}

create_namespace_topology() {
  local run_id=$1
  local enable_ipv6=${2:-0}
  local i2pr_ns="i2pr-$run_id"
  local ref_ns="ref-$run_id"
  local token
  token=$(topology_token_for "$run_id")
  local i2pr_if="i2pr-v${token}a"
  local ref_if="ref-v${token}b"
  [[ "$run_id" =~ ^[a-zA-Z0-9-]{1,80}$ ]] || die "invalid namespace run id"
  [[ "${#i2pr_if}" -le 15 && "${#ref_if}" -le 15 ]] || die "generated interface name is too long"
  root_run ip netns add "$i2pr_ns" || return 1
  root_run ip netns add "$ref_ns" || { root_run ip netns del "$i2pr_ns" >/dev/null 2>&1 || true; return 1; }
  if ! root_run ip link add "$i2pr_if" type veth peer name "$ref_if" || \
     ! root_run ip link set "$i2pr_if" netns "$i2pr_ns" || \
     ! root_run ip link set "$ref_if" netns "$ref_ns"; then
    destroy_namespace_topology "$run_id"
    return 1
  fi
  for namespace in "$i2pr_ns" "$ref_ns"; do
    root_run ip -n "$namespace" link set lo up || { destroy_namespace_topology "$run_id"; return 1; }
    root_run ip -n "$namespace" link set peer0 up >/dev/null 2>&1 || true
  done
  root_run ip -n "$i2pr_ns" link set "$i2pr_if" name peer0 || { destroy_namespace_topology "$run_id"; return 1; }
  root_run ip -n "$ref_ns" link set "$ref_if" name peer0 || { destroy_namespace_topology "$run_id"; return 1; }
  root_run ip -n "$i2pr_ns" link set peer0 up
  root_run ip -n "$ref_ns" link set peer0 up
  root_run ip -n "$i2pr_ns" addr add 192.0.2.1/30 dev peer0
  root_run ip -n "$ref_ns" addr add 192.0.2.2/30 dev peer0
  if [[ "$enable_ipv6" == "1" ]]; then
    root_run ip -n "$i2pr_ns" link set dev peer0 addrgenmode none
    root_run ip -n "$ref_ns" link set dev peer0 addrgenmode none
    root_run ip -n "$i2pr_ns" -6 addr add 2001:db8:36::1/64 dev peer0
    root_run ip -n "$ref_ns" -6 addr add 2001:db8:36::2/64 dev peer0
  fi
  install_namespace_firewall "$i2pr_ns" 192.0.2.1 192.0.2.2 45680 45678 "$enable_ipv6" 2001:db8:36::1 2001:db8:36::2 || { destroy_namespace_topology "$run_id"; return 1; }
  install_namespace_firewall "$ref_ns" 192.0.2.2 192.0.2.1 45678 45680 "$enable_ipv6" 2001:db8:36::2 2001:db8:36::1 || { destroy_namespace_topology "$run_id"; return 1; }
  printf '%s\n%s\n' "$i2pr_ns" "$ref_ns"
}

install_namespace_firewall() {
  local namespace=$1 local_ipv4=$2 peer_ipv4=$3 local_port=$4 peer_port=$5 enable_ipv6=$6 local_ipv6=$7 peer_ipv6=$8
  root_run ip netns exec "$namespace" nft -f - <<EOF
flush ruleset
table inet i2pr_interop {
  chain output {
    type filter hook output priority 0; policy drop;
    oifname "lo" accept
    ct state established,related accept
    ip daddr $peer_ipv4 tcp dport $peer_port accept
$(if [[ "$enable_ipv6" == "1" ]]; then printf '    ip6 daddr %s tcp dport %s accept\n' "$peer_ipv6" "$peer_port"; fi)
  }
  chain input {
    type filter hook input priority 0; policy drop;
    iifname "lo" accept
    ct state established,related accept
    ip saddr $peer_ipv4 tcp dport $local_port accept
$(if [[ "$enable_ipv6" == "1" ]]; then printf '    ip6 saddr %s tcp dport %s accept\n' "$peer_ipv6" "$local_port"; fi)
  }
}
EOF
}

destroy_namespace_topology() {
  local run_id=$1
  local token
  token=$(topology_token_for "$run_id")
  root_run ip netns del "i2pr-$run_id" >/dev/null 2>&1 || true
  root_run ip netns del "ref-$run_id" >/dev/null 2>&1 || true
  root_run ip link del "i2pr-v${token}a" >/dev/null 2>&1 || true
  root_run ip link del "ref-v${token}b" >/dev/null 2>&1 || true
}
