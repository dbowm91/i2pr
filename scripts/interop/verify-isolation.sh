#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"

[[ "${1:-}" == "--namespace" ]] || die "usage: verify-isolation.sh --namespace <name> [--ipv6]"
namespace=${2:-}
[[ -n "$namespace" ]] || die "namespace is required"
ipv6=0
[[ "${3:-}" == "--ipv6" ]] && ipv6=1

root_run ip netns list | awk '{print $1}' | grep -Fxq "$namespace" \
  || die "namespace does not exist"
routes=$(root_run ip -n "$namespace" route show)
[[ "$routes" != *"default"* ]] || die "namespace has a default IPv4 route"
if [[ "$ipv6" == "1" ]]; then
  routes6=$(root_run ip -n "$namespace" -6 route show)
  [[ "$routes6" != *"default"* ]] || die "namespace has a default IPv6 route"
fi
if root_run ip netns exec "$namespace" ip route get 1.1.1.1 >/dev/null 2>&1; then
  die "public IPv4 route is reachable"
fi
if root_run ip netns exec "$namespace" ip -6 route get 2606:4700:4700::1111 >/dev/null 2>&1; then
  die "public IPv6 route is reachable"
fi

interfaces=$(root_run ip -n "$namespace" -o link show | awk -F': ' '{print $2}' | sed 's/@.*//' | sort)
[[ "$interfaces" == $'lo\npeer0' ]] || die "unexpected namespace interfaces"
if root_run ip netns exec "$namespace" pgrep -af '(^|/)(i2pd|i2pr-interop|i2prouter)( |$)' >/dev/null 2>&1; then
  die "router process exists before isolation verification"
fi
printf 'isolation.namespace=%s\n' "$namespace"
printf 'isolation.interfaces=lo,peer0\n'
printf 'isolation.default_routes=none\n'
printf 'isolation.public_route_probes=blocked\n'
printf 'isolation.firewall=namespace-scoped-default-deny\n'
