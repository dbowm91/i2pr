# Plan 227 registration follow-up

Plan 226 remains closed at
`P226-BASELINE-B-ZERO-HOP-UNKNOWN`. Exact-pinned source review confirms the
active helper is deliberately zero-hop while Java client NetDBs intentionally
do not store RouterInfos. The zero-hop guard is therefore structural for the
current raw-helper profile.

Plan 227 is registered as the narrow successor. It uses Java I2P's own public
I2CP `explicitPeers` debug/testing option to request a genuine one-hop
raw-helper client tunnel through Router C, requires read-only proof that both
inbound and outbound tunnels are actually installed through C, and then reruns
the same target lookup.

```text
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
plan_227 = registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective
next_executable_plan = 227-m6-java-explicit-one-hop-client-tunnel-corrective
```

Plan 226's distinct-loopback topology remains unadmitted. Plan 227 does not
authorize profile mutation, client-NetDB RouterInfo injection, direct tunnel
installation, VMComm, or `netDb.alwaysQuery`.

# Plan 226 status — M6 Java loopback peer-diversity corrective

Status: **`passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary`**.

Plan of record:
[`226-m6-java-loopback-peer-diversity-corrective.md`](../../implementation/mixed-router-interop/226-m6-java-loopback-peer-diversity-corrective.md).

## Closure result

Plan 226 is closed as a completed, fail-closed diagnostic corrective. The
exact Plan-225 target `IterativeSearchJob` was observable, but the baseline
run did not prove the hypothesized Router-B IP-close skip. It instead proved
the earlier Java zero-hop rejection:

```text
P226-BASELINE-B-ZERO-HOP-UNKNOWN
```

Authoritative baseline trace from the final allowed attempt:

```text
target_job_count=1
b_selected_preflight=true
b_ip_close_skipped=false
b_old_router_rejected=false
b_zero_hop_unknown_rejected=true
b_encrypted_lookup_unsupported=false
no_ib_client_tunnel=false
no_reply_crypto=false
peer_try_count=0
query_to_b=false
search_failed=true
```

Because the exact target job showed a different pre-dispatch rejection, the
plan's topology gate correctly prevented any A/B/C correction. No distinct
topology run was admitted, and no M6 Java interoperability claim follows.
This is the prescribed §5/§6 stop behavior, not an authorization to bypass
Java peer selection with `netDb.alwaysQuery`.

## Implementation commits and pinned inputs

Implementation was committed before the counted external attempts:

```text
60839ec1dee1fec3191b4fd27c06db3289eb003d  interop: enforce Java loopback peer diversity corrective
933ac75330ec9cad72a4a97c6a2d2cba9006139  interop: accept Java hash log renderings
```

The second commit corrected the sanitizer to accept the two exact pinned-Java
hash renderings (`<base64>` and `[Hash: <base64>]`) without broadening
correlation. It added regression coverage for the bracketed rendering.

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin remained
`2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No dependency,
fixture, production protocol, or Java reference source changed.

## Requirement-to-evidence matrix

| Plan-226 requirement | Evidence / result |
|---|---|
| Exact target-job correlation | Final baseline `p226-target-job-trace`: `observable=true`, `target_job_count=1`, `target_job_id_overflow=false`; correlation is bounded by the numeric Java job ID and exact target/B hash rendering. |
| Baseline selector fact retained | `b_selected_preflight=true`, inherited from the exact Plan-222 production-equivalent selector preflight. |
| Baseline IP-close hypothesis tested, not inferred | `b_ip_close_skipped=false`; the exact Router-B skip line was not observed for the target job. Shared addresses therefore did not authorize correction. |
| Earlier rejection preserved | `b_zero_hop_unknown_rejected=true`, with `b_old_router_rejected=false`, `b_encrypted_lookup_unsupported=false`, `no_ib_client_tunnel=false`, and `no_reply_crypto=false`. |
| Search outcome retained | `query_to_b=false`, `peer_try_count=0`, and `search_failed=true`; the result is not mislabeled as an IP-diversity attribution. |
| Router-B answerability retained | Final attempt's P224 snapshot recorded `validated_present=true`, `received_as_published=true`, `entry_type=3`, `key_types=4`, and `current=true` for Router B's target LS2 before send. |
| Topology gate | Baseline preflight recorded `all_loopback=true`, `pairwise_mask3_distinct=false`, with all three baseline SSU2 hosts on `127.0.0.1`. The distinct preflight and RouterInfo checks are implemented but were not admitted because the exact IP-close gate was false. |
| Control-plane safety | SAM, I2CP, raw/streaming control, and diagnostic endpoints remain on `127.0.0.1`; only conditional Java SSU2 host arguments are topology-selectable. |
| Forbidden bypasses | Static acceptance guard rejects `netDb.alwaysQuery`, Java patch/reflection/state mutation, same-/24 purported corrections, raw-log promotion, and unconditional P226 terminals. |
| Exactly one terminal | The final destination evidence contains one `p226-classification` row: `P226-BASELINE-B-ZERO-HOP-UNKNOWN`. Early-stop paths emit `P226-OBSERVABILITY-GAP` exactly once. |
| Frozen reverse result | Final attempt retained ordered tracked-send status `[21]` and `frozen_payload_45s=false`; Plan 226 did not alter the 45-second window. |

## External attempt history

The exact destination lane was run only in baseline mode. Each attempt used a
fresh disposable Java RouterContext and the same bounded configuration.

| Attempt | Implementation SHA | Evidence directory | P226 result |
|---|---|---|---|
| 1 | `60839ec` | `target/interop/m6-java-baseline-evidence` | `P226-OBSERVABILITY-GAP`, because the first parser required bare Base64 and found no exact target job; this exposed the pinned Java `[Hash: …]` rendering and was corrected before the next SHA. |
| 2 | `933ac75` | `target/interop/m6-java-baseline-evidence-v2` | `P226-OBSERVABILITY-GAP`, pre-epoch lease-store stop; no topology correction was attempted. |
| 3 | `933ac75` | `target/interop/m6-java-baseline-evidence-v3` | `P226-BASELINE-B-ZERO-HOP-UNKNOWN`, exact target observed; IP-close gate false. |

No corrected-topology attempt was run: the required retained token
`P226-BASELINE-IP-DIVERSITY-CONFIRMED` was never produced, so the harness
would reject a `distinct` invocation by construction.

The enclosing legacy Plan-199 Java wrapper exited nonzero on all attempts
because its pre-existing destination/streaming acceptance rows remain
unqualified. The Plan-226 diagnostic rows and workspace-gate slice were still
emitted; raw Java logs remained scratch-only and were not promoted to evidence.

## Verification

Successful verification after implementation/final parser correction:

```text
cargo fmt --all --check                                      PASS
cargo check --locked --workspace --all-targets                PASS
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                               PASS (2530 passed, 16 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                               PASS
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                               PASS
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
                                                               PASS (7 passed)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh         PASS
bash -n tests/integration/m6-interop/run-java.sh                 PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh      PASS
```

The final external workspace-gate slice also passed formatting, workspace
check, dependency/runtime boundaries, fixture/vector checks, and the M6
acceptance checker. The full all-target test floor was run before the small
final hash-rendering parser correction; the final correction was compiled and
covered by the focused Plan-226 suite above.

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all new behavior is in
  the external Java driver, bounded sanitizer, launcher harness, and static
  evidence checker.
- No Java source patching, reflection, private-state mutation, NetDB/key/tunnel
  injection, publication retry, search-limit change, tunnel-profile change,
  timeout change, or `netDb.alwaysQuery` override was used.
- All configured SSU2 hosts are required to be IPv4 loopback and UDP-bindable;
  the rejected baseline deliberately remains `127.0.0.1` for all peers.
- Durable evidence contains only bounded booleans, counts, hashes, and status
  facts. Raw Java logs and possible reply-key/tag material remain disposable.
- The Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and frozen
  45-second reverse-payload authority remain unchanged.

## Findings and limitations

No security finding was introduced. The plan disproved its narrow IP-close
hypothesis for the exact target job in the controlled baseline and localized
the next observed boundary to Java's zero-hop lookup-to-unknown rejection.
That is diagnostic evidence only; it is not a product fix and does not prove
Java second-family destination or streaming interoperability.

## Roadmap and unblock audit

Plan 226 is formally closed as the bounded baseline corrective. The unblock
audit found no future plan eligible to resume:

```text
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan226-baseline-zero-hop-boundary
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226-baseline-zero-hop-boundary
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
active_plan = none
next_executable_plan = none
```

Plan 201 remains blocked because Plan 226 did not produce the conditional
distinct-topology qualification or Java second-family closure. Plan 204 is a
convergence plan and remains blocked on that same independent M6 closure; its
M10 authority is unchanged. Plan 205 remains retained/deferred because the
observed boundary is below the SAM bridge. No successor plan was registered:
the final terminal is a baseline stop, not a `NEXT-BOUNDARY` terminal requiring
a new corrective plan.
