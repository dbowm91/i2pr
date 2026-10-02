# Plan 278 status: stopped

- Plan: `plans/implementation/floodfill/278-m12-i2pd-controlled-qualification.md`
- Closure token: `stopped-m12-reference-client-rejects-the-controlled-routerinfo-before-any-matrix-row`
- Corrective required via: Plan 284
- Stop classification: external lane boundary, not an i2pr protocol defect and not a
  reference-unavailability finding. Plan 278 §12 forbids widening budgets, patching the
  reference, or broadening network access to escape it; it requires the exact boundary
  and a narrow corrective.

## 1. What was executed

The Plan 278 lane was built and executed against the exact-pinned stock i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5` (`i2pd --version` reports `2.61.0 (0.9.70)`),
on an unprivileged loopback-only mesh, with reseed disabled and no public network.

Landed artifacts (production-clean, no weakening of any guard):

- `tests/integration/floodfill/run-i2pd.sh` — pin-verified lane. Frozen attempt budget of 1,
  fresh per-attempt datadirs, PID-file lifecycle, exact source-locked `HashedStorage` netDb
  seeding, required-failure exit, sanitized evidence schema `i2pr-m12-floodfill-qualification-v1`.
- `crates/i2pr-daemon/tests/floodfill_i2pd_external.rs` — three `#[ignore]`-gated,
  fail-closed phases: `floodfill_prepare_against_i2pd` (stable controlled identity plus real
  controlled activation), `floodfill_qualify_against_i2pd` (matrix driver), and two
  `#[ignore]`-gated boundary diagnostics.
- Two `#[ignore]`-gated diagnostics: `floodfill_diagnose_version_gate` (re-signs the
  controlled identity with one extra options entry) and `floodfill_diagnose_routerinfo_structure`
  (side-by-side structural dump through the production codec).

Topology: two stock `floodfill=true` reference peers, one stock `floodfill=false` reference
client whose netDb contains only the controlled RouterInfo (so its own NetDB floodfill
selection is deterministic), and the i2pr controlled floodfill on a fixed loopback port.
The controlled identity and transport material persist in an owner-only attempt state
directory so the prepare and qualify phases present the same `caps=f` RouterInfo.

## 2. Where it stopped

Controlled activation itself **passes** and is proven: phase 1 installed a 646-byte RouterInfo
whose `caps` contains `f`, and phase 2 reached the `Active` role with the runtime's installed
RouterInfo matching the activation result.

The lane then stopped at the first externally observable row. The reference client deletes
the seeded controlled RouterInfo during NetDB load, so it never learns a floodfill and never
publishes to i2pr:

```text
NetDb: RI from <datadir>/netDb/r<C0>/routerInfo-<ident>.dat is invalid or too old. Delete
NetDb: 0 routers loaded (0 floodfils)
Router: Can't find floodfill to publish our RouterInfo
```

64 of 64 bucket copies are deleted on every attempt, in both the phase-2 client and the
diagnostic client. i2pr observed zero inbound control messages, so
`publisher store from reference client not observed` is the first required row, and no matrix
row (A–I) is reachable. Zero matrix rows are claimed passed.

## 3. The exact boundary

The rejection is inside the conjunction at `libi2pd/NetDb.cpp:533-536` in
`NetDb::LoadRouterInfo`:

```cpp
if (r->GetRouterIdentity () && !r->IsUnreachable () && r->HasValidAddresses () &&
    ts < r->GetTimestamp () + 24*60*60*NETDB_MAX_OFFLINE_EXPIRATION_TIMEOUT*1000LL &&
    (r->GetVersion () >= NETDB_MIN_ALLOWED_VERSION || r->IsHighBandwidth ()))
```

Verified from the exact-pinned source tree at
`target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5`:

- The signature is **not** the discriminator for file-seeded records. `NetDb::ReadFromFile`
  calls `ReadFromBuffer (false)` (`libi2pd/RouterInfo.cpp:155-161`), so the
  `m_RouterIdentity->Verify` call at `RouterInfo.cpp:189-195` is skipped on this path.
- `NETDB_MIN_ALLOWED_VERSION = MAKE_VERSION_NUMBER(0, 9, 58)` (`libi2pd/NetDb.hpp:58`).
  `m_Version` is initialised to 0 and only set by the `router.version` option
  (`libi2pd/RouterInfo.cpp:457-462`); `IsHighBandwidth()` is `m_Caps & eHighBandwidth`, set
  only by the `O`/`O[1-3]` cap letters (`RouterInfo.cpp:514-536`, `RouterInfo.h:104-109`).
- `HasValidAddresses()` is `m_SupportedTransports != 0`, which for SSU2 requires
  `transportStyle == eTransportSSU2 && isV2 && isStaticKey && isIntroKey`
  (`RouterInfo.cpp:394-397`), and `isStaticKey` additionally requires
  `!(address->s[31] & 0x80)` (`RouterInfo.cpp:285-287`).
- A `host` option in a reserved IPv4 range invalidates the whole address unless the
  `reservedrange` check is disabled (`RouterInfo.cpp:250-262`, `i2p::util::net::IsInReservedRange`
  at `libi2pd/util.cpp:793-815` covers `127.0.0.0/8`; `reservedrange` is wired through
  `SetCheckReserved` at `libi2pd/Transports.cpp:1464-1467`).

## 4. What was proven and what was ruled out

Proven, and deliberately not left as guesswork:

1. **The seeding mechanism is correct.** A control run seeded the reference's *own*
   `router.info` into a fresh client using the identical ident-derivation
   (`base64(SHA256(RouterIdentity[0..391]))`) and identical
   `<datadir>/netDb/r<C0>/routerInfo-<ident>.dat` placement. It loaded with **zero**
   deletions. The discriminator is therefore inside i2pr's RouterInfo content, not in the
   lane's netDb handling.
2. **`reservedrange = false` is required and was added.** Every pre-existing loopback lane in
   this repository already sets it; the first draft of this lane omitted it. This is lane
   configuration, not a router or protocol change.
3. **The version gate alone is not the cause.** A diagnostic probe re-signed the controlled
   identity with `router.version = 0.9.58`, cleared the static-key high bit, and ran against a
   client with `reservedrange = false`. The reference still rejected it 64/64. This rules out
   the simplest hypothesis and is why the corrective needs a per-condition bisection rather
   than a one-line version bump.
4. **Structural diff through the production codec** (both files decoded by
   `i2pr_proto::RouterInfo::decode`):

   | field | reference (accepted) | controlled (rejected) |
   |---|---|---|
   | `router.version` | `0.9.70` | absent |
   | router `caps` | `Of` | `f` |
   | `netId` | `2` | absent |
   | `netdb.knownRouters` / `knownLeaseSets` | `1` / `0` | absent |
   | address `caps` | `B` | `4` |
   | address `mtu` | `1280` | `1280` |
   | address cost | `8` | `10` |
   | address expiry | `Date(0)` | future wall-clock / `Date(9999999999999)` |
   | `signed_len` | 665 of 729 | 582 of 646 |

## 5. Security and resource review

- No secret, private key, SSU2 static/intro key, or `router.keys` material reached evidence.
  The attempt state directory is created owner-only and every file the driver writes there is
  `0600`; the reference datadirs stay in the ephemeral scratch directory and are removed.
- Evidence rows carry counts, digests, lengths, and categorical outcomes only.
- Reference logs are retained as diagnostics only and are never promoted to matrix evidence.
- The diagnostic probe is recorded as `diagnostic-only` and can never satisfy a required row:
  the runner's pass/fail gate ignores `diagnostic-only` statuses, and the evidence writer
  keeps them in a separate array.
- No production code was changed. No guard, boundary script, budget, or test-selection rule was
  weakened. `caps=f` remains non-advertised and no normal-config activation exists.
- Bounded throughout: one frozen attempt, per-step deadlines, one owned owner task, and
  cooperative cancellation with a bounded drain.

## 6. Local verification on this head

- `cargo fmt --all --check`
- `cargo check --locked --workspace --all-targets`
- `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` for the
  new test target
- `bash scripts/check-m12-floodfill-boundaries.sh`
- `bash scripts/check-dependency-direction.sh`
- `bash scripts/check-runtime-boundaries.sh`

The full routine floor and exact-head hosted CI are recorded in the Plan 284 closure record,
which owns the closing head for this work.

## 7. Retained work

Retained and reusable, not rebuilt:

- the exact-pinned i2pd 2.61.0 lane skeleton, including pin verification, frozen budget,
  PID-file lifecycle, and the source-locked netDb seeding plus its verified ident derivation;
- the two-phase stable-identity activation flow, which is the mechanism the corrective needs
  to publish a `caps=f` record the reference will accept;
- the inline production owner loop that calls `handle_authenticated_i2np`,
  `deliver_floodfill_effect_with_dial`, and `maintenance_tick` unchanged, with a sanitized
  observation sink that classifies matrix rows;
- the boundary diagnostics, which are the bisection instruments the corrective uses.

## 8. Unblock audit

- Plan 279 stays blocked: no one-family evidence exists yet, so no second-family gate and no
  normal opt-in `caps=f`.
- Plan 284 is the only executable plan and owns the bisection plus the narrow corrective.
- Nothing else in the M12 sequence changes: 270–276 stay passed, 280 stays stopped, 281 stays
  type-5-deferred, and 277/282 keep their retained work.
- No historical closure record was rewritten.

## 9. Root cause determined after the stop (owned by Plan 284)

Condition 5 of the `LoadRouterInfo` conjunction is the discriminator, and it cannot be
satisfied by a `router.version` value. i2pd parses `router.version` naively by stripping
non-digits (`libi2pd/RouterInfo.cpp:457-462`):

```cpp
m_Version = 0;
for (auto ch: value) { if (ch >= '0' && ch <= '9') { m_Version *= 10; m_Version += (ch - '0'); } }
```

while the threshold is a packed component number (`libi2pd/NetDb.hpp:58`):

```cpp
const int NETDB_MIN_ALLOWED_VERSION = MAKE_VERSION_NUMBER(0, 9, 58);   // 2362
```

So on the file-load path the only satisfiable branch of
`GetVersion() >= NETDB_MIN_ALLOWED_VERSION || IsHighBandwidth()` is `IsHighBandwidth()`,
which requires the `O` cap letter (`RouterInfo.cpp:514-536`, `RouterInfo.h:104-109`).
Computed for the real values: `0.9.58 → 958`, `0.9.69 → 969`, `0.9.70 → 970`, `1.0.0 → 100`;
none reach 2362. i2pd's own record passes only because its caps are `Of`.

This fully explains both observations in §4: the production record (caps `f`, no version) is
rejected, and the version-only probe (version `0.9.58`, caps `f`) is also rejected.

**The correct response is not to advertise `O`.** The `O` cap is a high-bandwidth claim.
i2pr is an experimental loopback router with a single-bitness pool, so claiming it would be
a false capability advertisement, forbidden by `specs/CONFORMANCE.md` ("advertise the lowest
truthful current feature level compatible with its implemented subset"), by ADR 0027 §9, and
by the repository guardrails on capability advertisement. Plan 284 therefore must not change
i2pr's advertised capabilities to pass this lane.

The same rule governs i2pd's runtime sweep (`NetDb.cpp:711`,
`r->GetVersion() < NETDB_MIN_ALLOWED_VERSION && !r->IsHighBandwidth()`), so netDb seeding
cannot be the injection path at all. The viable path is the one i2pd already exempts:
`NetDb.cpp:728-730` re-admits a record once the peer is connected. The SSU2 handshake
carries the initiator's RouterInfo in the SessionRequest, so an authenticated session is
sufficient for the reference to learn the controlled RouterInfo without any seeded file.

That makes the corrective a topology/injection change, not a capability-claim change.

## 10. Defects owned by the corrective (Plan 284)

Recorded for the corrective to confirm against a fresh reference client. These are no longer
open hypotheses: §9 localizes the gate, and the two "candidate content gaps" from the first
pass are explained by it rather than being independent defects.
