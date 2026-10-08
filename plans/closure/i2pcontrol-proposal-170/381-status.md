# Plan 381 — status

Status: **in progress** — WP1 and WP2 complete; WP3 payload row now passes (NONE mode, i2pd→i2pr); WP4 matrix and WP5 open.

Subsystem: Proposal 170 / I2PControl, and Red25519 + ELS2.

Predecessor: **Plan 380 passed** (`380-status.md`). Plan 380 is this plan's
closed hard dependency and it is satisfied: i2pr can present a client credential.

Plan: `plans/implementation/i2pcontrol-proposal-170/381-els2-live-external-driver-lane.md`

This record is deliberately **not** a pass. It closes the cheap gate and the
lane runner, and retires **three** of the plan's six stop conditions by
execution. No row about i2pr exists yet and nothing below is offered in their
place.

## What this plan inherited, and what was wrong with it

Plan 381 was registered one commit ago (`b76d02f9`) on an executed substrate
audit. Executing its first work package found that **two of its recorded
"established" facts were wrong**, both in the same direction: they had been
derived from `strings` output of the pinned binary and then hardened into prose.
This is recorded because it is the reason the rest of the plan's facts were
re-checked rather than trusted.

### Stop condition 1 — resolved: the colon, not the key name

The plan and `tests/integration/els2/reference-freeze.md` recorded that the
2.61.0 binary contains no indexed `i2cp.leaseSetClient.psk.nnn` group, only a
single `i2cp.leaseSetClient.psk`, and corrected the freeze document to say so.

**That correction was wrong and is corrected forward.** `strings` could not have
shown otherwise: the implementation never contains an indexed spelling as a
literal, because the reader is a **prefix match**
(`libi2pd_client/ClientContext.cpp:465-473`, `libi2pd/Destination.cpp:1607-1622`).
`Destination.h:82` says so in a comment: `// group of i2cp.leaseSetClient.psk.nnn`.
Both the bare and the indexed spelling are accepted. The freeze document's
original `[.nnn]` claim was right.

The real constraint is worse than a wrong key name and is now what the lane
writer enforces. `ReadAuthKey` (`libi2pd/Destination.cpp:1612-1620`) keeps only
the bytes after the first `:` and **silently drops a colon-less value with no
diagnostic**. Verified by running the pinned binary, one `tunnels.conf` variant
per spelling, reading `Destination: <N> auth keys read` from the log:

| `tunnels.conf` line | i2pd 2.61.0 |
|---|---|
| `i2cp.leaseSetClient.psk = <base64>` | **no auth keys read** |
| `i2cp.leaseSetClient.psk = 0:<base64>` | 1 auth key read |
| `i2cp.leaseSetClient.psk.0 = <base64>` | **no auth keys read** |
| `i2cp.leaseSetClient.psk.0 = 0:<base64>` | 1 auth key read |
| *(no client key line)* | no auth keys read |
| `i2cp.leaseSetAuthType = 0`, no key line | no auth-key line at all |

So the answer to the plan's stop condition is the good one: the PSK and DH keys
**can** be set from `tunnels.conf`. No SAM `SESSION CREATE` redesign is needed
and the publisher role keeps the shape the plan assumed.

A second mode-coupling trap was found at the same time and is also enforced now:
`Destination.cpp:1088-1091` selects the key group **by auth type**, so a PSK key
paired with `i2cp.leaseSetAuthType = 1` reads the `.dh` group and finds nothing.
The modes are `NONE = 0`, `DH = 1`, `PSK = 2` (`libi2pd/LeaseSet.h:290-292`).

### Stop condition 3 — retired: the pinned source tree is readable

The plan recorded that the pinned source tree is not retained in the cache (only
`bin/`, `logs/`, build metadata) and that a source-level claim would need a
re-fetch. **Not so**: the tree is present on this host at
`/tmp/i2pr-i2pd`, and `git rev-parse HEAD` there is exactly
`635b013a612ff47278ef02acf8580a28e10e26c5` with `git describe` → `2.61.0`. Every
source citation in this record and in the freeze document is therefore
source-level, not `strings`-level, and no re-fetch was needed.

The caveat is **not** that the source is unavailable; it is that it is not
*retained* — it lives in a scratch directory, not under the repo or the cache. A
future executor on a fresh host still cannot reproduce these citations. That is
a real reproducibility gap and it is recorded as a finding, not closed.

## WP1 — the cheap gate (no i2pd process)

### WP1.1 — the `tunnels.conf` writer and validator

`tests/integration/els2/els2-tunnels-conf.sh` (new, 259 lines) and
`tests/integration/els2/test-tunnels-conf.sh` (new, 36 rows, all passing).

Every rule is forced by the reference and source-cited in the file header: the
closed auth vocabulary, `leaseSetType = 5`, the mode→group pairing, the prefix
match, **the mandatory colon**, and the 32-byte `Tag<32>` key.

The rows that matter most are the two that exist because of the silent-drop
trap: the writer is asserted never to emit a colon-less value in any mode, and
the validator is asserted to refuse one that has been tampered into a file.

**A departure from the Plan 215 convention, stated here.** Plan 215's
`test-plan215-tunnels-conf.sh` duplicates the writer inline, and its comment
says the duplication is so the contract "can be re-verified independently of the
expensive external lane". This plan sources one shared library from both the
test and the runner instead. The stated reason does not apply — a sourced shell
library runs no i2pd, touches no network, and needs no binary, so it is already
independent of the lane — while what the duplication actually bought was two
copies that could disagree, with the comment acknowledging the divergence
without preventing it. Sharing one copy removes that failure mode rather than
documenting it. This is a considered deviation from precedent, not an
oversight.

### WP1.2 — the b33 destination extractor

`tests/integration/els2/clients/parse_i2pd_els2_destination.py` (new, 529
lines, 49 self-test rows, all passing).

The plan's WP1.2 asked to "derive the published `.b33` from the i2pd `.dat`
**and independently recompute** the address↔hash relationship". Executing that
found the premise to be a **category error**: there is no address↔hash
relationship for a b33. A b33 is not a hash of anything. It is a 35-byte
encoding of the destination's **signing public key** plus two signature-type
bytes and a per-client-auth flag, with a CRC32 of the key XOR-masked into the
three header bytes (`libi2pd/Blinding.cpp:200-214`, decoder at `:157-198`).

The extractor therefore does what actually has teeth:

1. derives the b33 from the `.dat`'s public bytes;
2. decodes it back through the reference's decoder;
3. asserts the recovered signing key **equals the `.dat`'s signing key** —
   reached by a different code path than the derivation that produced it, so a
   wrong derivation cannot pass; and
4. asserts re-encoding reproduces the b33 exactly.

Two honest limitations, both recorded rather than papered over:

- **The CRC is a mask, not a checksum.** The reference decoder XORs it back but
  never requires the result to be zero, so a b33 carries no error-detecting
  integrity property. The self-test does not pretend otherwise; the round-trip
  and key-match are the real checks.
- **The b33 is only recomputable for 32-byte signing keys.** `Blinding.cpp:202`
  returns `""` for anything longer, so P-384 and P-521 destinations have no b33
  at this pin. The module **refuses** rather than guessing a key length, and the
  self-test pins that refusal.

**Live verification against the pinned reference: one successful run, and a
reproduction caveat.** Running an i2pd 2.61.0 ELS2 destination
(`i2cp.leaseSetType = 5`, `authType = 0`) and comparing the value the extractor
derives from the `.dat` against the b33 i2pd publishes on its console produced
an exact **MATCH**. Two further attempts did not reproduce it, and the reason is
understood rather than mysterious: the console renders the b33 only inside
`if (dest->IsEncryptedLeaseSet())` (`daemon/HTTPServer.cpp:489`), which requires
the destination to hold an encrypted LeaseSet — a mesh-dependent condition. The
cheap gate therefore does **not** depend on this comparison; it depends on the
self-test, the round-trip, and the b32 cross-check below. The one MATCH is
recorded as evidence that the derivation is not merely a reading of the source.

**Independent cross-check that did reproduce.** The extractor's b32 agrees
byte-for-byte with `tests/integration/service-tunnels/clients/parse_i2pd_destination.py`,
a separate implementation written for a different lane. Two independent
derivations of the same identity hash agreeing is a real check, and it ran
against a live reference-generated `.dat`.

### WP1.3 — deferred, and why

The static evidence checker is **not** written, deliberately. Its subject is the
lane: `REQUIRED_ROWS` are the WP4 rows, which live in
`crates/i2pr-daemon/tests/els2_i2pd_external.rs`, which does not exist until
WP3. A checker written now would either fail (its required rows are absent) or
be weakened to pass over a lane that is not there — and the plan's own stop
conditions call a half-run matrix worse than an honest block. It lands with
WP5, over the rows that exist.

### WP1.4 — the R-side lane consumer

`crates/i2pr-daemon/tests/i2pcontrol_els2_lane_consumer.rs` (new, 6 rows, all
passing). Every row goes through a real TLS listener and a real JSON-RPC
`TunnelManager` call; no private API is touched.

The rows establish the three constraints the lane cannot choose its way around:
a `.b33` target, `DelayOpen` (because `i2pr-service-tunnels/src/config.rs:1160-1176`
refuses a `.b33` target without it and the TOML path hardcodes
`delay_open: false`, making I2PControl the only creation route — Plan 351 Gate
2, unchanged), and a non-floodfill composition.

**One row was written differently than planned, because the first version was
weak.** `plan381_a_b33_consumer_without_delay_open_is_refused` asserted only
that the create was refused, on the reasoning that pinning the message would
prove more. Executing it showed the wire does not carry that message: the daemon
collapses `ContradictoryOptions` to `"spec options contradict the kind"`, so a
text assertion would have been asserting on a string the control surface
deliberately omits. A lone refusal would also have passed if the request were
malformed for an unrelated reason. The row was rewritten as a **toggle** — the
identical request twice with only `DelayOpen` changed — which makes the refusal
attributable to the flag without depending on the message. This is a strictly
stronger row than the one the plan asked for.

### WP1.5 — the workflow

`.github/workflows/els2-external.yml` (new). `workflow_dispatch`, Ubuntu,
20 minutes. It runs what exists: the tunnels.conf contract, the extractor
self-test, the Plan 380 consumer-path guard, and the R-side consumer rows. It is
named and commented as running the **cheap gate only**, and says where the live
rows land when `run-i2pd-els2.sh` does. `check-workflow-validity.py` parses 11
workflow files.

The `i2pd` build is deliberately absent: nothing in WP1 needs the reference
binary, so adding a 40-minute reference build would be cost without evidence.

## Two bugs this plan found in itself

Recorded because both were caught by executing rather than reading, and both
would have been silent.

1. **Extended-certificate byte order.** The extractor first read the signing key
   type at extended offset **+2** and crypto at 0. `libi2pd/Identity.cpp:377`
   reads the signing type at offset 0 and `:390` the crypto type at +2. The
   wrong order yields a *well-formed but meaningless* type number — it read
   `DSA_SHA1` for an Ed25519 identity and refused. There is no error to notice;
   only comparing against the reference finds it. The module now pins the order
   with a named constant and a comment explaining that reading it the other way
   round produces a plausible wrong answer.
2. **The b33 length.** A b33 body is **56** base32 characters, not 52. Three
   header bytes plus a 32-byte key is 35 bytes, and 35 is a multiple of 5, so
   base32 emits 56 with no padding at all. This was confirmed independently from
   both ends: i2pr's own `BlindedAddress::to_text()` also produces 56
   (asserted in WP1.4), matching the i2pd encoding derived from source.

A third was a **test defect, not a code defect**, recorded because the shape is
recurring: the extractor's malformed-input row asserted that a truncated file is
refused while the fixture it built actually declared `cert_len = 0` and was
valid. The fixture was wrong. A negative test whose fixture is valid tests
nothing, and this is the second time in this plan's history that a "refuses"
row was found to be non-refusing (the other being the DelayOpen row above).

## Findings, attributed forward

None of these are fixed by this plan; each is recorded rather than absorbed.

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | The pinned i2pd source tree is **not retained** — it exists at `/tmp/i2pr-i2pd` on this host only, so every source citation in this record and the freeze document is **not reproducible on a fresh host** without a re-fetch | medium | Needs its own plan-of-record to pin a source-retention policy in `scripts/interop/` |
| 2 | `scripts/check-els2-type11-transcript-boundary.sh` is in the `AGENTS.md` floor but has **no `ci.yml` line** | medium | Recorded at Plan 381 registration; a boundary-condition fix, not ELS2 work |
| 3 | `tests/integration/floodfill/run-i2pd.sh` is in **no workflow at all** | low | Same as above; noted so the new `els2-external.yml` is not mistaken for covering it |
| 4 | i2pd's daemon-mode web console is the only surface that renders a b33, and it renders it only once the destination holds an encrypted LeaseSet | low | Not a defect; it is why WP1 derives the b33 and does not scrape HTML |
| 5 | `i2pr run`'s error projection collapses every `ContradictoryOptions` refusal to `"spec options contradict the kind"` | low | Operator-facing only; the specific reason is not actionable on the wire. Would need a user decision on whether to project reasons |
| 6 | **Plan 380 widened `i2pr-service-tunnels`'s public API and never re-snapshotted it**, so `scripts/check-portable-service-tunnel-api.py` has been failing on this branch since `4bbc1ea9` | medium | **Fixed in WP1, attributed forward** — see below |
| 7 | **The WP1 `tunnels.conf` validator rubber-stamped a configuration i2pd cannot use.** Every check it made was of the form "the file says X and the caller said X", so a caller passing an absolute `keys` path or a swapped port argument got a clean pass. Found when the first WP2 probe did exactly that | medium | **Fixed in WP2, attributed forward** — see below |
| 8 | i2pd's SAM rejects `SESSION CREATE` followed by `STREAM CONNECT` on one connection with `Socket already in use`, which reads as a port collision | low | Not a defect to fix; it is reference behaviour. Documented as fact 6 and driven around deliberately in `sam_b33_connect.py` |
| 9 | The reference-to-reference control row is in `results.tsv` alongside real rows and is one label away from being counted as an acceptance row | low | Mitigated by recording it with the word "control" in its detail and by this record naming it; the evidence checker in WP5 must refuse to promote it |

### Finding 7 — a validator that agreed with its own caller

The WP1 validator checked `port` and `keys` by comparing the emitted file
against the arguments it was given. That is not a check: if the caller is
wrong, both sides are wrong identically and the configuration is validated into
being wrong.

This was not theoretical. The first WP2 probe called
`write_els2_tunnels_conf "$path" ELS2PROBE ELS2PROBE.dat "$SCRATCH/ELS2PROBE.dat" …`
— arguments transposed, `keys` given as an absolute path — and the validator
returned **success**. i2pd would then have read `port = ELS2PROBE.dat` and
resolved `keys` through `DataDirPath`, which prepends the data dir
(`libi2pd/FS.h:175-181`), so the absolute path becomes `<datadir>//tmp/…`,
fails to open, and i2pd **silently creates a brand-new key pair there**
(`libi2pd_client/ClientContext.cpp:280-307`). The destination would have come up
with a different identity than the lane configured, and every address derived
from it would have been wrong for a reason that looks like a crypto defect.

Fixed in WP2: `write_els2_tunnels_conf` and `validate_els2_tunnels_conf` now
check the **shape** of each argument (a bare filename; a TCP port in 1–65535)
and the validator additionally re-reads `port` and `keys` out of the file body
and checks those independently, so a hand-edited config is caught even when the
caller repeats the same mistake. 21 new contract rows cover the argument
shapes, including the self-agreeing case.

The general lesson is recorded because it is the same shape as finding 6: a
check that compares a value against the value it came from is not a check.

### Finding 6 — the stale portable API snapshot, and why it was fixed rather than deferred

`python3 scripts/check-portable-service-tunnel-api.py` failed. It was verified
**pre-existing** by `git stash`-ing every uncommitted change and re-running it
at clean HEAD, where it fails identically — so it is not caused by anything in
this plan. The diff was exactly three declarations, all of them Plan 380's:

```text
outbound_secret:const:ELS2_CONSUMER_CREDENTIAL_MARKER
outbound_secret:fn:validate_els2_credential_form
outbound_secret:trait:RouterSecretOwner
```

Plan 380 added a second secret domain to the portable crate and evidently did
not run this line of the floor — the same class of gap as Plan 365's finding
that an unparseable `ci.yml` meant three jobs never ran.

The checker asks a human to **review** before writing, so the three were read
before the snapshot was updated, and all three are what Plan 380 intended:

- `ELS2_CONSUMER_CREDENTIAL_MARKER` — the third domain-separated marker,
  documented at `outbound_secret.rs:46-52` as deliberately distinct from both
  the outproxy and auth markers.
- `validate_els2_credential_form` — a one-line delegate to the private shared
  shape check, the exact peer of the existing public `validate_stored_form`.
- `RouterSecretOwner` — the supertrait Plan 380 introduced, documented at
  `:181-195` as total-and-fallible with the `OutboundSecretStore` half inherited
  unchanged.

Nothing was weakened and no boundary was relaxed; the snapshot grew by three
lines and the check now passes at 699 declarations. It is recorded rather than
absorbed because **Plan 380's closure record does not mention this gap**, and a
future reader of that record should know the floor line was missed there.

## Invariants

| # | Invariant | Status |
|---|---|---|
| 1 | No reference modification — i2pd stock at the pin | Held. Nothing was patched, vendored, or rebuilt; the binary under `target/interop/cache/` was executed unmodified. |
| 2 | `MAX_ATTEMPTS = 1` | Held vacuously — no lane exists yet, and no retry-until-green anywhere in what landed. |
| 3 | Missing environment fails | Held. The extractor refuses (exits non-zero) rather than skipping; the validator refuses rather than warning. |
| 4 | No `|| true`, `continue-on-error`, filename filtering, fake peer env, broad exclusions | Held. Grepped: none present in the landed files. |
| 5 | Raw reference logs are never evidence | Held. No i2pd log content enters any file; only counts and the derived addresses. |
| 6 | No advertisement change | Held. `specs/support.toml` untouched; type 5 stays `advertised = false`; `full-proposal-conformant` unset. |
| 7 | Every row goes over the real transport | **Not applicable yet.** WP1.4's rows go over real TLS + JSON-RPC; no row moves an application payload, because no lane exists. WP3/WP4 must satisfy this. |
| 8 | Plan 380's guard stays green | Held. `scripts/check-encrypted-service-consumer-caller.sh` passes, and the new workflow runs it, on the reasoning that a green configuration contract over a mutated request path proves nothing. |

## Commands run

Local, this plan, on 2026-10-07:

```text
bash tests/integration/els2/test-tunnels-conf.sh                              # 36 rows, exit 0
python3 tests/integration/els2/clients/parse_i2pd_els2_destination.py --self-test   # 49 rows, exit 0
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_lane_consumer -- --test-threads=1  # 6 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_lane_consumer -- --test-threads=1  # 5 passed, 1 failed (the DelayOpen row; rewritten)
python3 scripts/check-workflow-validity.py                                    # 11 workflows parse
python3 scripts/check-tooling-inventory.py                                   # ok
```

Reference probes (executed against the pinned binary; **not** part of the
routine floor, and **not** reproducible as written because they stood up an
ephemeral daemon-mode router):

```text
# the six-way key-spelling probe in "Stop condition 1"
# the live .dat -> b33 -> console comparison in WP1.2 (one MATCH, two non-reproductions)
# the b32 cross-check against the service-tunnels parser (reproduced)
```

Full routine floor, run at the repo root on 2026-10-07:

```text
cargo fmt --all --check                                                        # ok
cargo check --locked --workspace --all-targets                                 # ok
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost                       # ok
cargo test --locked --workspace --all-targets -- --test-threads=1              # exit 0
                                                                               # 179 binaries, 4646 passed, 0 failed, 35 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  # ok
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps           # ok
cargo test --locked --workspace --doc                                          # ok
python3 -m unittest discover -s tests/planning -p 'test_*.py'                  # 51 tests, OK
```

The workspace tally is **4 646** against Plan 380's **4 640**: the difference is
exactly the six `plan381_*` rows in `i2pcontrol_els2_lane_consumer.rs`, which is
the check that the new test file is actually collected by the floor and not
merely compiled.

Boundary, evidence and hygiene checkers, all green:

```text
bash scripts/check-dependency-direction.sh                bash scripts/check-console-boundaries.sh
python3 scripts/check-adr-number-uniqueness.py            bash scripts/check-console-browser-security.sh
bash scripts/check-runtime-boundaries.sh                  bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-encrypted-service-consumer-caller.sh   bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-config-secret-hygiene.sh               bash scripts/check-outproxy-request-path.sh
bash scripts/check-outproxy-wire-lane-evidence.sh         bash scripts/check-fixture-manifest.sh
python3 scripts/check-tooling-inventory.py                python3 scripts/check-workflow-validity.py
python3 scripts/check-global-plan-number-uniqueness.py    python3 scripts/check-portable-service-tunnel-api.py
bash scripts/check-portable-service-tunnel-consumer.sh
```

## WP2 — the lane runner, and stop condition 2 retired

WP2 exists to answer one question before any i2pr code is written: **can an
i2pd client consume a blinded destination in this controlled mesh at all?** Plan
381 named that as stop condition 2 and warned that if the answer were no, the
direction would be blocked on topology rather than on code.

**It can. Stop condition 2 is retired by execution.**

The answer was reached with a probe that deliberately involved **no i2pr at
all**: one stock i2pd floodfill publishing an ELS2 destination from a generated
`tunnels.conf`, one stock i2pd client with SAM on loopback, and the client
issuing a real SAM stream connect to the publisher's blinded address. If that
cannot work, no amount of i2pr work helps; that is why it was tested first. The
result was `STREAM STATUS RESULT=OK` followed by the fixture's
`ELS2-LANE-FIXTURE-OK` banner arriving back — a full round trip through a
blinded lookup, an encrypted LeaseSet2 fetch, a Streaming session and a server
tunnel.

### The seven facts the reference forced

Every one of these was found by executing, presents as a crypto or topology
defect, and is now asserted in code so it cannot be re-learned. They are
recorded in `tests/integration/els2/reference-freeze.md` §3.4 with citations.

| # | Fact | Why it misleads |
|---|---|---|
| 1 | `keys` must be a **bare filename** | `DataDirPath` prepends the data dir (`libi2pd/FS.h:175-181`), so an absolute path is mangled and i2pd **silently mints a new key pair**. Every later address derivation is then wrong. |
| 2 | The destination `.dat` is **not** a publication signal | It is key material, written at startup even with zero peers. A peerless first cycle produces one. |
| 3 | Seed **one direction, never both** | Mutual seeding makes both sides emit a SessionRequest at once; the AEAD machines cross and the retry fails `Retry AEAD verification failed`. |
| 4 | A blinded address needs a **`.b32.i2p`** suffix to reach i2pd | `AddressBook::GetAddress` has no `.b33.i2p` branch (`libi2pd_client/AddressBook.cpp:454-461`), so it falls through to base64 and SAM answers `INVALID_KEY`. |
| 5 | SAM 3.1 `SESSION CREATE` **requires** `DESTINATION` | Base64 or the literal `TRANSIENT`; otherwise `INVALID_KEY` (`SAM.cpp:427-441`). |
| 6 | `SESSION CREATE` and `STREAM CONNECT` need **separate connections** | A successful create marks that connection `Session` (`SAM.cpp:449`) and the connect is refused with `Socket already in use` (`SAM.cpp:529-533`). |
| 7 | A connect is gated on the **client's own tunnel pool** | `IsReady()` needs non-empty outbound tunnels (`libi2pd/Destination.h:152`), and the failure surfaces as `INVALID_KEY`, not as "not ready". |

Fact 1 was found because **the first probe passed an absolute `keys` path and
swapped two arguments, and the WP1 validator returned success**. That is a real
defect in committed work (`0edfbdaa`), not a probe typo, and it is described in
the findings section below.

### What the lane now does, and how it was run

`tests/integration/els2/run-i2pd-els2.sh` brings up the two reference routers
with the same fail-closed pin gates, `setsid` groups, `trap cleanup EXIT` and
`MAX_ATTEMPTS=1` freeze that `run-i2pd.sh` uses, generates its `tunnels.conf`
through the WP1 writer and validates it before any process starts, and then
gates each stage on a real signal rather than on a file's existence.

An executed run on this host produced **eight rows, all `passed`**:

```text
tunnels-conf-generated          passed
mesh-identity-generation        passed
mesh-peer-session               passed
reference-els2-published        passed
destination-derived             passed
blinded-address-cross-check     passed
client-tunnel-pool-ready        passed
control-reference-els2-roundtrip passed
```

`blinded-address-cross-check` compares the derived 56-character base32 body
against **i2pd's own rendering** of the same destination and requires them to be
byte-identical. The suffix is deliberately excluded from that comparison,
because fact 4 shows the suffix is a vocabulary choice and not part of the
derivation.

The lane then **fails closed with exit 1** because the WP3 driver does not exist
yet. That is the designed outcome: a live reference mesh with nothing to drive
it is not a partial matrix worth reporting, and recording it as anything other
than a failure would be exactly the "half-run matrix as evidence" the plan
forbids.

### A reporting bug the run caught

The first full lane run reported `mesh-peer-session`, `reference-els2-published`
and `client-tunnel-pool-ready` as **failed** while every one of them had
actually succeeded — the control round-trip had already returned the fixture
banner and the cross-check had already matched. The poll loops carry `*_OK`
flags where 1 means observed, which is the opposite of the 0-means-pass
convention `record_guarded` expects. Fixed with an explicit `observed_rc`
conversion at the reporting boundary rather than by flipping the flags, so the
loops stay readable.

This is worth naming because it is the failure mode this repository's rules
exist to prevent: the lane *looked* like it had found three reference defects,
and had that been recorded without reading the control row, three false
findings would have entered the record.

### WP2 deliverables

- `tests/integration/els2/run-i2pd-els2.sh` — the lane runner and `--self-test`
  gate. **11 self-test rows**, all green, with no i2pd invoked.
- `tests/integration/els2/clients/sam_b33_connect.py` — the reference SAM
  client, carrying facts 4, 5, 6 and 7 as explicit checks with cited reasons.
- `tests/integration/els2/els2-tunnels-conf.sh` — hardened for fact 1.
- `tests/integration/els2/test-tunnels-conf.sh` — 36 → **57 rows**.
- `tests/integration/els2/clients/parse_i2pd_els2_destination.py` — emits
  `dest_b33_i2pd`; 49 → **55 rows**.

### WP2 commands run, on this host, 2026-10-07

```text
bash tests/integration/els2/test-tunnels-conf.sh                 # 57 rows, exit 0
python3 tests/integration/els2/clients/parse_i2pd_els2_destination.py --self-test
                                                                   # 55 rows, exit 0
bash tests/integration/els2/run-i2pd-els2.sh --self-test         # 11 rows, exit 0
bash tests/integration/els2/run-i2pd-els2.sh                      # 8 rows passed, exit 1 at the WP3 handoff
```

The full lane is **not** part of the routine floor — it starts external
processes, like every other external lane — and it is environment-gated. It is
recorded here as a local run with its exact exit status, not as a CI result.

Full routine floor, re-run after WP2: green throughout. `cargo fmt --all
--check`, `cargo check --locked --workspace --all-targets`, `cargo build
--locked -p i2pr-app-fixture -p i2pr-apphost`, `cargo clippy --locked
--workspace --all-targets --all-features -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`, 51
planning tests, `check-global-plan-number-uniqueness.py`,
`check-adr-number-uniqueness.py`, `check-portable-service-tunnel-api.py` (699
declarations), `check-portable-service-tunnel-consumer.sh`,
`check-managed-app-process-boundary.py` **with `--self-test`**, and the boundary
/ evidence / hygiene set.

The workspace tally is **4 646 passed, 0 failed, 35 ignored across 179
binaries** — **byte-identical to the WP1 tally**, and it should be: WP2 adds no
Rust test row, only shell and Python. That is stated as a checked equality
rather than an inference, because a tally that moves without a corresponding
test addition is the signature of a test that stopped being collected.

**Not run, and why.** The WP3 driver does not exist, so the lane's own i2pr
half cannot run; the lane reaches that point and fails closed by design. The
NTCP2/SSU2/I2CP vector and evidence checkers are unrelated to this change and
were not re-run; the floor is the set that can gate it.

## WP3 — the driver exists; the payload row does not pass

WP3 is `crates/i2pr-daemon/tests/els2_i2pd_external.rs`, added after the user
chose the **in-process** composition over the shipped daemon. That decision was
needed because `RawNetDbConfig` exposes only `enabled`, `max_records`,
`max_encoded_bytes`, `min_router_infos` and `min_floodfill_advertisers` — there
is **no configuration surface for injecting one named peer** — so a
real-`i2pr run` lane cannot learn the reference's RouterInfo without a production
change that is outside this plan's scope.

The composition reuses what already exists rather than re-deriving it:
`ServiceProductSpec::reference` takes a `ReferencePeer` and the product dials
it, bootstraps the RouterInfo and builds real tunnels, and `shared_manager`
hands the *same* `Arc<ServiceTunnelManager>` to the I2PControl control state, so
a service created over JSON-RPC lands in the one manager that owns the runtime.
That is Plan 213 §C1's composition, reused.

### What runs

An executed lane run produced **eight `passed` rows and one `failed`**:

```text
tunnels-conf-generated             passed
mesh-identity-generation           passed
mesh-peer-session                  passed
reference-els2-published           passed
destination-derived                passed
blinded-address-cross-check        passed
client-tunnel-pool-ready           passed
control-reference-els2-roundtrip   passed
i2pr-rows                          failed
```

Two driver milestones were reached against the live reference mesh: the
`.b33` client was **created over a real TLS I2PControl listener** with
`DelayOpen` and no JSON-RPC error, and its **local listener bound**.

### The precise failure, and what is *not* yet claimed

The payload did not return, and the driver's own typed surfaces say exactly
why:

```text
b33-client-listener-bound   45819
encrypted-target-status     None
remote-counters             { … every field 0 … }
```

`encrypted_target_status` is `None` and `encrypted_target_resolved`,
`encrypted_target_failed` and `remote_lookup_started` are all **zero**. So no
lookup was ever attempted: the encrypted-target resolution path did not run for
a service created over I2PControl after `ServiceProduct::start`, even though the
same service bound its listener. That is the difference between "the lookup
happened and the record did not unwrap" and "the lookup never happened", and it
is why `EncryptedTargetStatus` and `RemoteDeliveryCounters` were added to the
evidence before anything was asserted about the payload.

### The confirmed defect, fixed here — and a second gate still open

WP3's obligation was to separate a product gap from a missing driver step.
Two earlier readings were wrong and are corrected here rather than left to
mislead.

**Reading one — the deferred-activation snapshot asymmetry.** Plausible, and
real code: `ensure_destination_active` (`service_tunnels.rs:793-819`) computes
deferred-ness live from current runtimes while `activate_deferred_destination`
(`service_product.rs:1516-1545`) gates on `inner.deferred_destination_ids` and
`inner.destination_runtimes`, both written at start. **It is not the cause.**
`advance_destination_pools_at` re-reads `manager.deferred_destination_ids()`
and repopulates `destination_runtimes` from `manager.destination_group_runtimes()`
whenever `committed_generation_id()` changes (`service_product.rs:1681-1712`),
and the I2PControl create path does call `manager.reconcile`
(`i2pcontrol_tunnels.rs:3707-3711`), which installs a new generation.

**Reading two — the address spelling.** Also wrong. `DestinationRef::parse`
dispatches `is_encrypted_service_address` *before* the b32 branch, and that
strips the `.b32.i2p` suffix and tests the body length
(`i2pr-proto/src/common/base32.rs:481-488`). The lane's spelling classifies
correctly.

**What the instrumentation actually showed.** Reading the three gates between
"listener bound" and "lookup started" gave the answer immediately:

```text
spec-reference-present   false
remote-target-projection <no reference>
```

`spec_reference_for_service` read **`self.config.specs`** — written once in
`ServiceTunnelManager::new` and never reassigned — while `reconcile` replaces
`committed_generation.committed_specs`. So every control-created service was
invisible to the whole remote-destination and ELS2 provisioning path.
`provision_all_service_router_material` consults that accessor and `continue`s
on `None`, which records no status and looks exactly like "the lookup never
ran". `spec_is_server` had the identical shape and the identical defect.

**Fixed here, on the user's instruction**, by routing both through
`committed_spec_for` — the accessor already documented as the live view.

**The fix is verified by execution, not by argument.** Same lane, same binary:

```text
spec-reference-present   true
remote-target-projection EncryptedService(EncryptedServiceAddress {
                           flags: 0, unblinded_sigtype: 7,
                           blinded_sigtype: 11, public_key: [...] })
```

The destination is now found and classified with the correct Red25519 blinded
type (11) over Ed25519 unblinded (7) — the types the derived b33 actually
carries.

### The second gate: where it is, and the next lead

`encrypted_target_status` is still `None` and every `RemoteDeliveryCounters`
field is still zero, so **provisioning is not reaching the encrypted branch**.
That branch records `MissingLookupSecret` the instant it is entered, so its
staying `None` is the proof that it is not entered. No cause is claimed.

What was checked, and cleared, reading outward from the branch:

| Gate | Verdict |
|---|---|
| `is_encrypted_service_address` classification | **passes** — strips `.b32.i2p`, tests body length |
| `spec_reference_for_service` | **fixed above** — now returns the encrypted reference |
| `project_remote_target` | **passes** — returns `EncryptedService`, correct sigtypes |
| `enable_deferred_activation` | **fires** — guarded on `router_bootstrap.is_some()`, true here |
| deferred snapshot refresh | **fires** — generation changes on control create |
| supervisor for the control-created runtime | **started** — `i2pcontrol_tunnels.rs:4181` |

**The next lead**, not yet proven. `sync_names`
(`i2pcontrol_tunnels.rs:4155-4190`) starts a supervisor only for diff classes
`Add | ReplaceListener | ReplaceDestination`, and it **`continue`s silently**
when the coordinator has no children scope or no cancellation token:

```rust
let scope = match (lock(&self.children).clone(), lock(&self.cancellation).clone()) {
    (Some(children), Some(cancellation)) => Some((children, cancellation)),
    _ => None,
};
...
let Some((children, cancellation)) = scope.clone() else { continue; };
```

A silently-skipped supervisor would produce exactly the observed shape: the
listener bound, the connection accepted and held, no request ever sent, every
counter zero. The driver calls `control_state.startup(&scope, &parent)` before
creating the service, which should populate both — so whether it does is the
thing to establish next, and it is a **runtime** question, not a reading
question. The `supervised` set is private, so the cheapest honest instrument is
an evidence row for the connection outcome rather than another source reading.



`encrypted_target_status` is still `None` and every `RemoteDeliveryCounters`
field is still zero, so **provisioning is still not reaching the encrypted
branch**. The projection is correct now, which means the remaining gap is
between "the runtime is enumerated" and "`provision_encrypted_service_target`
is called". `encrypted_target_status` would record `MissingLookupSecret` the
moment that branch is reached, so its staying `None` is the proof that the
branch is not reached.

This is a **separate** investigation from the defect fixed above, and no cause
is claimed for it. The row stays failed.

### WP3 resolution (2026-10-08) — the payload row passes

The second gate was **not** a missing supervisor or a missing registration
step. Provisioning reached the encrypted branch all along once the
`committed_spec_for` fix landed; what stayed red was a chain of six further
defects, each found by executing the lane and each fixed forward. The lane now
reports `Plan 381 ELS2 lane passed` with the fixture banner
`ELS2-LANE-FIXTURE-OK` round-tripping through the reference's server tunnel,
in auth mode NONE, direction i2pd→i2pr. All temporary `plan381-probe`
`eprintln` instrumentation has been removed (verified by grep: zero hits in
`crates/` and `tests/`).

| # | Defect | Fix | Evidence |
|---|---|---|---|
| 1 | Single-peer mesh cannot satisfy exact-three diverse peer selection | Lane bootstraps 3 family-distinct stock references (f/c/n); `ServiceProductSpec::extra_bootstrap_peers` + multi-bootstrap dial | `candidates=3 scanned=3 unqualified=0` |
| 2 | Inbound reply correlated on first hop; terminal hop forwards to creator so every inbound reply orphaned | Correlate inbound on terminal hop (`exploratory_build.rs`) | inbound builds install in ~10 ms |
| 3 | Inbound gateway route resolved creator id `ids[5]`; material keyed by endpoint `ids[9]` | Resolve `ids[9]` (`service_product.rs`) | `InboundBuildMissing` on installed builds gone |
| 4 | 10 s activation budget too short for cold provision (builds + ELS2 lookup) | 90 s first-activation budget, `READ_WINDOW_MS=150 s` | activation completes |
| 5 | Strict inner==outer publication-timestamp equality rejects every live reference record (i2pd wraps inner as-is, stamps outer with `now`) | Same-day window (future 3600 s / stale 86400 s) in `i2pr-netdb/src/els2.rs` | `ELS2 store ready` reached |
| 6 | Inner routinely carries `BLINDED_ON_PUBLICATION`; strict validation rejects it in three places | `allow_blinded_on_publication` policy (default false), opt-in at both `resolve` sites + daemon step-6 (`i2pr-netdb`, `i2pr-client`, `service_product.rs`) | `Resolved` + `encrypted_target_resolved: 1` |
| 7 | `.b33` carries no destination hash, so the generic client loop had no up-front `ClientTarget` and parked forever | `run_delay_open_encrypted_client_loop`: per-connection deferred-activate, read installed inner hash, connect via ordinary remote target (`service_tunnels.rs`) | listener accepts, activation runs on data path |
| 8 | **The SYN killer.** `route_outbound_remote_request` re-validated the cached LS2 strict, rejecting the just-admitted blinded record with `BlindedPublicationDeferred` — the SYN died in the sweep and `wait_for_established` timed out | Preserve the admitted shape per record: opt in iff the stored record itself carries the flag (`service_tunnels.rs`) | reference log shows `Incoming stream`, SYN-ACK returns, banner crosses both ways |

Defect 8 is recorded in full because it presented as a transport defect while
being a validation defect: the reference log showed `Incoming stream from …,
sSID=…` followed by `Resend #2, another remote lease has been selected`,
which reads as the reference giving up — but the reference was answering while
i2pr's own sweep dropped every SYN before dispatch. The probe that closed it
was a sweep-outcome `eprintln` (drained-N + delivered/unresolved/failed),
removed after use.

What WP3 proves, precisely: **one** WP4 row — i2pd publishes (NONE), i2pr
consumes, payload crosses both ways. The auth-mode matrix executed is **NONE
only**; PSK/DH, the reverse direction, the negatives, and the `.b32.i2p`
authority row are WP4 and remain open. No advertisement change; type 5 stays
`advertised = false`.

### WP3 deliverables

- `crates/i2pr-daemon/tests/els2_i2pd_external.rs` — the driver, compiling
  clean under `-D warnings`. Environment-gated `#[ignore]`, missing environment
  fails.
- `tests/integration/els2/run-i2pd-els2.sh` — gained the `R` bind port
  allocation and `I2PR_ELS2_SSU2_BIND`, because the controlled profile rejects
  `port = 0` and the port has to be allocated before the process starts.
- `I2PR_ELS2_SSU2_BIND` is now part of the documented hand-off in the resume
  point below.

## What remains unproven

Two defects in **this driver's own first draft** were found by executing it and
fixed: the tunnel identifier field is `Name`, not `ID` (`unknown TunnelManager
field ID`), and the listener read needed the product's inbound pump selected
alongside the socket read, because a control-created service is delivered by the
same poll loop that provisions it.

### The floor, after a production change

This commit changes `crates/i2pr-daemon/src/service_tunnels.rs`, so the floor
is the gate and it was re-run in full rather than reasoned about:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                    # exit 0, 4646 passed, 0 failed, 36 ignored
cargo fmt --all --check                           # ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   # ok
python3 scripts/check-portable-service-tunnel-api.py                              # 699 declarations
bash scripts/check-dependency-direction.sh        # ok
bash scripts/check-config-secret-hygiene.sh        # ok
```

**4 646 passed is unchanged from WP2**, which is the meaningful result: a
production fix that made control-created services visible to the provisioning
path changed no existing row. A behavioural fix to a path no existing test
exercised should move nothing, and it moved nothing.

### WP3 floor

`cargo test --locked --workspace --all-targets -- --test-threads=1` → exit 0,
**180 binaries, 4 646 passed, 0 failed, 36 ignored**. The signature is exactly
what a new environment-gated external test should produce: one more binary than
WP2's 179, one more ignored row than WP2's 35, and **no change to the passed
count** — which is the check that the new target is collected by the floor
rather than merely compiling, and that its one test did not quietly run
un-ignored. fmt, clippy `-D warnings`, 51 planning tests, dependency direction,
tooling inventory, workflow validity and config secret hygiene all pass.

The lane run itself is a local, environment-gated run and is **not** part of the
routine floor, exactly like every other external lane.

Named explicitly, because a partial pass is easy to read as a whole one:

- **Anything about i2pr.** WP2 proves the *reference* half of the lane and
  nothing about i2pr. No row in this plan moves an application payload through
  i2pr, resolves a destination with i2pr, or exercises an i2pr LeaseSet2 fetch.
  WP3 (the driver) and WP4 (the rows) have not been started.
- **The i2pd auth-mode matrix.** None of the three modes has been exercised
  against a live reference. WP1 pins the *configuration* for all three and the
  lane runs auth `NONE`; it proves nothing about whether PSK or DH
  authentication succeeds end-to-end. Plan 381 requires an explicit statement
  of the matrix actually executed, and against i2pr that statement is currently
  **none of the three**.
- **The negative rows** — wrong credential refused, wrong lookup secret refused,
  a record for a different day refused. All WP4.
- **The `.b32.i2p` authority row**, which exists to prove the lane has not broken
  the ordinary path. WP4.
- **Plan 375's Java direction, Plan 377's convergence, and Plan 378's gate.**
  Untouched. This plan delivers the i2pd half of the driver work only, and 377
  still needs both halves.
- **Reproducibility of the reference citations on a fresh host.** The pinned
  source tree is readable on this host, so every citation above is source-level,
  but the tree lives in a scratch directory rather than in the cache. See the
  standing finding below.

### A control row that is deliberately not an acceptance row

`control-reference-els2-roundtrip` proves that the mesh carries a blinded
lookup and an application payload between two **stock reference routers**. It
is not a row about i2pr, it cannot be promoted to one, and Plan 377 must not
count it as cross-router convergence evidence when that plan needs *i2pr*
against a reference. Its purpose is attribution: without it, a later i2pr row
failing would not be distinguishable from the mesh being broken.

## Roadmap disposition

No roadmap milestone is closed by this record. `plans/registry.md` keeps Plan 381
**in progress**. The Proposal 170 and Red25519/ELS2 roadmaps both note WP1
complete with the live rows outstanding. **Plans 374, 375, 377 and 378 remain
blocked**, and this plan does not unblock any of them: 377 needs both a live
i2pd lane and Plan 375's Java lane, 378 needs 377, and 374/375 are waiting on
this plan's WP2–WP5.

## Resume point

**WP3's remaining row.** The driver runs, the reference mesh is green, the
`.b33` client is created over I2PControl and binds — but no lookup is attempted
for it. The first thing to establish is which of the two explanations holds:

1. the product routes services that exist at `start` (and those whose
   destination enters through `deferred_destination_ids`) into
   `provision_all_service_router_material`, and a service that appears later in
   the shared manager is bound but never routed into either; or
2. the driver is missing a registration step.

The typed surfaces already distinguish the outcomes to look for:
`manager.encrypted_target_status(SERVICE_ID)` moving off `None`, and
`RemoteDeliveryCounters::remote_lookup_started` leaving zero. Whichever
explanation survives, the row must not be written as passing until the banner
comes back through the reference's server tunnel.

**Then** WP4's rows (both directions, the three auth modes on the i2pr side,
the negatives, the `.b32.i2p` authority row) and WP5's evidence and closure.

**Stop condition 2 no longer constrains this work.** The mesh was the one
remaining unknown that could have blocked the plan outright, and it is answered.