#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
netdb="$root/crates/i2pr-netdb/src"
proto="$root/crates/i2pr-proto/src/common/lease2.rs"
service="$netdb/floodfill_service.rs"
daemon="$root/crates/i2pr-daemon/src/floodfill.rs"
runtime="$root/crates/i2pr-runtime/src/ssu2_runtime.rs"
daemon_manifest="$root/crates/i2pr-daemon/Cargo.toml"

if rg -n 'i2pr_(daemon|runtime)|tokio::|std::(net|fs)::|TcpListener|TcpStream|UdpSocket|JoinHandle|tokio::spawn' "$netdb"; then
  echo "M12 NetDB must remain runtime-neutral and must not import daemon/runtime effects" >&2
  exit 1
fi

if rg -n 'DatabaseStoreData::EncryptedLeaseSet|ValidatedEncryptedLeaseSet|ServerEncryptedLeaseSet' "$netdb"; then
  echo "EncryptedLeaseSet type 5 is deferred and cannot enter server-authority NetDB storage" >&2
  exit 1
fi

rg -q 'pub const OFFLINE_SIGNATURE: u16 = 0x0001;' "$proto"
rg -q 'pub const UNPUBLISHED: u16 = 0x0002;' "$proto"
rg -q 'pub const BLINDED_ON_PUBLICATION: u16 = 0x0004;' "$proto"
rg -q 'pub const RESERVED_MASK: u16 = 0xfff8;' "$proto"
rg -q 'MAX_DATABASE_SEARCH_REPLY_PEERS' "$service"
rg -q 'reply_tags[.]len[(][)] == 1' "$service"
rg -q 'FloodfillRole::Serving' "$service"

if rg -n '^i2pr-transport-ssu2\s*=' "$daemon_manifest"; then
  echo "daemon must consume SSU2 through i2pr-runtime, never the transport implementation" >&2
  exit 1
fi
rg -q 'lookup\.from' "$service"
if rg -n 'lookup\.from\s*!=\s*peer\.hash\(\)|lookup\.from\s*==\s*peer\.hash\(\)' "$service"; then
  echo "DatabaseLookup.from is a reply route, not authenticated peer identity" >&2
  exit 1
fi
rg -q 'DeliveryStatusMessage::new' "$service"
rg -q 'message\.reply_token' "$service"
if ! rg -q -U 'DeliveryStatusMessage::new\(\s*message\.reply_token' "$service"; then
  echo "store acknowledgements must carry the DatabaseStore reply token" >&2
  exit 1
fi
rg -q 'pub fn publication_material' "$runtime"
rg -q 'state\.reachability\.snapshot\(now\)' "$runtime"
rg -q 'pub fn install_local_router_info' "$runtime"
rg -q 'I2npBody::Garlic' "$daemon"
rg -q 'I2npBody::TunnelGateway' "$daemon"
rg -q 'FloodfillDaemonEffect::DirectFlood \{ action \}' "$daemon"
rg -q 'store_type: i2pr_proto::DatabaseStoreType::EncryptedLeaseSet' "$daemon"
rg -q 'return Err\(FloodfillDeliveryOutcome::InvalidEffect\)' "$daemon"

if rg -n 'EciesSessionManager|ExistingSession|DestinationSession' "$service"; then
  echo "NetDB replies must use the one-shot supplied-key wrapper, not destination session state" >&2
  exit 1
fi

# Direct flood actions carry no tunnel route: failed direct replication can
# never name a tunnel fallback in its type.
if awk '/pub struct DirectFloodAction \{/,/^\}/' "$netdb/replication.rs" | rg -n 'tunnel|gateway'; then
  echo "direct flood actions must not contain a tunnel route" >&2
  exit 1
fi

# Normal daemon configuration can never construct advertisement authority.
if rg -n 'FloodfillAdvertisementPermit|build_floodfill' "$root/crates/i2pr-daemon/src/config.rs"; then
  echo "normal config must not construct floodfill advertisement authority" >&2
  exit 1
fi

# Controlled floodfill construction stays inside the daemon floodfill owner:
# only floodfill.rs may reference the permit-gated builder or the
# explicit-bind qualification recording.
if rg -n --glob '!floodfill.rs' 'build_floodfill|note_explicit_bind_for_controlled_qualification' "$root/crates/i2pr-daemon/src"; then
  echo "floodfill construction authority must stay inside the daemon floodfill owner" >&2
  exit 1
fi

# Plan 283: controlled activation/withdrawal composition lives only in
# the daemon floodfill owner (callers in tests only drive it).
if rg -n --glob '!floodfill.rs' 'fn activate_controlled|fn withdraw_controlled' "$root/crates/i2pr-daemon/src"; then
  echo "controlled activation/withdrawal composition must live only in daemon floodfill.rs" >&2
  exit 1
fi
rg -q 'pub async fn activate_controlled' "$daemon"
rg -q 'pub async fn withdraw_controlled' "$daemon"

# Plan 283: the withdrawal RouterInfo builder is permit-gated.
if ! rg -q -U 'pub fn build_floodfill_withdrawal\([^)]*FloodfillAdvertisementPermit' "$netdb/local.rs"; then
  echo "withdrawal RouterInfo builder must require the advertisement permit" >&2
  exit 1
fi

# Plan 283: corroboration recording stays inside the runtime service
# owner (ssu2_runtime.rs) plus the pre-existing Plan 160 relay service
# table; the controlled peer-test driver records nothing itself.
if rg -n 'PeerTestResult \{|\.record\(' "$root/crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs"; then
  echo "controlled peer-test driver must not record corroboration itself" >&2
  exit 1
fi
if rg -rn --glob '!ssu2_runtime.rs' --glob '!ssu2_peer_relay.rs' 'reachability\.record\(' "$root/crates" --glob '!*/tests/*' | rg -v 'crates/i2pr-runtime/src/(ssu2_runtime|ssu2_peer_relay)\.rs'; then
  echo "corroboration recording must stay inside the runtime service owner" >&2
  exit 1
fi

echo "M12 floodfill boundaries passed"
