# Plan 320 status — exact Proposal 170 canonical wire reconciliation

Status: **`passed-prop170-canonical-wire-contract-reconciliation`**.

Plan of record: [`plans/implementation/i2pcontrol-proposal-170/320-canonical-wire-contract-reconciliation.md`](../../implementation/i2pcontrol-proposal-170/320-canonical-wire-contract-reconciliation.md).

Implementation commits: Plan 320 contract inventory/parser/output commits on the current branch, followed by this closure record.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Freeze the 43 exact Proposal RouterInfo additions and separate the updated base API fields | `PROPOSAL_ROUTER_INFO_FIELDS`, `BASE_ROUTER_INFO_FIELDS`, presence-based selection, cardinality/type tests in `crates/i2pr-i2pcontrol/src/proposal_wire.rs` and contract tests | PASS |
| Use exactly the Proposal SetConfig fields and preserve internal address-book owner keys behind translation | `PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS`; canonical-to-internal mapping in daemon request decoding; unsupported `should_publish`, `etags`, `last_modified` report Plan 321 ownership | PASS |
| Freeze TunnelManager spelling, type/range/enum/object shape, alias groups, and top-level form | `PROPOSAL_TUNNEL_MANAGER_FIELDS`, typed validator, exact integer ranges and EncryptLeaseSet values; duplicate, case, scalar, list, range and alias tests | PASS |
| Emit canonical RouterInfo, AddressBook, TunnelManager, and ClientServicesInfo wire envelopes | Proposal RouterInfo selectors emit exact names; entry mutations use the documented top-level success/message envelope; TunnelManager returns status/info/results and never returns the old normalized result; canonical raw lifecycle test exercises all seven actions; client-service contracts retain the six exact selectors | PASS |
| Cover each published Proposal request/response example and pinned implementation overlaps | [`crates/i2pr-daemon/tests/fixtures/i2pcontrol-proposal-170-examples.json`](../../../crates/i2pr-daemon/tests/fixtures/i2pcontrol-proposal-170-examples.json), verified by `pinned_proposal_request_response_examples_are_frozen`; Java PR 6 and Emissary source pins and shared/differing action/type observations are recorded in the same fixture | PASS |
| Keep secrets out of TunnelManager rawConfig and responses | Secret classification filters rawConfig; raw wire regression checks absence of ProxyPassword and any normalized private fields | PASS |
| Keep old normalized request/result behavior out of the canonical endpoint | Parser accepts only exact Proposal field names; test-only legacy view is isolated inside the black-box lifecycle test helper | PASS |

## Verification

All commands were run during the implementation sequence. The serial workspace
run completed before the final bounded `.b32.i2p` derivation and an added
rawConfig secret regression; those final edits were checked by daemon-wide
`cargo check`/Clippy and the focused TunnelManager, differential, and secret
redaction tests below.

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS — 3,679 passed, 35 ignored, 132 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | PASS |
| `cargo test --locked --workspace --doc` | PASS |
| `cargo test --locked -p i2pr-i2pcontrol --all-targets` | PASS — 23 passed |
| `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1` | PASS — 13 passed on the final response mapper |
| `cargo test --locked -p i2pr-daemon --test i2pcontrol_differential -- --test-threads=1` | PASS — 1 passed, 1 ignored |
| `cargo test --locked -p i2pr-daemon --lib proposal_tunnel_raw_config_omits_secret_values` | PASS — 1 passed |
| `cargo test --locked -p i2pr-daemon --lib plan294_addressbook_method_drives_the_canonical_owner -- --test-threads=1` | PASS — 1 passed |
| `cargo test --locked -p i2pr-addressbook --all-targets` | PASS — 32 passed |
| Repository boundary/vector/evidence scripts from AGENTS.md | PASS; M12 self-test produced its expected rejected-forgiveness diagnostic and passed its integrity check |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS |
| `git diff --check` | PASS |

## Security and limitations

- Wire aliases do not broaden the canonical request namespace. Valid but not yet owned Proposal TunnelManager/AddressBook fields fail explicitly with their downstream owner markers.
- The rawConfig adapter omits secret-classified values and never returns normalized internal response keys.
- This closure establishes `canonical-wire` only. It does not claim that all RouterInfo sources, AddressBook fetch/publication, deep tunnel options, crypto capabilities, or independent-router behavior are operational; those remain in Plans 321–328.
- No new dependencies or router-runtime behavior were introduced by the wire translation.

## Roadmap disposition and unblock audit

Plan 320 is closed as **`passed-prop170-canonical-wire-contract-reconciliation`**. A fresh dependency audit confirms Plans 321, 323, and 325 have all hard prerequisites satisfied and are ready. Plan 322 remains blocked on 321; Plan 324 and 327 remain blocked on 323; Plan 326 remains blocked on 323, 324, and 325; Plan 328 remains blocked on 322, 326, and 327. The next active continuation is Plan 321. Historical Plans 286–297 and the M12/mainline readiness state are unchanged.
