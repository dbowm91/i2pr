# Plan 116 handoff

- Status: **passed-final-local-closure**
- Date: 2026-08-18
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- Final closure pass: [`116-final-closure.md`](116-final-closure.md)
- Terminal cleanup pass: [`116-terminal-cleanup.md`](116-terminal-cleanup.md)
- Current status authority: [`116-status.md`](116-status.md)
- Predecessor: [`115-status.md`](115-status.md)
- **Successor plan:** [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- **Successor handoff:** [`117-handoff.md`](117-handoff.md)
- Plan 117: **ready for execution**.

## Start here

Plan 116 is closed. Do not restart it from the beginning and do not create another Plan 116 validation pass merely because authenticated external transport remains unavailable in the current host environment.

The terminal cleanup pass landed the final local closure defects and later fragment hardening extended follow-on duplicate identity and fresh-key validation.

Current state:

```text
plan_115_Q0                         = passed-emissary-native-consumer
plan_116_material_transfer         = passed-local
plan_116_pool_material_only        = passed-local
plan_116_fragment_boundaries       = passed-local
plan_116_exact_byte_cross_tunnel   = passed-ordered-local
plan_116_out_of_order_cross_tunnel = passed-local
plan_116_duplicate_accounting      = passed-noop-exact-duplicates
plan_116_duplicate_expiry          = passed-no-refresh
plan_116_first_delivery_identity   = passed-conflict-detected
plan_116_follow_on_control_identity = passed-is-last-conflict-detected
plan_116_fresh_key_validation      = passed
plan_116                            = passed-final-local-closure
plan_117                            = ready-for-execution
Q1_authenticated_transport         = deferred
Q2_external_return_established     = deferred
normal_daemon_ntcp2                = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

## Retain the Plan 116 implementation

Do not rewrite these surfaces during Plan 117 unless independent native processing produces a reproducible defect localized to them:

```text
short.rs        real EstablishedMaterial extraction
short_state.rs  canonical registrar -> pool seam
established.rs  established secret/path ownership
pool.rs         material-bearing production entries
data.rs         Tunnel Message framing + exact fragment sizing
layer.rs        AES layer transforms + duplicate token window
roles.rs        runtime-neutral outbound/inbound role composition
fragment.rs     bounded reassembly + duplicate/control metadata hardening
```

The next work is **composition**, not reconstruction.

---

## What Plan 116 proved

The local runtime-neutral path is now established:

```text
ShortBuild Established
 -> real per-hop LayerKeys
 -> EstablishedMaterial
 -> real ExploratoryPool entry
 -> OutboundGatewayRole
 -> participant(s)
 -> OutboundEndpointRole
 -> ROUTER / TUNNEL delivery
 -> InboundGatewayRole
 -> participant(s)
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

Both ordered and out-of-order fragmented trajectories pass locally.

This is sufficient substrate for Plan 117 to join the tunnel data plane to the NetDB lookup/publication state machines.

---

## Non-blocking carry-forward hardening

One post-closure hardening case remains non-blocking:

```text
if follow-on sequence N has already declared is_last=true,
a later new fragment sequence > N should be rejected as contradictory terminal state
```

Do not reopen Plan 116 for this item.

If `fragment.rs` is already touched for a directly related Plan 117 reason, it may be corrected opportunistically with one narrow test. Otherwise leave it for the security/hardening backlog.

---

## Plan 117 starts from these concrete gaps

Plan 117 should not begin with an external harness. It begins with current product-composition gaps:

1. `LookupAction::SendDatabaselookup` currently carries only `encoded_len`, not the real `DatabaseLookupMessage` the state machine constructed.
2. `PublicationAttemptRecord` currently does not retain the real `DatabaseStoreMessage` it validated.
3. `NetDbSeam::pending_action_after_path()` still returns the old placeholder after accepting a reply path.
4. Established pool material needs a bounded one-shot activation owner for local OBGW/local inbound endpoint roles.
5. `i2pr-daemon` is still intentionally non-networked and does not yet compose `i2pr-tunnel`; Plan 117 may add that composition dependency without enabling normal-daemon NTCP2.

See [`117-handoff.md`](117-handoff.md) for the execution order.

---

## Plan 117 evidence semantics

Do not conflate these:

```text
117-L  local production composition
117-N  independent native Emissary composition
117-X  authenticated transport / live-process delivery
```

The pinned native reference remains upstream Emissary:

```text
revision = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package  = emissary-core 0.4.0
```

117-L and 117-N are the mandatory next product checkpoint.

117-X is attempted only if an already-qualified authenticated host lane is runnable. If not, record the environment defer once and continue router construction; do not rebuild the historical namespace/VM/Python harness.

---

## Fixed scope boundary carried forward

Plan 116 closure does not authorize:

```text
normal-daemon NTCP2 activation
SSU2
public-network participation
new Python interoperability harnesses
new rootless namespace work
Docker / Multipass / VM orchestration
LeaseSet/client tunnel implementation
general garlic subsystem
streaming
SAM
I2CP
SOCKS/HTTP proxying
```

Plan 117 has its own explicit scope in [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md).

---

## Validation floor to preserve

Before and after Plan 117 changes, keep the Plan 116 data-plane surfaces green under the repository-wide validation bar, including at least:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Do not modify historical interoperability scripts merely to make Plan 117 green.

---

## Successor

Execute now:

- [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- [`117-handoff.md`](117-handoff.md)

Plan 117 must turn the closed local tunnel data plane into a working exploratory NetDB composition and then validate that path once against the pinned independent Emissary implementation.
