# Plan 523 — the SSU2 introducer set is the specification's form

**Closure record.** The plan of record is
`plans/implementation/network-participation/523-the-ssu2-introducer-set-is-the-specifications-form.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 523 is **closed**. The SSU2 **introducer set** — the peers that will
introduce this router to others — is now the **specification's own form**: the
introducer's router hash, its expiry and its relay tag, published as `ih[N]`,
`iexp[N]` and `itag[N]`.

The tree had implemented the **SSU 1** form. The SSU2 specification says of
`ihost[N]`, `iport[N]` and `ikey[N]` that those options "are for SSU only and
are not used for SSU2", and the SSU2 tuple is `ih[N]` + `iexp[N]` + `itag[N]`
— the form the pinned i2pd 2.61.0 publishes and parses
(`libi2pd/RouterInfo.cpp:1402`, `:1413`, `:1424`; `:353`, `:359`, `:361`) and
the pinned Java I2P 2.13.0 marks with `itag0`
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2335`,
`:2576`).

Classification: **corrective** (the transport's SSU2 introducer form). The
router's parsing, publication and netdb qualification of the introducer set
now use the specification's tuple; the composition that would *use* the set is
explicitly not this plan's.
**Subsystem:** network-participation. **Closure date:** 2026-11-09.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the published
RouterInfo's **content** stay unchanged, the inventory's advertised count stays
**zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.

## What the plan closed

The tree's SSU2 introducer form was the SSU 1 form. Two defects followed, and
the first was **production-reachable**.

**The first defect: the router could not read a real peer's introducer set.**
The parser refused `ih` and `iexp` as unknown options, and that refusal is
reached by the daemon's dial path
(`crates/i2pr-daemon/src/service_product.rs:7759`), the daemon's identity path
(`crates/i2pr-daemon/src/router_i2np.rs:2108`), the floodfill service's
identity (`crates/i2pr-daemon/src/floodfill.rs:238`) and three runtime sites
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1649`, `:3556`, `:4448`). A peer
that publishes introducers — which a firewalled router must, and which the
pinned i2pd does — was refused outright, by every path that parses a peer
address.

**The second defect: the router's own introducer-only address, were it
published, would have been unreadable.** The publication builder emitted
`ihostN`/`iportN`/`ikeyN`/`itagN`, and the pinned i2pd reads none of them, so
a record this router signed would have shown the peer **no introducers at
all**.

What the plan closed, concretely:

- **The transport's introducer value.**
  `crates/i2pr-transport-ssu2/src/address.rs:485-489` — `Ssu2Introducer` is now
  `(peer_hash: [u8; 32], expires_secs: u64, relay_tag: u32)`. The endpoint and
  the introduction key are gone, because in SSU2 a peer resolves an introducer
  **by hash** through the network database
  (`libi2pd/SSU2.cpp:986` — `FindRouter(introducer.iH)`), and the
  specification forbids publishing a host or a port alongside introducers.
  The accessors are `peer_hash` (`:511`), `expires_secs` (`:517`) and
  `relay_tag` (`:522`); the endpoint accessors are removed. The `Debug` impl
  remains fully redacted (`:527-535`).
- **The parser.** `address.rs:1018-1022` — the three prefixes are
  `INTRODUCER_HASH_PREFIX` (`ih`), `INTRODUCER_EXPIRY_PREFIX` (`iexp`) and
  `INTRODUCER_TAG_PREFIX` (`itag`), with `INTRODUCER_HASH_LENGTH` (`:1025`).
  `parse_introducer_index` (`:1027-1042`) dispatches over exactly those
  three, so a peer's `ih0` and `iexp0` are read rather than refused.
  `parse_introducers` (`:1079-1125`) assembles the groups and refuses a
  partial one; `parse_introducer` (`:1127-1147`) reads one group — a 32-byte
  non-zero hash, an optional expiry and a non-zero tag;
  `parse_introducer_expiry` (`:1153-1161`) demands a canonical decimal.
- **The SSU 1 options are refused, not ignored.** They now fall through to the
  ordinary `UnknownOption` arm (`:1010`) exactly as any unrecognised option
  does. The specification says they are "for SSU only"; silently dropping one
  would turn a malformed record into a well-formed one that means something
  else.
- **The endpoint-with-introducers combination is refused.**
  `Ssu2AddressError::HostPortWithIntroducers` (`:137`) is raised at `:758-764`.
  The specification forbids publishing a host or a port in an address that
  publishes introducers, so the two forms are exclusive and the combination is
  refused rather than resolved by a preference.
- **`DirectWithIntroducers` is gone.** The class is now three variants
  (`:543-550`) — `Direct`, `IntroducerOnly`, `UnpublishedStatic` — and
  `address_class()` (`:682-692`) is total. The variant was left unreachable by
  the refusal above, and an unreachable variant is a trap for the next reader.
- **The publication builder renders the specification's form.**
  `crates/i2pr-transport-ssu2/src/publication.rs:326-364` — one `ih{index}`,
  one `iexp{index}` (omitted when the expiry is zero, which would otherwise
  read as the epoch) and one `itag{index}` per introducer, and nothing else.
  The branch's *structure* is unchanged: introducers present means `host`/`port`
  are omitted and the family capability is added.
- **The introducer table's mapping.**
  `crates/i2pr-transport-ssu2/src/introducer.rs:253-256` — the record's hash
  and expiry reach the value. The record already held them
  (`introducer.rs:69-76`); the mapping threw them away.
- **The handshake's introducer marker.**
  `crates/i2pr-transport-ssu2/src/handshake.rs:975-977` — the check that named
  `ihost0` could never fire once the vocabulary changed, so it now names `ih0`,
  `iexp0` and `itag0`.
- **The netdb's second qualification rule.**
  `crates/i2pr-netdb/src/floodfill_role.rs:277` —
  `is_qualified_ssu2_introducer_address` demands the `SSU2` style, **no**
  `host` and **no** `port`, a canonical `mtu`, `v == "2"`, a `caps` string of
  only `4` and/or `6`, a `s` and an `i` that decode to 32 non-zero bytes, an
  option vocabulary of the SSU2 form only, and at least one **complete**
  introducer group. It is a **second** rule, beside the direct one
  (`:197-261`), and it never weakens the direct rule: the direct rule still
  demands a host and a port, and the introducer rule forbids them. The
  ceiling is `MAX_SSU2_PUBLISHED_INTRODUCERS` (`:374`) — three, the
  specification's own `[0-2]`.
- **The netdb's second builder.**
  `crates/i2pr-netdb/src/local.rs:292` — `build_with_ssu2_introducer_address`
  builds a signed record for the introducer-only form, and it takes **no
  floodfill permit**, so it cannot produce `caps=f`. The direct builder
  (`:268-279`) is untouched. Both rules are exported at
  `crates/i2pr-netdb/src/lib.rs:92-93`.

### The registered plan's corrections

The implementation made no correction to the registered text's substance. The
plan's §3 recorded the tree as it stood at registration; the implementation
moved the code that §3 points at, so §3's line numbers drifted. §3 is
**evidence** and is **not** rewritten. The drift is recorded additively in the
plan's §17.2, which carries a pointer table for **every** section 3 citation:
where it lives now, and whether it **holds**, **drifted**, or was **removed**.
§17.3 carries the complementary index — every production item this plan added,
with its location — so a reviewer can navigate from the plan to the
implementation without reading §3's superseded lines at all.

Two of section 3's citations are **removed** by design: the
`DirectWithIntroducers` variant and the SSU 1 option prefixes. A reviewer who
follows §3's line numbers into the tree today will not find the SSU 1 form,
and should read that absence as this plan's result.

## What the plan did not close

- **The composition.** The router still never asks a peer for a relay tag
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2187`), never records an
  authenticated introducer
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101`), never publishes an
  introducer set (`ssu2_runtime.rs:2384`), and never advertises the `C`
  capability that would tell a peer this router can introduce. A received
  `SessionEvent::RelayTag` is counted and handed off but never recorded as an
  introducer (`ssu2_runtime.rs:6316-6326`). This is **Plan 524's** subject,
  and the sites are already located.
- **The stock-peer half.** Blocked on Plan 493's authorized two-host topology,
  and unreachable before the composition lands.
- **The withdrawal form's `i` option.** The firewalled snapshot publishes `s`
  and `v` (`crates/i2pr-transport-ssu2/src/publication.rs:389-398`) where the
  specification's unpublished SSU2 address carries `s`, `i` and `v`. A
  separate, smaller divergence on a path Plan 520 owns.
- **The public dial scope.** `Ssu2DialTarget::new` remains loopback-only and
  `new_private_lan` remains RFC1918-IPv4-only; that bound is Plan 431's.
- **Plan 433's scope** and **Plan 494's `caps=f`** are untouched.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `66cb8d94` | the plan of record, its registry row and its roadmap row |
| `4d05af04` | the transport's value, parser, publication builder and table mapping (WP1–WP3) |
| `fd037378` | the netdb's second qualification rule and second builder (WP4) |
| `89d8b931` | the lane and the guard (WP5a) |
| `22464cfc` | the documentation, the support inventory and the deep-dive (WP5b) |
| `a2deafd2` | the plan's amendment log: the citation drift and the item index |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the proto crate is untouched | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_13_the_proto_crate_carries_no_introducer_vocabulary` |
| 2 | the direct form is untouched | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_direct_form_is_unchanged`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_09_the_netdb_direct_rule_is_unchanged` |
| 3 | the published RouterInfo's production content does not change | `specs/support.toml`, the `ssu2.introducer-address-form` surface is `advertised = false`; `Cargo.toml` and `Cargo.lock` are byte-identical |
| 4 | the SSU 1 options are **refused**, not ignored | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_ssu1_introducer_options_are_refused_not_ignored`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_01_the_ssu1_introducer_options_are_absent` |
| 5 | a malformed address is refused, never repaired | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_netdb_refuses_every_malformed_introducer_form` |
| 6 | the transport and the netdb stay runtime-neutral | `bash scripts/check-runtime-boundaries.sh` — PASS |
| 7 | every refusal is typed and countable | `crates/i2pr-transport-ssu2/src/address.rs:137`, `Ssu2AddressError::HostPortWithIntroducers` |
| 8 | no new configuration key | `git diff 66cb8d94..a2deafd2 -- crates/i2pr-daemon/src/config.rs` — empty |
| 9 | the reference pins are untouched | no external router or client is patched, vendored or modified; the pins are unchanged |
| 10 | no boundary script is weakened | `git diff 66cb8d94..a2deafd2 -- scripts/` — empty |
| 11 | the value carries the specification's tuple and no endpoint | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_specifications_introducer_form_parses_and_carries_the_tuple`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_03_the_introducer_value_carries_the_specification_tuple` |
| 12 | the pinned reference's own published bytes parse | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_pinned_references_published_bytes_parse` |
| 13 | an endpoint alongside introducers is refused | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `an_address_with_both_an_endpoint_and_introducers_is_refused`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_04_the_parser_refuses_an_endpoint_with_introducers` |
| 14 | `DirectWithIntroducers` is gone | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_05_the_direct_with_introducers_class_is_gone` |
| 15 | the publication builder emits the specification's names | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_transport_publication_builder_emits_the_specifications_option_names`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_06_the_publication_builder_emits_the_specification_options` |
| 16 | the table's mapping carries the hash and the expiry | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_07_the_introducer_table_maps_the_hash_and_expiry` |
| 17 | the handshake's marker names the specification's options | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_08_the_handshake_marker_names_the_specification_options` |
| 18 | the netdb's introducer rule forbids an endpoint | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_10_the_netdb_introducer_rule_forbids_an_endpoint` |
| 19 | the netdb's introducer rule requires a complete group | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_11_the_netdb_introducer_rule_requires_a_complete_group` |
| 20 | the netdb's introducer builder carries no floodfill permit | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_netdb_introducer_builder_cannot_advertise_the_floodfill_role`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_12_the_netdb_introducer_builder_carries_no_permit` |
| 21 | the netdb's two rules are exclusive | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_two_netdb_rules_are_exclusive` |
| 22 | the netdb's rule accepts the specification's form | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_netdb_accepts_the_specifications_form` |
| 23 | the netdb's builder builds a signed record | `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_netdb_builds_a_signed_record_from_the_introducer_form` |
| 24 | a rule whose named item is absent **fails closed** | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `the_guard_reports_a_missing_named_item` |
| 25 | every guard rule is **inverted-proven** | `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `probe_01` … `probe_13` |
| 26 | the introducer's `Debug` stays redacted | `crates/i2pr-transport-ssu2/src/address.rs:527-535` |

## Focused verification

```text
cargo test --locked -p i2pr-daemon --test ssu2_introducer_address -- --test-threads=1
    11 passed; 0 failed
cargo test --locked -p i2pr-daemon --test ssu2_introducer_address_boundary -- --test-threads=1
    27 passed; 0 failed
cargo test --locked -p i2pr-transport-ssu2 --all-targets
    229 passed; 0 failed
cargo test --locked -p i2pr-netdb --all-targets
    438 passed; 0 failed
cargo check --locked --workspace --all-targets
    clean
cargo fmt --all --check
    clean
cargo clippy --locked -p i2pr-transport-ssu2 -p i2pr-netdb -p i2pr-runtime -p i2pr-daemon --all-targets --all-features -- -D warnings
    clean
```

## The routine floor

**82 steps, 82 PASS, 0 FAIL.** The reconciliation is
exact: the script declares its steps and the log carries one result for each,
the step names match one for one, and no step failed.

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

## The negative controls

Every guard rule has an **inverted probe** that applies a mutation and
asserts the rule **fires**. The mutations are applied to an **in-memory copy**
of the source — `source_of(...)` reads the file into a `String` and the
mutation is a `String::replace` on that value — so **no file on disk is ever
written**: the restoration is byte-for-byte by construction, not by a
cleanup step that could be skipped.

| Probe | Mutation applied to the in-memory copy | Rule it must break |
| --- | --- | --- |
| `probe_01_the_ssu1_introducer_options_are_absent_fires` | `INTRODUCER_HASH_PREFIX` becomes `"ihost"` | 01 — the SSU 1 introducer options are absent |
| `probe_02_the_introducer_dispatch_names_the_specification_options_fires` | the `INTRODUCER_EXPIRY_PREFIX` arm is deleted from the dispatch | 02 — the dispatch names the specification's three options |
| `probe_03_the_introducer_value_carries_the_specification_tuple_fires` | an `endpoint: Ssu2Endpoint` field is added to `Ssu2Introducer` | 03 — the value carries the specification's tuple and no endpoint |
| `probe_04_the_parser_refuses_an_endpoint_with_introducers_fires` | `HostPortWithIntroducers` is renamed `UnusedVariant` | 04 — the parser refuses an endpoint with introducers |
| `probe_05_the_direct_with_introducers_class_is_gone_fires` | the `DirectWithIntroducers` variant is re-inserted into the class enum | 05 — the class is gone |
| `probe_06_the_publication_builder_emits_the_specification_options_fires` | `format!("ih{index}")` becomes `format!("ihost{index}")` | 06 — the publication builder emits the specification's options |
| `probe_07_the_introducer_table_maps_the_hash_and_expiry_fires` | `record.peer_hash` becomes `record.endpoint` | 07 — the table maps the hash and the expiry |
| `probe_08_the_handshake_marker_names_the_specification_options_fires` | `get("ih0")` becomes `get("ihost0")` | 08 — the handshake marker names the specification's options |
| `probe_09_the_netdb_direct_rule_is_unchanged_fires` | the direct rule's option allow-set drops `"i"` | 09 — the netdb's direct rule is unchanged |
| `probe_10_the_netdb_introducer_rule_forbids_an_endpoint_fires` | the introducer rule's `host`/`port` refusal is deleted | 10 — the introducer rule forbids an endpoint |
| `probe_11_the_netdb_introducer_rule_requires_a_complete_group_fires` | the complete-group test becomes `true` | 11 — the introducer rule requires a complete group |
| `probe_12_the_netdb_introducer_builder_carries_no_permit_fires` | a `_permit: &FloodfillAdvertisementPermit` parameter is added to the introducer builder | 12 — the introducer builder carries no permit |
| `probe_13_the_proto_crate_carries_no_introducer_vocabulary_fires` | `const IH0: &str = "ihost0";` is appended to the proto crate's sources | 13 — the proto crate carries no introducer vocabulary |

The guard's own property is `the_guard_reports_a_missing_named_item`: a rule
whose named item is **absent** must be **reported**, not passed. It is the
fail-closed property, and it is asserted twice — once against
`required_item_body` and once against a rule.

## Acceptance criteria

The plan's §14 states nine acceptance criteria. Each is met, and each is met
by evidence rather than by assertion:

| # | Acceptance criterion | Met by |
| --- | --- | --- |
| 1 | the transport's SSU2 introducer value is `(hash, expiry, tag)` and the SSU 1 tuple is gone from the tree | `crates/i2pr-transport-ssu2/src/address.rs:485-489`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_01` and `rule_03` |
| 2 | the parser reads the specification's options, refuses the SSU 1 ones, and refuses `host`/`port` alongside introducers | `address.rs:1018-1022`, `:758-764`; `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_ssu1_introducer_options_are_refused_not_ignored` and `an_address_with_both_an_endpoint_and_introducers_is_refused` |
| 3 | the publication builder renders the specification's form and the direct snapshot is byte-for-byte unchanged | `publication.rs:326-364`; `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_transport_publication_builder_emits_the_specifications_option_names` and `the_direct_form_is_unchanged` |
| 4 | the netdb has a second rule and a second builder for the introducer-only form, and the direct rule and builder are unchanged | `floodfill_role.rs:277`, `local.rs:292`; `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`, `the_netdb_accepts_the_specifications_form`, `the_netdb_builds_a_signed_record_from_the_introducer_form` and `the_two_netdb_rules_are_exclusive`; `crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs`, `rule_09` |
| 5 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and the tree is restored byte-for-byte after each | 11 lane rows and 27 guard tests, all green; the 13 probes above |
| 6 | the routine floor is green on the closing tree | the floor table below |
| 7 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero** | `specs/support.toml`, the `ssu2.introducer-address-form` surface; the inventory reads 116 surfaces, 111 experimental, 4 deferred, 1 ready and **0 advertised** |
| 8 | `router.version` stays `0.9.62`; the published RouterInfo's `caps` is unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical | `git diff 66cb8d94..a2deafd2 -- Cargo.toml Cargo.lock` is empty; the plan adds no version or capability |
| 9 | no boundary script is weakened | `git diff 66cb8d94..a2deafd2 -- scripts/` is empty |

**No stop condition fired.** The proto crate needed no change; the netdb's
second rule was written beside the direct rule without weakening it; the
references agree with the specification on the tuple's shape; and the
composition was **not** required for the form to be provable.

## Invariant review

1. **The proto crate is untouched.** No new I2NP message, no new SSU2 block
   type, no new wire format, no change to `i2pr-proto`. The guard proves it
   structurally: `rule_13_the_proto_crate_carries_no_introducer_vocabulary`
   scans the proto crate for any introducer option name and fails if it finds
   one, and `probe_13` shows the rule fires when the name is introduced.
2. **The direct form is untouched.** An address with `host`/`port` and no
   introducers parses, publishes and qualifies exactly as before.
   `is_qualified_ssu2_address` (`floodfill_role.rs:197-261`) keeps its rule and
   its callers, and `build_with_ssu2_address` (`local.rs:268-279`) keeps its
   signature and its behaviour. The guard's `rule_09` compares the direct rule
   and the direct builder against the shape the plan registered, and the lane's
   `the_direct_form_is_unchanged` drives it.
3. **The router's published RouterInfo content does not change in production.**
   The introducer-only form is still never published by this plan. The
   inventory's advertised count stays **zero**, `router.version` stays
   `0.9.62`, and the record's `caps` string is unchanged.
4. **The SSU 1 introducer options are refused in an SSU2 address, not
   ignored.** They fall through to the ordinary `UnknownOption` arm
   (`address.rs:1010`). The lane drives the refusal directly
   (`the_ssu1_introducer_options_are_refused_not_ignored`) and the guard proves
   the SSU 1 names are absent from the transport's production source
   (`rule_01`).
5. **A malformed address is refused, never repaired.** No option is defaulted,
   no group is completed from a neighbour's field, no expiry is invented. The
   lane's `the_netdb_refuses_every_malformed_introducer_form` drives the whole
   set of malformed forms: no group at all, a host alongside, a zero tag, a bad
   hash, an SSU 1 option, a `caps` carrying an unknown letter, and an index
   beyond the ceiling.
6. **The transport and the netdb stay runtime-neutral.** No `tokio::*`, no
   `std::net`, no `std::fs` in `i2pr-transport-ssu2` or `i2pr-netdb`.
   `scripts/check-runtime-boundaries.sh` passes.
7. **Every refusal is typed and countable.** The new refusal has its own
   variant, `Ssu2AddressError::HostPortWithIntroducers` (`address.rs:137`); the
   existing counters keep their meaning, and no counter's semantics changed.
8. **No new configuration key.** The plan changes no config surface; the
   daemon's configuration source is byte-identical across the plan's commits.
9. **The reference pins are untouched.** i2pd `2.61.0`
   (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
   (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched,
   vendored or modified; the plan reads them and cites them.
10. **No boundary script is weakened.** `scripts/` is byte-identical across the
    plan's commits. No `|| true`, no `continue-on-error`, no filename
    filtering, no early-return-success, and no production wire change was made
    to go green.

## Failure, cancellation, restart, and contention review

- **Failure.** Every malformed introducer form is refused with a typed error
  and no state is left behind: the parser's introducer fields are collected
  into a local `IntroducerFields` and only turned into values once a whole
  group is present, so a half-read group cannot be published.
- **Cancellation and restart.** The plan adds no task, no timer, no listener,
  no socket and no channel. The transport's parsing is a pure function of its
  input and the netdb's rule is a pure predicate over an address, so there is
  nothing to cancel and nothing to restart on either side.
- **Contention.** The plan adds no shared mutable state. The netdb's new rule
  reads only the `RouterAddress` it is given; the new builder borrows the
  builder's own immutable configuration.
- **Boundedness.** The introducer count is bounded at three by
  `MAX_SSU2_PUBLISHED_INTRODUCERS`, which matches the specification's own
  `[0-2]`; the transport's own `MAX_SSU2_INTRODUCERS` still bounds the parser.
  The new hash is a fixed 32 bytes, and the expiry is a `u64`.
- **One local condition is never written into a peer's profile.** The plan
  writes nothing about any peer. The refusals it introduces are the router's
  own condition — "this record does not parse" — and no peer profile, no
  reachability state and no score is touched.

## Migration and compatibility review

- **No wire migration.** The router publishes no introducer set and no
  `caps=f`, so the published RouterInfo's content is byte-identical to what it
  was before this plan. There is nothing for a peer to migrate from.
- **A reading change, not a writing change.** The router's *parser* changed:
  it now reads a form it previously refused. That is strictly an increase in
  what the router accepts, and the SSU 1 options it used to accept are now
  refused — which is the point, and which the specification requires.
- **The direct form's compatibility is total.** A peer that publishes
  `host`/`port` and no introducers is parsed, published and qualified exactly
  as before.
- **A divergence from Java I2P, recorded not hidden.** Java I2P tolerates an
  address that carries both `host`/`port` and introducers; this plan's parser
  **refuses** the combination, because the specification forbids it. The
  divergence is recorded as a finding below.
- **No configuration migration.** No key is added, removed or renamed.

## Security review

- **The introducer's `Debug` stays fully redacted.** `Ssu2Introducer`'s
  `Debug` impl (`address.rs:527-535`) prints `<redacted>` for the hash, the
  expiry and the tag. The plan did not widen it, and it did not add a
  `Display`, a `Serialize` or any other unrestricted rendering.
- **No secret is added and none is removed.** The router hash is a public
  identifier, not a secret; the relay tag is a session-scoped token that the
  specification publishes in the clear, and the value still refuses a **zero**
  tag on construction (`address.rs:500-502`).
- **No key material is logged.** The plan adds no logging, and the values it
  introduces are refused rather than reported on any path.
- **The direct endpoint is not published with introducers.** The refusal is
  what keeps the specification's "must not publish host or port" rule, and it
  is enforced in the parser, not only in the builder — so a peer's malformed
  record cannot smuggle an endpoint into this router's view either.
- **The builder cannot over-claim.** The introducer builder takes no
  floodfill permit, so it cannot sign a record claiming `caps=f`. The
  permission to claim the floodfill role stays with `build_floodfill`, which
  requires an opaque active-role permit.
- **The proto crate gains no wire vocabulary**, so the surface the attacker
  reaches through a new message type is unchanged.

## Limitations

1. **The composition is not done.** The router never asks a peer for a relay
   tag, never records an introducer, never publishes an introducer set, and
   never advertises the `C` capability. The form is right; the *use* of it is
   a successor's subject (Plan 524).
2. **The stock-peer half is not proved and is not claimed.** The lane drives
   the real parser, the real netdb rule and the real netdb builder over the
   specification's form and over the pinned i2pd's own published bytes. It
   does not drive a stock peer, and the stock half needs Plan 493's authorized
   two-host topology.
3. **The published `caps` still carries only the family letter.** The `C`
   capability is not advertised, because advertising a role capability is an
   advertisement and this plan advertises nothing.
4. **The daemon's `withheld_no_introducers` counter is still permanently
   zero.** `crates/i2pr-daemon/src/self_publication.rs:125`, `:157`, `:186`,
   `:224`, `:270` — the variant is produced only by
   `introducer_publication_material` (`ssu2_runtime.rs:2390`), which the
   composition will call. Until then the counter reads zero, which is honest
   but uninformative. The composition's plan must make the counter mean
   something.
5. **The withdrawal form's `i` option is still missing.** The firewalled
   snapshot publishes `s` and `v` where the specification's unpublished SSU2
   address carries `s`, `i` and `v`.
6. **Java I2P's tolerance for endpoint-plus-introducers is a divergence this
   plan does not follow.**
7. **The `iexp` option is optional on read and omitted on publish when the
   expiry is zero.** A peer that omits `iexp` is read with an expiry of zero,
   which reads as "no expiry published" rather than as the epoch. This is the
   specification's own table, which lists the expiry as optional.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

- **L1 — the composition is absent.** The router cannot yet use the form it
  now publishes correctly. Plan 524's subject.
- **L2 — the withdrawal form omits the `i` option.** A separate, smaller
  divergence on a path Plan 520 owns.
- **L3 — this router's parser refuses endpoint-plus-introducers, where Java
  I2P tolerates it.** The specification forbids the combination, so this plan
  follows the specification. A successor that wanted Java's tolerance would
  need a plan-of-record and a reason.
- **L4 — the daemon's `withheld_no_introducers` counter is permanently zero**
  until the composition lands.

### Informational

- **I1 — the registered §3's citations drifted.** The drift is recorded
  additively in the plan's §17.2, which carries a pointer table for every
  citation, and §3 itself is not rewritten. Two of §3's citations are removed
  by design.
- **I2 — the plan's own §17.3 is the index a reviewer should use** to get
  from the plan to the implementation, because it lists every production item
  the plan added.
- **I3 — the guard's rules fail closed.** An absent named item is a defect,
  not a pass, and `the_guard_reports_a_missing_named_item` proves it.

## Roadmap disposition

The roadmap's `network-participation` sheet records **Plan 523** as closed.
The row it carries names the closure.

The consequences that changed:

- the **RouterInfo** row's introducer-form cell no longer says that the
  transport's SSU2 introducer set is the SSU 1 form: the **form** is now the
  specification's, and only the **composition** — publishing the set and
  advertising `C` — remains;
- the roadmap's **completion definition** (§11) still requires that the router
  "publishes the validated introducer set in its RouterInfo address", and that
  half is **not** discharged: this plan corrected the form and did not publish
  the set.

The **Transports**, **Peers**, **NetDb**, **Roles**, **I2NP** and **Clients**
rows are unchanged.

The **stock-peer** half of this plan, Plan 520's stock-peer half, Plan 497's
M2, Plan 487's stock-peer half, and Plans 519–522's stock-peer halves all stay
blocked on **Plan 493's** authorized two-host topology.

## Unblock audit

The unblock audit asks, of every blocked item this line carries, whether this
plan changed its status. The answer is **no**, except for the one item this
plan was opened to discharge.

| Blocked item | Before Plan 523 | After Plan 523 |
| --- | --- | --- |
| The **SSU2 introducer set's form** | the SSU 1 form | **delivered** — the specification's `ih`/`iexp`/`itag` tuple, with the SSU 1 options refused |
| The **composition** — ask for a tag, record an introducer, publish the set, advertise `C` | missing | missing — untouched; a successor's subject |
| Plan 523's stock-peer half | blocked on Plan 493, and unreachable before the composition | blocked on Plan 493, and unreachable before the composition |
| Plan 520's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 521's stock-peer half | blocked on Plan 493, and on the router-level 231 responder | blocked on Plan 493, and on the router-level 231 responder |
| Plan 522's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 497's **M2** / limitation 1 — the stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 487's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plans 519–522's stock-peer halves | blocked on Plan 493 | blocked on Plan 493 |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | blocked on external environments | blocked on external environments — untouched |
| The **dial scope** — the transport's own loopback/RFC1918 profile | a transport boundary | a transport boundary — untouched, with its own plan-of-record needed |
| The no-**NTCP2**-ingress and no-**OBEP**-consumer gaps | missing | missing — untouched |
| The **`caps=f`** advertisement | Plan 494, blocked on Plan 493 | Plan 494, blocked on Plan 493 |
| The **withdrawal form's `i` option** | missing | missing — untouched; a path Plan 520 owns |

Nothing was unblocked that this plan did not itself open, and nothing was
promoted. Plan 493's two-host topology is the gate on every remaining
stock-peer half.

## Handoff notes

- The next free plan number after this one is **524**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- **Plan 490's closure record and registry row describe "the specification's
  introducer-only address form".** That description is **not accurate** of the
  tree it describes, because the form it delivered is the SSU 1 form. The
  closed record is **not** rewritten; the correction lives in this record and
  in this plan's registry row.
- The **composition** is the next thing in this line: asking a peer for a relay
  tag (`crates/i2pr-runtime/src/ssu2_runtime.rs:2187`), recording an
  authenticated introducer
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101`), publishing the router's
  own introducer set (`ssu2_runtime.rs:2384`, with the new netdb builder), and
  advertising the `C` capability
  (`crates/i2pr-transport-ssu2/src/address.rs`, `supports_relay`).
- A received `SessionEvent::RelayTag` is counted and handed off but never
  recorded as an introducer (`ssu2_runtime.rs:6316-6326`); the composition must
  close that.
- The daemon's `withheld_no_introducers` counter (`self_publication.rs:125`,
  `:157`, `:186`, `:224`, `:270`) is permanently zero until the composition
  calls `introducer_publication_material`.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- The **dial-scope** limit is a transport boundary and needs its own
  plan-of-record; it is not discharged here.
