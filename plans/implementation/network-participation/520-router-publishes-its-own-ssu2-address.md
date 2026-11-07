# Plan 520 — the router publishes its own SSU2 address

**Status:** registered. **Subsystem:** network-participation. **Class:** capability (the
router becomes findable by a peer). **Predecessor:** Plan 159 (the runtime-neutral SSU2
publication policy), Plan 160 (the reachability tracker and the typed peer-test outcome),
Plan 285 (the daemon's peer test), Plan 490 (the introducer role and the family
capability), Plan 515–519 (the publication cycle: periodic, confirmable, retrying, fresh,
off the transport's loop). This plan does not reopen any closed closure record.

## 1. Objective

The router's own published RouterInfo must carry its own qualified SSU2 address, so a
peer can find `i2pr` by the record `i2pr` itself publishes.

One bounded outcome: under an explicit operator opt-in, the router publishes the
**direct** SSU2 form — host and port — **only** when its own reachability evidence
admits it, and publishes the firewalled form, which carries no host and no port, when it
does not.

Nothing else changes. Not the publication's period, not its fan-out, not its ladder, not
its correlator, not its freshness, not its own task, not the `caps` string, not the
`router.version` declaration.

## 2. Why ready

- **Plan 159** closed the runtime-neutral SSU2 address-publication policy
  (`crates/i2pr-transport-ssu2/src/publication.rs`). The repository's own specification
  states the position exactly: *"direct host/port requires `Reachable` plus explicit
  opt-in; otherwise the unpublished static-only firewalled form; snapshots carry
  evidence expiry for withdrawal. **No production publication is enabled.**"*
  (`specs/protocols/09-ssu2.md:161-167`).
- **Plan 160** closed the conservative reachability tracker
  (`crates/i2pr-transport/src/reachability.rs`), with family-only typed signals, a
  validation-enforced corroboration floor, contradiction downgrade and evidence expiry.
- **Plan 285** closed the daemon's bounded peer test, which produces the typed
  `PeerTestOutcome` the reachability vocabulary consumes.
- **Plans 515–519** closed the publication itself: periodic (515), confirmable (516),
  retrying (517), fresh (518), and off the transport's loop (519). The publication puts a
  record on the wire; the record it puts on the wire names no endpoint.
- **The netdb already owns the qualification rule.** `is_qualified_ssu2_address`
  (`crates/i2pr-netdb/src/floodfill_role.rs:195-260`) defines the exact address form that
  may be published: style `SSU2`, options `host`, `port`, `mtu`, `v`, `caps`, `s`, `i`,
  a non-zero port, an MTU in `1280..=9000`, `v = 2`, the family capability, and no other
  option key. The transport's snapshot emits exactly that form, so the two authorities
  already agree.

**The reference agrees.** i2pd 2.61.0 keeps the published addresses of its own RouterInfo
current (`libi2pd/RouterContext.cpp:556-610`, `UpdateAddress`) and expresses reachability
as a **caps letter** — `eReachable` / `eUnreachable`
(`libi2pd/RouterInfo.h:102-110`) — set from the router's own status
(`libi2pd/RouterContext.cpp:343-370`), with the stated policy *"we assume reachable
until we discover firewall through peer tests"* (`libi2pd/RouterContext.cpp:1212`). Java
I2P 2.13.0 says the same thing in its own API: `RouterInfo.setAddresses` takes
*"a set of RouterAddress structures **at which this router can be contacted**"*
(`router/java/src/net/i2p/data/router/RouterInfo.java:210-226`), and the caps property
is `PROP_CAPABILITIES = "caps"` (`:86`).

## 3. Current implementation evidence

The gap is not a missing protocol. It is a missing **owner** and a missing **wiring**,
and both are provable from the tree.

### 3.1 The published record names no endpoint

- `crates/i2pr-netdb/src/local.rs:207-219` — the ordinary builder *"carries zero
  `RouterAddress` entries"* and rejects any attempt to inject an address. The comment
  cites *"Plan 103 §4.2 and Plan 101 authority"*.
- `crates/i2pr-daemon/src/config.rs:2005-2006` — the field's own doc: *"Whether this
  router advertises an SSU2 address (always false)."*
- `crates/i2pr-daemon/src/config.rs:3385-3392` — the parser refuses the switch:
  `ssu2.advertise = true` is a semantic error, *"SSU2 address publication is unavailable
  in this milestone"*.
- `crates/i2pr-daemon/src/router_i2np.rs:1571-1576` — the socket boundary refuses it a
  second time: *"SSU2 controlled profile forbids advertise"*.
- `crates/i2pr-daemon/src/router_i2np.rs:2133-2138` — the service constructor refuses it
  a third time.
- `crates/i2pr-daemon/src/lib.rs:3943-3949` — the service closure refuses it a fourth
  time: *"SSU2 service requires the strict controlled profile"*.

Four refusals, and one published record that names no address. The router publishes
itself; nobody can find it.

### 3.2 The machinery that would publish it exists and is unused

- `crates/i2pr-transport-ssu2/src/publication.rs:215` — `build_publication_snapshot`.
- `crates/i2pr-transport-ssu2/src/publication.rs:55-70` — `PublicationPolicy::new(allow_direct,
  allow_introducers)`.
- `crates/i2pr-transport-ssu2/src/publication.rs:126-135` — `PublicationOutcome`:
  `Direct`, `IntroducerOnly`, `Firewalled`, `Withheld(WithholdReason)`.
- `crates/i2pr-transport-ssu2/src/publication.rs:247-260` — the decision: the direct form
  requires `policy.allow_direct()` **and** `reachability.state == Reachable` **and** an
  endpoint; otherwise the firewalled form, *"never a fabricated direct"*.
- `crates/i2pr-transport/src/reachability.rs:327-400` — `ReachabilityTracker`.
- `crates/i2pr-transport/src/reachability.rs:66` — `MIN_CORROBORATION_FLOOR = 2`.
- `crates/i2pr-transport/src/reachability.rs:475-492` — `supporting_classes` counts
  **distinct** supporting classes in the latest family, so one peer's repeated
  observation can never reach the threshold.

### 3.3 No production code records a reachability signal about the router itself

- `crates/i2pr-transport/src/reachability.rs:119-149` — the five signals:
  `LocalConfiguredBind`, `AuthenticatedPeerObservedExternalAddress`, `ValidatedPath`,
  `PeerTestResult`, `RelayFirewalledSignal`.
- `crates/i2pr-runtime/src/ssu2_peer_relay.rs:465-483` — the only production recorder of
  a `PeerTestResult` signal, and it records it about **a peer**, never about this router.
- `crates/i2pr-transport-ssu2/tests/peer_relay.rs:756-803` — the only other recorders are
  the transport's own tests.

So the tracker has no owner, and the publication builder has no caller. This is the same
shape Plan 504 found for the tunnel-liveness scheduler and Plan 505 found for the
exploratory build driver: **the mechanism was complete; only its inputs and its
installation were missing.**

### 3.4 The record that would carry the address is already published

- `crates/i2pr-daemon/src/lib.rs:4179` — the daemon installs its signed local RouterInfo
  at startup.
- `crates/i2pr-daemon/src/lib.rs:4752-4767` — Plan 518's cycle re-dates, re-signs and
  re-installs it once per cycle.
- `crates/i2pr-daemon/src/router_i2np.rs:1709` — `refresh_local_router_info_record`, the
  re-dating path.

A plan that puts the address into the **installed** record therefore reaches the network
through the publication cycle that Plans 515–519 already closed. No new publisher is
needed, and no new timer is needed.

## 4. Invariants that must not regress

1. **`caps=f` stays refused** without the floodfill advertisement permit. The new
   addressed builder is not a floodfill builder and must not produce the `f` capability.
2. **The loopback default publishes nothing.** With the default profile and no
   corroborating evidence, the router's published record carries no address, exactly as
   today.
3. **The publication cycle is unchanged**: the period, the fan-out, the ladder's timing,
   the correlator, the freshness rule and the task's ownership are byte-for-byte the
   ones Plan 519 left.
4. **Nothing is advertised.** `router.version` stays `0.9.62`, the published RouterInfo's
   `caps` string is unchanged, the support inventory's advertised count stays zero, and
   `Cargo.toml` and `Cargo.lock` stay byte-identical.
5. **One decision, one owner.** The publication decision is the transport's
   `build_publication_snapshot` and nothing else. The daemon never computes a second
   answer to the same question.
6. **The netdb's qualification is the authority.** An address that the netdb's own
   `is_qualified_ssu2_address` would refuse is never installed.
7. **No fabricated evidence.** The daemon records only signals it genuinely holds. A
   missing signal is a missing signal, never an inferred one.

## 5. Scope

### In scope

- A daemon-owned, bounded **self-reachability owner** that feeds the transport's tracker
  with the signals the router genuinely has.
- The **publication snapshot** built by the transport's own builder, with the endpoint
  taken from the router's **bound socket**.
- A **non-floodfill addressed builder** in `i2pr-netdb`, so a record may carry one
  qualified SSU2 address without the floodfill permit.
- The **wiring**: `ssu2.advertise` becomes a real, explicit, default-off opt-in; the
  addressed record is installed; the existing publication cycle publishes it; the address
  is withdrawn when the evidence expires.
- A lane that proves the behaviour and a guard that proves the wiring, with the negative
  controls the house pattern requires.

### Not in scope

- The **introducer-only** form. It needs validated introducer records, which no
  production code produces yet. The plan records that as a finding.
- **`caps=f`** and the floodfill role. That is Plan 494, blocked on Plan 493.
- The **stock-peer** half. That is Plan 493's authorized two-host topology.
- The **external-address observation**. No production code produces
  `AuthenticatedPeerObservedExternalAddress`, and this plan does not invent one. The
  finding is recorded instead.
- **IPv6 publication.** The first case is IPv4 only.

## 6. Required production changes

1. `crates/i2pr-netdb/src/local.rs` — a builder that carries **one** qualified SSU2
   address without the floodfill permit. The existing floodfill builders and their permit
   are untouched.
2. `crates/i2pr-daemon` — a new module owning the router's own reachability: it records
   the signals, it polls the expiry, it builds the snapshot through the transport's
   builder, and it hands the resulting address form to the netdb's builder.
3. `crates/i2pr-daemon/src/config.rs` — `ssu2.advertise` stops being a parse-time error
   and becomes an explicit opt-in that is **off** by default and that still refuses an
   endpoint the selected profile does not permit.
4. `crates/i2pr-daemon/src/router_i2np.rs` — the two `advertise` refusals become the
   profile's own check, so the address is published only from a permitted bind.
5. `crates/i2pr-daemon/src/lib.rs` — the composition root builds the owner, and the
   installed record carries the address when the snapshot admits one.

## 7. Design decisions the plan must make explicit

- **D1 — The switch is `ssu2.advertise`, and it stays off by default.** The surface
  already exists and is already parsed; the plan makes it mean what it says. A second
  switch is not invented.
- **D2 — The endpoint comes from the bound socket, never from configuration.** The
  transport's `Ssu2Endpoint` is built from the runtime's own `local_v4()`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:1498`), so the published port is the port the
  router actually listens on. A configured **non-zero** port that differs from the bound
  port is a refusal, not a publication: it would mean the router publishes an endpoint it
  does not hold. A configured port of zero is the ephemeral case, and the bound port is
  published instead.
- **D3 — The daemon records only signals it genuinely holds.** `LocalConfiguredBind` is
  recorded from the operator's configured bind, and only when the policy's
  `configured_direct_allowed` is set. `ValidatedPath` is recorded from the transport's own
  path validation. `PeerTestResult` is recorded from the daemon's peer test. Nothing else
  is recorded, and a signal that is not held is never assumed.
- **D4 — The address survives Plan 518's re-dating.** The cycle re-dates and re-signs the
  record once per cycle. The re-dated record must carry the same address, or the
  published record would change shape every cycle. This is a structural requirement, not
  a nicety.
- **D5 — The withdrawal is the expiry.** The snapshot carries the evidence expiry, and
  the address is withdrawn when the evidence lapses. The router does not keep publishing
  an address it can no longer justify.
- **D6 — One decision, one owner.** The daemon calls
  `i2pr_transport_ssu2::publication::build_publication_snapshot` and takes the outcome.
  It never re-implements the direct-versus-firewalled rule, and the guard proves the
  daemon's own copy of the rule does not exist.
- **D7 — The floodfill permit stays separate.** The new builder takes an address and no
  permit; the floodfill builders take a permit and an address. Neither can produce the
  other's capability.
- **D8 — The published address carries the family letter and nothing else.** The netdb's
  qualification requires the **address's** `caps` option to be exactly `4` or `6`
  (`crates/i2pr-netdb/src/floodfill_role.rs:246-249`), while the transport's
  `caps_with_family` **prepends** the family letter to whatever capability set it is
  given (`crates/i2pr-transport-ssu2/src/publication.rs:364-379`). The daemon therefore
  passes an **empty** capability set, so the emitted value is exactly the family letter.
  The router's own capabilities live in the **RouterInfo's** `caps` option, which is a
  different option and is not this plan's business. A snapshot carrying more than the
  family letter would be refused by the netdb's own qualification, and the plan refuses
  it rather than weakening the qualification.

## 8. Work packages

### WP1 — The router's own reachability owner

A bounded owner in the daemon that feeds the transport's `ReachabilityTracker` with the
signals the router genuinely holds, using the transport's own policy (the corroboration
floor, the observation TTL and the `configured_direct_allowed` switch), and that polls
the expiry so a lapsed signal downgrades the state rather than leaving a stale claim.
Every recorded signal is counted by kind; a refusal to record is counted, never silent.

### WP2 — The publication snapshot

The owner builds the snapshot through the transport's own `build_publication_snapshot`,
with the endpoint taken from the runtime's bound socket, the public static key and the
intro key taken from the router's own identity, the MTU taken from the transport's
validated path, the capability set **empty** so that the emitted address's `caps` option
is exactly the family letter, and the introducer list empty. The outcome is the
transport's: `Direct`, `IntroducerOnly`, `Firewalled` or `Withheld`. Each outcome is
counted under its own reason.

### WP3 — The netdb's non-floodfill addressed builder

`i2pr-netdb` gains a builder that signs a record carrying **one** qualified SSU2
address, validated by the netdb's own `is_qualified_ssu2_address`, with the `caps` string
that carries no `f`. The existing zero-address builder and the floodfill builders stay
exactly as they are, and the floodfill permit remains the only route to `caps=f`.

### WP4 — The wiring, the opt-in, and the withdrawal

`ssu2.advertise` becomes a real opt-in; the socket and service boundaries keep refusing
an endpoint outside the selected profile; the composition root builds the owner, installs
the addressed record, and the existing publication cycle publishes it. The record is
re-dated with the address intact, and the address is withdrawn when the evidence
expires.

## 9. Failure, cancellation, restart, and contention semantics

- A **missing signal** is not a failure: it lowers the corroboration count, and the
  snapshot's own decision follows. The router publishes the firewalled form rather than
  a direct one it cannot justify.
- A **contradicting signal** (an address mismatch, a firewalled indication) takes
  precedence over corroboration, exactly as the transport's tracker decides
  (`crates/i2pr-transport/src/reachability.rs:528-556`). The router never publishes
  through a contradiction.
- An **expired snapshot** is withheld: the address is withdrawn and the record returns to
  its zero-address form. The withdrawal is counted under its own reason.
- A **restart** rebuilds the owner and the tracker. The evidence does not survive the
  restart, because it was never persisted: a restarted router publishes no address until
  it has earned the evidence again. That is the conservative outcome, and it is the
  reference's: i2pd assumes reachable only until a peer test finds a firewall.
- A **re-dating cycle** re-signs the record with the address intact; the cycle's record
  and the installed record stay one record.
- **Contention** — one owner, one tracker, one snapshot at a time. A tick that finds a
  build already running leaves it alone.

## 10. Compatibility and migration

- No wire change. The address form is the form the netdb already qualifies and the
  transport already emits.
- No dependency change. `Cargo.toml` and `Cargo.lock` stay byte-identical.
- The published RouterInfo's `caps` string is unchanged, and `router.version` stays
  `0.9.62`.
- The default configuration changes in **no** way: the switch stays off, so the default
  router publishes exactly what it publishes today.
- Plans 515's, 516's, 517's and 518's lanes and guards stay green and byte-identical.

## 11. Required tests

### The lane — the public behaviour

1. The router publishes **no** address when the opt-in is off, and the installed record
   stands byte-identical.
2. The router publishes the **direct** form when the evidence admits it, and the address
   is the router's own bound endpoint.
3. The router's published record names **no** endpoint when the corroboration is short,
   and the netdb's own ordinary builder produces that form.
4. The **lapsed** evidence withdraws the address: the record returns to its zero-address
   form and the withdrawal is counted under its own reason.
5. The **re-dated** record still carries the address.
6. The address's `caps` option is exactly the family letter, and the netdb's own
   qualification accepts the published form.
7. The record's `caps` string carries **no** `f`, and the floodfill permit still gates
   `caps=f`.
8. The **floodfill** path's record is left alone: its address is the floodfill path's.
9. Every outcome is counted under its own reason.

### The guard — the wiring

1. `ssu2.advertise` is not refused at parse time, and it defaults to off.
2. The daemon's own copy of the direct-versus-firewalled rule does not exist: the owner
   asks the runtime for the material and never names a reachability state.
3. The owner never builds an address: the address is the transport's, whole, so the
   published endpoint is the runtime's bound endpoint and never a configured literal.
4. The netdb's addressed builder qualifies the address with the netdb's **own** rule and
   carries no `f` capability and needs no floodfill permit.
5. The floodfill builders still require the permit.
6. The runtime's material carries the **family** capability, and the transport writes the
   `caps` option only when the capability set is non-empty.
7. The record is installed, and the publication cycle publishes it **after** the owner
   has refreshed it.
8. The re-dating path preserves the address.
9. The expiry withdraws the address.
10. The daemon records **no** reachability signal of its own: the evidence is the
    runtime's.
11. The owner starts no task, no timer and no listener, and the configuration gains no new
    key.
12. The runtime's install gate binds every SSU2 address the record carries to the
    router's own endpoint and keys, and refuses two conflicting ones.

### The negative controls

Every control must be caught by an assertion, must still compile, and must be reverted
byte-for-byte with the SHA-256 asserted.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test router_info_publication_address -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_address_boundary -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Plus the full routine floor from `AGENTS.md` on the closing tree.

## 13. Documentation updates

- `specs/support.toml` — a new `experimental` surface for the router's own address
  publication, with `advertised = false`. The advertised count stays zero.
- `specs/CONFORMANCE.md` — the achieved position, stated truthfully: the address is
  published only when the evidence admits it, and the stock-peer half is not claimed.
- `specs/QUALIFICATION-GATES.md` — a new section naming what this plan gates and, as
  Plan 519's §18.8 did, what it does **not**.
- `docs/architecture/i2pr-daemon.md` — the new module, the changed counts, and the
  section describing the owner.
- `docs/architecture/i2pr-netdb.md` — the new builder.
- `specs/protocols/09-ssu2.md` — the line that says *"No production publication is
  enabled"* becomes true of the direct form under an explicit opt-in, and the file must
  say so.

## 14. Acceptance criteria and stop conditions

The plan closes when:

1. The router's published RouterInfo carries its own qualified SSU2 address under an
   explicit opt-in, and the direct form appears **only** when the evidence admits it.
2. The address the router publishes is one the netdb's own qualification accepts.
3. The record's `caps` string carries no `f`, and `caps=f` remains permit-gated.
4. The publication cycle is unchanged, and Plans 515–519's lanes and guards are green and
   byte-identical.
5. The lane, the guard and the negative controls are green, and the routine floor is
   green on the closing tree.
6. The support inventory, `specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md` and the
   deep-dives state the achieved position truthfully.

**Stop conditions.** The plan stops, and the finding is recorded rather than worked
around, when:

- the direct form cannot be produced from genuine evidence in the controlled setting,
  because then the plan would be publishing an address the router cannot justify;
- the transport's own builder cannot be used without changing its decision rule, because
  then the rule would have two owners;
- the netdb's qualification would have to be weakened to accept the published form,
  because then the acceptance would be worthless.

## 15. Closure evidence required

- The lane's rows, the guard's rules and the controls' table.
- The floor's per-step record on the closing tree.
- The requirement-to-evidence matrix, the invariant review, the failure and contention
  review, the migration and security review, the limitations, the findings by severity,
  the roadmap disposition and the unblock audit.

## 16. Handoff notes

- The next free plan number after this one is **521**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Do not patch or vendor either
  reference.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host topology.
- The router's published record has carried **no address** since the tree began. This
  plan is the first to give it one, and it gives one only when the evidence earns it.

## 17. Amendment log

**Amended during implementation, 2026-11-05.** Four things in the registered
plan did not survive contact with the tree. They are recorded here, in the
plan's own log, rather than by rewriting the registered text.

### 17.1 The premise was wrong: the published record **does** name an endpoint

**Amended.** Section 3.1 is titled *"The published record names no
endpoint"*, and section 4's second invariant reads *"the router's published
record carries no address, exactly as today"*. **Both are false.** The
daemon's published record has carried one SSU2 address since the tree began:
`generate_identity_with_keys`
(`crates/i2pr-daemon/src/router_i2np.rs:2036-2050`) builds it from the
**bound** endpoint with the options `host`, `port`, `v`, `s` and `i`, and
that record is what the composition installs
(`crates/i2pr-daemon/src/lib.rs:4179`) and what the publication cycle
publishes.

**Why it matters.** The real gap is narrower and sharper than the registered
one: the published address is **unqualified**. It omits the `caps` and
`mtu` options the specification's SSU2 form carries and the netdb's own
`is_qualified_ssu2_address` demands
(`crates/i2pr-netdb/src/floodfill_role.rs:195-260`), so no peer can qualify
the endpoint `i2pr` publishes. The reference publishes both
(`libi2pd/RouterInfo.cpp:1434-1449`, `:281`).

The corrected objective is therefore: **the published address is qualified —
it carries the family capability and the transport's MTU — and it is
published only while the router's own reachability evidence admits it.**

### 17.2 Decision **D8** was unimplementable as written

**Amended.** The registered **D8** says the daemon passes an **empty**
capability set, "so the emitted value is exactly the family letter". That
does not work: the transport's `direct_snapshot` writes the `caps` option
**only when the capability set is non-empty**
(`crates/i2pr-transport-ssu2/src/publication.rs:296-298`), so an empty set
produces an address with **no** `caps` option at all, and the netdb's own
qualification refuses an address with no `caps`
(`crates/i2pr-netdb/src/floodfill_role.rs:225-229`).

The daemon therefore passes **no capability set at all**: it takes the
address **whole** from the transport's snapshot, which already carries the
family letter because the runtime passes it
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2442-2452`). The daemon never
writes an address's options, and the guard pins that.

### 17.3 The owner asks the runtime; it does not build the snapshot

**Amended.** The registered section 6.2 puts the owner in a new daemon
module that "builds the snapshot through the transport's builder". The
daemon has **no** `i2pr-transport-ssu2` dependency, and section 10 requires
`Cargo.toml` and `Cargo.lock` to stay byte-identical, so it cannot call
`build_publication_snapshot` at all.

The **runtime** already owns exactly that: `Ssu2RuntimeService::publication_material`
applies the transport's builder to the router's own reachability evidence
and refuses anything the builder refuses. The daemon's owner therefore
**asks the runtime** for the material and takes the transport's answer. The
decision keeps exactly one owner — the transport's — and the daemon owns
only the opt-in, the hand-off to the netdb's builder, the install, the
withdrawal and the count.

### 17.4 The withdrawal is the **zero-address** form, and the runtime's gate had to yield

**Amended.** Section 9 says the lapsed address returns the record to its
**zero-address** form. That form could not be installed: the runtime's gate
required **exactly one** SSU2 address naming the router's own bound
endpoint (`crates/i2pr-runtime/src/ssu2_runtime.rs:3355-3361`), so a record
with no address was refused outright.

That gate was also the reason the daemon's published address was rendered
**unconditionally**: the transport's own policy renders the endpoint-less
form when the evidence does not admit a direct endpoint, and the daemon's
ad-hoc address bypassed that policy. The gate now accepts a record with
**no** SSU2 address, while still binding every address the record *does*
carry to the router's own endpoint and keys, and still refusing two
conflicting SSU2 addresses. The transport's policy is therefore the single
authority on whether an endpoint may be published, and the withdrawal is
the netdb's ordinary zero-address form.

### 17.5 What the amendment does **not** change

The plan number, the objective, the invariants **1**–**7**, the decisions
**D1**–**D7**, the scope, the compatibility promise and the stop conditions
are unchanged. The switch is still `ssu2.advertise`, still off by default,
and the default configuration still changes in no way: with the switch off
the installed record stands byte-identical.

Section 11's two lists were rewritten to name the surface that was actually
built. The lane's **firewalled** row became the **zero-address** row (the
netdb's ordinary builder is what produces the endpoint-less form, and the
runtime's gate now admits it), the **contradiction** row became the
**floodfill stand-down** row (the transport's own tracker owns the
contradiction, and the daemon never re-implements it), and the **restart**
row became the **re-dating** row (a restarted router has no evidence and
therefore publishes no address, which the withdrawal row already shows).
The guard's list grew from eleven rules to twelve: the runtime's install
gate is now a rule of its own.

### 17.6 One finding the implementation exposed

The direct form requires `Reachable`, which the transport's tracker defines
as **three distinct corroboration classes**
(`crates/i2pr-transport/src/reachability.rs:66`, `:557-561`). The runtime's
production recorders can supply them only inside the **controlled floodfill
activation**, and that path installs the *floodfill* record, whose address
this plan's stand-down rule leaves alone. There is therefore **no production
entry point** that starts a peer test outside the floodfill activation, and
the ordinary record's address cannot be earned in production today. That is
recorded as a finding in the closure record; it is not worked around.
