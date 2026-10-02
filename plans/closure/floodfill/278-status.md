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

**Correction.** An earlier revision of this section claimed i2pd's numeric `router.version`
threshold was unreachable, on the assumption that `MAKE_VERSION_NUMBER` bit-packs its
components. That assumption was wrong and the claim is withdrawn. The macro is decimal:

```cpp
// libi2pd/version.h:18
#define MAKE_VERSION_NUMBER(a,b,c) ((a*100+b)*100+c)
```

so `NETDB_MIN_ALLOWED_VERSION` is 958 (0.9.58) and `NETDB_MIN_FLOODFILL_VERSION` is 962
(0.9.62), which matches i2pd's digit-stripping `router.version` parse exactly. The version
gate is satisfiable by an ordinary version string; there is no parsing bug.

### 9.1 Why the seeded record was rejected

The rejection happens at loader condition 2, not condition 5, and it is caused by two missing
RouterInfo options. At the end of address/property parsing:

```cpp
// libi2pd/RouterInfo.cpp:507-508
if (!m_SupportedTransports || !isNetId || !m_Version)
    SetUnreachable (true);
```

i2pr's published record advertises neither `netId` nor `router.version`, so `isNetId` is false
and `m_Version` is 0; the record is marked unreachable and `LoadRouterInfo` (`NetDb.cpp:531-536`)
deletes it. That is the proximate cause of the 64/64 rejection. Condition 5 would also have
failed independently: caps `f` carries no `O` cap letter, so `IsHighBandwidth()` is false
(`RouterInfo.h:281`) and `m_Version` is 0.

Note that `netId` is also validated, not merely required: a `netId` that does not equal
`i2p::context.GetNetID()` (2 in the reference's own configuration) also sets unreachable
(`RouterInfo.cpp:480-489`).

### 9.2 The gate that cannot be cleared honestly

The decisive constraint is not in the loader. It is in floodfill eligibility:

```cpp
// libi2pd/RouterInfo.cpp:1022-1029
bool RouterInfo::IsEligibleFloodfill () const
{
    return m_Version >= NETDB_MIN_FLOODFILL_VERSION && (IsPublished (true) ||
        (IsReachableBy (eNTCP2V4 | eSSU2V4) && IsPublished (false))) &&
        GetIdentity ()->GetSigningKeyType () != SIGNING_KEY_TYPE_DSA_SHA1;
}
```

`NETDB_MIN_FLOODFILL_VERSION` is 962, and **this test has no high-bandwidth alternative**.
i2pd consults it at every `m_Floodfills.Insert` for a peer (`NetDb.cpp:296`, `338`, `473`,
`541`) and in `SetUnreachable` (`NetDb.cpp:476-478`). The only unconditional insert is
`NetDb.cpp:86`, which is i2pd's own RouterInfo.

The wire-learned path does not help. `NetDb::AddRouterInfo` (`NetDb.cpp:311-352`) validates a
received record with no version and no bandwidth check at all, and it verifies the signature,
so an honestly signed record learned over the wire is inserted into `m_RouterInfos`. But
`m_Floodfills.Insert` on that same path still requires `IsEligibleFloodfill()`.

**Therefore i2pd can only use i2pr as a floodfill if i2pr advertises
`router.version >= 0.9.62`, and no injection path, seeding or wire, avoids that.**

### 9.3 Why i2pr must not simply declare 0.9.62

`router.version` is a compatibility claim about the implemented feature subset, and
`O` is a high-bandwidth claim. i2pr can make neither:

- `O` is false; i2pr is an experimental router on loopback with a single-bitness pool.
- `router.version = 0.9.62` would assert conformance to the I2P 0.9.62 feature set, which
  `specs/CONFORMANCE.md` restricts to a reviewed, tested subset and which i2pr does not
  implement (NTCP2 is experimental and non-advertised, SSU1 is absent, SAM/I2CP and
  service-tunnels are disabled by default and non-advertised). `specs/support.toml` records
  i2pr's actual level, which is below the reference minimum.

So this is a conformance boundary, not a code defect. There is no change to i2pr's
RouterInfo content, capability set, or transport that makes the reference treat i2pr as a
floodfill without a false claim.

### 9.4 The probe result, now explained

The version-only probe carried `router.version = 0.9.58`, a masked static key, and
`reservedrange = false`, yet was still rejected 64/64. It is now explained: the probe set
`router.version` but not `netId`, so `RouterInfo.cpp:508` still marked it unreachable and
loader condition 2 still failed. The probe varied the version gate while leaving the
`netId` gate in place, so it could not have passed and its result says nothing about
conditions 3, 4, or 5.

This is consistent with the retained structural comparison in §6, which already showed the
controlled record lacking `router.version`, `netId`, and the `netdb.known*` counts while the
reference record had `netId=2` and `router.version=0.9.70`. Those entries were not an
independent defect; they are the proximate cause.

## 10. Defects owned by the corrective (Plan 284)

### 10.1 Gate 1 fixed: the controlled record now declares `netId` and `router.version`

The proximate rejection is corrected in code, not only diagnosed. `activate_controlled` built the
floodfill record with an empty options mapping (`crates/i2pr-daemon/src/floodfill.rs`, previously
`Mapping::empty()`), so the record carried neither `netId` nor `router.version` and the reference
marked it unreachable at `libi2pd/RouterInfo.cpp:508`. The controlled SSU2 identity path already
published both options, so the two controlled publication paths had drifted.

`i2pr_netdb::controlled_router_options()` is now the single source of truth, used by the
controlled floodfill build, the withdrawal build, and the controlled SSU2 identity build. The
declaration is `netId = 2` and `router.version = 0.9.58`, the value the controlled SSU2 path
already published.

The declaration constant is documented as capped by the implemented I2NP surface, with a test
that fails if it is raised above the surface.

### 10.1b Gate 2 closed by Plan 285

`TunnelTestMessage` (231) is now implemented with exact-consumption decoding, a
reference-derived golden vector, and a typed dispatcher arm, with a bounded responder and
outstanding-probe tracker in `i2pr-daemon::peer_test`. `i2pr` therefore implements the
complete I2NP message surface the pinned reference enumerates, so the declaration is
0.9.62 — the level the reference pairs with peer testing and the minimum
`IsEligibleFloodfill` accepts. A test pins the declaration to the surface so the claim
cannot outrun the implementation.

Plan 285 closes **retained**, not passed: the `specs/CONFORMANCE.md` mixed-router step is
unmet, so the mechanical gates are closed in code but the reference has not yet been
observed admitting the record. Plan 278 is unblocked to `ready` for one bounded exact-pinned
attempt. See `plans/closure/floodfill/285-status.md`.

The runtime install guard compares `netId` against the currently installed record, so the
`floodfill_controlled_lifecycle` harness, which hand-rolled its identity record with an empty
options mapping, could not install a later record that declared one. The harness now mirrors the
daemon's controlled identity publication. The guard itself is unchanged and was not weakened.

### 10.2 Gate 2 is the only remaining blocker, and it is now scoped

`RouterInfo::IsEligibleFloodfill` requires `router.version >= 0.9.62` with no high-bandwidth
alternative, so the 0.9.58 declaration clears loading but not floodfill eligibility. Per
`specs/protocols/02-i2np.md`, `router.version` is an I2NP feature/API version, which makes the
question a finite set of message types. `i2pr` implements every I2NP type through short
tunnel-build and lacks exactly one: `TunnelTestMessage` (231), the 0.9.62-era peer-testing type.
Plan 285 owns that gap and the resulting version-declaration decision.


Recorded for the corrective to confirm against a fresh reference client. These are no longer
open hypotheses: §9 localizes the gate, and the two "candidate content gaps" from the first
pass are explained by it rather than being independent defects.
