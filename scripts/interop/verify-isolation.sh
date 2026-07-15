#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"

namespace=""; local_ipv4=""; peer_ipv4=""; local_ipv6=""; peer_ipv6=""
local_port=""; peer_port=""; ruleset_digest=""; ipv6=0
while (($#)); do
  case "$1" in
    --namespace) namespace=$2; shift ;;
    --local-ipv4) local_ipv4=$2; shift ;;
    --peer-ipv4) peer_ipv4=$2; shift ;;
    --local-ipv6) local_ipv6=$2; shift ;;
    --peer-ipv6) peer_ipv6=$2; shift ;;
    --local-port) local_port=$2; shift ;;
    --peer-port) peer_port=$2; shift ;;
    --ruleset-digest) ruleset_digest=$2; shift ;;
    --ipv6) ipv6=1 ;;
    *) die "unknown isolation option: $1" ;;
  esac
  shift
done
[[ "$namespace" =~ ^(i2pr|ref)-[A-Za-z0-9-]+$ ]] || die "invalid namespace"
[[ "$local_ipv4" && "$peer_ipv4" && "$local_port" && "$peer_port" && "$ruleset_digest" ]] \
  || die "all topology verification values are required"
[[ "$local_port" =~ ^[0-9]+$ && "$peer_port" =~ ^[0-9]+$ ]] || die "invalid topology ports"

root_run ip netns list | awk '{print $1}' | grep -Fxq "$namespace" || die "namespace does not exist"
interfaces=$(root_run ip -n "$namespace" -o link show | awk -F': ' '{print $2}' | sed 's/@.*//' | sort)
[[ "$interfaces" == $'lo\npeer0' ]] || die "unexpected namespace interfaces"
actual_ipv4=$(root_run ip -n "$namespace" -4 -o addr show dev peer0 scope global | awk '{print $4}')
[[ "$actual_ipv4" == "$local_ipv4/30" ]] || die "unexpected local IPv4 address"
routes=$(root_run ip -n "$namespace" route show)
grep -Fq "192.0.2.0/30 dev peer0" <<<"$routes" || die "unexpected IPv4 peer route"
[[ "$(grep -Ec '^192\.0\.2\.0/30 dev peer0' <<<"$routes")" == "1" ]] || die "unexpected number of IPv4 routes"
[[ "$routes" != *"default"* ]] || die "namespace has a default IPv4 route"
if [[ "$ipv6" == "1" ]]; then
  actual_ipv6=$(root_run ip -n "$namespace" -6 -o addr show dev peer0 scope global | awk '{print $4}')
  [[ "$actual_ipv6" == "$local_ipv6/64" ]] || die "unexpected local IPv6 address"
  routes6=$(root_run ip -n "$namespace" -6 route show)
  grep -Fq "2001:db8:36::/64 dev peer0" <<<"$routes6" || die "unexpected IPv6 peer route"
  [[ "$(grep -Ec '^2001:db8:36::/64 dev peer0' <<<"$routes6")" == "1" ]] || die "unexpected number of IPv6 routes"
  [[ "$routes6" != *"default"* ]] || die "namespace has a default IPv6 route"
fi
if root_run ip netns exec "$namespace" ip route get 1.1.1.1 >/dev/null 2>&1; then die "public IPv4 route is reachable"; fi
if root_run ip netns exec "$namespace" ip -6 route get 2606:4700:4700::1111 >/dev/null 2>&1; then die "public IPv6 route is reachable"; fi
if root_run ip netns exec "$namespace" sysctl -n net.ipv4.ip_forward | grep -vq '^0$'; then die "IPv4 forwarding is enabled in namespace"; fi
if root_run ip netns exec "$namespace" sysctl -n net.ipv6.conf.all.forwarding | grep -vq '^0$'; then die "IPv6 forwarding is enabled in namespace"; fi
if root_run ip -4 addr show | grep -Eq '192\.0\.2\.[12]'; then die "host namespace retains scenario IPv4 endpoint"; fi
if root_run ip -6 addr show | grep -Eq '2001:db8:36::[12]'; then die "host namespace retains scenario IPv6 endpoint"; fi

rules=$(root_run ip netns exec "$namespace" nft list ruleset)
canonical=$(awk '{$1=$1; print}' <<<"$rules" | sha256sum | awk '{print $1}')
[[ "$canonical" == "$ruleset_digest" ]] || die "nftables ruleset changed after installation"
grep -Fq "ip daddr $peer_ipv4 tcp dport $peer_port accept" <<<"$rules" || die "missing narrow IPv4 output rule"
grep -Fq "ip saddr $peer_ipv4 tcp dport $local_port accept" <<<"$rules" || die "missing narrow IPv4 input rule"
if [[ "$ipv6" == "1" ]]; then
  grep -Fq "ip6 daddr $peer_ipv6 tcp dport $peer_port accept" <<<"$rules" || die "missing narrow IPv6 output rule"
  grep -Fq "ip6 saddr $peer_ipv6 tcp dport $local_port accept" <<<"$rules" || die "missing narrow IPv6 input rule"
fi
if root_run ip netns exec "$namespace" pgrep -af '(^|/)(i2pd|i2pr-interop|i2prouter)( |$)' >/dev/null 2>&1; then
  die "router process exists before isolation verification"
fi
printf 'isolation.namespace=%s\n' "$namespace"
printf 'isolation.interfaces=lo,peer0\n'
printf 'isolation.default_routes=none\n'
printf 'isolation.public_route_probes=blocked\n'
printf 'isolation.forwarding=disabled\n'
printf 'isolation.firewall=exact-peer-address-and-port\n'
printf 'isolation.ruleset_sha256=%s\n' "$ruleset_digest"
