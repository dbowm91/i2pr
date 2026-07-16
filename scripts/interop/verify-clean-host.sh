#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"

mode=""
baseline="$BUILD_ROOT/clean-host-baseline.json"
while (($#)); do
  case "$1" in
    --record-baseline) mode=record ;;
    --verify) mode=verify ;;
    --baseline)
      (($# >= 2)) || die "--baseline requires a path"
      baseline=$2
      shift
      ;;
    *) die "usage: verify-clean-host.sh --record-baseline|--verify [--baseline <path>]" ;;
  esac
  shift
done
[[ -n "$mode" ]] || die "usage: verify-clean-host.sh --record-baseline|--verify [--baseline <path>]"

for command in awk find grep ip nft pgrep python3 sha256sum sysctl; do require_command "$command"; done
ensure_target_dirs
marker="$BUILD_ROOT/clean-host-verification.json"
[[ "$mode" == "record" ]] || rm -f "$marker"

canonical_digest() {
  awk '{$1=$1; print}' | sha256sum | awk '{print $1}'
}

host_nft_digest=$(root_run nft list ruleset | canonical_digest)
host_route_digest=$(root_run ip route show table all | canonical_digest)
host_route6_digest=$(root_run ip -6 route show table all | canonical_digest)
forwarding4=$(root_run sysctl -n net.ipv4.ip_forward)
forwarding6=$(root_run sysctl -n net.ipv6.conf.all.forwarding)

failures=()
mapfile -t namespaces < <(root_run ip netns list 2>/dev/null | awk '$1 ~ /^(i2pr|ref|java|i2pd)-[A-Za-z0-9-]+$/ {print $1}')
((${#namespaces[@]} == 0)) || failures+=("residual-namespace")
mapfile -t interfaces < <(root_run ip -o link show 2>/dev/null | awk -F': ' '$2 ~ /^(i2pr-v|ref-v|jv[0-9a-f]{8}a|iv[0-9a-f]{8}b)/ {print $2}')
((${#interfaces[@]} == 0)) || failures+=("residual-veth")
processes=$(ps -eo pid=,comm= | awk '$2 ~ /^(i2pd|i2pr-interop|i2prouter|java)$/' || true)
  [[ -z "$processes" ]] || failures+=("residual-router-process")
if [[ -d "$RUNS_ROOT" ]] && find "$RUNS_ROOT" -mindepth 1 -print -quit | grep -q .; then
  failures+=("secret-bearing-run-root")
fi

if [[ -d "$INTEROP_TARGET/evidence" ]]; then
  if find "$INTEROP_TARGET/evidence" -type f \( -name 'router.identity' -o -name 'ntcp2.static.key' -o -name '*.pcap' -o -name '*.pcapng' -o -name '*.log' \) -print -quit | grep -q .; then
    failures+=("forbidden-retained-artifact")
  fi
  if find "$INTEROP_TARGET/evidence" -type f -print0 | xargs -0 grep -E -n -i -- 'BEGIN .*PRIVATE KEY|/home/|/root/|router\.identity|ntcp2\.static\.key|RouterInfo|I2NP|[0-9]{1,3}(\.[0-9]{1,3}){3}:[0-9]+' >/dev/null 2>&1; then
    failures+=("unsanitized-retained-evidence")
  fi
fi

if [[ "$mode" == "verify" ]]; then
  require_file "$baseline"
  if ! python3 - "$baseline" "$host_nft_digest" "$host_route_digest" "$host_route6_digest" "$forwarding4" "$forwarding6" <<'PY'
import json
import sys
from pathlib import Path

expected = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
actual = {
    "nft_sha256": sys.argv[2],
    "route_sha256": sys.argv[3],
    "route6_sha256": sys.argv[4],
    "forwarding4": sys.argv[5],
    "forwarding6": sys.argv[6],
}
if expected.get("schema") != 1 or expected.get("state") != actual:
    raise SystemExit("global host state changed during interop lane")
PY
  then
    failures+=("global-host-state-changed")
  fi
fi

if ((${#failures[@]} != 0)); then
  printf 'clean-host verification failed: %s\n' "${failures[*]}" >&2
  exit 1
fi

if [[ "$mode" == "record" ]]; then
  python3 - "$baseline" "$host_nft_digest" "$host_route_digest" "$host_route6_digest" "$forwarding4" "$forwarding6" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
value = {
    "schema": 1,
    "state": {
        "nft_sha256": sys.argv[2],
        "route_sha256": sys.argv[3],
        "route6_sha256": sys.argv[4],
        "forwarding4": sys.argv[5],
        "forwarding6": sys.argv[6],
    },
}
path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
path.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
path.chmod(0o600)
PY
  printf 'recorded clean-host baseline\n'
else
  python3 - "$marker" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
path.write_text(json.dumps({"schema": 1, "result": "clean"}, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
path.chmod(0o600)
PY
  printf 'clean-host verification passed\n'
fi
