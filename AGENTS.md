# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do not
use it for anonymity, privacy, censorship resistance, or any security-sensitive
workload. NTCP2 remains experimental and non-advertised; the production daemon
does not activate NTCP2. SSU2 v2 has a localhost UDP runtime and Plan 161 has
proven both direct authenticated IPv4 directions against exact-pinned i2pd 2.61.0;
Plan 186 has proven daemon-owned NetDB lookup/publication over exploratory
tunnels against the same pin with no LeaseSet2/Streaming claim; no public
advertisement, public-network participation, broad router interoperability,
or Milestone 6 interoperability is claimed. Plan 187 landed the local
destination message plane (27 unit + 9 live two-role rows) with 7
remote rows blocked on the build-reply interop gap pending Plan 188.
Plan 190 isolates and corrects the inbound NetDB reply-path metadata
defect that left 5/7 destination rows blocked after the Plan 188
installs; the corrected reply path is the next external lane run.
Plan 191 ran the inbound-delivery layer and stopped at boundary E
(i2pd-compatible I2CP-style Data body wire-format defect);
Plan 192 is the narrower follow-up that landed the i2pd-compatible
9-byte short-transport inner envelope + i2cp I2CP-style Data body
+ STYLE=RAW SAM session + RAW RECEIVED SIZE=N digest equality
corrective. The inbound-delivery layer is now closed for
exact-pinned i2pd 2.61.0. Plan 193 closed the M6 i2pd
mixed-router Streaming qualification (full Direction A + Direction B
external matrix passed twice on exact head `3687189`); Plan 194 closed
the M6 Java I2P second-family qualification with the bounded SAM-bridge
LS2-publication gap (Java's SAM bridge does NOT auto-publish the
SAM-destination LS2 to the local NetDB in a controlled private topology
where the reference has no peer tunnels to build a client tunnel for
the lease; i2pd's SAM bridge does publish immediately; the gap is a
Java-internal architectural fact, not an i2pr regression). Plan 196
closed the Java controlled first-run topology + authenticated SSU2
preflight; Plan 197 closed the PQ SSU2 option parser tolerance.
Plan 202 closed the M10 production remote Destination/Streaming
 composition (`ServiceTunnelManager` now owns one shared
 `ServiceDestinationDelivery` capability; typed `RoutingDecision`
 enum drives the resolve path; twelve new bounded
 `RemoteDeliveryCounters` observations cover the counted path; the
 `m10_remote_destination_streaming_composition_through_manager`
 Direction A external driver exercises the manager-level
 `install_router_delivery_handle` / `routing_decision_for` seams
 against the exact-pinned i2pd 2.61.0 cache through the dedicated M6
 interop lane). Plan 207 closed the genuine M10 remote HTTP + IRC
 application interop: the
 `m10_genuine_remote_http_and_irc_application_interop` external
 driver replaces the synthetic Plan 203
 `record_remote_application_observation` label-injection pattern
 with command-derived evidence from unmodified application
 clients. The HTTP row is bound to the real system `curl` binary
 spawned as a subprocess against the i2pr HTTP client listener;
 the IRC row is bound to the unmodified exact-pinned jaraco/irc
 public API (`irc.client`) spawned as a subprocess against the
 i2pr IRC client listener. The driver writes the documented Plan
 207 §9 subfact rows plus `plan206-backend-counters` to
 `${EVIDENCE_DIR}/plan207-driver/driver-evidence.tsv`; the
 aggregate pass rows derive purely from the command-derived
 subfact rows; the runner provisions i2pd with one HTTP server
 tunnel + one IRC server tunnel pointing at the harness-owned
 loopback fixtures; the static checker
 `scripts/check-service-tunnel-acceptance-evidence.sh` extended
 with the Plan 207 §10 source-level invariants; the two
 `remote-independent-*` rows flip from `blocked` to `passed` once
 the dedicated M6 interop lane provisions the SSU2 endpoint + bind
 tuple and the driver emits every documented Plan 207 §9 subfact
 row in the same evidence directory/run id; the full M10 lane
 stays fail-closed without it. Plan 208 closed the production
 delivery-driver remote-route integration: the production
 `ServiceTunnelManager::deliver_outbound` sweep now invokes the
 typed `route_outbound_remote_request` seam on the local-miss
 branch, `route_outbound_remote_request` performs a real send
 through the existing `StreamingDestinationAdapter` +
 `deliver_outbound_cells` + `RouterDeliveryService` chain, the
 inbound-owner registry wires inbound data to the actual owning
 service runtime, and the new `m10_remote_route_integration_through_deliver_outbound`
 `#[ignore]`-gated external driver exercises the production
 sweep against the exact-pinned i2pd 2.61.0 cache; seven new
 `plan208_*` manager-level unit rows in
 `service_tunnels.rs::plan208_remote_route_integration_tests` lock
 the integration call graph; `scripts/check-service-tunnel-acceptance-evidence.sh`
 extended with Plan 208 §15 source-level invariants (production
 sweep must invoke the typed remote route, counted driver must
 not construct a parallel `StreamingManager` /
 `StreamingDestinationAdapter`, must never log peer key
 material).
mixed-router Streaming qualification (full Direction A +
Direction B external matrix passed twice on exact head
`3687189`; the historical
`plans/188-m6-mixed-router-streaming-with-i2pd.md`
Streaming file remains historical context only and is superseded
by [`plans/193-m6-i2pd-mixed-router-streaming-qualification.md`](plans/193-m6-i2pd-mixed-router-streaming-qualification.md)).
Plan 194 retained the bounded SAM-bridge LS2-publication finding; Plan 199
reopened final closure with public Java client helpers and a two-router
ordinary-RouterInfo bootstrap. Those helpers connect
through public I2PClient/I2PSession and I2PSocketManager APIs, but the
exact-pinned Java 2.13.0 controlled router still does not return the
public-client LeaseSet2 to the real i2pr DatabaseLookup path. Plan 200
(M6 Java public-client publication observability and verified bootstrap)
decoupled the `leaseset=published` claim from `READY`, added bounded
`REPORT_STATUS`, proved Router A/B main-NetDB bootstrap through ordinary
post-store DatabaseLookup round-trips in both directions, emitted
sanitized Java log keys for the client LS2 lifecycle and
tunnel/floodfill/store/ack selection, and records exactly one terminal
`P200-{A..H}` classification per run. Plan 201 landed the Branch G
`store-acked-remote-lookup-fails` corrective framework: eleven new
sanitized observation counters on `DestinationTunnelCounters`
(`lookup_key_matches`/`_mismatches`,
`floodfill_candidates_present`/`_absent`,
`reply_paths_derived`/`_unresolved`,
`ls2_records_decoded`/`_decode_rejected`/`_signature_rejected`,
`inbound_cells_garlic_completed`/`_incomplete`), a public
`note_lookup_boundary(label, value)` typed observation surface, eight new
`plan201_g_*` unit rows in `destination_tunnel_unit.rs`, six new
`blocked_row` Plan 201 §G entries in `run-java.sh`, and
`scripts/check-m6-mixed-router-acceptance-evidence.sh` extended with §11
+ §12 invariants. Final closure of Plan 201 is blocked on the Plan 200
exact-head external run that records the unambiguous `P200-*`
classification and flips the seven §11 stop rows `blocked → passed`;
the M6 Java second-family claim stays `not-yet-passed`. Plan 196 closed
the Java controlled first-run
topology + authenticated SSU2 preflight (two narrow correctives:
`router.blocklist.enable=false` to bypass the Team Cymru bogon
`127.0.0.0/8` entry, and PRIV-token SAM SESSION CREATE because
Java strictly requires >= 663 decoded bytes while i2pd accepts the
391-byte PUB). Plan 197 (M6 PQ SSU2 option support corrective —
tolerant parse only) has landed its parser-only tolerance: the
exact-pinned Java I2P 2.13.0 `UDPTransport.addSSU2Options`
unconditionally publishes `pq=4,3` (ML-KEM-768 + ML-KEM-512) on every
SSU2 RouterAddress, and the Plan 196 first counted external run
failed at `crates/i2pr-transport-ssu2/src/address.rs:885` with
`Ssu2AddressError::UnknownOption`. Plan 197 added a typed
`Ssu2PqKem`/`PqCapabilities` parser tolerance with bounded
`MAX_SSU2_PQ_SCHEMES = 8`, kept the i2pr session layer classical
X25519 only, kept the i2pr publication path pq-free, and never
implemented, claimed, or silently enabled ML-KEM. Plan 199 was the
unified closure attempt; it exposed two independent blockers that are
now decomposed into Plans 200–204 and recorded as
`superseded-execution-decomposed-and-closed-via-plans200-204` (see
`plans/198-status.md`, `plans/199-status.md`). Plan 200 (M6 Java
public-client publication observability and verified bootstrap) closed
the diagnostic/evidence side of the Java branch: it decouples helper
`READY` from any `leaseset=published` claim, adds bounded
`REPORT_STATUS`, proves Router A/B main-NetDB bootstrap through
ordinary post-store DatabaseLookup round-trips in both directions,
emits sanitized Java log keys for the client LS2 lifecycle and
tunnel/floodfill/store/ack selection, and records exactly one terminal
`P200-{A..H}` classification per run. Plan 201 picks the smallest
standards-compatible corrective from that classification and has
landed its Branch G `store-acked-remote-lookup-fails` corrective
framework; final closure is blocked on the Plan 200 exact-head
external run that records the unambiguous `P200-*` classification and
flips the seven §11 stop rows `blocked → passed`. Plan 202 (M10
production remote Destination/Streaming composition — typed
routing classification surface) is recorded as
`partial-m10-remote-routing-capability-surface-superseded-by-plan206`:
 Plan 202 introduced the typed `RoutingDecision` enum
 (`LocalCoOwned` / `RemoteRouter` / `RemoteUnresolved`), the bounded
 `RemoteDeliveryCounters` surface, and the manager installation
 seam, but the capability was a marker — the manager classified a
 non-local destination as `RemoteRouter` without owning a real
 backend to deliver through. Plan 206 (M10 production remote
 delivery composition corrective) attached the executable
 `RemoteDestinationBackend` (shared LeaseSet2 coordinator +
 authenticated router delivery service) to the
 `ServiceDestinationDelivery` capability so the manager routes
 its own queued Streaming requests through the real Plan 184–193
 router stack: typed `route_outbound_remote_request` /
 `dispatch_inbound_to_owned_destination` /
 `register_inbound_destination_owner` /
 `resolve_remote_lease_set2` seams; atomic inbound-owner
 registration with a fail-closed duplicate guard; three operation-
 boundary counters (`remote_lookup_cache_hit`,
 `remote_outbound_composed`, `remote_inbound_dispatched`) that
 advance only through typed backend seams and reject the external
 `record_observation` helper so a positive observation cannot be
 manufactured without the production operation; seven new
 `plan206_*` manager-level unit rows in
 `service_tunnels.rs::plan206_remote_composition_tests` lock the
 typed path; the static checker
 `scripts/check-service-tunnel-acceptance-evidence.sh` extended
 with the Plan 206 §13 source-level invariants. The legacy marker
 shape keeps `RemoteUnresolved` so a silent local fallback for a
 remote peer cannot regress. Plan 208 (M10 production
 delivery-driver remote-route integration corrective) wired the
 executable Plan 206 backend into the production
 `ServiceTunnelManager::deliver_outbound` sweep so a reachable
 remote peer no longer dies at the pre-Plan-208 `unknown_peer`
 terminal branch: the local-miss branch now invokes the typed
 `route_outbound_remote_request` seam, the seam performs a real
 send through `StreamingDestinationAdapter` +
 `deliver_outbound_cells` + `RouterDeliveryService`, and the
 inbound-owner registry wires inbound data to the actual owning
 service runtime; seven new `plan208_*` manager-level unit rows in
 `service_tunnels.rs::plan208_remote_route_integration_tests` lock
 the integration call graph; the new `#[ignore]`-gated
 `m10_remote_route_integration_through_deliver_outbound` external
 driver exercises the production sweep against the exact-pinned
 i2pd 2.61.0 cache; `scripts/check-service-tunnel-acceptance-evidence.sh`
 extended with Plan 208 §15 source-level invariants. Plan 203
 closed the positive M10 remote HTTP + IRC application interop: the
 `m10_positive_remote_http_and_irc_application_interop` Direction A
 external driver declares `http-client` + `irc-client` specs whose
 destination is the i2pd-owned HTTP + IRC server-tunnel destination
 b64, asserts `RoutingDecision::RemoteRouter` after
 `install_router_delivery_handle`, advances the typed Plan 203 §5/§6
 documented observation set through the new public
 `record_remote_application_observation` helper, exercises the
 underlying Plan 184–193 router stack with real one-hop builds +
 lease lookup + Streaming `Established`, and never logs peer key
 material; the legacy marker shape keeps `RemoteUnresolved` so a
 silent local fallback for a remote peer cannot regress. Plan
207 closed the genuine M10 remote HTTP + IRC application interop:
the `m10_genuine_remote_http_and_irc_application_interop` external
driver replaces the synthetic Plan 203
`record_remote_application_observation` label-injection pattern
with command-derived evidence from unmodified application
clients. The HTTP row is bound to the real system `curl` binary
spawned as a subprocess against the i2pr HTTP client listener;
the IRC row is bound to the unmodified exact-pinned jaraco/irc
public API (`irc.client`) spawned as a subprocess against the
i2pr IRC client listener. The driver writes the documented Plan
207 §9 subfact rows plus `plan206-backend-counters` to
`${EVIDENCE_DIR}/plan207-driver/driver-evidence.tsv`; the
aggregate pass rows derive purely from the command-derived
subfact rows; the runner provisions i2pd with one HTTP server
tunnel + one IRC server tunnel pointing at the harness-owned
loopback fixtures; the static checker rejects literal `record
"... passed"` lines, requires the positive rows to flow through
`record_guarded` + the documented Plan 207 §9 subfact rows, and
extends `scripts/check-service-tunnel-acceptance-evidence.sh`
with the Plan 207 §10 source-level invariants; the two
`remote-independent-*` rows flip from `blocked` to `passed` once
the dedicated M6 interop lane provisions the SSU2 endpoint + bind
tuple and the driver emits every documented Plan 207 §9 subfact
row in the same evidence directory/run id; the full M10 lane
stays fail-closed without it. Plan 204 is the M10 final closure
documentation and authority normalization pass: it landed the
docs/CI normalization pass on top of the already-passed Plans
200/202/206/207 and remains blocked on the Plan 201 exact-head
external run; per Plan 204 §1, milestone 6 final closure and
milestone 10 final acceptance cannot be claimed until Plan 201
records the terminal `P200-{A..H}` classification and lands its
narrow corrective (see `plans/204-status.md` and the §7/§12
authority transitions in
[`plans/204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](plans/204-m10-final-closure-evidence-authority-and-documentation-normalization.md)).
Plan 195 (M10 remote service interop) is reactivated to
`evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization`
because Plan 207 supplies the positive application evidence Plan
195 was originally registered to provide. Plan 206 attaches the
executable backend; **Plan 207 closed the genuine remote HTTP +
IRC application interop** by replacing the synthetic Plan 203
label-injection pattern with command-derived evidence from
unmodified application clients: the HTTP row is bound to the
real system `curl` binary spawned as a subprocess against the
i2pr HTTP client listener; the IRC row is bound to the unmodified
exact-pinned jaraco/irc public API (`irc.client`) spawned as a
subprocess against the i2pr IRC client listener; the aggregate
pass rows derive purely from the documented Plan 207 §9 subfact
rows the driver writes to its evidence file;
`record_remote_application_observation` is explicitly forbidden
in the Plan 207 driver; the static checker
`scripts/check-service-tunnel-acceptance-evidence.sh` extended
with the Plan 207 §10 source-level invariants.
through the Plan 206 manager path.

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current product/plan status.
2. `GUARDRAILS.md` — non-negotiable engineering/security/interoperability constraints.
3. `CONTRIBUTING.md` — local quality/runtime/test conventions.
4. [`plans/README.md`](plans/README.md) and the newest relevant status files.
5. `specs/support.toml` plus `specs/CONFORMANCE.md` for support/evidence claims.

Current authority:

```text
Plan 146 = passed private-destination reference evidence
Plan 147 = raw-driver implementation retained
Plan 149 = passed self-composing localhost SAM product
Plan 150 = external-client core evidence retained-passed
Plan 150 final acceptance = superseded-by-plan151
Plan 151 = passed final acceptance evidence correction
Plan 152 = passed narrow M6 streaming corrective
Plan 153 = passed post-M7 authority/CI hygiene
Milestone 7 SAM localhost = closed (experimental, loopback-only)
Milestone 8 roadmap = Plan 154
Plan 155 = passed SSU2 v2 protocol foundation
Plan 156 = passed SSU2 v2 handshake/token/RouterInfo establishment
Plan 157 = passed SSU2 v2 data-phase reliability/fragmentation
Plan 158 = passed SSU2 v2 UDP runtime and local session product
Plan 159 = passed SSU2 v2 path validation/publication/transport selection
Plan 160 = passed SSU2 v2 peer test and relay reachability
Plan 161 = passed M8 SSU2 independent IPv4 interop and final closure
Plan 162 = passed external-test lane isolation / routine-CI corrective
Plan 163 = registered M9 I2CP roadmap
Plan 164 = passed M9 I2CP protocol and wire foundation
Plan 165 = passed M9 I2CP connection/session/options
Plan 166 = passed M9 I2CP client-owned destination + LeaseSet2 bridge
Plan 167 = passed M9 I2CP loopback server runtime
Plan 168 = passed M9 I2CP message data plane
Plan 169 = passed M9 I2CP self-composed local product and hardening
Plan 170 external wire/data-plane = retained-passed
Plan 170 final acceptance = superseded-by-plan172
Plan 171 = passed M9 I2CP invalid-preamble close and CI corrective (retained)
Plan 172 = passed M9 I2CP independent LeaseSet2 lifecycle corrective
Milestone 9 I2CP final acceptance = closed-via-plan172 (experimental, loopback-only)
Plan 173 = registered M10 service-tunnels roadmap
Plan 174 = passed M10 service-tunnel foundation and shared stream runtime
Plan 175 = passed M10 generic client/server service tunnels and persistent server destinations
Plan 176 = passed M10 HTTP `.i2p` proxy and CONNECT
Plan 177 = passed M10 SOCKS5 `.i2p` CONNECT proxy
Plan 178 = passed M10 IRC `.i2p` client profile and privacy filtering
Plan 179 = passed M10 IRC `.i2p` server profile and authenticated peer hostname
Plan 180 = passed M10 service-tunnel composition, reconcile, and hardening
Plan 181 = passed-m10-independent-application-and-service-interop-via-plan207 (reclassified by Plan 204 docs/CI normalization from `blocked-by-m6-mixed-router-streaming-blocker`: the retained local 29-row matrix stays green; the two §6.3 remote application rows now flip `blocked → passed-on-env` through the Plan 207 positive external driver)
Plan 182 = passed M10 local-delivery corrective
Plan 183 = registered M6 mixed-router streaming interop program
Plan 184 = passed M6 authenticated I2NP runtime and reference preflight
Plan 185 = passed M6 live one-hop exploratory tunnels and liveness
Plan 186 = passed M6 mixed-router NetDB lookup and publication
Plan 187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 5/7 flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
Plan 188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; 5/7 destination rows flipped; 2 inbound-delivery rows blocked on plan192; 2 ordering rows passed)
Plan 189 = registered M6 Java second-family qualification and mixed-router closure (blocked-by-plan188-plan190-plan191-plan192; §8 cross-family ledger/checker/workflow landed, no second-family Java row yet)
Plan 190 = passed M6 inbound NetDB reply-path tunnel-ID corrective (local rows passed; 3 destination rows flipped blocked -> passed in fresh external run)
Plan 191 = stopped-by-inbound-delivery-boundary-E (retained-passed via plan192; narrower follow-up registered + closed)
Plan 192 = passed M6 i2pd-compatible I2CP-style Data body wire-format corrective (9-byte short-transport inner envelope + i2cp I2CP-style Data body + STYLE=RAW SAM session + RAW RECEIVED SIZE=N digest equality; 2 inbound-delivery rows flipped blocked -> passed; inbound-delivery layer closed for i2pd 2.61.0)
Plan 193 = passed M6 i2pd mixed-router Streaming qualification (local rows passed; full Direction A + Direction B external matrix passed twice on exact head; static checker wired into CI floor)
  Plan 194 = retained-partial-java-qualification-sam-ls2-publication-boundary
  Plan 196 = passed-m6-java-controlled-first-run-topology-corrective (out-of-tree ControlledRouter.java test-only launcher + rewritten run-java.sh + extended static checker + `router.blocklist.enable=false` controlled-launcher fix + PRIV-token SAM SESSION CREATE fix; controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge proven on the exact-pinned Java I2P 2.13.0 cache; `external-session-established-java` flipped failed -> passed; the two narrow correctives are bounded to the controlled-launcher and fail-closed at the daemon boundary)
 Plan 197 = passed-m6-pq-ssu2-option-support-corrective (parser-only tolerance of the SSU2 `pq` KEM-scheme option Java I2P 2.13.0 unconditionally publishes; typed Ssu2PqKem/PqCapabilities surface with bounded MAX_SSU2_PQ_SCHEMES = 8; i2pr session layer remains classical X25519 only; i2pr publication path stays pq-free; ML-KEM not implemented, claimed, or silently enabled; 21 required test rows green locally)
  Plan 198 = superseded-execution-decomposed-and-closed-via-plans200-204 (Plan 198's original `blocked-public-java-client-leaseset2-publication` verdict is retained verbatim; the active Java branch is Plan 201 and Plan 204 owns the docs/CI normalization pass)
  Plan 199 = superseded-execution-decomposed-and-closed-via-plans200-204 (retained historical umbrella for the final M6/M10 requirements; decomposed and reclassified by Plan 204)
  Plan 200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap (Java helpers decoupled `leaseset=published` from `READY` and added bounded `REPORT_STATUS`; Rust driver adds post-bootstrap RouterInfo DatabaseLookup proofs in both directions; sanitized Java log keys for the client LS2 lifecycle and tunnel/floodfill/store/ack selection are emitted to evidence; exactly one terminal `P200-{A..H}` classification per run; `scripts/check-m6-mixed-router-acceptance-evidence.sh` extended with Plan 200 §B/C/D/§11 invariants; downstream M6 rows stay blocked until Plan 201 picks the smallest standards-compatible corrective)
  Plan 201 = in-progress-branch-a-corrective-landed-branch-g-framework-unchanged (this run consumed the first Plan 200 / Plan 201 exact-head external run against the exact-pinned Java I2P 2.13.0 cache (`9134f808337b401e8e53c73734c81fab04280c9d`, staged unmodified via `bash scripts/interop/fetch-m6-java.sh --rebuild`); the terminal `P200-B-router-b-missing-router-a` classification was recorded consistently across three sequential exact-head runs (`java-main-netdb-a-knows-b=true`, `java-main-netdb-b-knows-a=false`); the corresponding Plan 201 §3 Branch A corrective landed as a narrow test-driver fix — `decode_inbound_i2np` helper that mirrors the production dispatcher's standard-first / short-transport-fallback ordering and the probe-side gzip decompression that matches `i2pr_netdb::decompress_router_info` — so the bootstrap probe now decodes Java's standard-form, gzipped `RouterInfoCompressed` `DatabaseStore` responses correctly; the one-direction `p200-routerinfo-lookup-a-knows-b` row flipped `false → passed` (Branch A proof) while the asymmetric `b-knows-a` direction stays on the Java B / i2pr bootstrap-storage asymmetry side per Plan 201 §3 Branch A's "do not change i2pr production NetDB code unless the captured transcript proves i2pr encoded an invalid I2NP message" rule; the Branch G `store-acked-remote-lookup-fails` corrective framework (eleven sanitized observation counters on `DestinationTunnelCounters` + public `note_lookup_boundary(label, value)` typed observation surface + eight new `plan201_g_*` unit rows in `destination_tunnel_unit.rs` + six new `blocked_row` Plan 201 §G entries in `run-java.sh` + static checker `scripts/check-m6-mixed-router-acceptance-evidence.sh` §11 + §12 invariants) is retained as the documented attribution surface for any future `P200-G` exact-head run; the seven §11 stop rows stay `blocked` with the documented Plan 198/199 stop provenance until the Java-side LeaseSet2 publication gap closes; the M6 Java second-family claim stays `not-yet-passed`)
 Plan 202 = passed-m10-production-remote-destination-and-streaming-composition
 Plan 203 = passed-m10-positive-remote-http-and-irc-application-interop (the positive `m10_positive_remote_http_and_irc_application_interop` Direction A external driver exercises the same M6 interop lane Plan 202 uses, declares `http-client` + `irc-client` specs whose destination is the i2pd-owned HTTP + IRC server-tunnel destination b64, asserts `RoutingDecision::RemoteRouter` after `install_router_delivery_handle`, advances the typed Plan 203 §5/§6 documented observation set through the new `record_remote_application_observation` helper, exercises the underlying Plan 184–193 router stack with real one-hop builds + lease lookup + Streaming `Established`, and never logs peer key material; the static checker `scripts/check-service-tunnel-acceptance-evidence.sh` rejects literal `record "... passed"` lines and requires the positive rows to flow through `record_guarded` + the documented evidence keys (`http-remote-application-established`, `irc-remote-application-established`, `manager-routing-decision`); the two `remote-independent-*` rows flip from `blocked` to `passed` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple; the full M10 lane stays fail-closed without it)
 Plan 195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization (Plan 195 absorbed the Plan 207 positive application evidence; reactivated from the historical `registered-blocked-by-plan199` interpretation by the Plan 204 docs/CI normalization pass)
   Plan 204 = in-progress-docs-and-authority-normalization-blocked-on-plan205-sam-bridge-pivot (intentionally narrow docs/CI/evidence-authority normalization pass on top of Plans 200/202/206/207/208; the M6/M10 closed authority transitions in §7/§12 of `plans/204-m10-final-closure-evidence-authority-and-documentation-normalization.md` stay deferred until Plan 205's SAM-bridge helper pivot closes Plan 201's Java mandatory rows; no product bug hidden, no synthetic `passed` evidence introduced)
 Plan 205 = in-progress-registered-blocked-on-plan198-publication-boundary (the documented next executable plan after Plan 201; rewrites the Java second-family helpers from direct I2CP to the public SAM bridge path mirroring what i2pd's SAM bridge does successfully in the Plan 202/207 reference destination path; three-router topology retained from Plan 201 Branch C/D; no public I2P, no Java patching, no i2pr production wire change)
  Plan 206 = retained-partial-executable-backend-seams-superseded-by-plan208 (the executable `ServiceDestinationDelivery` backend attaches a shared `RemoteDestinationBackend` (coordinator + router delivery); the manager exposes typed `route_outbound_remote_request` / `dispatch_inbound_to_owned_destination` / `register_inbound_destination_owner` / `resolve_remote_lease_set2` seams; inbound owner registration is atomic with a fail-closed duplicate guard; the three operation-boundary counters — `remote_lookup_cache_hit`, `remote_outbound_composed`, `remote_inbound_dispatched` — advance only through typed backend seams and reject the external `record_observation` helper so a positive observation cannot be manufactured without the production operation; seven new `plan206_*` manager-level unit rows in `service_tunnels.rs::plan206_remote_composition_tests` lock the typed path; the static checker `scripts/check-service-tunnel-acceptance-evidence.sh` extended with the Plan 206 §13 source-level invariants; Plan 208 promotes the typed backend into the production `deliver_outbound` sweep)
 Plan 208 = passed-m10-production-delivery-driver-remote-route-integration (`crates/i2pr-daemon/src/service_tunnels.rs::deliver_outbound` invokes the typed `route_outbound_remote_request` seam on the local-miss branch; `route_outbound_remote_request` performs a real send through `StreamingDestinationAdapter` + `deliver_outbound_cells` + `RouterDeliveryService`; the inbound-owner registry wires inbound data to the actual owning service runtime; seven new `plan208_*` manager-level unit rows in `service_tunnels.rs::plan208_remote_route_integration_tests`; the new `m10_remote_route_integration_through_deliver_outbound` external driver is `#[ignore]`-gated and exercises the production sweep against the exact-pinned i2pd 2.61.0 cache; `scripts/check-service-tunnel-acceptance-evidence.sh` extended with Plan 208 §15 source-level invariants)
 Plan 207 = passed-m10-genuine-remote-http-and-irc-application-interop (replaces the synthetic Plan 203 label-injection pattern with command-derived evidence from unmodified application clients: the HTTP row is bound to the real system `curl` binary spawned as a subprocess against the i2pr HTTP client listener; the IRC row is bound to the unmodified exact-pinned jaraco/irc public API (`irc.client`) spawned as a subprocess against the i2pr IRC client listener; the driver writes the documented Plan 207 §9 subfact rows plus `plan206-backend-counters` to `${EVIDENCE_DIR}/plan207-driver/driver-evidence.tsv`; the aggregate pass rows derive purely from the command-derived subfact rows; `record_remote_application_observation` is explicitly forbidden in the Plan 207 driver; the runner provisions i2pd with one HTTP server tunnel + one IRC server tunnel pointing at the harness-owned loopback fixtures; the static checker `scripts/check-service-tunnel-acceptance-evidence.sh` extended with the Plan 207 §10 source-level invariants; the two `remote-independent-*` rows flip from `blocked` to `passed` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple and the driver emits every documented Plan 207 §9 subfact row in the same evidence directory/run id)
   Plan 203 = retained-partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207 (the `m10_positive_remote_http_and_irc_application_interop` driver is retained as a historical scaffold; its `record_remote_application_observation` label-injection pattern is no longer accepted as a counted proof by the Plan 207 lane; the manager's free observation helper remains for backwards compatibility but new code must depend on the documented Plan 207 §9 subfact rows for counted evidence)
 m10_remote_transport_core = passed-via-plan208
 next_m10_application_plan = 209 (Plan 208 closed the production delivery-driver remote-route integration; Plan 209 owns the cleanup of the Plan 207 application driver (removal of the synthetic label-injection pattern, drive the application rows through the Plan 208 production sweep); Plan 204 owns the docs/CI/evidence-authority normalization pass on top of Plans 200/202/206/207/208)
Milestone 10 foundation = passed-via-plan174 (no listener yet)
Milestone 10 generic tunnels = passed-via-plan175 (profile; byte round-trip proven-via-plan182)
Milestone 10 HTTP proxy = passed-via-plan176 (profile; byte round-trip proven-via-plan182)
Milestone 10 SOCKS5 = passed-via-plan177 (profile; byte round-trip proven-via-plan182)
Milestone 10 IRC client = passed-via-plan178 (profile; byte round-trip proven-via-plan182)
Milestone 10 IRC server = passed-via-plan179 (profile; byte round-trip proven-via-plan182)
Milestone 10 local product = passed-via-plan180-and-plan182 (reconcile model + local-delivery driver)
Milestone 10 local round-trip = passed-via-plan182 (generic/HTTP/SOCKS/IRC success paths)
Milestone 10 independent application clients = passed-via-plan181-and-plan207 (Plan 181 §6.3 retained local rows remain green; the two remote application rows now flip `blocked → passed-on-env` through the Plan 207 positive external driver when the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple and the driver emits every documented Plan 207 §9 subfact row + `plan206-backend-counters` in the same evidence directory/run id)
Milestone 10 remote service interop = passed-via-plan202-plan206-plan207-and-plan208 (Plan 202 closed the typed routing classification + bounded counters; Plan 206 attached the executable `RemoteDestinationBackend` to the `ServiceDestinationDelivery` capability; Plan 208 wired the executable backend into the production `deliver_outbound` sweep so the manager routes its own queued Streaming requests through the real Plan 184–193 router stack — the typed `route_outbound_remote_request` / `dispatch_inbound_to_owned_destination` / `register_inbound_destination_owner` / `resolve_remote_lease_set2` seams cover both Direction A (client tunnel) and Direction B (server tunnel) without a parallel test-owned stack; Plan 207 replaced the synthetic Plan 203 label-injection pattern with command-derived evidence from real system `curl` + exact-pinned jaraco/irc public API subprocess invocations against the production M10 listeners; the aggregate pass rows derive purely from the documented Plan 207 §9 subfact rows)
Milestone 10 final acceptance = not-yet-closed (Plan 208 closed the production delivery-driver remote-route integration; Plan 209 owns the cleanup of the Plan 207 application driver (removal of the synthetic label-injection pattern, drive the application rows through the Plan 208 production sweep); the milestone can only close via Plan 204 once Plan 201 records the terminal `P200-{A..H}` classification and lands its narrow corrective — see the §7/§12 authority transitions in `plans/204-m10-final-closure-evidence-authority-and-documentation-normalization.md`)
M6 authenticated I2NP preflight = passed-via-plan184 (no tunnel/NetDB/Streaming claim)
M6 exploratory one-hop tunnels = passed-via-plan185 (no multi-hop / LeaseSet2 / Streaming claim)
M6 NetDB lookup/publication = passed-via-plan186 (no LeaseSet2 / Streaming claim)
M6 destination local product = passed-via-plan187 (27 unit + 9 live rows; no remote claim)
M6 destination remote interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-passed-via-plan192 (i2pd only; Java second family not yet run; Streaming passed-via-plan193)
M6 inbound NetDB reply-path correction = passed-via-plan190 (typed route + adapter; 3 destination rows flipped blocked -> passed in fresh external run)
M6 inbound destination delivery boundary E = closed-via-plan192 (i2pd-compatible I2CP-style Data wire-format)
M6 inbound destination delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format; STYLE=RAW SAM session; 9-byte short-transport inner envelope)
M6 i2pd mixed-router Streaming qualification = passed-via-plan193 (full Direction A + Direction B matrix, two exact-head passes; static checker wired)
M6 java public-client publication observability = landed-via-plan200 (helper READY decoupled from leaseset=published; REPORT_STATUS added; post-bootstrap RouterInfo DatabaseLookup proofs in both directions; sanitized Java LS2 lifecycle keys; one terminal P200-{A..H} classification per run)
M6 java second-family qualification = public-client-corrective-diagnosed-via-plan200-and-partially-closed-via-plan201-branch-a (Plan 199 two-router bootstrap passes, Plan 200 closed the diagnostic/evidence side and emits one terminal `P200-{A..H}` classification per run, Plan 201 Branch A corrective flipped `p200-routerinfo-lookup-a-knows-b` false → passed; the asymmetric `b-knows-a` direction stays blocked on a Java-B-NetDB bootstrap-storage asymmetry per Plan 201 §3 Branch A's rule; the remaining Plan 201 Branches B-F stay on the Java-side LeaseSet2 publication gap recorded in Plan 194 / Plan 200 §C / §D)
M6 mixed-router cross-family ledger = retained-partial-via-plan193-and-plan194; Plan 200 added the Plan 200 §B/C/D/§11 invariants; Plan 201 added the §G observation framework + §11 + §12 static checker invariants; final closure pending Plan 201 exact-head external run + Plan 204 convergence
M6 ssu2 pq option tolerance = landed-via-plan197-typed-parser-surface (Ssu2RouterAddress::parse accepts Java `pq=4,3`; typed PqCapabilities surfaced on every parsed address via `pq_capabilities()` accessor; first-family i2pd 2.61.0 lane stays green because i2pd does not publish pq)
milestone6_java_mixed_router_interop = not-yet-passed (Plan 199 two-router bootstrap passes, Plan 200 closed the diagnostic/evidence side; Plan 201 Branch A corrective landed the i2pr test-driver decode gap and flipped the one-direction `p200-routerinfo-lookup-a-knows-b` row false → passed; the remaining Java-side LeaseSet2 publication gap stays pending Plan 201 final branch implementation or a follow-up Java-side corrective)
milestone6_interoperable = not-yet-claimed
next_executable_plan = 205-sam-bridge-helper-pivot (Plan 200 / Plan 201 Branch A corrective landed (one-direction proof + symmetric b-knows-a fix via decode fix); Plan 201 Branch C/D three-router topology + 1-hop helper profile attempted and proved blocked on Java's ProfileOrganizer `_thresholdSpeedValue` under controlled loopback topology; the remaining Java-side LeaseSet2 publication gap stays on Plan 205's SAM-bridge helper pivot — the smallest standards-compatible corrective that satisfies Plan 198 / Plan 201 §4 constraints; no public I2P, no Java patching, no i2pr production wire change)
  next product layer = m10-unified-final-closure-blocked-on-java-leaseset2-publication-gap (the M10 production remote transport is passed via Plan 208; the positive remote application interop is passed via Plan 203; Plan 204 landed the docs/CI normalization pass; the remaining work is the Java-side LeaseSet2 publication gap that Plan 201 Branches B-F own, the Plan 209 application driver cleanup that drives the application rows through the Plan 208 production sweep, and the Plan 204 final authority transition)
```

For current SSU2 interop work, read in this order:

1. [`plans/162-status.md`](plans/162-status.md)
2. [`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md)
3. [`plans/161-status.md`](plans/161-status.md)
4. [`plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md)
5. [`plans/160-status.md`](plans/160-status.md) through [`plans/155-status.md`](plans/155-status.md)
6. [`plans/154-status.md`](plans/154-status.md) for the M8 roadmap authority.

Read in this order for SAM work:

1. [`plans/151-status.md`](plans/151-status.md)
2. [`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](plans/151-m7-sam31-final-acceptance-evidence-correction.md)
3. [`plans/150-status.md`](plans/150-status.md) — retained external-client core evidence, not final closure
4. [`plans/149-status.md`](plans/149-status.md) — passed product-composition authority
5. Plans 146–148 for historical/reference context.

Read in this order for Milestone 9 I2CP work:

1. [`plans/172-status.md`](plans/172-status.md) — closed final acceptance
2. [`plans/170-status.md`](plans/170-status.md)
3. [`plans/170-m9-i2cp-independent-clients-and-final-closure.md`](plans/170-m9-i2cp-independent-clients-and-final-closure.md)
4. [`plans/171-status.md`](plans/171-status.md)
5. [`plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md)
6. [`plans/169-status.md`](plans/169-status.md)
7. [`plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`](plans/169-m9-i2cp-self-composed-local-product-and-hardening.md)
8. [`plans/168-status.md`](plans/168-status.md)
9. [`plans/168-m9-i2cp-message-data-plane.md`](plans/168-m9-i2cp-message-data-plane.md)
10. [`plans/167-status.md`](plans/167-status.md)
11. [`plans/167-m9-i2cp-loopback-server-runtime.md`](plans/167-m9-i2cp-loopback-server-runtime.md)
12. [`plans/166-status.md`](plans/166-status.md)
13. [`plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`](plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md)
14. [`plans/165-status.md`](plans/165-status.md)
15. [`plans/165-m9-i2cp-connection-session-and-options.md`](plans/165-m9-i2cp-connection-session-and-options.md)
16. [`plans/164-status.md`](plans/164-status.md)
17. [`plans/164-m9-i2cp-protocol-and-wire-foundation.md`](plans/164-m9-i2cp-protocol-and-wire-foundation.md)
18. [`plans/163-m9-i2cp-roadmap.md`](plans/163-m9-i2cp-roadmap.md) — planning authority
19. Milestone 9 is closed via Plan 172.

Read in this order for M6 mixed-router Streaming work:

1. [`plans/193-status.md`](plans/193-status.md) — closed authority (M6 i2pd mixed-router Streaming qualification; local rows passed, full Direction A + Direction B external matrix passed twice on exact head)
2. [`plans/193-m6-i2pd-mixed-router-streaming-qualification.md`](plans/193-m6-i2pd-mixed-router-streaming-qualification.md) — Plan of record
3. [`plans/193-streaming-status.md`](plans/193-streaming-status.md) — M6 i2pd mixed-router Streaming execution status
4. [`plans/192-status.md`](plans/192-status.md) — Plan 192 i2pd-compatible I2CP-style Data body wire-format corrective (i2pd inbound-delivery layer closed)
5. [`plans/192-m6-i2cp-wire-format-corrective.md`](plans/192-m6-i2cp-wire-format-corrective.md)
6. [`plans/194-status.md`](plans/194-status.md) — Java second-family qualification (now executable; unblocked by Plan 193)
7. The historical `plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming file is superseded by Plan 193; do not execute it as a numbered plan.

Read in this order for Milestone 10 service-tunnel work:

1. [`plans/207-status.md`](plans/207-status.md) — passed M10 genuine remote HTTP + IRC application interop corrective (current authority: Plan 206 transport layer + Plan 207 application row promotion through command-derived evidence from unmodified application clients)
2. [`plans/207-m10-genuine-remote-http-and-irc-application-interop-corrective.md`](plans/207-m10-genuine-remote-http-and-irc-application-interop-corrective.md)
3. [`plans/206-status.md`](plans/206-status.md) — passed M10 production remote delivery composition corrective
4. [`plans/206-m10-production-remote-delivery-composition-corrective.md`](plans/206-m10-production-remote-delivery-composition-corrective.md)
5. [`plans/202-status.md`](plans/202-status.md) — passed M10 production remote Destination/Streaming composition (superseded by Plan 206)
6. [`plans/202-m10-production-remote-destination-and-streaming-composition.md`](plans/202-m10-production-remote-destination-and-streaming-composition.md)
7. [`plans/203-status.md`](plans/203-status.md) — retained-partial M10 positive remote HTTP + IRC application interop scaffold (superseded by Plan 207)
8. [`plans/203-m10-positive-remote-http-and-irc-application-interop.md`](plans/203-m10-positive-remote-http-and-irc-application-interop.md)
9. [`plans/181-status.md`](plans/181-status.md) — local matrix green; remote rows now flow through Plan 207
10. [`plans/181-m10-independent-application-and-service-interop-final-closure.md`](plans/181-m10-independent-application-and-service-interop-final-closure.md)
11. [`plans/182-status.md`](plans/182-status.md) — passed M10 local-delivery corrective
12. [`plans/182-m10-local-delivery-corrective.md`](plans/182-m10-local-delivery-corrective.md)
13. [`plans/183-status.md`](plans/183-status.md) — registered M6 mixed-router program
14. [`plans/183-m6-mixed-router-streaming-interop-program.md`](plans/183-m6-mixed-router-streaming-interop-program.md)
15. [`plans/180-status.md`](plans/180-status.md) — passed M10 composition, reconcile, and hardening
16. [`plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md`](plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md)
17. [`plans/179-status.md`](plans/179-status.md) — passed IRC `.i2p` server profile and authenticated peer hostname
18. [`plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md)
19. [`plans/178-status.md`](plans/178-status.md) — passed IRC `.i2p` client profile and privacy filtering
20. [`plans/178-m10-irc-client-profile-and-privacy-filtering.md`](plans/178-m10-irc-client-profile-and-privacy-filtering.md)
21. [`plans/177-status.md`](plans/177-status.md) — passed SOCKS5 `.i2p` CONNECT
22. [`plans/177-m10-socks5-i2p-connect-proxy.md`](plans/177-m10-socks5-i2p-connect-proxy.md)
23. [`plans/176-status.md`](plans/176-status.md) — passed HTTP `.i2p` proxy + CONNECT
24. [`plans/176-m10-http-i2p-proxy-and-connect.md`](plans/176-m10-http-i2p-proxy-and-connect.md)
25. [`plans/175-status.md`](plans/175-status.md) — passed generic client/server tunnels
26. [`plans/175-m10-generic-client-server-service-tunnels.md`](plans/175-m10-generic-client-server-service-tunnels.md)
27. [`plans/174-status.md`](plans/174-status.md) — passed foundation
28. [`plans/208-status.md`](plans/208-status.md) — passed M10 production delivery-driver remote-route integration (Plan 208 closes the integration gap so a reachable remote peer no longer dies at the pre-Plan-208 `unknown_peer` terminal branch; the production `deliver_outbound` sweep invokes the typed `route_outbound_remote_request` seam, performs a real send through `StreamingDestinationAdapter` + `deliver_outbound_cells` + `RouterDeliveryService`, and wires inbound data to the actual owning service runtime)
29. [`plans/208-m10-production-delivery-driver-remote-route-integration-corrective.md`](plans/208-m10-production-delivery-driver-remote-route-integration-corrective.md)
28. [`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
29. [`plans/173-status.md`](plans/173-status.md) — roadmap authority
30. [`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md)
31. Do not claim M10 final closure yet: Plan 207 closed the genuine
    remote HTTP + IRC application interop through the Plan 206
    manager path; Plan 204 owns the final authority/CI
    normalization pass that closes both milestones on the same
    exact head.

Plan 171 corrective (retained): every terminal pre-session I2CP
rejection terminates TCP explicitly on the common per-connection
path (`handle_connection` calls `stream.shutdown()` before
`teardown_connection` + `drop_connection`; shutdown failure never
blocks cleanup; no frame is written for an invalid first byte).
`wrong_protocol_byte_is_closed` stays strict — timeout is failure —
and proves a 24-iteration rejection trajectory with zeroed
baselines plus a subsequent valid client; the paused test waits
via a bounded yield-pump/`try_read` drain (no virtual-time
timeout, which raced server polling intermittently on macOS)
and the non-paused
`wrong_protocol_byte_is_closed_real_time` companion separates
product-close evidence from paused-clock timer behavior. Never
revert to drop-timing dependence to make a test convenient.

Do **not** trust prose that disagrees with executable tests/scripts. The newest
explicit superseding status wins when historical records conflict.

## Workspace layout

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — protocol cryptographic wrappers.
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — shared runtime-neutral contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral transport/link codecs.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation and local storage.
- `i2pr-runtime` — production owner of Tokio, sockets, timers, channels, cancellation; also contains non-production external transport test drivers under `tests/`.
- `i2pr-daemon` — CLI/composition root and SAM runtime/socket ownership.
- `i2pr-tunnel` — runtime-neutral exploratory/tunnel substrate.
- `i2pr-client` — destination lifecycle, LeaseSet2, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 parsing/state/registry/FORWARD/NAMING plus the M9 I2CP wire/profile foundation (no sockets).
- `i2pr-service-tunnels` — runtime-neutral M10 service-tunnel config/policy (no sockets; foundation only, no listener yet).
- `i2pr-testkit` — deterministic simulation/fault fixtures; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Architecture details live under `docs/architecture/`; ADRs live under
`docs/adr/`.

## Current SAM architecture

Retain the working Plan 149 product structure unless a newer executable test
exposes a concrete defect:

- `SESSION CREATE` transactionally builds the supported localhost product;
- one `Arc<DestinationIdentity>` allocation is shared by destination runtime and SAM bridge;
- `SamLocalProductFabric` creates the localhost LeaseSet2/outbound/inbound-delivery material with OS CSPRNG;
- local peer LeaseSet2 is resolved/validated through the SAM-owned directory;
- one supervised per-destination runtime driver is started automatically;
- raw CONNECT/ACCEPT permanently transfers socket ownership out of the line parser;
- same-read command+raw bytes are preserved;
- `SILENT` behavior and non-silent ACCEPT peer Destination metadata are byte-exact;
- `DeliverySweepCounters` surface typed bounded delivery failure accounting.

The canonical product-composition test is
`crates/i2pr-daemon/tests/sam_stream_self_composed.rs`. After listener startup,
it drives behavior only through TCP/SAM and must not invoke private bridge,
LeaseSet2, tunnel-factory, driver, delivery, or byte-moving setup APIs.

## Plan 151 scope (retained)

Plan 151 was an acceptance/evidence correction, not a SAM rewrite. It added
executable proof for the items Plan 150 claimed but did not fully run:

- no synthetic/unconditional `passed` evidence rows;
- two simultaneous sibling streams and close-one/keep-one isolation;
- slow-reader and slow-writer boundedness;
- deterministic DATA-drop, ACK-drop, duplicate, reorder, corruption, and retransmission-ceiling behavior beneath real SAM sockets;
- CLOSE/RESET/control-session cleanup;
- complete FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regression commands;
- current-head routine CI plus manual external-client workflow.

If a new test exposes an M6 Streaming protocol defect, write a narrow protocol
corrective rather than weakening the test. That stop fired once as Plan 152
(passed narrow M6 corrective, no wire change).

## Plan 153 scope (closed)

Plan 153 was documentation and CI hygiene only: it normalized the
authoritative `plans/152-status.md`, removed stale Plan 151/152 prose,
added the Plan 152 closure pointer to the support ledger, and enforced
the Plan 151 SAM evidence-integrity checker in routine Linux CI and
the manual SAM external workflow. No `crates/` or `Cargo.lock` changes
were made.

## Plan 161 interop evidence (passed, retained)

Plan 161 has proven both independent direct SSU2 v2 directions against
exact-pinned i2pd (see `plans/161-status.md` for the full matrix and
the 24-criterion checklist):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
direction = i2pr initiator -> i2pd responder
transport = real loopback UDP
```

The passed trajectory includes tokenless Retry establishment, mutual
authentication, one small and one fragmented DatabaseStore from i2pr to i2pd,
DeliveryStatus traffic back over the authenticated session, and graceful
resource cleanup. Independent comparison exposed three real handshake
transcript divergences that were corrected in Plan 161; do not revert them to
make loopback tests match older fixtures.

Direction B, the cached-token/malformed rows, and the final
ledger/checker/workflow lane have passed locally and hosted (see
`plans/161-status.md`); Java I2P is recorded nonblocking secondary
debt. Direction A+B evidence
does not imply public I2P or broad router interoperability. Milestone 8
is closed within this bounded scope; the next layer is milestone9-planning.

## Plan 162 scope (closed)

Plan 162 was a narrow test-lane/CI corrective. Routine CI run
`33915994884` on Plan 161 direction-A head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution ran
`crates/i2pr-runtime/tests/ssu2_independent.rs` without an external i2pd
environment. Dependency policy and MSRV passed.

Required correction:

- keep the Plan 161 external test compiled by all-target checks;
- mark the environment-dependent test explicitly ignored for ordinary libtest execution;
- run it only with explicit `--ignored --exact` in the external lane;
- keep missing external environment a hard failure after explicit selection;
- do not add filename filtering, `|| true`, `continue-on-error`, fake peer values, or broad workspace exclusions;
- re-run direction A against the exact same pinned i2pd after gating;
- require ordinary Ubuntu + macOS CI green on the Plan 162 closing head;
- then return directly to Plan 161. These conditions passed on implementation
  commit `624e8cce177040674376163160cfbda47e6a60fe`, hosted CI run
  `33941941145`.

Do not change SSU2 production source or wire semantics inside Plan 162. If the
explicit external re-run exposes a real protocol defect, stop and create a
separate narrow protocol corrective.

## Hard boundaries

These remain non-negotiable and are CI-enforced where applicable:

- preserve workspace dependency direction; no production crate depends on `i2pr-testkit`;
- no unbounded channels/queues introduced for convenience;
- Tokio/socket/timer/task ownership stays in runtime/daemon layers;
- every spawned task has explicit ownership/cancellation;
- SAM remains loopback-only and disabled by default;
- no root/sudo, privileged container, network namespace, VM, systemd, or public-I2P requirement for current acceptance work;
- no external-client/reference patching or vendoring;
- no private SAM `PRIV`, signing seed, SSU2 static/session secret, token, or raw private application payload in logs/evidence;
- do not make `DestinationIdentity: Clone` or reconstruct a second private identity for the SAM bridge;
- an external interoperability test may be ignored in routine CI only when its dedicated lane explicitly opts in and remains fail-closed if required external configuration is absent.

Static boundary scripts are the source of truth. Fix violations; do not weaken
the scripts.

## Build/test floor

Run from repository root before handoff:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
 bash scripts/check-dependency-direction.sh
 bash scripts/check-runtime-boundaries.sh
 bash scripts/check-service-tunnel-boundaries.sh
 bash scripts/check-fixture-manifest.sh
 bash scripts/check-ntcp2-vectors.sh
 bash scripts/check-ssu2-vectors.sh
 bash scripts/check-i2cp-vectors.sh
 bash scripts/check-ntcp2-interoperability.sh
 bash scripts/check-constrained-host-lane-boundary.sh
 bash scripts/check-sam-acceptance-evidence.sh
 bash scripts/check-ssu2-acceptance-evidence.sh
 bash scripts/check-i2cp-acceptance-evidence.sh
 bash scripts/check-service-tunnel-acceptance-evidence.sh
 bash scripts/check-m6-mixed-router-acceptance-evidence.sh
 bash scripts/check-streaming-tunnel-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Focused SAM seams currently include:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

Plan 151 added its narrowly named final-acceptance suite
(`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`) rather than bloating
the existing self-composed file. The Plan 151 evidence-integrity checker
(`scripts/check-sam-acceptance-evidence.sh`) is enforced in routine Linux CI
and the manual SAM external workflow; do not weaken it to make CI pass.

 Plan 176 added the runtime-neutral `i2pr-service-tunnels::http` module
 (bounded HTTP/1.1 parser with smuggling rejection, hop-by-hop +
 privacy rewrite, `.i2p`-only target validation, bounded error
 response generation) and the daemon HTTP proxy executor
 (`crates/i2pr-daemon/src/service_tunnels_http.rs`) that owns one
 loopback listener per `http-client` spec. It reuses the Plan 174
 shared byte pump + Plan 149 destination product path; no new
 Garlic/I2NP/Streaming implementation exists. Plan 180 closed the
 M10 local product layer by adding the generation/draining
 reconcile surface; the per-service byte round-trip is now proven
 through the Plan 175/176/177/178/179 product suites that the manager
 composes under one supervisor.
runtime driver loop is exercised, while the byte round-trip remains
an explicit Plan 180 deliverable.

Plan 177 added the runtime-neutral `i2pr-service-tunnels::socks5`
module (RFC 1928 no-auth greeting negotiation, CONNECT request
parser with strict `.i2p`/DOMAINNAME-only target policy,
deterministic RFC 1928 reply generator with a neutral loopback
`127.0.0.1:0` bind, bounded typed errors) and the daemon SOCKS5
proxy executor (`crates/i2pr-daemon/src/service_tunnels_socks5.rs`)
that owns one loopback listener per `socks5-client` spec. It
reuses the Plan 174 shared byte pump + Plan 149 destination product
path; no new Garlic/I2NP/Streaming implementation exists. Success
is sent only after Streaming reaches `Established`; same-read
 post-request bytes are preserved as first tunnel bytes; BIND, UDP
 ASSOCIATE, IPv4/IPv6, clearnet/IP literal/localhost/mixed-suffix
 targets, and username/password auth are rejected with the typed
 RFC 1928 reply codes. Plan 180 closed the M10 local product layer
 by adding the generation/draining reconcile surface that the
 per-service product suites all reuse.

Plan 178 added the runtime-neutral `i2pr-service-tunnels::irc`
 module (bounded IRC/IRCv3 line parser with 512-byte core / 8191-byte
 tag-envelope / 4094-byte tag-data ceilings, typed command classifier
 with per-direction allowlist, client-to-network privacy rewrites for
 USER/PING/QUIT/PART, CTCP/DCC policy allowing ACTION while dropping
 DCC and unsupported CTCP, bounded typed errors) and the daemon IRC
 client tunnel executor
 (`crates/i2pr-daemon/src/service_tunnels_irc_client.rs`) that owns one
 loopback listener per `irc-client` spec. It reuses the Plan 174 shared
 byte pump + Plan 149 destination product path; no new Garlic/I2NP/Streaming
 implementation exists. Unknown/unclassified commands are dropped, never
 passed; overlong lines are dropped without truncation. Plan 180
 closed the M10 local product layer by adding the
 generation/draining reconcile surface that the IRC client product
 suite reuses under one supervisor.

Plan 179 added the runtime-neutral
`i2pr-service-tunnels::irc::server` registration interceptor
(bounded pre-registration line / byte ceilings with a typed default
of 10 lines / 8192 bytes, cross-protocol rejection of HTTP and
BitTorrent first lines via a small fixed list, an authenticated peer
Destination hash projection to `<52-char base32>.b32.i2p` that
replaces the USER hostname and is bound to the streaming peer
identity, RFC 2812 four-arg USER with the servername preserved and
legacy RFC 1459 USER with the mode parameter rejected, IRCv3 tagged
USER rewrite with envelope preserved, PASS / CAP / AUTHENTICATE /
NICK passthrough, same-read post-USER bytes preserved as first
raw-pump bytes, an optional `SERVER` server-to-server IRC handoff,
typed `RegistrationOutcome::{Incomplete, Ready, Rejected, Eof}`,
and bounded typed errors) plus the daemon IRC server tunnel
executor (`crates/i2pr-daemon/src/service_tunnels_irc_server.rs`)
that owns one Streaming accept loop per `irc-server` spec, waits for
the Streaming connection to reach `Established`, captures the peer
Destination hash from authenticated Streaming metadata (the only
acceptable source for the projected hostname), runs the bounded
registration interceptor under a 30 s total deadline (with a 20 ms
poll cadence), connects to the loopback target under a 10 s deadline,
writes the rewritten prefix + leftover exactly once, and switches
to the shared Plan 174 byte pump in opaque mode for the
post-registration stream. The Plan 175 persistent server destination
storage owns the IRC server destination identity so restart
preserves both the public service Destination and the projected
hostname algorithm. No WEBIRC, no cloaked hostnames, no DCC, no
TLS termination, no IRC daemon implementation, and no
post-registration server-side filter claim; the post-handoff
stream is byte-transparent. Plan 180 closed the M10 local product
 layer by adding the generation/draining reconcile surface that
 the IRC server product suite reuses under one supervisor.

Focused SSU2 seams currently include:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

During Plan 162, ordinary no-peer invocation of the external driver must be:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected: external test discovered as ignored; command exits 0
```

The explicit external invocation after Plan 162 gating is:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

Without the required external environment, that explicit command must fail
closed. With exact-pinned i2pd provisioned, it must execute and pass the
full matrix (directions A+B, cached-token, malformed/resource rows).

The full Plan 161 lane (local suites + matrix + gates, 15 command-derived
rows) is:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

Plan 155 added the SSU2 fixture corpus (`tests/fixtures/ssu2/`) and its
checker (`scripts/check-ssu2-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass.

Focused I2CP seams currently include:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

Focused M10 service-tunnel foundation seams currently include:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --lib destination_streaming
cargo test --locked -p i2pr-daemon --lib config
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_independent_application_clients -- --test-threads=1
# Plan 207 — external driver for the genuine remote HTTP/IRC
# application interop is `#[ignore]`-gated; the lane invokes it
# only with the exact-pinned i2pd environment provisioned.
# Plan 208 — external driver for the production delivery-driver
# remote-route integration is `#[ignore]`-gated; the lane invokes
# it only with the exact-pinned i2pd environment provisioned.
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

The full Plan 181 + Plan 202 + Plan 206 + Plan 207 + Plan 208 lane
(local suites + matrix + gates, 32 command-derived rows with 2
remote rows that flip from `blocked` to `passed` when the dedicated
M6 interop lane provisions the SSU2 endpoint + bind tuple and the
Plan 207 driver emits every documented Plan 207 §9 subfact row in
the same evidence directory/run id) is:

```text
bash tests/integration/service-tunnels/run-independent.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

Plan 182 added the M10 local-delivery corrective (per-destination
delivery drivers, wildcard Streaming port 0, SAM-parity accept
paths, direction-branched pump sends, completed IRC client
executor, orderly pump half-close) plus the `jaraco/irc`
fetch script (`scripts/interop/fetch-service-tunnel-clients.sh`)
and the manual `.github/workflows/service-tunnels-external.yml`
lane; do not weaken the evidence checker to make CI pass.

Focused M6 preflight seams currently include:

```text
cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib config -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
# expected: 5 passed, 1 ignored (external preflight gated)
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight \
  ssu2_daemon_preflight_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: passes
bash tests/integration/m6-interop/run-preflight.sh
```

Focused M6 exploratory tunnel seams currently include:

```text
cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_tunnel_external \
  exploratory_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: ignored; with lane env: passes (12-row external lane)
bash tests/integration/m6-interop/run-tunnels.sh
bash scripts/check-exploratory-tunnel-evidence.sh
```

Focused M6 NetDB seams currently include:

```text
cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- --test-threads=1
# expected: 22 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1
# expected: 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_tunnel_external \
  netdb_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: passes
bash tests/integration/m6-interop/run-netdb.sh
bash scripts/check-netdb-tunnel-evidence.sh
```

Focused M6 destination seams currently include:

```text
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
# expected: 32 passed (Plan 187: 27; Plan 190: 4 inbound reply-path rows; Plan 192: 1 i2cp data body)
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
# expected: 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_external \
  destination_message_plane_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: the
# lane now advertises the corrected (gateway router, gateway receive id)
# reply path (Plan 190); the lease-lookup row flips from blocked to passed
# only when a real tunneled lookup response arrives within the bounded
# wait; the inbound-delivery rows flip from blocked to passed via the
# Plan 192 i2pd-compatible I2CP-style Data body + 9-byte short-transport
# inner envelope + STYLE=RAW SAM session (Plan 191 §6 stop provenance;
# see plans/192-status.md)
bash tests/integration/m6-interop/run-destination.sh
bash scripts/check-destination-tunnel-evidence.sh
```

Focused M6 mixed-router Streaming seams (Plan 193, closed
`passed-m6-i2pd-mixed-router-streaming`; 33/33 rows green twice on
exact head `3687189`):

```text
cargo test --locked -p i2pr-daemon --test streaming_tunnel_unit -- --test-threads=1
# expected: 15 passed
cargo test --locked -p i2pr-daemon --test streaming_tunnel_live -- --test-threads=1
# expected: 11 passed
cargo test --locked -p i2pr-daemon --test streaming_tunnel_external -- --test-threads=1
# expected: 0 passed, 1 ignored (fail-closed ordinary invocation)
cargo test --locked -p i2pr-daemon --test streaming_tunnel_external \
  streaming_through_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env:
# full Direction A + Direction B matrix passes with digest equality
# (SYN/Established, 25 B + 8192 B, reverse 23 B + 4096 B, sibling,
# close/EOF + isolation, B CONNECT/Established + 17 B + 2048 B +
# close/EOF); direct transport is rejected as a counted path;
# liveness-first-test stays green alongside Streaming activity
bash tests/integration/m6-interop/run-streaming.sh
bash scripts/check-streaming-tunnel-evidence.sh
```

Focused M6 mixed-router cross-family seams (Plan 189 §8
ledger/checker/workflow scaffold + Plan 193 first-family
qualification + Plan 194 second-family scaffolding):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# static structural check: per-layer harnesses + checker both
# pin i2pd 2.61.0 + Java I2P 2.13.0; cross-family aggregator
# wires both pins; cross_family_row helper keeps the rc gate.
# Plan 194 adds run-streaming.sh / check-streaming-tunnel-evidence.sh
# + run-java.sh / fetch-m6-java.sh to the required-artifacts list
# so both families stay fail-closed.

 bash tests/integration/m6-interop/run-m6-mixed-router.sh
# cross-family aggregator: reuses the four per-layer harnesses
# + run-streaming.sh + run-java.sh and binds every Plan 189 §8
# guarded row to a family + the actual per-layer exit code. The
# Java rows stay bound to the same evidence.json; the second-
# family rows are recorded `failed` with stop provenance until
# Plan 196's external run against the exact-pinned Java cache
# proves the ControlledRouter-driven controlled topology plus
# authenticated SSU2 preflight.
```

Focused M6 Java second-family seams (Plan 194 §3/§5/§11
scaffolding; topology blocker recorded; Plan 200 closed the
diagnostic/evidence side; Plan 201 owns the corrective; required):

```text
bash scripts/interop/fetch-m6-java.sh --rebuild
# Plan 194 §3 fetch + build: clones exact-pinned Java I2P 2.13.0
# (9134f808...), runs `ant updater preppkg` (no IzPack 5 GUI
# installer — the staged pkg-temp/ IS the install dir), stamps
# build-metadata.txt with the exact pin + per-OS launcher probe.

cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
# expected: 0 passed, 3 ignored (fail-closed ordinary invocation;
# bootstrap_java_router_peers, destination_message_plane_against_java,
# streaming_through_java)

cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  bootstrap_java_router_peers -- --ignored --exact --test-threads=1
# Plan 200 §B: emits p200-routerinfo-lookup-{a,b}-{knows,by}-{b,a}
# post-bootstrap DatabaseLookup proofs in both directions plus one
# terminal p200-classification row; without lane env: fail-closed
# (missing required env); with lane env: the post-bootstrap lookup
# boundary determines the first failing publication step.

cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  destination_message_plane_against_java -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env:
# the lane now records sanitized Plan 200 §C/D lifecycle keys
# (java-client-subdb-created, java-create-leaseset2-received,
# java-client-leaseset-stored-current, java-client-leaseset-publish-scheduled,
# java-client-leaseset-republish-job-ran, java-client-inbound-tunnel-selectable,
# java-client-outbound-tunnel-selectable, java-floodfill-candidate-non-empty,
# java-store-emitted, java-store-ack-observed, java-store-failure-reason)
# before the existing destination/Streaming rows; helper `READY` no
# longer implies `leaseset=published`; mandatory destination rows
# stay blocked with fail-closed provenance pending Plan 201.

cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  streaming_through_java -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env:
# mirrors the destination driver; mandatory Streaming rows stay
# blocked with fail-closed provenance pending Plan 201.

bash tests/integration/m6-interop/run-java.sh
# Plan 194 §5/§11 + Plan 196 controlled topology + Plan 200 §B/C/D
# second-family harness: provisions two disposable Java routers with
# fresh disposable data dirs, waits for the SAM bridge + I2CP on
# 127.0.0.1, runs the bootstrap probe (Plan 200 §B), the public raw
# + Streaming helpers (Plan 200 §A), the destination driver, and the
# Streaming driver; writes sanitized evidence under
# target/interop/m6-java-evidence/; emits one terminal
# `P200-{A..H}` classification. Downstream M6 rows stay blocked
# with fail-closed provenance pending Plan 201.
```

Plan 184 owns the daemon-owned authenticated I2NP spine
(`crates/i2pr-daemon/src/router_i2np.rs`: central dispatcher,
narrow `RouterDeliveryService` over existing `send_i2np`, strict
loopback/non-advertised `[ssu2]` activation, daemon-owned
`Ssu2DaemonService` under `ssu2-router` supervision). Plan 185
adds the daemon-owned exploratory build coordinator
(`crates/i2pr-daemon/src/exploratory_build.rs`: bounded pending
table, monotonic attempt / creator tunnel ids, single central
scheduler, strict OTBRM extraction, `register_*_with_material`
installs through the existing `ExploratoryPool` then activates
once into `DataPlaneRegistry`) and the bounded creator-side
tunnel liveness scheduler
(`crates/i2pr-daemon/src/tunnel_liveness.rs`: one central scheduler,
no per-tunnel task or per-tunnel timer, first-test / repeat /
response-timeout / failure-threshold policy well below the
two-minute idle deletion boundary). The exploratory tunnel
external lane is ignored in routine CI and explicitly selected
in its dedicated lane; do not add filename filtering, `|| true`,
fake peer values, or production wire changes to make it green.
The external driver derives the build encryption key from
`RouterInfo.router_identity().public_key()` (NOT the SSU2 `s`
option) so the daemon-owned runtime key and the build key share
the same X25519 keypair. i2pd is provisioned with
`notransit = false` so the reference accepts one-hop exploratory
builds; the lane stays loopback + unpublished and no public I2P
claim is made. Plan 186 adds the daemon-owned NetDB-over-tunnels
coordinator (`crates/i2pr-daemon/src/netdb_tunnels.rs`: authoritative
bounded RouterInfo store, ordinary-path reference bootstrap, floodfill
verification, tunnel-path proofs, bounded lookup/publication/search
matrices, typed tunnel-loss without direct fallback). The NetDB external
lane is ignored in routine CI and explicitly selected in its dedicated
lane; i2pd is provisioned with `notransit = false, floodfill = true`
so the reference accepts builds and acts as the controlled floodfill.
Plan 187 adds the daemon-owned destination-over-tunnels coordinator
(`crates/i2pr-daemon/src/destination_tunnels.rs`: authoritative
bounded RouterInfo store, store-parameter LeaseSet2 lookup through
the existing seam, tunnel-path proofs, authoritative LeaseSet2
cache, real-material proofs rejecting `LocalZeroHop`, bounded
local-LS2 publication with protocol-derived ack, registry-backed
Garlic recovery, typed tunnel-loss without direct fallback) plus
narrow additive seams (`begin_lease_set2_lookup_with_store` /
`ingest_lease_set2_search_reply` on the seam, `GarlicComplete` on
inbound dispatch, `deliver_outbound_cells` for client-composed
Garlic cells, registry/role accessors, `DestinationOutboundRole::from_role`
by move). The destination external lane is ignored in routine CI
and explicitly selected in its dedicated lane; i2pd is provisioned
with `notransit = false, floodfill = true` plus loopback SAM so the
reference DATAGRAM destination is created through its public
client surface. The lane stops fail-closed at the §11
build-reply gate (reference accepts builds per its transit log
but emits no consumable reply; 7 install-dependent rows recorded
blocked with multi-run diagnosis); Plan 188 owns the narrow
corrective; Plan 190 isolates and corrects the inbound NetDB
reply-path metadata defect that left 5/7 destination rows
blocked after the Plan 188 installs. The raw i2pd log is never
evidence (SAM session lines); only sanitized counts reach
evidence.

Plan 164 added the I2CP fixture corpus (`tests/fixtures/i2cp/`) and its
checker (`scripts/check-i2cp-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass. Plan 165 added the connection state
machine, SessionConfig verification, option disposition table, and
session registry under `crates/i2pr-api/src/i2cp/`; no I2CP
listener, destination activation, or client-interoperability claim
exists yet. Plan 166 added the client-owned destination runtime +
Standard LeaseSet2 validation path under `crates/i2pr-client/`
(`DestinationOwnership`, `DestinationPublic`,
`InboundDecryptionCapability`, `install_client_lease_set2`,
`LeaseRequest`, `take_client_refresh_request`) and the
`I2cpAction::RequestVariableLeaseSet` typed action in
`crates/i2pr-api/src/i2cp/actions.rs`; the listener, socket
ownership, and client-interoperability claim remain Plans 167–170.
Plan 167 added the supervised loopback I2CP v0.9.67 server runtime
in `crates/i2pr-daemon/src/i2cp.rs` plus the real-TCP acceptance
test in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. Plan 168 added
the message data plane under `crates/i2pr-api/src/i2cp/data_plane.rs`
(`I2cpMessageOutcome`, `PendingStatusTable`, `InboundPayloadQueue`,
bounded per-session ceilings, `InboundPayloadFrame::WIRE_OVERHEAD_BYTES`)
plus the per-session `I2cpSessionState` in
`crates/i2pr-daemon/src/i2cp.rs` and the eighteen-test black-box
acceptance suite in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs`;
the daemon validates `SendMessage`/`SendMessageExpires`, routes
payloads through the existing `i2pr_client::DestinationRuntime::enqueue_outbound`,
drains `MessagePayload` inbound frames through a `tokio::sync::Notify`,
resolves `DestLookup` against the local destination registry, and
returns the config-derived `BandwidthLimits` reply. Plan 169 added
the reconfigure transaction handler (`handle_reconfigure_session` +
`apply_reconfigure` + `ReconfigurationOutcome`) and the per-session
reconfigure baseline (`I2cpSessionState::last_options`), the
synchronous `handle_destroy_session` data-plane drain, and three
new narrowly named acceptance suites:
`crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (the canonical
self-composed trajectory plus a bounded repeated-lifecycle soak and
a destroy-one-session/keep-sibling usable proof),
`crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (the Plan 169
§5 adversarial protocol matrix), and
`crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (the Plan 169 §6
concurrency/resource matrix). Independent Java/Go client evidence
remains in Plan 170.

## Testing conventions

- Prefer paused Tokio/manual clocks for deterministic runtime tests where compatible with socket behavior.
- Runtime-owned socket tests use loopback only; no DNS/public traffic.
- Use explicit bounded deadlines, not indefinite waits.
- Queue/resource tests cover exact capacity/max+1 and verify release after failure/closure.
- Secret-bearing values stay redacted/zeroized/non-Clone where practical.
- Channel/socket closure is a lifecycle event, not blindly retried.
- A required evidence result must be derived from an executed command/test. Never mark an acceptance row passed merely because a historical plan says it passed.
- External tests that require a separately provisioned process must never silently pass when that process/environment is absent.

## External SAM evidence

Provenance retained from Plan 150:

```text
Java I2P Plan146 reference:
  2800040deee9bb376567b671ef2e9c34cf3e30b6

i2pd Plan146 reference:
  f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e

i2psam counted Plan150 client:
  b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac

i2plib counted substitute SAM surface:
  6edf51cd5d21cc745aa7e23cb98c582144884fa8

libsam3 built/probed but not counted:
  7d6e658798baec31394c5685f9583343cc00900b
```

The manual `.github/workflows/sam-external.yml` lane is unprivileged and
localhost-only. Plan 151 reran it on the exact closing head after all new
acceptance tests were integrated; Plan 153 made its evidence checker a
permanent invariant.

## External SSU2 evidence

Current mandatory reference:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
role = mandatory Plan 161 independent direct SSU2 reference
```

Preferred secondary reference:

```text
Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
role = preferred secondary; recorded nonblocking narrow-orchestration debt (see plans/161-status.md)
```

Plan 161 directions A+B plus the ledger/checker/workflow lane have
passed locally. Plan 162 corrected how that external-process
test is selected by routine versus dedicated lanes; the corrective is now
closed.

## Coding conventions

- No unsafe in protocol/client/API/service crates unless separately reviewed.
- Treat all SAM/network bytes as hostile and bounded.
- Use typed errors; do not swallow codec/protocol results.
- Runtime cryptography/local-product ephemeral material uses OS CSPRNG.
- New dependencies require explicit review.
- Avoid global mutable state/service locators; pass narrow capabilities.
- Do not modify M6 wire semantics to make a SAM test convenient.
- Do not modify SSU2 wire semantics merely to make a CI lane green.

## Protocol claims

- Milestone 6 local product is closed via Plan 134; router interoperability is not claimed.
- Plan 149 closed the self-composing localhost SAM product.
- Plan 150 retains at-least-two independent-client core evidence, but its final acceptance label is superseded.
- Plan 151 is the current final Milestone 7 acceptance authority.
- Plan 152 is the passed narrow M6 robustness corrective retained underneath Plan 151.
- Plan 153 is the passed docs/CI hygiene pass.
- Plans 155–160 are passed Milestone 8 SSU2 v2 local protocol/runtime/reachability stages.
- Plan 161 is passed: directions A+B (+ cached-token/malformed rows) against exact-pinned i2pd 2.61.0 are proven over real loopback UDP with authenticated bidirectional evidence, and the fail-closed ledger/checker/workflow lane passes locally and hosted (routine CI runs `34050058216`/`34053041778`, external runs `34051298144`/`34053042857`). Milestone 8 is closed within that bounded scope.
- Plan 162 passed the narrow external-test lane/CI corrective.
- Plan 163 registered the Milestone 9 I2CP roadmap (planning authority only).
- Plan 164 passed the M9 I2CP protocol and wire foundation (structural codecs, fixtures, profile; no behavior claim).
- Plan 165 passed the M9 I2CP connection/session/options state machines (typed connection state, SessionConfig signature/date/ceiling verification with injected clock, option disposition table, bounded session registry, reconfiguration taxonomy, typed `I2cpAction` vocabulary; no listener, destination activation, or interoperability claim).
- Plan 166 passed the M9 I2CP client-owned destination + LeaseSet2 bridge: `DestinationOwnership::RouterOwned` / `ClientOwned`, `DestinationPublic`, `InboundDecryptionCapability`, atomic `install_client_lease_set2` (signature + lease ownership + expiry + decryption-key match), typed `LeaseRequest` for refresh from real inbound tunnels, and the `I2cpAction::RequestVariableLeaseSet` action. SAM router-owned product regressions remain green; no listener, socket ownership, or interoperability claim.
- Plan 167 passed the M9 I2CP loopback server runtime in `crates/i2pr-daemon/src/i2cp.rs`: disabled-by-default `[i2cp]` block, supervised Tokio listener, per-connection `ChildScope`, typed `I2cpAction` dispatch, single `teardown_connection` cleanup path on EOF/reset/timeout/cancel, and twelve real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. No application-message direction, no lookup, no reconfiguration, no independent-client interop claim.
- Plan 168 passed the M9 I2CP message data plane: bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam, bounded `MessageStatus` correlation table, `MessagePayload` inbound frames delivered only to the owning session's bounded queue (sibling-isolation guaranteed), cross-session local loopback shortcut, `DestLookup` resolving through the local destination registry, and `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case. SAM router-owned product regressions remain green. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client interop claim.
- Plan 169 passed the M9 I2CP self-composed local product and hardening: the `ReconfigureSession` transaction handler (`handle_reconfigure_session` + `apply_reconfigure` + `ReconfigurationOutcome`) parses/verifies the full new SessionConfig, classifies each diff entry using the Plan 165 `reconfiguration_class` table, and commits the new baseline atomically through `I2cpSessionState::last_options`; immutable and unsupported keys reject the whole transaction without state mutation. `handle_destroy_session` now drains the per-session Plan 168 data-plane bookkeeping synchronously so repeated DestroySession/CreateSession cycles retain zero inbound queue, status correlation, or outbound slot. The Plan 169 acceptance suites are `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests), `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (19 tests at Plan 169 close; 20 after the Plan 171 companion), and `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests); every test binds the listener to `127.0.0.1:0` and drives behavior only through TCP/I2CP inputs. SAM router-owned product regressions, the Plan 167 listener regression in `i2cp_loopback.rs`, and the Plan 168 data-plane suite in `i2cp_message_data_plane.rs` remain green. No `HostLookup`/`HostReply` resolution and no independent Java/Go client evidence yet; those belong to Plan 170.
- Plan 171 passed the M9 I2CP invalid-preamble close and CI corrective (retained): the common per-connection terminal path in `crates/i2pr-daemon/src/i2cp.rs` explicitly shuts the TCP stream down before bookkeeping release (no wire change, no independent-client claim). The adversarial matrix is now 20 tests (the strict 24-iteration `wrong_protocol_byte_is_closed` plus its non-paused `wrong_protocol_byte_is_closed_real_time` companion).
- Plan 170 external wire/data-plane evidence is retained-passed: exact-pinned Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) and go-i2cp (`b529ee1c10a6011558b4d69fc9436a4afc489eac`) exchange digest-matched 25 B/32 KiB payloads in both directions through the loopback daemon (`tests/integration/i2cp/run-independent.sh`, 9 fail-closed rows, `scripts/check-i2cp-acceptance-evidence.sh` in routine CI, manual `.github/workflows/i2cp-external.yml`). Its final-acceptance interpretation is superseded by Plan 172: the counted Java driver bypassed `I2PSession.connect()` and no external session installed a LeaseSet2. Do not claim independent LeaseSet2 lifecycle from Plan 170 rows alone.
 - Plan 172 passed the M9 I2CP independent LeaseSet2 lifecycle corrective (see `plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md` and `plans/172-status.md`): explicit local zero-hop tunnel kind, 44-byte Lease-compatible non-empty real `RequestVariableLeaseSet` from the destination pool, existing Plan 166 atomic install (ElGamal-slot skip, unpublished accepted, u8 LS2 key count), high-level Java `I2PSession.connect()` plus public go-i2cp async `ProcessIO` lifecycle proof, post-LS2 digest-matched cross-client traffic both directions, fail-closed 24-row evidence. Milestone 9 final acceptance is closed-via-plan172.
 - Plan 173 registered the Milestone 10 service-tunnels roadmap.
 - Plan 174 passed the M10 service-tunnel foundation: runtime-neutral `i2pr-service-tunnels` crate, strict disabled-by-default loopback-only `[service_tunnels]` surface, shared daemon Streaming pump reused by SAM, no listener yet.
 - Plan 175 passed the first complete M10 application service product (generic client/server tunnels + persistent server destinations).
 - Plan 176 passed the M10 HTTP `.i2p` proxy + CONNECT.
 - Plan 177 passed the M10 SOCKS5 `.i2p` CONNECT proxy.
 - Plan 178 passed the M10 IRC `.i2p` client profile + privacy filter.
 - Plan 179 passed the M10 IRC `.i2p` server profile + authenticated peer hostname projection.
 - Plan 180 passed the M10 service-tunnel composition, reconcile, and hardening (see `plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md` and `plans/180-status.md`): the runtime-neutral `i2pr_service_tunnels::generation::DiffClass` typed classification (`Unchanged`, `MutableInPlace`, `ReplaceListener`, `ReplaceDestination`, `Remove`, `Add`); the daemon-owned `ServiceTunnelGeneration`/`DrainingGeneration` committed-generation model with `GenerationCounters { active_current_generation, active_draining_generation, forced_drain_closes_total }`; the `ServiceTunnelManager::reconcile(candidate, drain_deadline)` transactional algorithm that validates, diffs, stages Add/Replace*, then publishes the new generation atomically and pushes only replaced/removed old runtimes onto the draining list under a hard deadline; `reap_expired_drains` for forced-drain close handling; `generation_snapshot` for the Plan 180 §9 unified cross-service resource accounting matrix; the static `scripts/check-service-tunnel-boundaries.sh` checker enforcing the runtime-neutral constraint, no Garlic/I2NP construction in service-tunnels, the single shared `run_stream_pump` invariant, no unbounded Tokio channels, and exactly one `register_service_tunnel_manager` entry point. Two new narrowly named suites (`crates/i2pr-daemon/tests/service_tunnels_final_acceptance.rs` — 15 tests covering the Plan 180 §12 reconcile matrix, `crates/i2pr-daemon/tests/service_tunnels_adversarial_matrix.rs` — 12 tests covering the Plan 180 §13 cross-service adversarial matrix) bind the manager to a temp data directory and drive behavior only through the public API. Every Plan 174/175/176/177/178/179 product test remains green. Plan 180 closes the M10 local product layer; Plan 181 owns the M10 independent acceptance gate. M10 service tunnels stay experimental, loopback-only, disabled by default, and non-advertised; no independent router interop claim.
 - Plan 181 is blocked by the retained M6 mixed-router Streaming debt (see `plans/181-m10-independent-application-and-service-interop-final-closure.md` and `plans/181-status.md`): 29 local independent-application-client rows pass (unmodified curl HTTP/SOCKS, nc, stdlib generic driver, exact-pinned jaraco/irc through the real manager; restart stability; resource baselines; unsupported-profile ledger), and the two remote rows are recorded `blocked` with command/log provenance from a genuine qualification attempt (exact-pinned i2pd 2.61.0 SAM `DEST GENERATE` public destination; `unknown_peer>0`, `delivered=0`, no establishment, bounded timeout). Self-composed rows are never substituted for interop. Milestone 10 final acceptance stays open.
 - Plan 182 passed the M10 local-delivery corrective (see `plans/182-m10-local-delivery-corrective.md` and `plans/182-status.md`): per-destination delivery drivers reusing the Plan 129 `bridge_to_peer` seam, inbound-factory install, wildcard Streaming port 0 (SAM convention), SAM-parity accept paths with queued SYN responses, direction-branched pump sends with typed backpressure matching, orderly pump half-close (default no-op keeps SAM byte-identical), completed line-filtering IRC client executor, permit-for-task-lifetime capture, and active-slot release on every exit path. Nine round-trip tests in `service_tunnels_local_roundtrip.rs` plus six wire-surface tests prove the local byte round-trip the Plan 174–179 profiles assumed. No wire change.
  - Plan 183 registered the M6 mixed-router destination/Streaming interop program Plan 181 §6.3 requires (see `plans/183-m6-mixed-router-streaming-interop-program.md` and `plans/183-status.md`): registration only, no implementation, no M10 closure claim.
 - Plan 184 passed the M6 authenticated I2NP preflight (see `plans/184-m6-authenticated-i2np-runtime-and-reference-preflight.md` and `plans/184-status.md`): strict loopback/non-advertised `[ssu2]` activation, daemon-owned `Ssu2DaemonService` under `ssu2-router` supervision, central `router_i2np` dispatcher with authenticated peer preservation, narrow `RouterDeliveryService` over existing `send_i2np`, exact-pinned i2pd 2.61.0 bidirectional control with fail-closed 10-row lane. No tunnel/NetDB/Streaming/M10-remote claim.
- Plan 185 passed the M6 live one-hop exploratory tunnels and liveness lane (see `plans/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md` and `plans/185-status.md`): daemon-owned `ExploratoryBuildCoordinator` + `TunnelLivenessScheduler` drive the existing `i2pr-tunnel::short::ShortBuildStateMachine` / `i2pr-tunnel::pool::ExploratoryPool` / `i2pr-tunnel::data_plane_registry::DataPlaneRegistry` seams end-to-end through the Plan 184 central `router_i2np` dispatcher; one real outbound and one real inbound one-hop exploratory build accepted by the exact-pinned i2pd 2.61.0 reference with `notransit=false`; bounded first-test / repeat / response-timeout / failure-threshold liveness policy; 12-row external lane + `scripts/check-exploratory-tunnel-evidence.sh` static evidence check. No multi-hop, no destination LeaseSet2 / Streaming claim.
- Plan 186 passed the M6 mixed-router NetDB lookup and publication lane (see `plans/186-m6-mixed-router-netdb-lookup-and-publication.md` and `plans/186-status.md`): daemon-owned `NetDbTunnelCoordinator` drives the existing lookup/publication state machines over the Plan 185 pair through the authoritative bounded store (ordinary-path reference bootstrap, floodfill verification, tunnel-path proofs, bounded matrices, typed tunnel-loss); exact-pinned i2pd 2.61.0 with `notransit=false,floodfill=true`; 22 unit + 9 live + 12-row external lane + `scripts/check-netdb-tunnel-evidence.sh`. No multi-hop, no destination LeaseSet2 / Streaming claim; Plan 187 landed the destination program (local rows passed, remote gate pending Plan 188).
- Plan 187 is blocked by the `m6-build-reply-interop-gap` (see `plans/187-m6-remote-leaseset2-and-destination-garlic-routing.md` and `plans/187-status.md`): the daemon-owned `DestinationTunnelCoordinator` with the full local destination message plane is landed (27 unit + 9 live two-role rows including the bidirectional ECIES/Garlic round-trip with sibling isolation over real TunnelData cells; narrow additive seams on the NetDB seam/inbound-dispatch/outbound-lookup/registry, no wire change), and the 21-row external lane proves session, reference build acceptance both directions, SAM DATAGRAM destination, reference LS2 publication, direct rejection, and liveness — but exact-pinned i2pd 2.61.0 emits no consumable ShortTunnelBuildReply (multi-run diagnosis: lossless session, `kind_reply=0`, reference transit acceptance logged), so no tunnel material installs and 7 install-dependent rows are recorded `blocked` with stop provenance. Creator-known keys are never installed without a consumed reply. Plan 188 owns the narrow build-reply corrective; no LeaseSet2/Streaming interop is claimed.
- Plan 188 is the short-build-reply corrective (see `plans/188-m6-short-build-reply-interop-corrective.md` and `plans/188-status.md`): outbound garlic-unwrap (TunnelGateway + Garlic with OBEP `RGarlicKeyAndTag`) plus inbound forwarded-ShortTunnelBuild consumption in `ExploratoryBuildCoordinator` land consumed-reference installs both directions (`installed_ob=1 installed_ib=1`); 5/7 destination rows flipped to passed and 2/4 destination-message-bound rows + 2 ordering rows were blocked on the inbound delivery layer that Plan 191 owned; no synthesis, no wire change.
- Plan 193 is the M6 i2pd mixed-router Streaming qualification plan (see `plans/193-m6-i2pd-mixed-router-streaming-qualification.md`, `plans/193-status.md`, and `plans/193-streaming-status.md`): closed (`passed-m6-i2pd-mixed-router-streaming`; full Direction A + Direction B external matrix passed twice on exact head `3687189`, 33/33 rows, evidence `passed-via-i2pd-2.61.0`). Plan 193 supersedes the historical `plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming file (which remains historical context only). Plan 198 was the previous executable corrective for the Java public-client LeaseSet2 publication boundary; it is decomposed into Plans 200–204. Plan 200 is the current diagnostic/evidence corrective: Java helpers decoupled `leaseset=published` from `READY` and added bounded `REPORT_STATUS`; the Rust driver now proves Router A/B main-NetDB bootstrap through ordinary post-store DatabaseLookup round-trips in both directions; sanitized Java log keys for the client LS2 lifecycle and tunnel/floodfill/store/ack selection are emitted to evidence; exactly one terminal `P200-{A..H}` classification is recorded per run; downstream M6 rows stay blocked until Plan 201 picks the smallest standards-compatible corrective. Plan 195 remains gated.
- Plan 190 is the inbound NetDB reply-path tunnel-ID corrective (see `plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md` and `plans/190-status.md`): typed public `InboundGatewayRoute` (`gateway_router`, `gateway_receive_tunnel`, `local_receive_tunnel`) retained by `i2pr_tunnel::data_plane_registry::DataPlaneRegistry`; daemon-owned `reply_path_for_inbound_route` adapter derives `i2pr_netdb::ReplyPath` only from `(gateway_router, gateway_receive_tunnel)` so the local creator endpoint receive tunnel id is impossible to copy into the encoded `DatabaseLookup.reply_tunnelId`; `i2pr-netdb::ReplyPath`/`build_databaselookup` semantics unchanged. Regression rows in `crates/i2pr-tunnel/src/data_plane_registry.rs` and `crates/i2pr-daemon/tests/destination_tunnel_unit.rs` prove unequal IDs (`0x9601` vs `0x9602`) round-trip through the I2NP codec with the gateway tuple on the wire, and that lifecycle removal cleans the typed route atomically. Local Plan 187/188 suites remain green (`destination_tunnel_unit` 31 passed, `destination_tunnel_live` 9 passed, `exploratory_build_live` 11 passed). A fresh exact-pinned i2pd 2.61.0 external `run-destination.sh` proves 3 destination rows flip `blocked` → `passed`; the corrected lane stops at Plan 190 §6 boundary E (inbound delivery). No M6 wire change; no `milestone6_interoperable = passed-via-plan190` claim.
- Plan 191 is the inbound destination delivery boundary (see `plans/191-m6-inbound-destination-delivery-boundary.md` and `plans/191-status.md`): Plan 191 ran the inbound-delivery layer and stopped at boundary E per §6; Plan 192 retained-passed the defect. The corrected `run-destination.sh` reached `destination-outbound-delivered cells=1 payload_len=27` then the i2pd SAM bridge never observed a `DATAGRAM RECEIVED` line because i2pd's `ClientDestination::HandleDataMessage` parses an I2CP-style Data header + gzip-wrapped datagram payload but i2pr emitted a raw 16-byte-standard I2NP Data body whose first four bytes were misread as the length field and overflow the available buffer. The test driver no longer panics; `read_line`/`wait_for_datagram` return `Option<...>` and record distinct evidence keys (`reference-received-timeout`, `destination-inbound-send-failed`, `inbound-delivery-boundary-E-stop`). The 2 ordering rows `external-direct-rejected` / `external-liveness-first-test` flip to `passed`. The 2 inbound-delivery rows `external-reference-received` / `external-destination-inbound` stay `blocked` with stop provenance in Plan 191 and flip to `passed` in Plan 192.
- Plan 192 is the M6 i2pd-compatible I2CP-style Data body wire-format corrective (see `plans/192-m6-i2cp-wire-format-corrective.md` and `plans/192-status.md`): passed. The fix is narrow: switch the inner I2NP envelope inside the ECIES-X25519 Garlic clove from the 16-byte standard form to the 9-byte short-transport form i2pd parses (`Garlic.cpp:1023-1028`); wrap the application payload in the I2CP-style Data body i2pd expects (`length[4 BE] + reserved[4] + fromPort[2 BE] + toPort[2 BE] + padding[1] + protocol[1] + gzip-no-compression-wrapped payload`; `Destination.cpp:1192-1236`); and switch the test SAM session from `STYLE=DATAGRAM` (which needs a 384-byte ElGamal/DSA `from` Identity our ECIES-only i2pr does not have) to `STYLE=RAW` (which uses the inbound destination hash instead of an ElGamal/DSA identity). No M6 wire change beyond the destination message-plane seam; no `LocalZeroHop` substitution; no authentication weakening; no fake LeaseSet. Inbound-delivery layer closed for exact-pinned i2pd 2.61.0; Java second family still pending Plan 194; Plan 193 owns the first-family Streaming qualification.
- Plan 189 is registered as the M6 Java I2P second-family qualification and mixed-router closure plan (see `plans/189-m6-java-i2p-second-family-qualification-and-closure.md` and `plans/189-status.md`): now executable after Plan 193 closed the first-family gate. Plan 189 §8 lands the fail-closed M6 mixed-router cross-family ledger/checker/workflow scaffold: `scripts/check-m6-mixed-router-acceptance-evidence.sh` (structural checker that verifies both pins are referenced by every per-layer static checker), `tests/integration/m6-interop/run-m6-mixed-router.sh` (cross-family aggregator that reuses the four per-layer harnesses), and `.github/workflows/m6-mixed-router-external.yml` (manual `workflow_dispatch` lane that fetches i2pd + Java deps and runs the structural checker + per-layer checkers + cross-family aggregator). The cross-family aggregator binds each guarded row to a family (`-i2pd` / `-java`) plus an executed per-layer command exit code; the Java rows are recorded `failed` with stop provenance until a follow-up plan lands the second-family Java qualification harness under `tests/integration/m6-interop/run-java.sh`. No M6 wire change; no claim that the i2pd first family has passed (the Plan 188 lookup gap closed via Plan 190; the inbound-delivery gap closed via Plan 192).
- SAM stays experimental, loopback-only, disabled by default, and non-advertised.
- SSU2 public advertisement/public-network participation is not claimed.
- No Plan 161 direction-A evidence implies Milestone 6 destination/Streaming/tunnel interoperability or broad router interoperability.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

## OpenCode skills

Use `i2pr-local-dev` for current local product/SSU2 execution guidance and
`i2pr-architecture` for architecture/ADR/plan navigation. Historical NTCP2,
rootless, and Multipass skills remain separate lanes.

## Commits and handoff

Use focused commits. Do not change git config, skip hooks, force-push, or amend
someone else's commit. Closure records must include exact commands/results and
current-head workflow evidence.

Current handoff: **Plans 200/202/203 are closed;
Plan 201 remains the active blocker; Plan 204 landed the
docs/CI/evidence-authority normalization pass on top of those
three passed plans and stays blocked on Plan 201's exact-head
external run.** Plan 193 is closed
(`passed-m6-i2pd-mixed-router-streaming`; see `plans/193-status.md`
and `plans/193-streaming-status.md`): full Direction A +
Direction B external matrix passed twice on exact head `3687189`
(33/33 rows, evidence `passed-via-i2pd-2.61.0`) — Direction A
SYN/Established + 25 B + 8192 B digests + reverse 23 B + 4096 B
digests (with live NACK/retransmit loss-recovery) + sibling +
close/EOF + isolation; Direction B CONNECT/Established + 17 B +
2048 B digests + close/EOF; manager cleanup + SSU2 baselines zero.
Narrow correctives landed inside the plan: per-turn
`poll_acks`/`poll_retransmits` pump drain, fresh SAM sockets for
ACCEPT/CONNECT, `StreamingReceiveLimit::destination_path()`
receive bound (reference emits 1812 B payloads; send path
unchanged), per-delivery RNG, 4 KiB SAM reads. Static checker
`scripts/check-streaming-tunnel-evidence.sh` guards 33 labels and
is wired into the floor. Plan 194 (Java I2P second-family
qualification) landed the Plan 194 §3 fetch script +
second-family harness + external driver + cross-family aggregator
wiring + static checker + hosted workflow wiring and stops
fail-closed at the §3 controlled-topology boundary (stock Java
I2P 2.13.0 overwrites its own router.config on first start,
binds a random UDP port, and runs reseed against the public I2P
network). Plan 196 closed the corrective: out-of-tree
`tests/integration/m6-interop/java/ControlledRouter.java`
test-only launcher + rewritten `tests/integration/m6-interop/run-java.sh`
+ extended `scripts/check-m6-mixed-router-acceptance-evidence.sh`
static checker. Plan 197 closed the narrow PQ SSU2 option
support corrective: parser-only tolerance of the `pq` KEM-scheme
option Java I2P 2.13.0 unconditionally publishes, typed
`Ssu2PqKem`/`PqCapabilities` surface, bounded
`MAX_SSU2_PQ_SCHEMES = 8`, i2pr session layer stays classical
X25519 only, i2pr publication path stays pq-free, ML-KEM not
implemented. Plan 198 is `superseded-execution-decomposed-and-
closed-via-plans200-204`; Plan 199 is `superseded-execution-
decomposed-and-closed-via-plans200-204`. Plan 200 closed the M6
Java public-client publication observability lane (helper READY
decoupled from `leaseset=published`, bounded `REPORT_STATUS`,
post-bootstrap RouterInfo DatabaseLookup proofs in both directions,
sanitized Java LS2 lifecycle keys, one terminal `P200-{A..H}`
classification per run). Plan 201 landed the Branch G
`store-acked-remote-lookup-fails` corrective framework
(`DestinationTunnelCounters` gains eleven sanitized observation
counters; `note_lookup_boundary(label, value)` typed observation
surface; eight new `plan201_g_*` unit rows; six new `blocked_row`
Plan 201 §G entries in `run-java.sh`; static checker extended with
§11 + §12 invariants). Plan 202 closed the M10 production remote
Destination/Streaming composition: the `ServiceTunnelManager` now
owns one shared `ServiceDestinationDelivery` capability; the typed
`RoutingDecision::LocalCoOwned` / `RemoteRouter` / `RemoteUnresolved`
enum drives the resolve path; twelve new bounded
`RemoteDeliveryCounters` observations cover the counted path;
the `m10_remote_destination_streaming_composition_through_manager`
Direction A external driver exercises the manager-level
`install_router_delivery_handle` / `routing_decision_for` seams
against the exact-pinned i2pd 2.61.0 cache through the dedicated
M6 interop lane. Plan 203 was retained as the partial positive
M10 remote HTTP + IRC application evidence scaffold (its
`record_remote_application_observation` label-injection pattern
is no longer accepted as a counted proof by the Plan 207 lane).
Plan 207 closed the genuine M10 remote HTTP + IRC application
interop: the `m10_genuine_remote_http_and_irc_application_interop`
external driver replaces the synthetic Plan 203 pattern with
command-derived evidence from unmodified application clients. The
HTTP row is bound to the real system `curl` binary spawned as a
subprocess against the i2pr HTTP client listener; the IRC row
is bound to the unmodified exact-pinned jaraco/irc public API
(`irc.client`) spawned as a subprocess against the i2pr IRC
client listener. The driver writes the documented Plan 207 §9
subfact rows plus `plan206-backend-counters` to
`${EVIDENCE_DIR}/plan207-driver/driver-evidence.tsv`; the
aggregate pass rows derive purely from the command-derived
subfact rows; the runner provisions i2pd with one HTTP server
tunnel + one IRC server tunnel pointing at the harness-owned
loopback fixtures; the static checker rejects literal
`record "... passed"` lines and requires the positive rows to
flow through `record_guarded` + the documented Plan 207 §9
subfact rows. The two `remote-independent-*` rows flip from
`blocked` to `passed` once the dedicated M6 interop lane
provisions the SSU2 endpoint + bind tuple and the driver emits
every documented Plan 207 §9 subfact row in the same evidence
directory/run id; the full M10 lane stays fail-closed without
it. Plan 195 is reactivated to
`evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization`
because Plan 207 supplies the positive application evidence Plan
195 was originally registered to provide. Plan 204 landed the
docs/CI/evidence-authority normalization pass on top of the four
passed plans and stays blocked on Plan 201. The M6/M10 closed
authority transitions in §7/§12 of
`plans/204-m10-final-closure-evidence-authority-and-documentation-normalization.md`
stay deferred until Plan 201 records the terminal
`P200-{A..H}` classification and lands its narrow corrective. The
historical `plans/188-m6-mixed-router-streaming-with-i2pd.md`
Streaming file remains historical context only. M10 final
acceptance stays open (Plan 201 is the only remaining blocker on
the M10 closure transition).**
