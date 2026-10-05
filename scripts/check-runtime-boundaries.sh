#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-runtime/src" "$root/crates/i2pr-testkit/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in runtime/testkit source" >&2
  exit 1
fi

if grep -REn 'std::thread::sleep|thread::sleep|std::mem::forget|mem::forget' \
  "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" >/dev/null; then
  echo "wall-clock sleeps and handle-forgetting are forbidden in deterministic lanes" >&2
  exit 1
fi

spawn_matches=$(grep -REn 'tokio::spawn\(' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" || true)
if printf '%s\n' "$spawn_matches" | grep -Ev 'let .* =|push\(|JoinSet' | grep -Eq .; then
  echo "every tokio::spawn call must retain an explicit owner" >&2
  exit 1
fi

if grep -REn 'JoinHandle' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" >/dev/null; then
  echo "raw JoinHandle ownership requires a reviewed owner-specific implementation" >&2
  exit 1
fi

for manifest in "$root"/crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$manifest")")
  if [[ "$crate" != i2pr-runtime && "$crate" != i2pr-testkit ]] \
    && grep -En '^(tokio|tokio-util)[[:space:]]*=' "$manifest" >/dev/null; then
    echo "Tokio dependencies are confined to approved runtime/testkit manifests" >&2
    exit 1
  fi
done

testkit_dependents=$(grep -En 'i2pr-testkit' "$root/crates"/*/Cargo.toml || true)
if printf '%s\n' "$testkit_dependents" | grep -Ev 'crates/i2pr-testkit/Cargo.toml' | grep -Eq .; then
  echo "production crate depends on i2pr-testkit" >&2
  exit 1
fi

if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::' \
  "$root/crates/i2pr-transport/src" "$root/crates/i2pr-transport-ntcp2/src" "$root/crates/i2pr-transport-ssu2/src" >/dev/null; then
  echo "transport contract crates must not own Tokio, sockets, or filesystem I/O" >&2
  exit 1
fi

# Plan 286: i2pr-i2pcontrol is a runtime-neutral control-contract crate.
# No Tokio, sockets, filesystem ownership, async runtime, router-state
# imports, or unbounded channels. Only the daemon adapts the contract.
if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener|OpenOptions|File::|tokio::net|tokio::spawn|tokio::time|tokio::sync' \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "i2pr-i2pcontrol must remain runtime-neutral: no Tokio, sockets, listeners, tasks, timers, or filesystem I/O" >&2
  exit 1
fi

if grep -REn 'async[[:space:]]+fn|async_trait' \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "i2pr-i2pcontrol contracts must remain synchronous" >&2
  exit 1
fi

if grep -En 'i2pr-daemon|i2pr-runtime|i2pr-testkit|i2pr-netdb|i2pr-client|i2pr-service-tunnels|i2pr-tunnel|i2pr-transport' \
  "$root/crates/i2pr-i2pcontrol/Cargo.toml" >/dev/null; then
  echo "i2pr-i2pcontrol must not depend on daemon/runtime/service implementation owners" >&2
  exit 1
fi

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in i2pr-i2pcontrol source" >&2
  exit 1
fi

# No UI/frontend dependency may enter the control-contract crate.
if grep -REn 'egui|iced|tauri|dioxus|yew|leptos|slint' \
  "$root/crates/i2pr-i2pcontrol/src" "$root/crates/i2pr-i2pcontrol/Cargo.toml" >/dev/null; then
  echo "no UI/frontend dependency may enter i2pr-i2pcontrol" >&2
  exit 1
fi

# Plan 287: the test-only TLS accept-any verifier (`dangerous()`) is
# confined to integration tests. Production daemon source must never
# bypass certificate verification.
if grep -REn '\.dangerous\(\)|DangerousClientConfig|with_custom_certificate_verifier' \
  "$root/crates/i2pr-daemon/src" >/dev/null; then
  echo "dangerous TLS verifiers are forbidden in production daemon source" >&2
  exit 1
fi

if grep -REn 'async[[:space:]]+fn|async_trait|i2pr-(netdb|tunnel|client)' \
  "$root/crates/i2pr-transport" "$root/crates/i2pr-transport-ntcp2" "$root/crates/i2pr-transport-ssu2" >/dev/null; then
  echo "transport contracts must remain synchronous and independent of routing clients" >&2
  exit 1
fi

if grep -En 'i2pr-daemon|i2pr-runtime|i2pr-testkit' \
  "$root/crates/i2pr-transport/Cargo.toml" "$root/crates/i2pr-transport-ntcp2/Cargo.toml" "$root/crates/i2pr-transport-ssu2/Cargo.toml" >/dev/null; then
  echo "transport crates must not depend on runtime, daemon, or testkit" >&2
  exit 1
fi

if grep -REn 'tokio::|TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|tokio::net|tokio::spawn|tokio::time|tokio::sync' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "i2pr-service-tunnels must remain runtime-neutral: no Tokio, sockets, listeners, tasks, or timers" >&2
  exit 1
fi

# Plan 345: application protocol is a data/policy contract, never an OS or
# runtime owner. std::net address *values* are permitted; socket/DNS APIs are not.
python3 - "$root" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
source = root / "crates/i2pr-app-proto/src"
patterns = {
    "tokio": r"tokio::|async\s+fn|async_trait",
    "sockets": r"TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener|TcpSocket",
    "process": r"std::process|Command::new|\.spawn\s*\(",
    "filesystem": r"std::fs|OpenOptions|File::open|File::create",
    "resolver": r"ToSocketAddrs|to_socket_addrs|lookup_host|\b(dns|resolve_hostname)\s*\(",
    "dynamic loader": r"libloading|dlopen\s*\(|LoadLibrary",
    "sandbox backend": r"seccomp|landlock|AppContainer|Seatbelt|NetworkNamespace",
}

def violations(text):
    return [name for name, pattern in patterns.items() if re.search(pattern, text)]

# Positive control: every forbidden category must be detectable, proving these
# checks remain live if the source scan is edited later.
positive_control = """
tokio::spawn(async move {}); TcpStream::connect(addr);
std::process::Command::new(\"x\").spawn(); std::fs::read(\"x\");
name.to_socket_addrs(); libloading::Library::new(\"x\"); seccomp::apply();
"""
missing = set(patterns) - set(violations(positive_control))
if missing:
    raise SystemExit(f"app-proto boundary checker positive control missed: {sorted(missing)}")

bad = []
for path in source.rglob("*.rs"):
    for category in violations(path.read_text(encoding="utf-8")):
        bad.append(f"{path.relative_to(root)}: forbidden {category} API")
if bad:
    raise SystemExit("i2pr-app-proto runtime/OS boundary violation:\n" + "\n".join(bad))

manifest = (root / "crates/i2pr-app-proto/Cargo.toml").read_text(encoding="utf-8")
if re.search(r"^(tokio|tokio-util|rustix|libloading|nix|windows|objc)\s*=", manifest, re.M):
    raise SystemExit("i2pr-app-proto must not depend on runtime/OS backend crates")
print("i2pr-app-proto runtime/OS boundary: ok (positive control passed)")
PY

# std::net::IpAddr/SocketAddr values are allowed as validated data, but
# listener/stream ownership is forbidden in the runtime-neutral crate.
if grep -REn 'TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "i2pr-service-tunnels must not own listeners or streams; daemon owns sockets" >&2
  exit 1
fi

if grep -En 'i2pr-transport|i2pr-tunnel|i2pr-runtime|i2pr-daemon|i2pr-testkit' \
  "$root/crates/i2pr-service-tunnels/Cargo.toml" >/dev/null; then
  echo "i2pr-service-tunnels must not depend on transport/tunnel internals, runtime, daemon, or testkit" >&2
  exit 1
fi

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in service-tunnels source" >&2
  exit 1
fi

echo "runtime boundary checks passed"
