# Plan 516 — the router's publication asks for the network's acknowledgement

**Implementation plan.** The plan of record for the network-participation
subsystem. Registered 2026-11-02, immediately after Plan 515's closure. The
plan number is load-bearing and is never renumbered.

## 1. Objective

Make the router's own RouterInfo publication **confirmable**.

Today the router publishes its record and learns exactly one thing: that the
**transport accepted the message**. It never learns that the **network holds
the record**. Two facts, one gap.

1. **The publication never asks.** `i2pr_netdb::PublicationCoordinator::begin_attempt`
   (`crates/i2pr-netdb/src/publication.rs:233`) mints a correlation
   `reply_token` (`:253`), records it in the bounded token map (`:294`), and
   documents that a retry "reuses the previously minted token so the remote
   peer can correlate the delivery acknowledgement" (`:228-232`). It then
   writes **`reply_token: 0`** into the `DatabaseStoreMessage` it returns
   (`:276`). A `DatabaseStore` with a zero reply token asks for **no**
   acknowledgement, so no peer ever answers, and
   `PublicationCoordinator::correlate_delivery_status` (`:357`) can never
   match. The `acknowledged` counter in `PublicationSnapshot` is therefore
   structurally always zero.
2. **The acknowledgement already exists — on the other side of the wire.**
   This tree's **own floodfill service** answers a store that carries a
   nonzero reply token: `crates/i2pr-netdb/src/floodfill_service.rs:559` —
   such a store is a `PublishedStore`, not a `FloodReplica`; `:611` — the
   acknowledgement is produced for a **direct peer** ingress; `:612-619` —
   the `FloodfillAck` carries the token; and the `DeliveryStatus`'s
   `message_id` **is** the reply token (`:1361-1362`).

So both halves of the mechanism are in the tree, and the **join** is missing:
this router's floodfill service sends an acknowledgement that this router's
own publication never asks for.

The tree's own live test says so, in a comment
(`crates/i2pr-daemon/tests/netdb_tunnel_live.rs:341-355`):

```text
// Protocol-derived acknowledgement through the existing coordinator.
let token = record.attempt.request_id();
// The coordinator mints a nonzero reply token per attempt; the
// store message itself carries reply_token=0 (store-and-forget).
// Correlation uses the minted token.
let minted = { ... record.attempt.reply_token() };
let correlation = coord.correlate_publication_status(minted).expect("ack");
```

The "protocol-derived acknowledgement" is **manufactured in-process**: the
test takes the token out of the attempt and feeds it straight into the
correlator. No peer can ever produce it.

The reference asks for one. At the pinned commit
(`635b013a612ff47278ef02acf8580a28e10e26c5`, i2pd 2.61.0):

- `libi2pd/I2NPProtocol.cpp:255` — `CreateDatabaseStoreMsg(router, replyToken,
  replyTunnel)`; it writes the **real** token (`:272`);
- for a **direct** publication it writes a **zero** reply tunnel id and the
  router's own hash as the reply gateway (`:285`) — *zero tunnelID means
  direct reply*;
- `libi2pd/RouterContext.cpp:1321` — `PostDeliveryStatusMessage` matches the
  token and reports *"Publishing confirmed"* (`:1325`), then clears the
  per-cycle exclusion set so the next cycle asks the closest floodfill again.

**Plan 516 closes the join.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo stay unchanged, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## 2. Why ready

- Plan 515 made the publication **periodic**, **router-level**, and **counted**
  by reason, so there is now a periodic publication whose outcome can carry a
  confirmation.
- The **correlator already exists**: `PublicationCoordinator` is a bounded,
  runtime-neutral attempt book in `i2pr-netdb`, with `begin_attempt`,
  `correlate_delivery_status`, `mark_delivered`, `mark_delivery_failed`,
  `record_rejection`, `cancel`, `retry_attempt` and a bounded `snapshot`.
- The **acknowledgement already exists**: the floodfill service answers a
  direct-peer store that carries a nonzero reply token, and the
  `DeliveryStatus`'s message id is that token.
- The **inbound dispatcher already classifies** the answer:
  `crates/i2pr-daemon/src/router_i2np.rs:735` produces
  `RouterI2npOutcome::RouterControl { kind: DeliveryStatus,
  acknowledged_message_id }` — the identifier the correlator needs is already
  exposed.
- The reference's model is verified at the pin, with line-level citations in
  §1.

## 3. Current implementation evidence

**The correlator's own contract says the token reaches the peer.**
`crates/i2pr-netdb/src/publication.rs:228-232`:

```text
/// Builds the `DatabaseStore` I2NP body for one publication
/// attempt. Returns the body, the encoded RouterInfo bytes, the
/// bounded reply token, and the request identifier. A retry of
/// an existing attempt reuses the previously minted token so the
/// remote peer can correlate the delivery acknowledgement.
```

**The body does not carry it.** `crates/i2pr-netdb/src/publication.rs:276`:

```rust
let store_message = DatabaseStoreMessage {
    key: Hash::from_bytes(*local.router_hash().as_bytes()),
    reply_token: 0,
    reply_tunnel_id: None,
    reply_gateway: None,
    ...
```

with the comment at `:264-269`:

```text
// Publication uses reply_token=0 because the local router
// does not request a DeliveryStatus acknowledgement for the
// initial store-and-forget publish. The correlation token is
// reserved for the optional verification path; when a
// nonzero token is set, both reply_tunnel_id and reply_gateway must be present.
```

The "optional verification path" is the path Plan 516 opens.

**The token is safe to put on the wire.** `next_reply_token` starts at `1`
(`crates/i2pr-netdb/src/publication.rs:174`), so a minted token is never the
zero token that means "no acknowledgement requested".

**The codec already expresses the direct-reply form.** `reply_tunnel_id` is
an `Option<u32>` (`crates/i2pr-proto/src/i2np/netdb.rs:156`) and the encoder
writes it whenever the token is nonzero
(`crates/i2pr-proto/src/i2np/message.rs:804-816`). A tunnel id of **zero** is
the reference's "direct reply" form, and the decoder reads it back as
`Some(0)` (`crates/i2pr-proto/src/i2np/message.rs:602-606`). No codec change
is required.

**The live inbound path drops the answer.** The exploratory driver consumes
the inbound stream and matches only `ShortTunnelBuild`
(`crates/i2pr-daemon/src/exploratory_driver.rs:1165-1173`); a `DeliveryStatus`
falls through the `continue`. The dispatcher already classifies it — the
answer reaches the daemon, and nothing routes it to the correlator.

**The router's own publication builds its own store.**
`crates/i2pr-daemon/src/router_info_publication.rs`, `router_info_store_message`,
hard-codes `reply_token: 0`, `reply_tunnel_id: None`, `reply_gateway: None`.

## 4. Invariants that must not regress

1. **A publication is never counted as confirmed unless the network answered.**
   `Confirmed` requires a correlated `DeliveryStatus`. A transport admission is
   `Delivered`; the two are separate counters and separate outcomes.
2. **An unknown token is refused and counted.** A `DeliveryStatus` whose
   message id matches no live attempt is never silently dropped and never
   confirms anything.
3. **The correlator stays bounded.** The attempt book and the token map keep
   their existing ceilings; an expired attempt is counted by reason, not
   retained forever.
4. **The version gate still holds.** The confirmation path must not become a
   second selection: the targets are still the Plan 503 version-gated
   floodfills, selected through the same function.
5. **No secret crosses the boundary.** The reply token is a correlation
   identifier, not a key; it is never logged with a peer identity and never
   persisted.
6. **The publication remains the router's own.** The confirmation needs no
   authority beyond the record the router already installs.
7. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the inventory's advertised count stays
   zero.

## 5. Scope

**Lands:**

- the publication's **reply token on the wire**, in the reference's
  **direct-reply** form (`reply_tunnel_id: Some(0)`, `reply_gateway` = the
  local router hash);
- the **reply route** as an explicit input to the correlator, so the direct
  and tunnelled paths cannot disagree about the form;
- the **live inbound join**: an inbound `DeliveryStatus`'s acknowledged
  identifier is routed to the correlator, and an unknown token is refused and
  counted;
- a **distinct outcome**: `Delivered` (transport admission) and `Confirmed`
  (the network answered) are separate, both counted;
- the **records**: the inventory surface, the conformance note, the
  qualification gate, and the deep-dive.

**Does not land:**

- the reference's **resend burst** (Plan 515's finding **F1**): the reference
  retries on a *drop* and stops on a *confirmation*, so a faithful burst needs
  the confirmation this plan delivers. The burst is a successor's job;
- a **tunnelled** reply for the router's own publication: the reference's
  direct form is always available, and this tree's floodfill service only
  acknowledges a **direct peer** ingress
  (`crates/i2pr-netdb/src/floodfill_service.rs:611`);
- any change to the **published RouterInfo's** content;
- any **live** stock-peer evidence, which remains Plan 493's topology.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-netdb/src/publication.rs` | the reply route becomes an explicit input; `begin_attempt` puts the minted token on the wire in the route's form; a confirmed attempt is distinguishable from a delivered one |
| `crates/i2pr-proto/src/i2np/netdb.rs` | the reply-route form's documentation, if the codec's own docs need it (no wire change) |
| `crates/i2pr-daemon/src/router_info_publication.rs` | the refresh publishes through the tracked path, so the confirmation reaches the outcome; the outcome enum gains `Confirmed` |
| `crates/i2pr-daemon/src/netdb_tunnels.rs` | the coordinator's publication path passes the reply route; the inbound join lands in the daemon's inbound owner |
| `crates/i2pr-daemon/src/lib.rs` | the live inbound path routes a `DeliveryStatus` to the correlator |
| `crates/i2pr-daemon/tests/` | the new lane and the new boundary guard |

## 7. Design decisions the plan must make explicit

- **D1 — The reply form is direct.** The publication carries
  `reply_tunnel_id: Some(0)` and `reply_gateway: Some(local hash)`, the
  reference's *"zero tunnelID means direct reply"*
  (`libi2pd/I2NPProtocol.cpp:285`). A tunnelled reply needs an inbound
  exploratory tunnel the router may not hold, and this tree's floodfill
  service only acknowledges a direct-peer ingress.
- **D2 — The token is the correlator's, not the RNG's.** The token is minted
  by the correlator's bounded counter, so the wire token and the correlation
  key are the same value by construction. The publication never invents a
  token the correlator does not know.
- **D3 — The tracked path is the publication path.** The router's own
  publication goes through the correlator rather than through a second
  store-message builder, so the token, the wire message and the attempt book
  cannot diverge. Plan 515's shared `router_info_store_message` is either
  folded into the correlator or restricted to the paths that genuinely do not
  ask for an acknowledgement — and the choice is recorded, not implied.
  **The key, the gateway hash and the payload must all come from the same
  record.** The correlator's own snapshot (`LocalRouterInfo`) and the record
  the router installed (`Ssu2RuntimeService::installed_local_router_info`,
  `crates/i2pr-runtime/src/ssu2_runtime.rs:3247`) are two representations of
  the same identity, and the publication must not mix them: a `DatabaseStore`
  whose `key` is the hash of one record and whose payload is another would
  store a record under the wrong key. Whichever record the plan publishes, the
  key, the gateway and the payload are taken from **it**.
- **D4 — `Delivered` and `Confirmed` are separate.** A transport admission is
  not an acknowledgement. The counters distinguish them, and a `Confirmed`
  outcome carries the number of targets that answered.
- **D5 — Fail closed.** An unknown token is refused and counted under its own
  reason. A `DeliveryStatus` never confirms an attempt it cannot be
  correlated to.
- **D6 — An unconfirmed attempt expires.** An attempt that is never
  acknowledged is counted by reason at the next period and released. The
  correlator never retains an attempt forever.
- **D7 — The resend burst is not adopted here.** Plan 515's finding **F1**
  stays open and becomes the successor's subject.

## 8. Work packages

- **WP1** — the reply route as an explicit input to the correlator, with the
  direct form as the reference's.
- **WP2** — the token on the wire, minted by the correlator and never zero.
- **WP3** — the live inbound join: a `DeliveryStatus`'s acknowledged
  identifier routed to the correlator, an unknown token refused and counted.
- **WP4** — the outcome split: `Delivered` and `Confirmed`, both counted, with
  the refresh reporting them separately.
- **WP5** — the lane and the boundary guard, the negative controls, and the
  records.

## 9. Failure, cancellation, restart, and contention semantics

- A publication that is cancelled mid-flight is `Cancelled`, never
  `Confirmed`.
- A correlator whose lock is poisoned refuses the correlation and counts it;
  it never confirms an attempt it cannot read.
- A router that restarts loses its outstanding attempts; they are not
  persisted, and no confirmation for a previous process is ever counted.
- The attempt book and the token map are bounded; the ceilings are the
  correlator's existing ones and must not grow.
- Two publications in the same period cannot share a token: the counter is
  monotonic and the token map is keyed by value.

## 10. Compatibility and migration

- **No wire-format change.** The `DatabaseStore` layout is untouched; only
  the values of the existing reply fields change, and they change to the form
  the specification and the reference already use.
- **A peer that ignores the reply token** simply does not answer. The
  publication is still `Delivered`; it is never `Confirmed`, and it is
  counted as unconfirmed rather than as a failure.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.
- No configuration key: the confirmation is not optional at the protocol
  level, and the router has nothing to configure about it.

## 11. Required tests

**The lane** (`crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`):

- the publication's wire message carries the correlator's **minted** token,
  and the token is never zero;
- the direct-reply form is the reference's: a **zero** tunnel id and the
  local hash as the gateway;
- a correlated `DeliveryStatus` moves the attempt to `Acknowledged` and the
  snapshot's `acknowledged` counter to one;
- an **unknown** token is refused and counted, and confirms nothing;
- `Delivered` and `Confirmed` are **separate** counters, and a delivery
  admission alone never yields a confirmation;
- an attempt that is never acknowledged is counted by reason and released;
- the token map and the attempt book stay **bounded**.

**The boundary guard**
(`crates/i2pr-daemon/tests/router_info_publication_confirmation_boundary.rs`):

- the publication's reply token is the correlator's minted token, not a
  literal zero;
- the reply form is the reference's direct form;
- the live inbound path routes a `DeliveryStatus` to the correlator;
- an unknown token is refused, not silently dropped;
- the outcome enum distinguishes `Delivered` from `Confirmed`;
- the correlator's bounds are not widened.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

The AGENTS.md routine floor, on the closing tree, plus this plan's two steps.

## 13. Documentation updates

- `specs/support.toml` — the surface's notes gain the confirmation, and the
  advertised count stays zero.
- `specs/CONFORMANCE.md` — the confirmation is stated as an **implemented,
  structurally proved** capability, and the **stock-peer** half is stated as
  still blocked on Plan 493's topology.
- `specs/QUALIFICATION-GATES.md` — a new section: *how the router learns that
  the network holds its record*.
- `docs/architecture/i2pr-daemon.md` and
  `docs/architecture/i2pr-netdb.md` — the correlator's role, the live join,
  and re-pointed citations.

## 14. Acceptance criteria and stop conditions

**Accepted when:**

1. the publication's wire message carries a **nonzero** token equal to the
   correlator's minted token, in the reference's direct-reply form;
2. a `DeliveryStatus` that arrives on the live path and correlates moves the
   attempt to `Acknowledged`, and the counters distinguish it from a delivery
   admission;
3. an unknown token is refused and counted;
4. the correlator's bounds are unchanged;
5. the routine floor is green on the closing tree, and the negative controls
   are caught by an assertion;
6. nothing is advertised: `router.version` stays `0.9.62`, the inventory's
   advertised count stays zero, and `Cargo.toml` and `Cargo.lock` are
   byte-identical.

**Stop and record when:**

- a peer that answers with a token the correlator does not know — the answer
  is refused and counted, never treated as a confirmation;
- no stock peer answers the publication — the **stock-peer** half stays
  blocked on Plan 493's topology, and no live claim is made;
- the confirmation cannot be produced on the direct path — the plan stops at
  the structural boundary and records the finding.

## 15. Closure evidence required

- the lane and the guard, with their exact outcomes;
- the negative controls, each caught by an **assertion**;
- the routine floor's table on the closing tree;
- the requirement-to-evidence matrix, the invariant review, the failure and
  migration reviews, the limitations, the findings by severity, the roadmap
  disposition and the unblock audit;
- the registry and roadmap rows.

## 16. Handoff notes

- Plan 515's closure record is **not** rewritten. Plan 515's finding **F1**
  stays open, and the successor's closure record carries its disposition.
- Plan 497's, Plan 509's, Plan 513's and Plan 514's closure records are not
  rewritten, and their guards stay green and byte-identical unless this plan
  explicitly extends them.
- The reference pins are frozen: i2pd `635b013a612ff47278ef02acf8580a28e10e26c5`,
  Java I2P `9134f808337b401e8e53c73734c81fab04280c9d`.
- No boundary script is weakened. If a rule needs to change, the boundary is
  fixed and the rule is left alone.
