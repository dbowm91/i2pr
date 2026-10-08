#!/usr/bin/env bash
# Plan 381 §WP2 — the ELS2 external driver lane runner (i2pd direction).
#
# Topology (unprivileged, loopback-only, no reseed, no public network):
#
#   f  stock i2pd, floodfill=true, notransit=false, family lane-f
#      publishes an ELS2 destination from a generated tunnels.conf
#      (the reference *publisher* half: Plan 374's i2pd direction)
#   c  stock i2pd, floodfill=true, notransit=false, SAM on loopback,
#      family lane-c
#      (the reference *consumer* half, and the peer that proves the mesh can
#       carry a client tunnel to a blinded destination)
#   n  stock i2pd, floodfill=true, notransit=false, family lane-n
#      (a third peer. Plan 381: i2pr builds qualified three-hop
#       tunnels, and tunnel-peer selection needs three mutually diverse
#       candidates — distinct families and ports — so a two-router mesh
#       can never provision. All three are bootstrapped by the driver;
#       none is modified, vendored, or rebuilt.
#       All three are floodfills on purpose: i2pd only rewrites the
#       on-disk router.info (written familyless at context init, before
#       the daemon applies `family`) on a later UpdateRouterInfo event,
#       and for a quiet router the only reliable one is the
#       floodfill-gated UpdateStats on the publish timer (~10s after
#       start). A non-floodfill reference keeps its familyless file
#       forever and i2pr provisions zero candidates.)
#
# R (i2pr) is started and driven by `els2_i2pd_external.rs` in WP3. This runner
# owns the reference processes, the generated configuration and the mesh
# handshake; the driver owns i2pr and the payload rows. Neither reaches into
# the other's internals.
#
# ---------------------------------------------------------------------------
# Seven reference facts this runner encodes. All were found by executing the
# lane; each is cited at the pin 635b013a612ff47278ef02acf8580a28e10e26c5
# (i2pd 2.61.0) and each has cost a full lane cycle to learn.
#
# 1. `keys` MUST be a bare filename.
#    i2pd resolves it with `i2p::fs::DataDirPath`, which *prepends* the data
#    dir (libi2pd/FS.h:175-181), and `ClientContext::LoadPrivateKeys` then
#    opens that concatenation (libi2pd_client/ClientContext.cpp:280). An
#    absolute path becomes `<datadir>//abs/...`, fails to open, and i2pd then
#    *silently creates a brand-new key pair there*. The destination comes up
#    with a different identity than the lane believes it configured, and every
#    later address derivation is wrong for a reason that looks like a crypto
#    defect. `els2-tunnels-conf.sh` now refuses a non-bare `keys`.
#
# 2. The destination `.dat` is NOT a publication signal.
#    It is the destination's key material and i2pd writes it at startup even
#    with zero peers. A first cycle with no peers produces one. Publication is
#    `NetDb: LeaseSet2 updated` / `Publishing LeaseSet confirmed`.
#
# 3. Seed ONE direction, never both.
#    Seeding both makes f and c each find the other and start a SessionRequest
#    in the same instant; the two AEAD state machines cross and the retry fails
#    `Retry AEAD verification failed`, which reads like a crypto defect. The
#    client initiates; the floodfill learns the client from the inbound
#    session. This is the pattern `tests/integration/floodfill/run-i2pd.sh`
#    already uses, and it is why the mesh needs two cycles: identities must
#    exist before either can be seeded into the other's netDb layout.
#
# 4. A blinded address must carry a `.b32.i2p` suffix to i2pd.
#    `AddressBook::GetAddress` tests `address.find(".b32.i2p")` first and has
#    no `.b33.i2p` branch (libi2pd_client/AddressBook.cpp:454-461), so a
#    `.b33.i2p` string falls through to the full-base64 branch and SAM answers
#    `INVALID_KEY`. What makes an address blinded is its 35-byte body, not the
#    suffix, and i2pd's own console renders the blinded form as `.b32.i2p`
#    (daemon/HTTPServer.cpp:479-496). `parse_i2pd_els2_destination.py` emits
#    both spellings: `dest_b33` for i2pr's vocabulary, `dest_b33_i2pd` for the
#    reference's.
#
# 5. SAM 3.1 `SESSION CREATE` requires `DESTINATION`.
#    Empty, or anything that is neither base64 nor the literal `TRANSIENT`, is
#    answered `INVALID_KEY` (libi2pd_client/SAM.cpp:427-441). `TRANSIENT`
#    mints a fresh ephemeral destination, which is what a client connect wants.
#
# 6. `SESSION CREATE` and `STREAM CONNECT` must be on SEPARATE connections.
#    A successful create marks *that connection's* socket type `Session`
#    (SAM.cpp:449) and `ProcessStreamConnect` then refuses it with
#    `Socket already in use` (SAM.cpp:529-533). Sessions are bridge-wide, so
#    the create and the connect go on two connections.
#
# 7. The client pool must be up before a connect is attempted.
#    `LeaseSetDestination::IsReady()` is
#    `m_LeaseSet && !expired && m_Pool->GetOutboundTunnels().size() > 0`
#    (libi2pd/Destination.h:152), and `RequestDestinationWithEncryptedLeaseSet`
#    returns false — which SAM turns into `INVALID_KEY` — until then.
#
# Invariants inherited from the plan and not negotiable here: no reference
# modification, MAX_ATTEMPTS=1, missing environment fails, no `|| true` /
# `continue-on-error` / filename filtering / fake peer env, raw reference logs
# are never evidence, and no advertisement change.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LANE_DIR="${REPO_ROOT}/tests/integration/els2"
EVIDENCE_DIR="${I2PR_ELS2_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/els2-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
EXTRACTOR="${LANE_DIR}/clients/parse_i2pd_els2_destination.py"
DRIVER_TEST="els2_i2pd_external"
# Frozen before execution: no retry-until-green, no budget increase.
MAX_ATTEMPTS=1

SELF_TEST=0
case "${1:-}" in
  --self-test) SELF_TEST=1 ;;
  "") ;;
  *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac

# shellcheck source=tests/integration/els2/els2-tunnels-conf.sh
source "${LANE_DIR}/els2-tunnels-conf.sh"

# ---------------------------------------------------------------------------
# --self-test: the cheap gate. Runs no i2pd, opens no socket, touches no
# network. It proves the runner's own gates and the configuration it would
# generate, so a lane failure can never be blamed on the harness.
# ---------------------------------------------------------------------------
if [[ "${SELF_TEST}" -eq 1 ]]; then
  FAILURES=0
  ok()   { echo "  ok: $1"; }
  bad()  { echo "  FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }

  SCRATCH="$(mktemp -d -t i2pr-plan381-selftest.XXXXXX)"
  trap 'rm -rf "${SCRATCH}"' EXIT

  echo "== the frozen constants =="
  if [[ "${MAX_ATTEMPTS}" == "1" ]]; then
    ok "attempt budget is frozen at 1"
  else
    bad "attempt budget drifted to ${MAX_ATTEMPTS}"
  fi
  if [[ "${I2PD_PIN}" == "635b013a612ff47278ef02acf8580a28e10e26c5" ]] &&
     [[ "${I2PD_VERSION}" == "2.61.0" ]]; then
    ok "i2pd pin is unchanged (${I2PD_VERSION} @ ${I2PD_PIN:0:12}...)"
  else
    bad "i2pd pin drifted"
  fi

  echo "== the writer emits configurations the validator accepts =="
  for role in none psk dh; do
    case "${role}" in
      none) at="${ELS2_AUTH_NONE}"; key="" ;;
      psk)  at="${ELS2_AUTH_PSK}";  key="$(printf '%064x' 1)" ;;
      dh)   at="${ELS2_AUTH_DH}";   key="$(printf '%064x' 2)" ;;
    esac
    conf="${SCRATCH}/${role}.conf"
    if ! write_els2_tunnels_conf "${conf}" "ELS2-${role}" 18080 "${role}.dat" \
         "${ELS2_STORE_TYPE_ENCRYPTED}" "${at}" "${key}"; then
      bad "writer failed for ${role}"
      continue
    fi
    if validate_els2_tunnels_conf "${conf}" "ELS2-${role}" 18080 "${role}.dat" \
         "${ELS2_STORE_TYPE_ENCRYPTED}" "${at}" "${key}"; then
      ok "role ${role}: writer output validates"
    else
      bad "role ${role}: writer output failed its own validator"
    fi
  done

  # Fact 1, asserted rather than merely documented. An absolute path here is
  # the single most expensive mistake available in this lane.
  if write_els2_tunnels_conf "${SCRATCH}/abs.conf" "X" 18080 "/tmp/x.dat" \
       "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" 2>/dev/null; then
    bad "writer accepted an absolute keys path (fact 1 regression)"
  else
    ok "writer refuses an absolute keys path (fact 1)"
  fi

  echo "== the extractor self-test =="
  if python3 "${EXTRACTOR}" --self-test > "${SCRATCH}/extract.log" 2>&1; then
    rows="$(grep -c '^  ok:' "${SCRATCH}/extract.log" || true)"
    ok "extractor self-test holds (${rows} rows)"
  else
    bad "extractor self-test failed"
    sed -n '1,40p' "${SCRATCH}/extract.log" >&2 || true
  fi

  echo "== the extractor emits both address vocabularies =="
  # Fact 4. A lane that only produced `.b33.i2p` would be unable to drive the
  # reference at all, and the failure mode is a misleading INVALID_KEY.
  if python3 - "${EXTRACTOR}" "${SCRATCH}" <<'PY'
import importlib.util, os, sys, tempfile
spec = importlib.util.spec_from_file_location("els2x", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

# A synthetic Ed25519 identity in the documented layout, so this asserts
# behaviour rather than reading the source for the word "dest_b33_i2pd".
key = bytes(range(32))
public = bytes(mod.PUBLIC_KEY_LEN)
public += bytes(mod.SIGNING_KEY_FIELD_LEN - len(key)) + key
public += bytes([5, 0, 4])                        # extended certificate, type 5
public += (7).to_bytes(2, "big") + (4).to_bytes(2, "big")
raw = public + b"\x00" * 16                      # private tail, never read

scratch = sys.argv[2]
path = os.path.join(scratch, "synthetic.dat")
with open(path, "wb") as handle:
    handle.write(raw)
try:
    fields = mod.parse(path)
finally:
    os.unlink(path)

required = {"dest_b32", "dest_b33", "dest_b33_i2pd", "dest_hash"}
missing = required - set(fields)
if missing:
    print("missing output fields:", sorted(missing))
    raise SystemExit(1)
if not fields["dest_b33_i2pd"].endswith(".b32.i2p"):
    print("dest_b33_i2pd does not use the reference-facing suffix")
    raise SystemExit(1)
if not fields["dest_b33"].endswith(".b33.i2p"):
    print("dest_b33 does not use the i2pr-facing suffix")
    raise SystemExit(1)
PY
  then
    ok "extractor emits dest_b32, dest_b33 and dest_b33_i2pd (fact 4)"
  else
    bad "extractor does not emit the reference-facing b33 spelling (fact 4)"
  fi

  echo "== the self-test needed no reference =="
  if [[ -x "${I2PD_BIN}" ]]; then
    ok "an i2pd binary happens to be present but was not invoked"
  else
    ok "no i2pd binary present and the cheap gate still passed"
  fi

  if [[ "${FAILURES}" -ne 0 ]]; then
    echo "FAIL: ${FAILURES} Plan 381 lane self-test violation(s)" >&2
    exit 1
  fi
  echo "ok: Plan 381 ELS2 lane self-test holds"
  exit 0
fi

# ---------------------------------------------------------------------------
# The full lane.
# ---------------------------------------------------------------------------

rm -rf "${EVIDENCE_DIR}"
mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-plan381-lane.XXXXXX)"
LOG="${SCRATCH}/logs"
mkdir -p "${LOG}"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
PIDS=()

cleanup() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do wait "${pid}" 2>/dev/null || true; done
  if [[ -n "${I2PR_ELS2_KEEP_SCRATCH:-}" ]]; then
    echo "keeping lane scratch at ${SCRATCH}" >&2
    return 0
  fi
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

record() { # label status detail
  local label="$1" status="$2" detail="$3"
  detail="${detail//$'\t'/ }"; detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
}
record_guarded() { # label detail rc
  local rc="$3"
  if [[ "${rc}" -eq 0 ]]; then record "$1" passed "$2"; else record "$1" failed "$2 (exit ${rc})"; fi
}
# The poll loops below carry `*_OK` flags where 1 means "observed", which is the
# natural way to write them and the *opposite* of what record_guarded expects.
# Converting here keeps every recorded row truthful instead of inverting a
# success into a failure at the reporting boundary.
observed_rc() { # flag -> 0 when observed
  [[ "$1" -eq 1 ]] && echo 0 || echo 1
}

# ---- fail-closed pin gates, before any network use ------------------------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified source revision at the Plan 381 pin" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi
echo "==> attempt budget: ${MAX_ATTEMPTS} (frozen)"

freeport() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()'; }
F_PORT="$(freeport)"; C_PORT="$(freeport)"; N_PORT="$(freeport)"
C_SAM="$(freeport)"; FIX_PORT="$(freeport)"; F_HTTP="$(freeport)"
# R's own fixed loopback bind. The controlled profile rejects port = 0 because
# the in-band RouterInfo carries the port, so this one is allocated up front
# and handed to the driver rather than discovered.
R_PORT="${I2PR_ELS2_SSRU2_BIND_PORT:-$(freeport)}"

write_conf() { # name port floodfill samport httpport [family]
  local name="$1" port="$2" ff="$3" sam="$4" http="$5" family="${6:-}"
  mkdir -p "${SCRATCH}/${name}" "${SCRATCH}/${name}data"
  cat > "${SCRATCH}/${name}/i2pd.conf" <<EOF
daemon = false
loglevel = debug
netid = 2
family = ${family}
address4 = 127.0.0.1
host = 127.0.0.1
port = ${port}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = ${ff}
reservedrange = false
bandwidth = L
[ssu2]
enabled = true
published = true
port = ${port}
[ntcp2]
enabled = false
published = false
[http]
enabled = $( [[ "${http}" != "0" ]] && echo true || echo false )
port = ${http}
[httpproxy]
enabled = false
[socksproxy]
enabled = false
[sam]
enabled = $( [[ "${sam}" != "0" ]] && echo true || echo false )
port = ${sam}
[i2cp]
enabled = false
[i2pcontrol]
enabled = false
[upnp]
enabled = false
[reseed]
verify = false
urls =
threshold = 0
EOF
  # Plan 381: i2pd only publishes the `family` RI option when it can
  # self-sign it, which needs `<datadir>/family/<name>.key` (a P-256
  # PEM private key; Family.cpp `CreateFamilySignature`). i2pr's
  # tunnel-peer projection requires a family per candidate, so a lane
  # without keys provisions zero candidates. Keys are lane-ephemeral
  # fixture material in SCRATCH, never production secrets.
  if [[ -n "${family}" ]]; then
    mkdir -p "${SCRATCH}/${name}data/family"
    openssl ecparam -genkey -name prime256v1 -noout \
      -out "${SCRATCH}/${name}data/family/${family}.key" 2>/dev/null
  fi
}

start_i2pd() { # name
  local name="$1"
  setsid "${I2PD_BIN}" "--conf=${SCRATCH}/${name}/i2pd.conf" \
    "--tunconf=${SCRATCH}/${name}/tunnels.conf" \
    "--datadir=${SCRATCH}/${name}data" --log=file \
    "--logfile=${LOG}/${name}.log" >/dev/null 2>&1 </dev/null &
  PIDS+=("$!")
  printf '%s' "$!" > "${SCRATCH}/${name}.pid"
}

stop_one() { # name
  local name="$1" pid
  pid="$(cat "${SCRATCH}/${name}.pid" 2>/dev/null || true)"
  [[ -n "${pid}" ]] || return 0
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
}

# Readiness is a conjunction: the RouterInfo must exist AND the log must show
# the SSU2 listener. A live process with no RouterInfo has no identity and a
# RouterInfo with no listener cannot be reached.
wait_ready() { # name port attempts
  local name="$1" port="$2" n="$3" i
  for i in $(seq 1 "${n}"); do
    if [[ -f "${SCRATCH}/${name}data/router.info" ]] &&
       grep -Fq "Start listening on 127.0.0.1:${port}" "${LOG}/${name}.log" 2>/dev/null; then
      return 0
    fi
    kill -0 "$(cat "${SCRATCH}/${name}.pid" 2>/dev/null)" 2>/dev/null || break
    sleep 0.5
  done
  echo "ephemeral i2pd ${name} never published router.info / SSU2 listener" >&2
  return 1
}

ident_of() { # routerinfo
  ROUTERINFO="$1" python3 - <<'PY'
import base64, hashlib, os
from pathlib import Path
raw = Path(os.environ["ROUTERINFO"]).read_bytes()
ident = raw[:387]                      # Ed25519 identity length
digest = hashlib.sha256(ident).digest()
print(base64.b64encode(digest).decode().replace("+", "-").replace("/", "~"))
PY
}

# i2pd's HashedStorage layout: <datadir>/netDb/r<C0>/routerInfo-<ident>.dat over
# all 64 buckets, with the ident rendered in the i2p base64 alphabet.
seed_netdb() { # datadir src ident_b64
  local datadir="$1" src="$2" ident="$3" bucket
  for bucket in 0 1 2 3 4 5 6 7 8 9 a b c d e f g h i j k l m n o p q r s t u v w x y z \
                A B C D E F G H I J K L M N O P Q R S T U V W X Y Z - '~'; do
    mkdir -p "${datadir}/netDb/r${bucket}"
    cp "${src}" "${datadir}/netDb/r${bucket}/routerInfo-${ident}.dat"
  done
}

# ---- the application fixture the reference server tunnel terminates on ----
cat > "${SCRATCH}/fixture.py" <<'PY'
import socket, sys, threading
port = int(sys.argv[1]); banner = sys.argv[2].encode()
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port)); srv.listen(16)
def handle(c):
    try:
        c.sendall(banner + b"\n")
        c.settimeout(5.0)
        try: c.recv(4096)
        except Exception: pass
    finally:
        try: c.close()
        except Exception: pass
while True:
    conn, _ = srv.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
PY
python3 "${SCRATCH}/fixture.py" "${FIX_PORT}" "ELS2-LANE-FIXTURE-OK" \
  > "${LOG}/fixture.log" 2>&1 &
PIDS+=("$!")
sleep 1

echo "==> f ssu2 ${F_PORT}  c ssu2 ${C_PORT}  n ssu2 ${N_PORT}  c sam ${C_SAM}  f http ${F_HTTP}  fixture ${FIX_PORT}  R ssu2 ${R_PORT}"

# ---- the publisher auth mode (Plan 381 WP4) --------------------------------
# `I2PR_ELS2_AUTH_MODE` selects the mode the reference publisher uses:
# `none` (default), `psk`, or `dh`. Anything else fails closed before any
# process starts. For `psk`/`dh` the lane mints one fresh 32-byte client key
# per run: for PSK it is the shared secret (same value in tunnels.conf and
# in the driver's credential); for DH it is the client's private half —
# tunnels.conf carries the derived *public* key (what the publisher lists)
# while the driver holds `dh:<private-hex>` (what i2pr's credential parses).
# Key material lives in shell variables and the driver environment only. It
# is never echoed, never written to results.tsv/destinations.txt/driver
# evidence, and the run is scrubbed for it afterwards (see below).
AUTH_MODE="${I2PR_ELS2_AUTH_MODE:-none}"
case "${AUTH_MODE}" in
  none) AUTH_TYPE="${ELS2_AUTH_NONE}"; CLIENT_KEY_HEX=""; CREDENTIAL=""; EXTRACT_FLAG="" ;;
  psk)
    AUTH_TYPE="${ELS2_AUTH_PSK}"
    CLIENT_KEY_HEX="$(openssl rand -hex 32)"
    CREDENTIAL="psk:${CLIENT_KEY_HEX}"
    EXTRACT_FLAG="--per-client-auth"
    ;;
  dh)
    AUTH_TYPE="${ELS2_AUTH_DH}"
    DH_PRIV_HEX="$(openssl rand -hex 32)"
    CLIENT_KEY_HEX="$(python3 -c 'import sys; from cryptography.hazmat.primitives.asymmetric import x25519; print(x25519.X25519PrivateKey.from_private_bytes(bytes.fromhex(sys.argv[1])).public_key().public_bytes_raw().hex())' "${DH_PRIV_HEX}")"
    CREDENTIAL="dh:${DH_PRIV_HEX}"
    EXTRACT_FLAG="--per-client-auth"
    ;;
  *)
    echo "I2PR_ELS2_AUTH_MODE must be one of none|psk|dh, got '${AUTH_MODE}'" >&2
    exit 1
    ;;
esac

# ---- cycle 1: identity generation (fact 3) --------------------------------
write_conf f "${F_PORT}" true 0 "${F_HTTP}" "lane-f"
# `keys` is a bare filename (fact 1); the path is decided by the data dir.
# shellcheck disable=SC2086 -- the key argument must vanish (not empty-string)
# when the mode is NONE, or the writer refuses it as a NONE-with-key claim.
write_els2_tunnels_conf "${SCRATCH}/f/tunnels.conf" ELS2PUB "${FIX_PORT}" ELS2PUB.dat \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${AUTH_TYPE}" ${CLIENT_KEY_HEX:+${CLIENT_KEY_HEX}} \
  || { record tunnels-conf-generated failed "writer refused the ELS2 publisher section"; exit 1; }
validate_els2_tunnels_conf "${SCRATCH}/f/tunnels.conf" ELS2PUB "${FIX_PORT}" ELS2PUB.dat \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${AUTH_TYPE}" ${CLIENT_KEY_HEX:+${CLIENT_KEY_HEX}} \
  || { record tunnels-conf-generated failed "validator refused the ELS2 publisher section"; exit 1; }
record_guarded "tunnels-conf-generated" \
  "ELS2 publisher section written and validated before any process started" 0
# The evidence copy must not carry the client key: in authorized runs the
# section holds `i2cp.leaseSetClient.{psk,dh}.0 = 0:<base64>` — for PSK the
# shared secret itself. Redact the value, keeping the group line as proof of
# the mode→group pairing the writer enforces. NONE runs have no key line and
# copy verbatim.
if [[ -n "${CLIENT_KEY_HEX}" ]]; then
  sed -E 's/^(i2cp\.leaseSetClient\.[a-z]+\.0) = .*$/\1 = [redacted-client-key]/' \
    "${SCRATCH}/f/tunnels.conf" > "${EVIDENCE_DIR}/tunnels.conf"
else
  cp "${SCRATCH}/f/tunnels.conf" "${EVIDENCE_DIR}/tunnels.conf"
fi

write_conf c "${C_PORT}" true "${C_SAM}" 0 "lane-c"
: > "${SCRATCH}/c/tunnels.conf"
write_conf n "${N_PORT}" true 0 0 "lane-n"
: > "${SCRATCH}/n/tunnels.conf"

echo "==> cycle 1: identity generation"
start_i2pd f; start_i2pd c; start_i2pd n
wait_ready f "${F_PORT}" 120 || { record mesh-identity-generation failed "f never published router.info"; exit 1; }
wait_ready c "${C_PORT}" 120 || { record mesh-identity-generation failed "c never published router.info"; exit 1; }
wait_ready n "${N_PORT}" 120 || { record mesh-identity-generation failed "n never published router.info"; exit 1; }
F_RI="${SCRATCH}/fdata/router.info"; C_RI="${SCRATCH}/cdata/router.info"; N_RI="${SCRATCH}/ndata/router.info"
F_IDENT="$(ident_of "${F_RI}")"
record_guarded "mesh-identity-generation" \
  "all three reference identities created and persisted to disk" 0
stop_one f; stop_one c; stop_one n
sleep 2

# ---- cycle 2: seed one direction only, then run (fact 3) -------------------
seed_netdb "${SCRATCH}/cdata" "${F_RI}" "${F_IDENT}"
# Plan 381: n also learns f (never the reverse). Initiation stays
# one-way — n may dial f, but f does not know n, so no
# mutual-initiation pair (fact 3) exists anywhere in the mesh.
seed_netdb "${SCRATCH}/ndata" "${F_RI}" "${F_IDENT}"
echo "==> seeded f into c and n (one direction; clients initiate)"
start_i2pd f; start_i2pd c; start_i2pd n
wait_ready f "${F_PORT}" 120 || { record mesh-up failed "f not ready"; exit 1; }
wait_ready c "${C_PORT}" 120 || { record mesh-up failed "c not ready"; exit 1; }
wait_ready n "${N_PORT}" 120 || { record mesh-up failed "n not ready"; exit 1; }
N_RI="${SCRATCH}/ndata/router.info"

PEER_OK=0
for _ in $(seq 1 60); do
  if grep -aq "Session with 127.0.0.1:${F_PORT}" "${LOG}/c.log" 2>/dev/null; then PEER_OK=1; break; fi
  sleep 1
done
record_guarded "mesh-peer-session" \
  "the reference client established an SSU2 session to the reference floodfill" "$(observed_rc "${PEER_OK}")"
if [[ "${PEER_OK}" -ne 1 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  grep -aE "SSU2|AEAD|netDb" "${LOG}/c.log" | tail -20 >&2 || true
  exit 1
fi

# ---- the ELS2 publication (fact 2) -----------------------------------------
echo "==> waiting for the reference publisher to publish an ELS2 LeaseSet2"
PUB_OK=0
for _ in $(seq 1 240); do
  if grep -aqE "NetDb: LeaseSet2 updated|Publishing LeaseSet confirmed" "${LOG}/f.log" 2>/dev/null; then
    PUB_OK=1; break
  fi
  sleep 1
done
record_guarded "reference-els2-published" \
  "the reference published an encrypted LeaseSet2 for the ELS2 destination" "$(observed_rc "${PUB_OK}")"
if [[ "${PUB_OK}" -ne 1 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  grep -aE "Tunnel|LeaseSet|Destination" "${LOG}/f.log" | tail -20 >&2 || true
  exit 1
fi

# ---- derive and independently cross-check the blinded address -------------
DAT="${SCRATCH}/fdata/ELS2PUB.dat"
[[ -s "${DAT}" ]] || { record destination-derived failed "destination key file missing"; exit 1; }
EXT="${SCRATCH}/extract.out"
extract_rc=0
# shellcheck disable=SC2086
python3 "${EXTRACTOR}" ${EXTRACT_FLAG} "${DAT}" > "${EXT}" 2> "${LOG}/extract.err" || extract_rc=$?
record_guarded "destination-derived" \
  "the public destination prefix parsed and the blinded address recomputed from the signing key" "${extract_rc}"
if [[ "${extract_rc}" -ne 0 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  sed -n '1,20p' "${LOG}/extract.err" >&2 || true
  exit 1
fi
DEST_B32="$(awk -F: '$1 == "dest_b32" {print $2; exit}' "${EXT}")"
DEST_B33="$(awk -F: '$1 == "dest_b33" {print $2; exit}' "${EXT}")"
DEST_I2PD="$(awk -F: '$1 == "dest_b33_i2pd" {print $2; exit}' "${EXT}")"
# destinations.txt carries the mode but never key material: the address
# bodies are public, the keys are not.
{
  echo "auth_mode=${AUTH_MODE}"
  echo "reference_dest_b32=${DEST_B32}"
  echo "reference_dest_b33=${DEST_B33}"
  echo "reference_dest_b33_i2pd=${DEST_I2PD}"
} > "${EVIDENCE_DIR}/destinations.txt"

# Cross-check the 56-character body against the reference's own rendering.
# The suffix is a vocabulary choice (fact 4) and is not compared.
cross_rc=0
CONSOLE_B33=""
if curl -sf --max-time 5 "http://127.0.0.1:${F_HTTP}/?page=local_destination&b32=${DEST_B32}" \
     -o "${SCRATCH}/console.html" 2>/dev/null; then
  CONSOLE_B33="$(grep -oE '[a-z2-7]{56}\.b32\.i2p' "${SCRATCH}/console.html" | head -1 || true)"
fi
if [[ -n "${CONSOLE_B33}" && "${CONSOLE_B33%%.b32.i2p}" == "${DEST_B33%%.b33.i2p}" ]]; then
  cross_rc=0
else
  cross_rc=1
fi
record_guarded "blinded-address-cross-check" \
  "the derived blinded body is byte-identical to the reference's own rendering of the same destination" "${cross_rc}"
echo "==> derived b33: ${DEST_B33}"
echo "==> reference : ${CONSOLE_B33:-<not rendered>}"

# ---- control: the reference consuming its own ELS2 (fact 7) ----------------
echo "==> waiting for the client tunnel pool"
POOL_OK=0
for _ in $(seq 1 180); do
  if grep -aqE "Test of .* successful|Inbound tunnel .* has been created" "${LOG}/c.log" 2>/dev/null; then
    POOL_OK=1; break
  fi
  sleep 1
done
record_guarded "client-tunnel-pool-ready" \
  "the reference client built the outbound tunnels a b33 lookup gates on" "$(observed_rc "${POOL_OK}")"

# The reference consumer holds no credential: `RequestDestinationWithEncryptedLeaseSet`
# takes only the blinded public key (`Destination.cpp:778`) and the SAM
# session's local destination is TRANSIENT, so a stock i2pd consumer cannot
# present a PSK/DH credential at this pin. The authorized control is therefore
# genuinely unexecutable, not merely unimplemented — recorded as a documented
# skip with the reason in the row, never as a pass. Mesh-health attribution in
# authorized runs falls back to the NONE lane's proven control plus the
# same-run reference logs. WP5's checker pins this: the control row may be
# skipped only in authorized runs, and for no other reason.
if [[ "${AUTH_MODE}" == "none" ]]; then
  SAM_RC=0
  if [[ "${POOL_OK}" -eq 1 ]]; then
    SAM_OUT="$(I2PR_ELS2_SAM_PORT="${C_SAM}" I2PR_ELS2_SAM_DEST="${DEST_I2PD}" \
      I2PR_ELS2_SAM_EXPECT="ELS2-LANE-FIXTURE-OK" timeout 300 python3 "${LANE_DIR}/clients/sam_b33_connect.py" 2>&1)" || SAM_RC=$?
    echo "${SAM_OUT}"
    if [[ "${SAM_RC}" -ne 0 ]]; then
      SAM_RC=1
    fi
  else
    SAM_RC=1
  fi
  # This is a *control*, not an acceptance row: it proves the mesh carries a
  # blinded lookup and a payload between two stock reference routers, so a later
  # i2pr failure is attributable rather than ambiguous. It is never promoted to a
  # matrix row and never counted as evidence about i2pr.
  record_guarded control-reference-els2-roundtrip \
    "stock i2pd consumed a stock i2pd ELS2 destination and the fixture banner came back; this is a mesh control, not an i2pr acceptance row" \
    "${SAM_RC}"
else
  record control-reference-els2-roundtrip skipped \
    "control-skip: authorized mode ${AUTH_MODE} has no reference-consumer credential path (Destination.cpp:778 takes only the blinded key); mesh health is the NONE lane's proven control"
fi

# ---- mesh shape precondition: three family-published references -----------
# Plan 381: i2pr builds qualified three-hop tunnels, whose peer
# projection requires a `family` option per candidate. i2pd only
# publishes it when `<datadir>/family/<name>.key` exists at startup,
# AND only rewrites the on-disk router.info (written familyless at
# context init) on a later UpdateRouterInfo event — for these quiet
# routers that is the floodfill-gated UpdateStats on the publish
# timer (~10s after start), which is why all three are floodfills.
# Poll boundedly rather than asserting once; a mesh without it fails
# closed as a harness defect, never as an i2pr row.
FAM_OK=1
for _i in $(seq 1 36); do
  FAM_OK=1
  for _ri in "${F_RI}" "${C_RI}" "${N_RI}"; do
    if ! python3 -c 'import sys; sys.exit(0 if b"family" in open(sys.argv[1],"rb").read() else 1)' "${_ri}"; then
      FAM_OK=0
    fi
  done
  if [[ "${FAM_OK}" -eq 1 ]]; then break; fi
  sleep 5
done
if [[ "${FAM_OK}" -ne 1 ]]; then
  for _ri in "${F_RI}" "${C_RI}" "${N_RI}"; do
    python3 -c 'import sys; sys.exit(0 if b"family" in open(sys.argv[1],"rb").read() else 1)' "${_ri}" \
      || echo "reference ${_ri} publishes no family option; check its family key" >&2
  done
fi
record_guarded "reference-families-published" \
  "all three reference RouterInfos carry a family option for peer selection" "$(observed_rc "${FAM_OK}")"
if [[ "${FAM_OK}" -ne 1 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi

# ---- hand off to the WP3 driver -------------------------------------------
DRIVER="crates/i2pr-daemon/tests/${DRIVER_TEST}.rs"
if [[ ! -f "${REPO_ROOT}/${DRIVER}" ]]; then
  echo "WP3 driver ${DRIVER} does not exist yet; the reference mesh is up but there is" >&2
  echo "nothing to drive it with. This lane fails closed rather than reporting a" >&2
  echo "partial matrix as a pass." >&2
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi

driver_rc=0
# The credential travels by environment only, alongside the mode that selects
# it. In `none` runs both are empty and the driver takes the no-credential
# branch exactly as before.
I2PR_ELS2_REFERENCE_ROUTER_INFO="${F_RI}" \
I2PR_ELS2_REFERENCE_ENDPOINT="127.0.0.1:${F_PORT}" \
I2PR_ELS2_PEER2_ROUTER_INFO="${C_RI}" \
I2PR_ELS2_PEER2_ENDPOINT="127.0.0.1:${C_PORT}" \
I2PR_ELS2_PEER3_ROUTER_INFO="${N_RI}" \
I2PR_ELS2_PEER3_ENDPOINT="127.0.0.1:${N_PORT}" \
I2PR_ELS2_REFERENCE_DEST_B32="${DEST_B32}" \
I2PR_ELS2_REFERENCE_DEST_B33="${DEST_B33}" \
I2PR_ELS2_REFERENCE_DEST_B33_I2PD="${DEST_I2PD}" \
I2PR_ELS2_REFERENCE_CONSUMER_SAM_PORT="${C_SAM}" \
I2PR_ELS2_REFERENCE_CONSUMER_ENDPOINT="127.0.0.1:${C_PORT}" \
I2PR_ELS2_AUTH_MODE="${AUTH_MODE}" \
I2PR_ELS2_CLIENT_CREDENTIAL="${CREDENTIAL}" \
I2PR_ELS2_NEGATIVE="${I2PR_ELS2_NEGATIVE:-}" \
I2PR_ELS2_SSU2_BIND="127.0.0.1:${R_PORT}" \
I2PR_ELS2_EVIDENCE_DIR="${EVIDENCE_DIR}" \
timeout --foreground 1800 cargo test --locked -p i2pr-daemon \
  --test "${DRIVER_TEST}" -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/driver.log" 2>&1 || driver_rc=$?
record_guarded "i2pr-rows" "the WP3 driver ran the i2pr ELS2 rows against the live reference mesh (auth ${AUTH_MODE}, negative ${I2PR_ELS2_NEGATIVE:-none})" "${driver_rc}"
if [[ "${driver_rc}" -ne 0 ]]; then
  sed -n '1,80p' "${EVIDENCE_DIR}/driver.log" >&2 || true
fi

# ---- key-material scrub: the evidence must not contain the client key ------
# The credential is random per run, so searching for it proves absence rather
# than asserting on a fixed string. Either hex form (tunnels.conf base64 is a
# different encoding of the same bytes — check both) anywhere under the
# evidence dir fails the lane even if every row passed.
if [[ -n "${CLIENT_KEY_HEX}" ]]; then
  CLIENT_KEY_B64="$(printf '%s' "${CLIENT_KEY_HEX}" | xxd -r -p | base64 -w0)"
  CLIENT_KEY_I2PD="$(printf '%s' "${CLIENT_KEY_B64}" | tr -- '+/' '-~')"
  if grep -rqF "${CLIENT_KEY_HEX}" "${EVIDENCE_DIR}" \
      || grep -rqF "${CLIENT_KEY_B64}" "${EVIDENCE_DIR}" \
      || grep -rqF "${CLIENT_KEY_I2PD}" "${EVIDENCE_DIR}"; then
    echo "lane evidence contains client key material; failing closed" >&2
    record key-material-scrub failed "client key bytes found under ${EVIDENCE_DIR}"
  else
    record key-material-scrub passed "no client key bytes under ${EVIDENCE_DIR} (auth ${AUTH_MODE})"
  fi
  if [[ -n "${DH_PRIV_HEX:-}" && "${DH_PRIV_HEX}" != "${CLIENT_KEY_HEX}" ]]; then
    DH_PRIV_B64="$(printf '%s' "${DH_PRIV_HEX}" | xxd -r -p | base64 -w0)"
    DH_PRIV_I2PD="$(printf '%s' "${DH_PRIV_B64}" | tr -- '+/' '-~')"
    if grep -rqF "${DH_PRIV_HEX}" "${EVIDENCE_DIR}" \
        || grep -rqF "${DH_PRIV_B64}" "${EVIDENCE_DIR}" \
        || grep -rqF "${DH_PRIV_I2PD}" "${EVIDENCE_DIR}"; then
      echo "lane evidence contains DH private key material; failing closed" >&2
      record key-material-scrub-dh failed "DH private key bytes found under ${EVIDENCE_DIR}"
    else
      record key-material-scrub-dh passed "no DH private key bytes under ${EVIDENCE_DIR}"
    fi
  fi
fi
# The scrub rows above must reach the evidence copy: re-copy after them, so
# the evidence `results.tsv` is the same file the gate below decides on.
cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"

# A `skipped` row is accepted only with the frozen `control-skip:` marker in
# its detail — the authorized-mode control absence documented above. Any other
# non-passed row, including a skip without the marker, fails the lane. WP5's
# checker pins the admissible set further (control row, authorized runs only).
if awk -F'\t' '$2 != "passed" && !($2 == "skipped" && $3 ~ /^control-skip:/) { found = 1 } END { exit found ? 0 : 1 }' "${RESULTS_FILE}"; then
  echo "Plan 381 ELS2 lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 381 ELS2 lane passed (auth ${AUTH_MODE}); sanitized evidence: ${EVIDENCE_DIR}"