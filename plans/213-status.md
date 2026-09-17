# Plan 213 status — M10 router-backed generic external qualification

Status: **`in-progress-commit-e-local-double-pass-hosted-double-run-pending`**.

Plan of record: [`213-m10-router-backed-generic-external-qualification-and-evidence-corrective.md`](213-m10-router-backed-generic-external-qualification-and-evidence-corrective.md).

## Why this plan exists

Plan 212 source architecture landed, but its ignored generic Direction A/B driver was still a structural scaffold rather than an executable external proof: the Direction A/B success booleans remained immutable `false`, the counted loops did not perform application byte I/O, the current M10 runner normally skipped provisioning the generic reference destination, and several mandatory evidence rows were written as unconditional success literals.

Plan 213 owns only the qualification correction and execution:

```text
complete generic A/B application driver
  -> independent exact-pinned i2pd SAM STREAM reference service
  -> independent i2pd initiator for reverse direction
  -> command/state-derived evidence only
  -> standalone fail-closed Plan 213 runner
  -> hosted exact-head run twice
```

No new router architecture is authorized unless the real external run proves a product defect.

## Commits A–D landed (source side)

- **Commit A (driver + narrow product seams).** The Plan 212 generic driver now performs real local TCP application I/O through the M10 GenericClient listener (two sequential connections — small 24 B + 8192 B multi-packet — exact-length reads, SHA-256 comparison, orderly half-close) while `ServiceProduct::poll_inbound()` keeps pumping concurrently via the bounded `pump_while` combinator. Direction B spawns the independent harness-only SAM fixture in `connect` mode as a subprocess while inbound keeps pumping. All mandatory rows derive from executed I/O, typed product summaries, per-direction counter windows (`before_a`/`after_a`/`after_b`), or subprocess exit codes; no literal success remains (the static checker rejects same-line `"1"` value literals and driver-manufactured pin rows). Nine new `plan213_*` driver unit rows lock the evidence-parser/delta/classification helpers. Narrow read-only product surfaces, all reusing existing accessors: `ServiceProduct::service_destination_public_info` (hash/b32/b64 triple derived from the existing manager `service_destination_b64` public encoding), `ServiceProduct::service_router_network_summary` (existing manager summary enriched with the inbound-owner registry flag), `ServiceProduct::inbound_orphan_receives`, plus `lease_count`/`inbound_owner_registered` fields on `RouterNetworkSummary`. Two manager-level `plan213_*` unit rows prove the public triple is mutually consistent and exposes no secrets.
- **Narrow composition corrective (inside Commit A, Plan 213 §C/§7).** The per-service provisioning loop now publishes server LS2s unconditionally: a server spec carries no configured destination reference, so gating publication on `spec_reference_for_service` left the intended server-publication branch unreachable for every real server (Direction B's ordinary lookup could never succeed by construction). `publish_service_ls2_for_service` additionally completes the previously intent-only dispatch — it now composes the DatabaseStore through the service's real router-backed outbound role via the existing `compose_ls2_publication_via_tunnel` seam and hands every cell to the shared router delivery service, mirroring `resolve_remote_destination_for_service` and the proven destination external lane. No new composition path, no wire change.
- **Commit B (fixture + target facts).** New harness-only `tests/integration/service-tunnels/clients/sam_stream_fixture.py` (`server` mode: SESSION CREATE + fresh-socket STREAM ACCEPT echo with per-connection digest facts; `connect` mode: fresh-socket STREAM CONNECT with self-verified small/large round trips, nonzero exit on any failure). `fixtures/echo_fixture.py` gained the bounded `--facts` target-digest surface (Plan 213 §D).
- **Commit C (standalone runner).** New `tests/integration/service-tunnels/run-plan213-generic.sh`: fresh evidence dir, command-derived exact-pin verification (revision file + checkout HEAD + clean tree + `i2pd --version`), static checker, focused Plan 212/213 unit floors, Plan 193 controlled i2pd profile, fixture-server READY with recomputed public-destination cross-check, echo-target readiness, the ignored driver under explicit selection, sanitized reference-side counts (raw i2pd log stays in scratch), per-row key/value validation including exactly one `P213-N-passed` terminal classification, results.tsv/evidence.json without secrets, fail-closed cleanup.
- **Commit D (checker + workflow).** `scripts/check-service-tunnel-acceptance-evidence.sh` extended with the Plan 213 §27 invariants (scaffold rejection, literal-success rejection, driver pin-row prohibition, extended anti-shadow rule, TCP I/O presence, independent-initiator presence, target-observation presence, separate-window presence, runner pin verification, skip-as-success rejection, no-secret audit, §13 row presence, runner-owned pin rows). `.github/workflows/service-tunnels-external.yml` renamed off the stale Plan 203 title and now offers `local-only` / `router-generic` / `full` lanes without coupling to the Java M6 lane.

## Commit E — execution (local double-pass complete, hosted pending)

The lane proved five narrow product defects (all fixed in production
seams, no wire change, no shadow stack):

1. **Short-transport Garlic u32 framing** (`i2pr-proto/src/i2np/message.rs`):
   `decode_short_transport` now strips the framing length i2pd emits
   around direct ECIES garlics; unit row
   `short_transport_garlic_strips_u32_length_framing`.
2. **AckRequest block tolerance** (`i2pr-proto/src/ecies_payload.rs`):
   `BLOCK_TYPE_ACK_REQUEST=9` parsed/skipped; unit row
   `ack_request_block_is_skipped_and_clove_survives`.
3. **Inbound LS2 mirror** (`sam/streams.rs`):
   `install_remote_lease_set2_into_router_state` mirrors the
   validated inbound LS2 into the bridge-level sender directory and
   the canonical dispatcher registers+binds in
   `with_shared_identity`, so decryptable traffic no longer fails
   with `UnknownDestination`.
4. **Wire Date expirations** (`routing.rs`, `streaming_adapter.rs`,
   `service_tunnels.rs::compose_remote_cells`): I2NP cell/inner
   expirations derive from wall-clock `now_seconds` (monotonic-scale
   dates read as 1970 and i2pd drops every cell).
5. **Server SYN-ACK route cache gap** (`service_tunnels.rs` +
   `sam/streams.rs::cached_router_remote_lease_set2`): the
   authenticated inbound handshake installs the peer LS2 into bridge
   router state but not the backend coordinator cache, so
   `route_outbound_remote_request` failed the SYN-ACK with
   `NotCached`; the route now falls back to the bridge mirror (both
   stores absent still fails closed). Two new `plan213_*`
   manager-level unit rows lock the mirror miss/hit behavior.

Harness hardening: the dirB helper passes `--destination-pub=<b64>`
(equals form — I2P base64 can start with `-`, which the space form
misparses as a flag and fails with a silent argparse exit 2), and
failure-path helper stdout tails are captured bounded into the stop
row for provenance.

Two consecutive local passes on the final working tree (temps
reverted, clippy clean): `P213-N-passed` + `P213-N-passed` with
Direction A digests green and Direction B echo green, plus a third
confirmatory pass after the Plan 213 unit rows + `cargo fmt`
landed. Per §17 the local passes do not substitute for closure:
Plan 213 stays open until the hosted `router-generic` lane passes
twice on the pushed SHA.

## Authority

```text
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = in-progress-commit-e-local-double-pass-hosted-double-run-pending
plan_214 = registered-blocked-by-plan213

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 213 may become passed only after its generic Direction A + Direction B hosted qualification succeeds twice on the same exact source SHA with all mandatory evidence rows command/state-derived (Commit E: `bash tests/integration/service-tunnels/run-plan213-generic.sh` twice on one SHA, or the hosted `router-generic` lane twice).
