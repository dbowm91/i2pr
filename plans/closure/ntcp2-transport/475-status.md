# Plan 475 closure — Java two-role product qualification

**Status token:** `closed-java-two-role-authenticated-product-and-reply`  
**Disposition:** closed for this plan's controlled normal-Java product qualification. Both fresh NTCP2 initiating roles, authenticated I2NP product effects, and correlated replies passed on the exact pinned Java 2.13.0 lane. This does not change the historical Plan 470 closure or authorize general/public NTCP2 support.  
**Implementation commit:** recorded by the commit that contains this closure.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Freeze exact Java source, binary, listener, and selector preconditions | Exact `2.13.0@9134f808337b401e8e53c73734c81fab04280c9d`; signed candidate loaded/admitted/selected; current NTCP2 bid and normal Store policy available; loopback listener readiness read without a TCP probe | Passed |
| Stock Java initiated fresh send | The stock `FloodfillStoreJob` selected Java's real NetDB-owned RouterInfo, submitted the reply-requested Store through normal `OutNetMessage`, and its success callback completed; Java opened the outbound connection and the daemon accepted the authenticated RouterInfo Store and submitted the owed DeliveryStatus | Passed |
| Independent i2pr initiated fresh session and product request | A second daemon process generation used the persisted identity, opened a fresh outbound link, submitted a direct LeaseSet lookup, and Java's stock floodfill lookup handler recorded it handled with no drop | Passed |
| Correlated authentic reply | Java's `DatabaseSearchReply` matched the submitted lookup key and authenticated peer; daemon emitted `controlled NTCP2 initiator DatabaseLookup response correlated` | Passed |
| Attribute the earlier SessionCreated EOF | The old readiness TCP probe sent an empty connection. Pinned Java `EventPumper` treats inbound EOF-before-data as a source-IP block; removing the probe and polling the stock NTCP listener state eliminated blocklist events. Both ordered fresh-role phases then completed | Source cause identified and corrected in test fixture |
| Negative and lifecycle controls / routine floor | NTCP2 transport handshakes cover wrong static-key binding, malformed/duplicate blocks and cancellation/deadline/disconnect terminals; runtime and daemon suites exercise link ownership and teardown. The complete AGENTS.md floor passed for this iteration | Passed |

## Source-derived timing correction

Pinned Java 2.13.0 `FloodfillNetworkDatabaseFacade.startup()` constructs its
`LookupThrottler`, but `LookupThrottler._max` is initialized by the first
120-second `Cleaner` callback. Until then, `shouldThrottleLookup()` increments
the peer counter and rejects the first request because the limit is zero. The
lane now observes the private `_max` read-only through its out-of-tree
diagnostic and waits for admission before phase 2. It never calls
`shouldThrottleLookup()` from diagnostic code and does not patch stock Java.

The request uses the normal LeaseSet lookup class. Java's source-defined direct
lookup anti-floodfill drops apply to `ANY` and `EXPL`, which this product
correlation probe does not require. The lookup is encoded and identity-bound by
the ordinary i2pr product builder.

## Verification completed this iteration

| Command | Outcome |
| --- | --- |
| `python3 tests/integration/ntcp2/plan470_java_ntcp2.py` | All emitted stages passed on the exact pinned Java 2.13.0 profile, including both fresh initiating roles and correlated product replies |
| `cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed (35 tests) |
| `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed (135 tests, 1 ignored) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `cargo fmt --all --check` / `cargo check --locked --workspace --all-targets` | Passed |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed (4,854 tests, 39 ignored across 187 suites) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps`; `cargo test --locked --workspace --doc` | Passed |
| Repository boundary, evidence, planning, vector, workflow, and `cargo deny check advisories bans sources` floor | Passed |
| `bash scripts/check-ntcp2-vectors.sh` | Passed |
| `bash scripts/check-ntcp2-interoperability.sh` | Passed (historical harness static check) |
| `python3 -m py_compile tests/integration/ntcp2/plan470_java_ntcp2.py` | Passed |

The exact Java lane's sanitized stage receipt included
`java_store_job_succeeded=1`, `daemon_inbound_product_effect=1`,
`daemon_probe_correlated=1`, `daemon_lookup_submitted=1`,
`daemon_lookup_correlated=1`, and `java_lookup_admission_ready=1`. No raw router
log, peer identity, endpoint, or key was retained in output.

## Invariants and remaining closure work

No NTCP2 wire behavior, authentication rule, RouterInfo acceptance policy,
capability advertisement, or reference source/jar changed. The controlled
profile's outbound scheduler now waits for explicit qualification requests, so
the stock Java StoreJob phase cannot collide with the independent i2pr
initiator phase. The Java listener readiness observer sends no probe bytes.

The final close audit is complete. The controlled qualification profile remains
opt-in; no support claims were added to `specs/support.toml`. Plan 470's older
blocked closure remains immutable and Plan 444's separate Java floodfill
selection/store qualification remains open.
