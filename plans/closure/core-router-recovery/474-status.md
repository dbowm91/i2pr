# Plan 474 status — blocked before SAM owner-summary requalification

**Status token:** `blocked-stock-sam-attempt-stops-at-inbound-build-before-owner-gate`  
**Disposition:** source change and deterministic owner-generation checks retained; full SAM owner lifecycle and STREAM capability remain blocked.  
**Implementation commit:** `d1d9ff4` (`fix(plan474): publish SAM inbound owners atomically`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Publish bridge material and inbound owner bindings as one manager generation | `install_service_router_material_with_inbound_owners` validates destination, nonzero unique receive IDs, bridge IDs, and owner conflicts while serializing owner publication with summary snapshots | Implemented |
| Reject partial or mismatched owner generations | `plan474_mismatched_owner_generation_leaves_no_partial_bridge_or_owner` | Passed |
| Verify a complete summary after valid publication | `plan474_router_material_and_inbound_owner_publish_as_one_generation` checks receive count, owner count, completeness, and lookup | Passed |
| Establish the historical 0/2 cause and run a post-fix stock SAM exchange | `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Blocked: the run terminated at `inbound-build`, before material installation or the owner-summary predicate |
| Prove genuine nonzero-hop leases and byte-exact STREAM in both directions | No successful external SAM product receipt | Not established |

The atomic publication change closes the source-visible partial-generation window. The historical discrepancy's concrete triggering interleaving was not reproduced, so this is a source-boundary correction, not proof that it caused the earlier 0/2 observation. The pinned rerun failed earlier than the owner gate (one attempt observed; selected, admitted, and emitted counts were each one; no authenticated reply match or installation). That result cannot validate or falsify the owner-summary fix.

## Verification

| Command | Outcome |
| --- | --- |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon plan474_router_material_and_inbound_owner_publish_as_one_generation -- --test-threads=1` | Passed (1 test) |
| `cargo test --locked -p i2pr-daemon plan474_mismatched_owner_generation_leaves_no_partial_bridge_or_owner -- --test-threads=1` | Passed (1 test) |
| `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed (4 tests) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed (15 tests) |
| `python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed (6 tests) |
| `python3 tests/integration/core-router-recovery/test_plan471_stock_receipt.py` | Passed (3 tests) |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `bash scripts/check-sam-acceptance-evidence.sh` | Passed (28 command-derived rows) |
| `bash tests/integration/m6-interop/run-preflight.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 184 lane |
| `bash tests/integration/m6-interop/run-tunnels.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 185 lane |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Failed at `inbound-build`; no owner summary was reached |

The supplied AGENTS.md routine floor also passed: format/check/build, 4,853 workspace tests (39 ignored), Clippy, rustdoc/doc-tests, repository boundary/evidence scripts, planning suite (51 tests), and `cargo deny check advisories bans sources`.

## Invariant, compatibility, and security review

No SAM wire behavior, tunnel construction, LS2 encoding, or support claim changed. Summary completeness remains fail-closed; a bridge receive ID without a current matching owner still rejects. Owner registration is checked against the installed bridge IDs and requested destination. No identity, receive ID, RouterInfo, payload, or raw stock log is written into the closure evidence.

## Limitations and next executable step

The Plan 474 acceptance gate remains open. The next attempt must first establish the earlier inbound-build reply/commit transition with attempt-scoped evidence, then re-run the exact pinned normal-process SAM exchange after the previously installed-generation regression. It must reach a stable N/N owner summary and real bidirectional STREAM before this plan can pass. No successor plan is registered: the active plan's required source and product evidence remain unresolved.

**Unresolved findings:** high — normal-process SAM inbound build and bidirectional application path remain unqualified; medium — historical 0/2 owner discrepancy lacks a reproduced causal interleaving.

**Roadmap disposition:** blocked; no dependent capability is unblocked.
