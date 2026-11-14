# Plan 523 — the SSU2 introducer set is the specification's form

## 1. Objective

The SSU2 **introducer set** — the peers that will introduce this router to
others — is the only way a firewalled router is reachable at all. This
router's implementation of that set is the **SSU 1** form, and the SSU2
specification says of the SSU 1 form, in as many words, that those options
"are for SSU only and are not used for SSU2".

Two consequences follow, and **both are defects today**:

1. **The router cannot read a real peer's introducer set.** The SSU2
   address parser refuses the specification's `ih` and `iexp` options as
   unknown, so a peer's SSU2 address that carries introducers is refused
   outright. That refusal is production-reachable: the daemon's dial path,
   the floodfill service's own identity, and the runtime's session setup
   all parse peer addresses through this one parser.
2. **The router's own introducer-only address, were it published, would be
   unreadable.** The publication builder emits the SSU 1 options. The
   pinned i2pd 2.61.0 reads only `ih`, `itag` and `iexp`, so it would see
   **no introducers at all** in a record this router signed.

One bounded outcome: **the SSU2 introducer set has the specification's form
everywhere this tree expresses one** — the transport's value type, its
parser, its publication builder, and the netdb's qualification and builder.

This plan does **not** compose the publication. The router still never asks
a peer for a relay tag, still never records an introducer, and still never
publishes an introducer set. That composition is a successor's subject, and
§5.2 says so in as many words.

The gap is named in the network-participation roadmap's completion
definition: *"the router answers a peer's SSU2 Relay Request as the
introducer, so that a firewalled peer is reachable through it, **and
publishes the validated introducer set in its RouterInfo address**"*
(`plans/subsystems/network-participation-roadmap.md:427-429`). The first
half of that clause is Plan 490's, closed; the second half is this plan's
first half, and the composition that completes it is a successor's.

## 2. Why ready

- The specification is unambiguous, and the **pinned** references agree with
  it. The SSU2 specification's *Published Router Info* section defines the
  SSU2 introducer tuple as `ih[0-2]` (the introducer's router hash),
  `iexp[0-2]` (its expiry) and `itag[0-2]` (its tag), and says that
  `ihost[0-2]`, `iport[0-2]` and `ikey[0-2]` "are for SSU only and are not
  used for SSU2". The pinned i2pd 2.61.0 publishes and parses exactly that
  tuple, and the pinned Java I2P 2.13.0 uses `itag0` as the introducer
  marker. This plan needs no new research and no new authority.
- The **mechanism** is already built and already bounded. The transport owns
  a validated introducer table with a ceiling of eight records and a
  published ceiling of three, an expiry, and a deterministic selection
  (`crates/i2pr-transport-ssu2/src/introducer.rs:33-37`, `:234-256`). The
  runtime owns the peer-relay service that holds it
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:893-898`). The publication
  builder already has the introducer-only branch and already omits
  `host`/`port` when introducers are present
  (`crates/i2pr-transport-ssu2/src/publication.rs:255-265`). Only the
  **form** is wrong.
- The netdb's **own** qualification is the authority on what this router
  may publish, and it already refuses an address it does not understand
  (`crates/i2pr-netdb/src/floodfill_role.rs:195-258`). The plan gives that
  authority a second rule for the introducer-only form rather than
  weakening the first.
- The proto crate is **not** touched. The introducer set is carried in the
  RouterInfo's address options, which the proto crate already encodes; no
  new I2NP message, no new SSU2 block, no new wire type is introduced.

The **stock-peer** half — that a pinned stock peer reads this router's
introducer set, and that it uses it — needs Plan 493's authorized two-host
topology and a published introducer set, which this plan does not produce.
It is not claimed here.

## 3. Current implementation evidence

### 3.1 The transport's introducer value is the SSU 1 tuple

`Ssu2Introducer` carries an endpoint, an introduction key and a relay tag
(`crates/i2pr-transport-ssu2/src/address.rs:467-473`):

```rust
pub struct Ssu2Introducer {
    endpoint: Ssu2Endpoint,
    intro_key: IntroKey,
    relay_tag: u32,
}
```

The SSU2 specification's tuple is `(router hash, expiry, tag)`. The
endpoint and the introduction key are the SSU 1 shape; in SSU2 the peer
that reads an introducer looks the introducer **up by hash**
(`libi2pd/SSU2.cpp:986` — `i2p::data::netdb.FindRouter(introducer.iH)`),
because an SSU2 address must not publish a host or a port alongside its
introducers.

The accessor surface follows the value: `endpoint()`, `ip()`, `port()`,
`intro_key()` (`crates/i2pr-transport-ssu2/src/address.rs:492-510`).

### 3.2 The parser refuses the specification's options and accepts the SSU 1 ones

The option prefixes are the SSU 1 names
(`crates/i2pr-transport-ssu2/src/address.rs:1001-1004`):

```rust
const IHOST_PREFIX: &str = "ihost";
const IPORT_PREFIX: &str = "iport";
const IKEY_PREFIX: &str = "ikey";
const ITAG_PREFIX: &str = "itag";
```

`parse_introducer_index` dispatches on exactly those four
(`crates/i2pr-transport-ssu2/src/address.rs:1006-1019`), so a peer's
`ih0` and `iexp0` fall through to the general option arm and are refused:

```rust
_ => return Err(Ssu2AddressError::UnknownOption),
```

(`crates/i2pr-transport-ssu2/src/address.rs:994`, and again at `:1035`).

**This is the production-reachable defect.** The refusal is not confined to
a test: the daemon's own dial path parses a peer record's address here
(`crates/i2pr-daemon/src/service_product.rs:7759`), the daemon's identity
path refuses an unparseable address here
(`crates/i2pr-daemon/src/router_i2np.rs:2108`), the floodfill service
parses its own identity here
(`crates/i2pr-daemon/src/floodfill.rs:238`), and the runtime parses peer
addresses in three places
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1649`, `:3556`, `:4448`). A peer
that publishes introducers — which the specification says a firewalled
router must, and which the pinned i2pd does — is refused by all of them.

The parser also accepts the **combination** the specification forbids: an
address with both a direct endpoint and introducers is classified
`Ssu2AddressClass::DirectWithIntroducers`
(`crates/i2pr-transport-ssu2/src/address.rs:539`, `:679`).

### 3.3 The publication builder emits the SSU 1 options

`introducer_snapshot` writes four options per introducer
(`crates/i2pr-transport-ssu2/src/publication.rs:325-356`):

```rust
options.push((format!("ihost{index}"), introducer.ip().to_string()));
options.push((format!("ikey{index}"), encode_i2p_base64(introducer.intro_key().as_bytes())));
options.push((format!("iport{index}"), introducer.port().to_string()));
options.push((format!("itag{index}"), introducer.relay_tag().to_string()));
```

The pinned i2pd 2.61.0 publishes `iexp<N>`, `ih<N>` and `itag<N>`
(`libi2pd/RouterInfo.cpp:1402`, `:1413`, `:1424`) and parses only `itag`,
`ih` and `iexp` (`libi2pd/RouterInfo.cpp:353`, `:359`, `:361`). A record
this builder produced would therefore carry an introducer set that the
pinned peer **cannot read at all**.

The *structure* of the branch is right and stays: introducers present means
`host`/`port` are omitted and the family capability is added
(`crates/i2pr-transport-ssu2/src/publication.rs:255-265`, `:364-379`).

### 3.4 The runtime and the daemon hold the right facts and never use them

The runtime's peer-relay service owns a validated introducer table and a
mapping into the transport's value type:

- `select_introducers` (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:893`)
  → `IntroducerTable::validated_introducers`
  (`crates/i2pr-transport-ssu2/src/introducer.rs:249-256`), which builds
  `Ssu2Introducer::new(record.endpoint, record.intro_key, record.relay_tag)`.

The transport's own introducer **record** already holds what the SSU2 tuple
needs and more: the peer's hash, the endpoint, the intro key, the relay tag
and the expiry (`crates/i2pr-transport-ssu2/src/introducer.rs:69-76`). The
mapping throws the hash and the expiry away.

- `Ssu2RuntimeService::introducer_publication_material`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2384-2438`) builds the
  introducer-only publication material from that set. Its own doc comment
  says the API "only proves the address form".
- `Ssu2PeerRelayService::note_authenticated_introducer`
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101-1119`) is the one
  insertion path for an authenticated introducer record.

**None of the three has a production caller.** The router's own published
record therefore carries **no** introducer set, which is why the wrong form
has never reached a wire.

### 3.5 The daemon has a counter for a refusal it can never reach

`SelfPublicationOwner` counts a `NoIntroducers` refusal under its own
reason (`crates/i2pr-daemon/src/self_publication.rs:125`, `:157`, `:186`,
`:224`, `:270`). The direct publication path returns
`Ssu2PublicationUnavailable::NoIntroducers` from **nowhere** — the variant
is produced only by `introducer_publication_material`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2390`) — so the counter is
permanently zero. A counter that can never move is as much a fact as one
that does, and this one is misleading: it reads as "the introducer form was
refused" when the truth is "the introducer form was never asked for".

### 3.6 The netdb has no rule for the introducer-only form

`is_qualified_ssu2_address` is the netdb's authority on the router's own
published address (`crates/i2pr-netdb/src/floodfill_role.rs:195-258`). It
requires a `host` and a `port` (`:200-212`) and its option allow-set is
exactly `host | port | mtu | v | caps | s | i` (`:253-257`). An
introducer-bearing address is refused by both halves.

`LocalRouterInfoBuilder::build_with_ssu2_address` refuses any address that
rule refuses (`crates/i2pr-netdb/src/local.rs:265-277`), so the
introducer-only form has no builder. Plan 520 recorded this as its own
limitation 5.

### 3.7 The references agree with the specification

**Pinned i2pd 2.61.0** (`635b013a612ff47278ef02acf8580a28e10e26c5`):

- the introducer tuple is a router hash, a tag and an expiry
  (`libi2pd/RouterInfo.h:135-141`):

  ```cpp
  struct Introducer
  {
      Introducer (): iTag (0), iExp (0) { iH.Fill (0); };
      IdentHash iH;
      uint32_t iTag;
      uint32_t iExp;
  };
  ```

- it is published as `iexp<N>`, `ih<N>` and `itag<N>`
  (`libi2pd/RouterInfo.cpp:1402`, `:1413`, `:1424`) and parsed as `itag`,
  `ih` and `iexp` (`libi2pd/RouterInfo.cpp:353`, `:359`, `:361`);
- the published introducer's hash is the **remote** peer's hash and the tag
  is the tag that peer issued (`libi2pd/SSU2.cpp:1392-1394`);
- a peer with introducers is dialled through them
  (`libi2pd/SSU2.cpp:934`), by looking the introducer up by hash
  (`libi2pd/SSU2.cpp:986`);
- the tag is requested by the initiator when it is firewalled and the peer
  is an introducer (`libi2pd/SSU2Session.cpp:755-762`), generated by the
  responder on the request (`libi2pd/SSU2Session.cpp:1881-1882`), and
  stored by the initiator when it arrives (`libi2pd/SSU2Session.cpp:1889`).

**Pinned Java I2P 2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`):
`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2335` and
`:2576` read `addr.getOption("itag0")` as the introducer marker, and the
`itag<N>` numbering matches the specification's.

**The specification** (<https://i2p.net/en/docs/specs/ssu2/>, *Published
Router Info* → *Introducers*):

- `ih[0-2]=(Base64 hash)` — "A router hash for an introducer."
- `iexp[0-2]` — "Expiration of this introducer."
- `itag[0-2]` — "Introducer's tag 1 - (2**32 - 1)"
- "The following options are for SSU only and are not used for SSU2. In
  SSU2, Alice gets this information from Charlie's RI instead." — said of
  `ihost[0-2]`, `ikey[0-2]` and `itag[0-2]`.
- "A router must not publish host or port in the address when publishing
  introducers."
- "A router must publish 4 and/or 6 caps in the address when publishing
  introducers."

## 4. Invariants that must not regress

1. **The proto crate is untouched.** No new I2NP message, no new SSU2 block
   type, no new wire format, no change to `i2pr-proto`.
2. **The direct form is untouched.** An address with `host`/`port` and no
   introducers parses, publishes and qualifies exactly as it does today.
   `is_qualified_ssu2_address` keeps its rule and its callers.
3. **The router's published RouterInfo content does not change in
   production.** The introducer-only form is still never published by this
   plan; `router.version` stays `0.9.62`; the record's `caps` string is
   unchanged; the inventory's advertised count stays **zero**.
4. **The SSU 1 introducer options are refused in an SSU2 address, not
   ignored.** A peer that publishes `ihost0` in an SSU2 address is refused
   with a typed error and counted, exactly as an unknown option is today.
   Silently dropping an option would turn a malformed record into a
   well-formed one that means something else.
5. **A malformed address is refused, never repaired.** No option is
   defaulted, no group is completed from a neighbour's field, no expiry is
   invented.
6. **The transport and the netdb stay runtime-neutral.** No `tokio::*`, no
   `std::net`, no `std::fs` in `i2pr-transport-ssu2` or `i2pr-netdb`.
7. **Every refusal is typed and countable.** The new refusals have their own
   error variants; the existing counters keep their meaning.
8. **No new configuration key.** The plan changes no config surface.
9. **The reference pins are untouched.** i2pd `635b013a…`, Java
   `9134f808…`; no external router or client is patched, vendored or
   modified.
10. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no early-return-success,
    and no production wire change made to go green.

## 5. Scope

### 5.1 In scope

- The transport's SSU2 introducer **value type**: `(router hash, expiry,
  tag)`.
- The transport's **parser**: `ih`/`iexp`/`itag`, the refusal of the SSU 1
  options in an SSU2 address, and the refusal of `host`/`port` alongside
  introducers.
- The transport's **publication builder**: the specification's form.
- The transport's introducer **table mapping** and the runtime's
  `select_introducers` mapping: the hash and the expiry reach the value.
- The netdb's **qualification and builder** for the introducer-only SSU2
  address form.
- The lane, the guard, the documentation, and the support inventory.

### 5.2 Out of scope

- **The composition.** Asking a peer for a relay tag, recording an
  authenticated introducer, publishing the router's own introducer set,
  and advertising the `C` capability that tells a peer this router can
  introduce. Those are a successor's subject: the form must be right before
  it is used, and this plan does not use it.
- **The public dial scope.** `Ssu2DialTarget::new` remains loopback-only and
  `new_private_lan` remains RFC1918-IPv4-only; that bound is Plan 431's.
- **Plan 433's scope** — the NetDB lookup composition and the OBEP
  consumer — is untouched.
- **`caps=f`** and the floodfill role: Plan 494's, blocked on Plan 493.
- **The SSU 1 transport.** This tree has none; the SSU 1 introducer options
  are refused here, not supported.
- **The withdrawal form's `i` option.** The firewalled snapshot publishes
  `s` and `v` (`crates/i2pr-transport-ssu2/src/publication.rs:380-395`)
  where the specification's unpublished SSU2 address carries `s`, `i` and
  `v`. That is a separate, smaller divergence on a path Plan 520 owns; it
  is recorded as a finding in the closure record, not fixed here.
- **The stock-peer lane.** Blocked on Plan 493's authorized two-host
  topology, and unreachable before the composition lands.

## 6. Required production changes

1. **`crates/i2pr-transport-ssu2/src/address.rs`** — `Ssu2Introducer`
   becomes `(peer_hash: [u8; 32], expires_secs: u64, relay_tag: u32)`; the
   parser reads `ih`/`iexp`/`itag`; the SSU 1 options are refused; the
   `host`/`port`-with-introducers combination is refused; the
   `DirectWithIntroducers` classification goes, because the specification
   forbids the form it describes.
2. **`crates/i2pr-transport-ssu2/src/publication.rs`** — `introducer_snapshot`
   renders `ih`/`iexp`/`itag` and nothing else per introducer.
3. **`crates/i2pr-transport-ssu2/src/introducer.rs`** —
   `validated_introducers` maps the record's hash and expiry into the
   value.
4. **`crates/i2pr-runtime/src/ssu2_peer_relay.rs`** and
   **`crates/i2pr-runtime/src/ssu2_runtime.rs`** — the mapping follows the
   new value type; no behavioural change.
5. **`crates/i2pr-netdb/src/floodfill_role.rs`** and
   **`crates/i2pr-netdb/src/local.rs`** — a second qualification rule and a
   second builder for the introducer-only SSU2 address form, alongside the
   untouched direct rule.

## 7. Design decisions the plan must make explicit

**D1 — The SSU2 introducer tuple is `(router hash, expiry, tag)`.** The
SSU 1 tuple is refused in an SSU2 address. The hash is the **introducer's**
router hash — the peer that issued the tag — because that is what the
specification says and what the pinned i2pd publishes and resolves
(`libi2pd/SSU2.cpp:986`).

**D2 — The endpoint is not published.** The SSU2 introducer group carries
no host and no port, because the specification forbids publishing a host or
a port in an address that publishes introducers. The peer resolves the
introducer by hash through the network database. This is a deliberate
departure from the tree's current value, which carries an endpoint and an
introduction key.

**D3 — `iexp` is optional on read, always written on publish.** The
specification's address table marks the expiry "Optional (even if other
properties for this introducer are present)", while its *Introducers*
section lists it among the options that are present. The parser therefore
**accepts its absence** and **refuses a malformed value when it is
present**; the builder **always writes it**, because the validated record's
expiry is always known. A group missing `ih` or `itag` is refused — those
are not optional.

**D4 — An SSU2 address that carries introducers must not carry `host` or
`port`, and the parser refuses the combination.** The specification forbids
publishing it, the parser is strict by contract, and the combination has no
meaning: the peer would have to choose between a direct endpoint and a set
of introducers, and the specification says it must not have both. The
refusal is typed and counted. The `DirectWithIntroducers` classification is
removed rather than left as an unreachable variant.

**D5 — The SSU 1 options are refused, not ignored.** `ihost`, `iport` and
`ikey` in an SSU2 address are a **malformed** address, not an address with
extra fields. The existing `UnknownOption` refusal already covers them
once the four SSU 1 prefixes are removed from the introducer dispatch; a
dedicated variant is not added, because the specification's rule and the
parser's existing rule then say the same thing.

**D6 — The netdb gains a second rule, and the first is untouched.**
`is_qualified_ssu2_address` stays exactly as it is. A new
`is_qualified_ssu2_introducer_address` governs the introducer-only form:
the style is `SSU2`, `s`, `i`, `v=2`, `mtu` in `1280..=9000` and `caps`
naming the family are present, `host` and `port` are **absent**, and at
least one introducer group is complete with a nonzero tag. The option
allow-set gains `ih`, `iexp` and `itag` and nothing else. A new builder
`build_with_ssu2_introducer_address` refuses any address the new rule
refuses, and carries no floodfill permit — so it cannot produce `caps=f`.

**D7 — The published `caps` string is unchanged.** The introducer-only
form carries the family capability the specification demands (`4` and/or
`6`) and nothing more. The `B` and `C` role capabilities are a successor's
subject: advertising `C` is an advertisement, and this plan advertises
nothing.

**D8 — The plan's subject is the form, and the form is proven against the
references' own source, not against a fixture.** The lane drives the real
parser and the real builder over an address in the specification's form and
over the pinned i2pd's own published bytes.

## 8. Work packages

### WP1 — the transport's SSU2 introducer value is the specification's tuple

`Ssu2Introducer` carries `(peer_hash, expires_secs, relay_tag)`. The
constructor refuses a zero tag, exactly as it does today. The accessors
become `peer_hash()`, `expires_secs()` and `relay_tag()`; the endpoint and
intro-key accessors go. `Debug` stays redacted. The transport's introducer
**record** is unchanged — it already holds the hash and the expiry — and
`validated_introducers` maps them into the new value.

### WP2 — the transport's parser reads the specification's options

`ih<N>` is the introducer's router hash in I2P base64; `iexp<N>` is the
expiry in ASCII seconds; `itag<N>` is the nonzero tag. The SSU 1 prefixes
are removed from the introducer dispatch, so they fall through to the
existing unknown-option refusal. `host`/`port` alongside introducers is
refused with a typed error. The `DirectWithIntroducers` classification is
removed.

### WP3 — the transport's publication builder renders the specification's form

`introducer_snapshot` writes `ih<N>`, `iexp<N>` and `itag<N>` per introducer
and nothing else. The branch's existing structure — introducers present
means no `host`/`port`, and the family capability is added — is unchanged.
`parse_snapshot` round-trips the new form through the strict production
parser.

### WP4 — the netdb qualifies and builds the introducer-only SSU2 address

The new rule and the new builder, beside the untouched direct rule and
builder. The builder carries no floodfill permit. A record it builds is an
ordinary router's record.

### WP5 — the lane, the guard, the documentation and the inventory

- A **lane** that drives the real parser and the real builder: the
  specification's form parses; the pinned i2pd's published bytes parse; the
  SSU 1 options are refused; the combination is refused; the builder's
  output round-trips; the netdb's new builder accepts exactly what its rule
  accepts.
- A **guard** that pins the form structurally: the SSU 1 prefixes are gone
  from the transport, the specification's prefixes are present, the SSU 1
  options are refused rather than ignored, the publication builder emits
  the specification's option names, the netdb's direct rule is unchanged,
  and the plan's boundaries hold. Every rule fails closed, and every rule
  carries an inverted probe.
- The **documentation**: the transport's deep-dive, the netdb's deep-dive,
  `specs/protocols/09-ssu2.md`, `specs/CONFORMANCE.md`,
  `specs/QUALIFICATION-GATES.md`, and `specs/support.toml`.

## 9. Failure, cancellation, restart, and contention semantics

- The parser is pure and has no state to lose. A refused address leaves
  nothing behind.
- The introducer **value** is `Copy` and bounded; the table that holds the
  records is already bounded, already has an expiry, and already reaps on
  selection.
- The **expiry** is carried, not computed: the value's `expires_secs` is
  the record's expiry, and the table's own reaping decides what is live.
  The value never invents an expiry.
- A **restart** loses the table, as it does today; the publication's own
  re-publication re-establishes the form.
- **Contention**: the mapping takes the table's lock once and releases it
  before the value is built, exactly as `select_introducers` does today.

## 10. Compatibility and migration review

- **Wire**: the SSU2 introducer group changes shape. No production writer
  emits it, so no production wire changes; a reader that meets the SSU 1
  form in an SSU2 address is now refused where it was previously accepted
  and silently mis-read. That is the intended correction: the previous
  acceptance was wrong.
- **Published record**: unchanged in production. `router.version` stays
  `0.9.62`; `caps` is unchanged; the advertised count stays zero.
- **API**: `Ssu2Introducer`'s field set and accessor names change. The
  crate is workspace-internal; the two production call sites are updated in
  the same change.
- **Configuration**: no key added, removed or changed.
- **No migration**: no persisted state carries the SSU 1 introducer form.
  The peer-profile projection, the RouterInfo projection and the netdb's
  stores carry RouterInfos, not address option names, and a stored
  RouterInfo is re-parsed on use.

## 11. Required tests

### The transport's value and parser

- The specification's form parses: `ih0`/`iexp0`/`itag0` with a nonzero tag
  and a valid base64 hash, and the value carries the hash, the expiry and
  the tag.
- A group with a zero tag is refused.
- A group with a malformed hash is refused.
- A group with a malformed expiry is refused; an **absent** expiry is
  accepted.
- A group missing `ih` or `itag` is refused.
- The SSU 1 options `ihost0`, `iport0` and `ikey0` are each **refused** in
  an SSU2 address.
- An address with `host`/`port` **and** introducers is refused.
- An address with more than three introducer groups is refused.
- The direct form is unaffected: it still parses, and it still classifies
  as direct.

### The transport's publication builder

- The builder's introducer-only snapshot contains `ih0`, `iexp0` and
  `itag0` and contains **no** `ihost`, `iport` or `ikey`.
- The snapshot contains no `host` and no `port`.
- The snapshot carries the family capability.
- The snapshot round-trips through the strict production parser.
- The direct snapshot is byte-for-byte unchanged.

### The netdb

- The new rule accepts exactly the introducer-only form the plan
  describes and refuses the direct form, a form with `host`/`port`, a form
  with no introducer group, and a form with an unknown option.
- The new builder accepts what the rule accepts and refuses what it
  refuses.
- The new builder cannot produce `caps=f`.
- `is_qualified_ssu2_address` is unchanged: its existing cases still hold.

### The negative controls

- The guard's every rule is inverted: each mutation of the real tree must
  fail the rule it attacks, and the tree must be restored byte-for-byte.
- The lane must fail before the fix and pass after it, on the same tree.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
```

The plan's own lane and guard are added to the routine floor with the
names `plan523-ssu2-introducer-lane` and `plan523-ssu2-introducer-boundary`.

## 13. Documentation updates

- `docs/architecture/i2pr-transport-ssu2.md` — the introducer group's form.
- `docs/architecture/i2pr-netdb.md` — the second qualification rule and the
  second builder.
- `specs/protocols/09-ssu2.md` — Plan 523's scope.
- `specs/CONFORMANCE.md` — the inventory paragraph and a Plan 523 section.
- `specs/QUALIFICATION-GATES.md` — the plan's gates.
- `specs/support.toml` — one new `experimental`, non-advertised surface for
  the SSU2 introducer address form.
- `plans/subsystems/network-participation-roadmap.md` — the milestone row
  and the current-state paragraph.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. The transport's SSU2 introducer value is `(hash, expiry, tag)` and the
   SSU 1 tuple is gone from the tree.
2. The parser reads the specification's options, refuses the SSU 1 ones,
   and refuses `host`/`port` alongside introducers.
3. The publication builder renders the specification's form and the
   direct snapshot is byte-for-byte unchanged.
4. The netdb has a second rule and a second builder for the introducer-only
   form, and the direct rule and builder are unchanged.
5. The lane and the guard are green, every guard rule has an inverted probe
   that fires it, and the tree is restored byte-for-byte after each.
6. The routine floor is green on the closing tree.
7. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero**.
8. `router.version` stays `0.9.62`; the published RouterInfo's `caps` is
   unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical.
9. No boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened,
if:

- correcting the form turns out to require a change to the proto crate, to
  a wire block, or to the published RouterInfo's content;
- the netdb's second rule cannot be written without weakening the direct
  rule;
- the references disagree with the specification on the tuple's shape — in
  which case the disagreement is recorded and the plan is re-derived against
  the specification, not against the references;
- the composition turns out to be required for the form to be provable — in
  which case the composition becomes this plan's subject and the scope
  changes explicitly rather than by drift.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the tests it broke, and
  the byte-for-byte restoration.
- The findings by severity, each either fixed or recorded with its owner.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the **form**. The composition — asking for a tag,
  recording an introducer, publishing the set, advertising `C` — is
  explicitly **not** this plan's, and the closure record must say so
  plainly so that a successor does not mistake the two.
- Plan 490's closure record and registry row describe "the specification's
  introducer-only address form". **That description is not accurate of the
  tree it describes**, and the form it delivered is the SSU 1 form. The
  closed record is **not** rewritten; the correction lives in this plan's
  closure record and in the registry's row for this plan.
- Plan 520's closure record recorded the introducer-only form as a finding
  and did not work around it. This plan discharges the **form** half of it.

## 17. Amendment log

### 17.1 — 2026-11-09, registration

Plan 523 is registered as the corrective on the SSU2 introducer address
form: the transport's value, parser and publication builder move to the
specification's `ih`/`iexp`/`itag` tuple, the SSU 1 options are refused in
an SSU2 address, and the netdb gains the introducer-only qualification and
builder. The composition is out of scope and is a successor's subject.

The citations in §3 were verified against the tree at registration
(commit `86635da5`), against the pinned i2pd checkout at
`635b013a612ff47278ef02acf8580a28e10e26c5`, against the pinned Java I2P
checkout at `9134f808337b401e8e53c73734c81fab04280c9d`, and against the
SSU2 specification at <https://i2p.net/en/docs/specs/ssu2/>.
### 17.2 — 2026-11-09, the citation drift in section 3

Section 3 is **evidence**: it records the tree as it stood at registration,
before this plan's own commits, and it is **not** rewritten. The
implementation commits moved the code that section 3 points at, so some of
its line numbers now land elsewhere, and two of them land on code that no
longer exists at all.

This subsection is the additive pointer table. Every code citation in
section 3 is listed with its current location in the tree at commit
`22464cfc` and a verdict. A verdict of **holds** means the citation still
points at the code it described. A verdict of **drifted** means the code
still exists but moved, or changed shape. A verdict of **removed** means the
thing the citation described is gone — and that is the *intended* outcome
of this plan, not a defect in it.

| Section 3 citation | What it described | Current location | Verdict |
|---|---|---|---|
| `crates/i2pr-transport-ssu2/src/address.rs:467-473` | the `Ssu2Introducer` fields | `address.rs:485-489` | **drifted** — the fields are now `(peer_hash, expires_secs, relay_tag)` |
| `crates/i2pr-transport-ssu2/src/address.rs:492-510` | the endpoint accessor surface | `address.rs:495-508` (`new`) and `:510-524` (the accessors) | **drifted** — `endpoint()`, `ip()`, `port()` and `intro_key()` are gone |
| `crates/i2pr-transport-ssu2/src/address.rs:1001-1004` | the four option prefixes | `address.rs:1018-1022` | **drifted** — the prefixes are `ih`/`iexp`/`itag` |
| `crates/i2pr-transport-ssu2/src/address.rs:1006-1019` | `parse_introducer_index` | `address.rs:1027-1042` | **drifted** — the same function over the new prefix set |
| `crates/i2pr-transport-ssu2/src/address.rs:994` | the general `UnknownOption` refusal | `address.rs:1010` | **drifted** by 16 lines; the arm is unchanged |
| `crates/i2pr-transport-ssu2/src/address.rs:1035` | the introducer group's `UnknownOption` refusal | `address.rs:1059` | **drifted** — the arm now dispatches the three SSU2 prefixes |
| `crates/i2pr-transport-ssu2/src/address.rs:539` | the `DirectWithIntroducers` variant | — | **removed** — the variant no longer exists; the enum is `address.rs:543-550` |
| `crates/i2pr-transport-ssu2/src/address.rs:679` | `address_class()` | `address.rs:682-692` | **drifted** — the classification is now total |
| `crates/i2pr-transport-ssu2/src/publication.rs:325-356` | `introducer_snapshot` | `publication.rs:326-364` | **drifted** by one line — the same function, a new body |
| `crates/i2pr-transport-ssu2/src/publication.rs:255-265` | the introducer/direct branch | `publication.rs:255-265` | **holds** |
| `crates/i2pr-transport-ssu2/src/publication.rs:364-379` | the family capability helper | `publication.rs:372-387` | **holds** — `caps_with_family` |
| `crates/i2pr-runtime/src/ssu2_peer_relay.rs:893` | `select_introducers` | unchanged | **holds** |
| `crates/i2pr-transport-ssu2/src/introducer.rs:249-256` | `validated_introducers` | `introducer.rs:253-256` | **drifted** by 4 lines; the same function |
| `crates/i2pr-transport-ssu2/src/introducer.rs:69-76` | `IntroducerRecord` | unchanged | **holds** |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2384-2438` | `introducer_publication_material` | unchanged | **holds** |
| `crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101-1119` | `note_authenticated_introducer` | unchanged | **holds** |
| `crates/i2pr-daemon/src/self_publication.rs:125`, `:157`, `:186`, `:224`, `:270` | the `NoIntroducers` counter | unchanged | **holds** |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2390` | the `NoIntroducers` producer | unchanged | **holds** |
| `crates/i2pr-netdb/src/floodfill_role.rs:195-258` | the direct qualification rule | `floodfill_role.rs:197-261` | **drifted** by 2 lines; the rule itself is unchanged |
| `crates/i2pr-netdb/src/floodfill_role.rs:200-212` | the rule's host/port demand | `floodfill_role.rs:202-214` | **drifted** by 2 lines |
| `crates/i2pr-netdb/src/floodfill_role.rs:253-257` | the rule's option allow-set | `floodfill_role.rs:255-259` | **drifted** by 2 lines |
| `crates/i2pr-netdb/src/local.rs:265-277` | `build_with_ssu2_address` | `local.rs:268-279` | **drifted** by 3 lines; the builder is unchanged |
| `crates/i2pr-daemon/src/service_product.rs:7759` | the daemon's dial-path parse | unchanged | **holds** |
| `crates/i2pr-daemon/src/router_i2np.rs:2108` | the daemon's identity-path parse | unchanged | **holds** |
| `crates/i2pr-daemon/src/floodfill.rs:238` | the floodfill service's identity parse | unchanged | **holds** |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1649`, `:3556`, `:4448` | the runtime's peer-address parses | unchanged | **holds** |
| section 3.7's i2pd, Java I2P and specification citations | the reference behaviour | the external checkouts at the pins | **holds** — the pins are unchanged |

Two of section 3's citations are the substance of the plan and are
therefore **removed** rather than merely moved: the `DirectWithIntroducers`
variant and the SSU 1 option prefixes. A reviewer who follows the section 3
line numbers into the tree today will not find the SSU 1 form, and should
read that absence as this plan's result rather than as a missing file.

### 17.3 — 2026-11-09, the index of the new production items

Section 3's pointer table answers "where did the code this plan cited go?".
This subsection answers the other direction: "what did this plan add?".
Every production item this plan introduced or changed is listed with its
location in the tree at commit `22464cfc`, so that a reviewer can navigate
from the plan to the implementation without reading section 3's superseded
lines at all.

**The transport** (`crates/i2pr-transport-ssu2/src/`):

| Item | Location | What it is |
|---|---|---|
| `Ssu2AddressError::HostPortWithIntroducers` | `address.rs:137` | the typed refusal of an endpoint published alongside introducers |
| `Ssu2Introducer` | `address.rs:485-489` | the specification's tuple: a router hash, an expiry, a relay tag |
| `Ssu2Introducer::new` | `address.rs:495-508` | the constructor, which refuses a zero relay tag |
| `peer_hash` / `expires_secs` / `relay_tag` | `address.rs:511` / `:517` / `:522` | the accessors, replacing the endpoint accessors |
| `Ssu2AddressClass` | `address.rs:543-550` | three variants: `Direct`, `IntroducerOnly`, `UnpublishedStatic` |
| `address_class()` | `address.rs:682-692` | the total classification |
| the endpoint-with-introducers refusal | `address.rs:758-764` | the parser's exclusive-forms check |
| `INTRODUCER_HASH_PREFIX` / `INTRODUCER_EXPIRY_PREFIX` / `INTRODUCER_TAG_PREFIX` | `address.rs:1018` / `:1020` / `:1022` | `ih`, `iexp`, `itag` |
| `INTRODUCER_HASH_LENGTH` | `address.rs:1025` | the exact byte length of a router hash |
| `parse_introducer_index` | `address.rs:1027-1042` | the dispatch over the three prefixes |
| `parse_introducers` | `address.rs:1079-1125` | the per-group assembly, refusing a partial group |
| `parse_introducer` | `address.rs:1127-1147` | one group: the hash, the expiry and the nonzero tag |
| `parse_introducer_expiry` | `address.rs:1153-1161` | a canonical decimal, with no leading zeros |
| `introducer_snapshot` | `publication.rs:326-364` | the publication builder's `ih`/`iexp`/`itag` rendering |
| `has_direct_endpoint` | `publication.rs:173` | matches only the `Direct` class |
| the introducer marker | `handshake.rs:975-977` | `ih0`, `iexp0`, `itag0` |
| `validated_introducers` | `introducer.rs:253-256` | maps the record's hash, expiry and tag |

**The netdb** (`crates/i2pr-netdb/src/`):

| Item | Location | What it is |
|---|---|---|
| `is_qualified_ssu2_introducer_address` | `floodfill_role.rs:277` | the second qualification rule: no host, no port, a complete group |
| `MAX_SSU2_PUBLISHED_INTRODUCERS` | `floodfill_role.rs:374` | the ceiling on published introducers |
| `IntroducerOptionKind` | `floodfill_role.rs:378-385` | the option vocabulary the rule reads |
| `introducer_option` | `floodfill_role.rs:392-407` | classifies one indexed introducer option key |
| `build_with_ssu2_introducer_address` | `local.rs:292` | the builder for the introducer-only form; it takes no floodfill permit |
| the export | `lib.rs:92-93` | both qualification rules are public |

**The tests**: `crates/i2pr-daemon/tests/ssu2_introducer_address.rs` (the
lane, 11 rows) and
`crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs` (the guard,
13 rules and 13 inverted probes).

**The documentation**: `specs/support.toml` (the
`ssu2.introducer-address-form` surface), `specs/CONFORMANCE.md`,
`specs/protocols/09-ssu2.md`, `specs/QUALIFICATION-GATES.md` (section 22)
and `docs/architecture/i2pr-transport-ssu2.md`.
