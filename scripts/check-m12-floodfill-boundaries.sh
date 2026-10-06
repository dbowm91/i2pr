#!/usr/bin/env bash
# M12 floodfill boundary guard.
#
# The type-5 rule here was written under Plan 281, which deferred EncryptedLeaseSet
# (DatabaseStore type 5) for want of a vetted Red25519 provider. Plan 281's own closure
# records that the rule "rejects `DatabaseStoreData::EncryptedLeaseSet`, `ValidatedEncryptedLeaseSet`,
# and `ServerEncryptedLeaseSet` inside `crates/i2pr-netdb/src`", and it was a correct
# description of the tree at that time.
#
# It is no longer. Plan 332 implemented type 5 as a first-class protocol structure, Plan 333
# added PSK/DH client authorization, Plan 334 mapped it onto the Proposal 170 control surface,
# and Plan 346 / ADR 0032 corrected the type-11 transcript. The script therefore began exiting
# 1 on the legitimate tree, while sitting in neither the routine floor nor `ci.yml`, so it failed
# silently. A guard that always fails is worse than an absent guard: it converts "this boundary
# is enforced" into "this boundary reports something is wrong".
#
# Erasing the rule to reach exit 0 is the failure mode Plan 364 exists to prevent. So the word
# ban is replaced by the rule the closure records actually establish. Each assertion below
# carries the record that establishes it. `--self-test` negative-tests every one of them.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
netdb="$root/crates/i2pr-netdb/src"
proto="$root/crates/i2pr-proto/src/common/lease2.rs"
service="$netdb/floodfill_service.rs"
daemon="$root/crates/i2pr-daemon/src/floodfill.rs"
runtime="$root/crates/i2pr-runtime/src/ssu2_runtime.rs"
daemon_manifest="$root/crates/i2pr-daemon/Cargo.toml"
store_message="$netdb/store_message.rs"
lookup_engine="$netdb/lookup_engine.rs"
server_store="$netdb/server_store.rs"
els2="$netdb/els2.rs"
support="$root/specs/support.toml"

fail() {
  echo "M12 floodfill boundaries: $*" >&2
  exit 1
}

if rg -n 'i2pr_(daemon|runtime)|tokio::|std::(net|fs)::|TcpListener|TcpStream|UdpSocket|JoinHandle|tokio::spawn' "$netdb"; then
  echo "M12 NetDB must remain runtime-neutral and must not import daemon/runtime effects" >&2
  exit 1
fi

# --- the type-5 boundary (corrected by Plan 364) ------------------------------------------
#
# Plan 281 forbade the *word* `EncryptedLeaseSet` in `i2pr-netdb`, which was correct while type 5
# was deferred. Plans 332/333/334 implemented it and Plan 346 / ADR 0032 corrected its transcript,
# so the honest rule is no longer "type 5 is absent". The closure records establish exactly what
# replaced it, and these are the assertions that state it.
#
# Takes its paths as arguments so `--self-test` can drive the identical gates against synthetic
# fixtures. The gates are not duplicated in the self-test; it mutates a tree and re-runs these.
check_type5_boundaries() {
  local t5_service="$1" t5_els2="$2" t5_server_store="$3" t4_store_message="$4"
  local t5_lookup_engine="$5" t5_support="$6"

# (1) Plan 332, "NetDB store/serves type 5 opaquely; type-5 is not a RouterInfo payload":
#     a type-5 record reaches server-authority storage only through the ELS2 owner's validator.
#     Without `ValidatedEncryptedLeaseSet2::validate` in the floodfill's store arm, a type-5
#     body would be admitted unvalidated -- the exact thing Plan 332 says is impossible.
rg -q -U '(?s)DatabaseStoreData::EncryptedLeaseSet\(value\) => \{.*?ValidatedEncryptedLeaseSet2::validate\(' "$t5_service" \
  || fail "a type-5 record must enter the floodfill store through ValidatedEncryptedLeaseSet2::validate, not as an unvalidated body (Plan 332)"

# (2) Plan 332 security review: "Storing unvalidated records is impossible: `Els2Store` accepts
#     only `ValidatedEncryptedLeaseSet2`." The admission type is the whole control; an insert
#     that takes the raw record type would re-open it.
rg -q 'pub fn insert\(&mut self, validated: ValidatedEncryptedLeaseSet2\) -> Els2InsertOutcome' "$t5_els2" \
  || fail "Els2Store::insert must take only a ValidatedEncryptedLeaseSet2; an unchecked insert re-opens unvalidated type-5 storage (Plan 332)"

# (3) Plan 332: the server store files type 5 under the *blinded* storage key and admits it
#     through that one validated record variant. An admission keyed on anything else, or
#     bypassing the validated variant, would file type-5 bytes under a key it never checked.
rg -q 'ValidatedNetDbRecord::EncryptedLeaseSet2\(value\) => match self\.els2\.insert\(value\)' "$t5_server_store" \
  || fail "server storage must admit type 5 only as ValidatedNetDbRecord::EncryptedLeaseSet2 into the Els2Store (Plan 332)"

# (4) Plan 332: "`store_message.rs` and `lookup_engine.rs` treat type 5 as a non-RouterInfo
#     payload, so the maintenance sweep and the unsolicited-store path neither index it as a
#     router nor feed it to the Explorer." This is the Plan-281 violation class in its surviving
#     form: **unsolicited / unreviewed type-5 publication**. Both paths must keep refusing it.
rg -q -U '(?s)DatabaseStoreData::MetaLeaseSet\(_\)\s*\|\s*DatabaseStoreData::EncryptedLeaseSet\(_\)\s*\|\s*DatabaseStoreData::Deferred \{ \.\. \} => \{\s*return Err\(UnsolicitedStoreError::UnsupportedPayload\);' "$t4_store_message" \
  || fail "an unsolicited type-5 DatabaseStore must be refused as an unsupported payload, never indexed as a RouterInfo (Plan 332)"
rg -q -U '(?s)DatabaseStoreData::MetaLeaseSet\(_\)\s*\|\s*DatabaseStoreData::EncryptedLeaseSet\(_\)\s*\|\s*DatabaseStoreData::Deferred \{ \.\. \} => \{\s*return Ok\(ResponseOutcome::Continue\);' "$t5_lookup_engine" \
  || fail "an unsolicited type-5 response must not complete a lookup or reach the Explorer (Plan 332)"

# (5) Plan 350: the type-5 hold-back in `FloodfillStoreService::handle` was removed on the
#     strength of Plan 346 / ADR 0032, so type 5 must remain reachable *and* must remain
#     validated on the way in. A reintroduced `record_type == 5` refusal would silently
#     re-defer type 5; an unvalidated admission is (1).
if sed -n '/    pub fn handle(/,/^    }/p' "$t5_service" | rg -n 'record_type == 5'; then
  fail "FloodfillStoreService::handle must not refuse record_type 5; Plan 350 removed the hold-back (Plan 350)"
fi

# (6) Plan 332 security review: "A floodfill cannot decrypt what it stores... it never derives
#     the subcredential." The floodfill service and the server store are the only two files that
#     stand between a stored type-5 record and a decrypt call, so they may not name one.
if rg -n 'decrypt_outer_ciphertext|decrypt_no_auth_outer_ciphertext|derive_els2_credentials|recover_auth_cookie|resolve_with_auth' "$t5_service" "$t5_server_store" | rg -v ':[0-9]+: *//'; then
  fail "the floodfill must store and serve type 5 opaquely; a decrypt or subcredential derivation in the store path defeats it (Plan 332)"
fi

# (7) Plan 332/350: the server answers a type-5 lookup from the validated store, under the
#     blinded storage key. Serving from anywhere else would hand out bytes no validator approved.
rg -q -U '5 => \{[^}]*encrypted_lease_set2_for_answer\(\s*BlindedStorageKey::from_hash\(key\)' "$t5_server_store" \
  || fail "a type-5 DatabaseStore answer must come from encrypted_lease_set2_for_answer under the blinded storage key (Plan 332/350)"

# (8) The `Deferred` passthrough must stay a refusal. It is the pre-Plan-332 shape, and a type-5
#     body arriving as `Deferred` would skip the validator in (1) entirely.
rg -q -U 'DatabaseStoreData::Deferred \{ \.\. \} => return Ok\(None\)' "$t5_service" \
  || fail "a Deferred payload must never become a validated floodfill record (Plan 332)"

# (9) Type 5 stays non-advertised. Plan 346 / ADR 0032 and Plan 334 both state `advertised =
#     false` unchanged, and Plan 364 corrects a guard -- it is not a capability promotion. M12
#     stays stopped at Plan 306, so `caps=f` must stay unadvertised too.
rg -q '^m12_caps_f_advertised = false$' "$t5_support" \
  || fail "m12 caps=f advertisement must remain false; M12 stays stopped at Plan 306 (Plan 364)"
local family_surface
family_surface="$(sed -n '/^id = "common\.leaseset2-family"$/,/^\[\[surface\]\]$/p' "$t5_support")"
[ -n "$family_surface" ] || fail "the common.leaseset2-family support surface is missing"
printf '%s\n' "$family_surface" | rg -q '^advertised = false$' \
  || fail "type 5 must stay non-advertised: common.leaseset2-family advertised = false (Plan 346 / ADR 0032)"
}

# --- self-test: every corrected type-5 assertion must actually be able to fail ------------
#
# A guard that has never failed is a comment. This builds a fixture tree that satisfies all
# nine type-5 gates, confirms it passes, then breaks exactly one condition at a time and
# requires the *named* gate to be the one that fires. It re-runs the identical
# `check_type5_boundaries` gates above rather than a copy of them, so a gate cannot be
# negative-tested by a weaker duplicate.
#
#   bash scripts/check-m12-floodfill-boundaries.sh --self-test
if [[ "${1:-}" == "--self-test" ]]; then
  selftest_root="$(mktemp -d "${TMPDIR:-/tmp}/i2pr-m12-type5-selftest.XXXXXX")"
  trap 'rm -rf "${selftest_root}"' EXIT

  fixture="$(mktemp -d "${TMPDIR:-/tmp}/i2pr-m12-type5-fixture.XXXXXX")"
  trap 'rm -rf "${selftest_root}" "${fixture}"' EXIT

  # A tree that satisfies every gate. Kept minimal on purpose: each mutation below removes or
  # inverts exactly one condition, so a passing clean fixture proves the gates are satisfiable
  # and each failing fixture attributes the failure to one gate.
  write_clean_fixture() {
    local dir="$1"
    mkdir -p "${dir}/netdb"
    cat > "${dir}/netdb/floodfill_service.rs" <<'RS'
impl FloodfillStoreService {
    pub fn handle(&self, record_type: u8) -> FloodfillStoreEffect {
        FloodfillStoreEffect::Accepted
    }
}
fn validate(data: &DatabaseStoreData) -> Result<Option<ValidatedNetDbRecord>, ()> {
    match data {
        DatabaseStoreData::EncryptedLeaseSet(value) => {
            let value = ValidatedEncryptedLeaseSet2::validate(
                value.clone(),
                Some(BlindedStorageKey::from_hash(key)),
                Els2ValidationContext::new(now),
            )
            .map_err(|_| ())?;
            ValidatedNetDbRecord::EncryptedLeaseSet2(value)
        }
        DatabaseStoreData::Deferred { .. } => return Ok(None),
    }
}
RS
    cat > "${dir}/netdb/els2.rs" <<'RS'
impl Els2Store {
    pub fn insert(&mut self, validated: ValidatedEncryptedLeaseSet2) -> Els2InsertOutcome {
        Els2InsertOutcome::Inserted
    }
}
RS
    cat > "${dir}/netdb/server_store.rs" <<'RS'
fn insert(&mut self, record: ValidatedNetDbRecord) -> ServerInsertOutcome {
    match record {
        ValidatedNetDbRecord::EncryptedLeaseSet2(value) => match self.els2.insert(value) {
            Els2InsertOutcome::Inserted => ServerInsertOutcome::Inserted,
            _ => ServerInsertOutcome::Rejected,
        },
    }
}
fn database_store_for_answer(&self, record_type: u8, key: Hash) -> Option<DatabaseStoreMessage> {
    match record_type {
        5 => {
            let Some(record) = self.encrypted_lease_set2_for_answer(
                BlindedStorageKey::from_hash(key),
                now_ms,
                max_age_ms,
            )? else {
                return None;
            };
            DatabaseStoreData::EncryptedLeaseSet(Box::new(record.record().clone()))
        }
        _ => return None,
    }
}
RS
    cat > "${dir}/netdb/store_message.rs" <<'RS'
fn handle_unsolicited_databasestore(store: &mut RouterInfoStore, message: &DatabaseStoreMessage) -> Result<(), UnsolicitedStoreError> {
    let compressed = match &message.data {
        DatabaseStoreData::RouterInfoCompressed(payload) => payload.as_bytes().to_vec(),
        DatabaseStoreData::LeaseSet(_)
        | DatabaseStoreData::LeaseSet2(_)
        | DatabaseStoreData::MetaLeaseSet(_)
        | DatabaseStoreData::EncryptedLeaseSet(_)
        | DatabaseStoreData::Deferred { .. } => {
            return Err(UnsolicitedStoreError::UnsupportedPayload);
        }
    };
}
RS
    cat > "${dir}/netdb/lookup_engine.rs" <<'RS'
fn on_store_response(store_message: &DatabaseStoreMessage) -> ResponseOutcome {
    let compressed = match &store_message.data {
        DatabaseStoreData::RouterInfoCompressed(payload) => payload.as_bytes().to_vec(),
        DatabaseStoreData::LeaseSet(_)
        | DatabaseStoreData::LeaseSet2(_)
        | DatabaseStoreData::MetaLeaseSet(_)
        | DatabaseStoreData::EncryptedLeaseSet(_)
        | DatabaseStoreData::Deferred { .. } => {
            return Ok(ResponseOutcome::Continue);
        }
    };
}
RS
    cat > "${dir}/support.toml" <<'TOML'
m12_caps_f_advertised = false

[[surface]]
id = "common.leaseset2-family"
protocol = "common-structures"
advertised = false

[[surface]]
id = "control.i2pcontrol-leaseset-modes"
protocol = "i2pcontrol"
advertised = false
TOML
  }

  run_gates() {
    # Runs the real gates in a subshell so a `fail` exit is observed, not propagated.
    ( check_type5_boundaries \
        "${1}/netdb/floodfill_service.rs" \
        "${1}/netdb/els2.rs" \
        "${1}/netdb/server_store.rs" \
        "${1}/netdb/store_message.rs" \
        "${1}/netdb/lookup_engine.rs" \
        "${1}/support.toml" ) 2>&1
  }

  write_clean_fixture "${fixture}"
  if ! clean_output="$(run_gates "${fixture}")"; then
    echo "M12 floodfill boundaries: self-test clean fixture failed its own gates:" >&2
    printf '%s\n' "${clean_output}" >&2
    exit 1
  fi

  # Each case: a label, the expected failure substring, and a mutation over the fixture tree.
  selftest_failures=0
  selftest_cases=0
  selftest_case() {
    local label="$1" expect="$2"
    selftest_cases=$((selftest_cases + 1))
    local broken
    broken="$(mktemp -d "${TMPDIR:-/tmp}/i2pr-m12-type5-broken.XXXXXX")"
    cp -R "${fixture}/." "${broken}/"
    # shellcheck disable=SC2086
    ( cd "${broken}" && eval "${3}" )
    local output rc
    output="$(run_gates "${broken}")" && rc=0 || rc=$?
    rm -rf "${broken}"
    if [[ "${M12_SELFTEST_VERBOSE:-0}" == "1" ]]; then
      local reason
      reason="$(printf '%s\n' "${output}" | grep -F 'M12 floodfill boundaries: ' | head -1)"
      printf '  self-test %-38s rc=%d  %s\n' "${label}" "${rc}" "${reason:-NO GATE FIRED}"
    fi
    if [[ "${rc}" -eq 0 ]]; then
      echo "self-test ${label}: MUTATED FIXTURE PASSED -- the guard does not enforce this" >&2
      selftest_failures=$((selftest_failures + 1))
    elif ! printf '%s\n' "${output}" | grep -Fq "${expect}"; then
      echo "self-test ${label}: failed for the WRONG reason; expected to see:" >&2
      echo "    ${expect}" >&2
      echo "    actual: ${output}" >&2
      selftest_failures=$((selftest_failures + 1))
    fi
  }

  # (1) type-5 admission without the ELS2 validator -- the core Plan-332 control.
  selftest_case "gate1-unvalidated-type5-admission" \
    "ValidatedEncryptedLeaseSet2::validate" \
    "sed -i 's/ValidatedEncryptedLeaseSet2::validate(/skip_validate(/' netdb/floodfill_service.rs"

  # (2) an unchecked insert that takes the raw record type.
  selftest_case "gate2-unchecked-els2-insert" \
    "must take only a ValidatedEncryptedLeaseSet2" \
    "sed -i 's/validated: ValidatedEncryptedLeaseSet2/raw: EncryptedLeaseSet2/' netdb/els2.rs"

  # (3) type 5 admitted into server storage without the validated variant.
  selftest_case "gate3-unvalidated-server-admission" \
    "ValidatedNetDbRecord::EncryptedLeaseSet2" \
    "sed -i 's/ValidatedNetDbRecord::EncryptedLeaseSet2(value)/ValidatedNetDbRecord::Deferred(value)/' netdb/server_store.rs"

  # (4a) THE PLAN-281 VIOLATION CLASS: unsolicited type-5 publication accepted as a RouterInfo.
  selftest_case "gate4a-unsolicited-type5-publication" \
    "unsolicited type-5 DatabaseStore must be refused" \
    "sed -i 's/| DatabaseStoreData::EncryptedLeaseSet(_)//' netdb/store_message.rs"

  # (4b) unsolicited type-5 response completing a lookup / reaching the Explorer.
  selftest_case "gate4b-unsolicited-type5-response" \
    "unsolicited type-5 response must not complete a lookup" \
    "sed -i 's/| DatabaseStoreData::EncryptedLeaseSet(_)//' netdb/lookup_engine.rs"

  # (5) the Plan-350 hold-back silently reintroduced.
  selftest_case "gate5-reintroduced-type5-holdback" \
    "must not refuse record_type 5" \
    "sed -i 's/FloodfillStoreEffect::Accepted/if record_type == 5 { return FloodfillStoreEffect::Unsupported; }\n        FloodfillStoreEffect::Accepted/' netdb/floodfill_service.rs"

  # (6) the floodfill deriving/decrypting what it stores.
  selftest_case "gate6-floodfill-decrypts" \
    "must store and serve type 5 opaquely" \
    "printf 'fn leak() { let _ = decrypt_outer_ciphertext(); }\n' >> netdb/server_store.rs"

  # (7) a type-5 answer served from outside the validated store.
  selftest_case "gate7-unvalidated-type5-answer" \
    "must come from encrypted_lease_set2_for_answer" \
    "sed -i 's/self.encrypted_lease_set2_for_answer(/self.anything_else(/' netdb/server_store.rs"

  # (8) the pre-Plan-332 Deferred passthrough admitting an unvalidated record.
  selftest_case "gate8-deferred-passthrough-admits" \
    "Deferred payload must never become a validated floodfill record" \
    "sed -i 's/DatabaseStoreData::Deferred { .. } => return Ok(None)/DatabaseStoreData::Deferred { payload } => return Ok(Some(ValidatedNetDbRecord::RouterInfo(payload)))/' netdb/floodfill_service.rs"

  # (9a) type 5 advertised -- a capability promotion the guard must refuse to permit.
  selftest_case "gate9a-type5-advertised" \
    "type 5 must stay non-advertised" \
    "sed -i '0,/^advertised = false$/s//advertised = true/' support.toml"

  # (9b) M12 caps=f advertised, which would also reopen Plan 306.
  selftest_case "gate9b-m12-caps-advertised" \
    "m12 caps=f advertisement must remain false" \
    "sed -i 's/^m12_caps_f_advertised = false$/m12_caps_f_advertised = true/' support.toml"

  if [[ "${selftest_failures}" -ne 0 ]]; then
    echo "M12 floodfill boundaries: self-test caught ${selftest_failures}/${selftest_cases} weakened gates" >&2
    exit 1
  fi
  echo "M12 floodfill type-5 self-test: ${selftest_cases}/${selftest_cases} negative cases each failed on their own gate"
fi

check_type5_boundaries "$service" "$els2" "$server_store" "$store_message" "$lookup_engine" "$support"

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
# explicit-bind qualification recording. `build_floodfill` matches the
# plain, withdrawal, and reachable builders by substring; the
# reachability-proof mint is named separately and gated the same way.
if rg -n --glob '!floodfill.rs' 'build_floodfill|note_explicit_bind_for_controlled_qualification|attest_confirmed_peer_test' "$root/crates/i2pr-daemon/src"; then
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

# Plan 306 / ADR 0030: the reachable (`fR`) builder requires both the
# role permit and the peer-test-confirmed reachability proof, so `R`
# can only be appended beside the decided evidence. The normal path
# carries no proof and stays `caps=f`.
if ! rg -q -U 'pub fn build_floodfill_reachable\([^)]*FloodfillAdvertisementPermit[^)]*LoopbackReachabilityProof' "$netdb/local.rs"; then
  echo "reachable RouterInfo builder must require the permit and the reachability proof" >&2
  exit 1
fi
rg -q 'reachability_proof: None' "$daemon"
if rg -n 'attest_confirmed_peer_test' "$root/crates/i2pr-daemon/src/config.rs"; then
  echo "normal config must not mint reachability proof authority" >&2
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
