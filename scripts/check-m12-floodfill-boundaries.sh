#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
netdb="$root/crates/i2pr-netdb/src"
proto="$root/crates/i2pr-proto/src/common/lease2.rs"
service="$netdb/floodfill_service.rs"

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
rg -q 'reply_tags.len() == 1' "$service"
rg -q 'FloodfillRole::Serving' "$service"

if rg -n 'EciesSessionManager|ExistingSession|DestinationSession' "$service"; then
  echo "NetDB replies must use the one-shot supplied-key wrapper, not destination session state" >&2
  exit 1
fi

echo "M12 floodfill boundaries passed"
