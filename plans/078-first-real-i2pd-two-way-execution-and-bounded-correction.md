# Plan 078: first real i2pd two-way execution and bounded correction

## Status and dependencies

- Status: blocked-preflight (2026-07-31 host result recorded in `plans/078-status.md`).
- Parent roadmap: Plan 074.
- Requires Plans 075, 076, and 077 closed with a qualified full-runtime lane.
- Must close before Plan 079.
- Plan type: first genuine mixed-router execution, failure localization, and bounded corrective implementation.

## Objective

Obtain the first truthful authenticated NTCP2 data-phase result between i2pr and pinned i2pd in both directions:

```text
i2pr -> i2pd
i2pd -> i2pr
```

This plan prioritizes real execution and earliest-stage failure evidence. It must not expand schemas, orchestration frameworks, or isolation infrastructure unless a concrete observed failure proves the current surface insufficient.

## Preconditions

Before any protocol run, verify:

- corrected runner invokes one i2pr and one real reference process;
- real i2pd driver contains linked pinned i2pd symbols;
- instrumented and control manifests validate;
- selected execution lane qualification is current;
- exact binaries inside lane match expected digests;
- no public interface or route exists during execution;
- fresh state and unique ports/message ID are available;
- no synthetic provenance or automatic milestone promotion remains.

If any precondition fails, stop at preflight. Do not report a protocol failure.

## Required positive proof per direction

A direction passes only when all are observed and correlated:

```text
source_lock_verified = true
real_reference_process_executed = true
fresh_state = true
router_info_signature_and_identity_continuity = true
tcp_connected = true
sender_ntcp2_authenticated = true
receiver_ntcp2_authenticated = true
sender_frame_write_completed = true
receiver_frame_authenticated_and_decrypted = true
receiver_i2np_delivery_status_decoded = true
delivery_status_message_id_matches = true
peer_router_hash_matches = true
cleanup_clean = true
lane_no_public_network = true
```

A listening socket, process survival, sender queue acceptance, handshake-only result, or generic log line is not a pass.

## Execution order

### Phase 1. Instrumented inspect/control

Inside the selected lane:

1. run instrumented i2pd inspect mode;
2. run control i2pd inspect mode;
3. verify equivalent RouterInfo/network/address results;
4. verify no observer-dependent initialization behavior;
5. verify clean shutdown.

### Phase 2. `i2pr-to-i2pd-ipv4`

Roles:

```text
i2pd = listener/responder
i2pr = dialer/initiator
```

Sequence:

1. create fresh run root and identities;
2. start real i2pd listener;
3. wait for real structured listener-ready and RouterInfo export;
4. validate exact signed RouterInfo and selected NTCP2 address;
5. render i2pr dialer scenario with exact RouterInfo bytes and correlation fields;
6. start i2pr dialer;
7. consume both structured event streams;
8. require authentication on both sides;
9. require i2pr successful encrypted frame write for one DeliveryStatus;
10. require i2pd post-AEAD/post-I2NP exact decode;
11. verify ID and Router Hash continuity;
12. terminate and verify no residual process/socket/state;
13. write a Level 1 result record.

### Phase 3. `i2pd-to-i2pr-ipv4`

Roles:

```text
i2pr = listener/responder
i2pd = dialer/initiator
```

Use a completely fresh run root, identities, ports, and message ID.

Require i2pd to construct and send one real DeliveryStatus through the real transport path. Immediate send/queue return is insufficient; require later frame-write completion and i2pr exact decode.

### Phase 4. Control build comparison

After an instrumented direction passes, repeat a bounded control run using the uninstrumented i2pd binary.

The control run may rely on i2pr-side exact receive/send evidence plus process/transport outcome for observer neutrality, but it may not substitute for the instrumented receiver-side event record.

Block closure when the instrumented build succeeds and control build fails at protocol level.

## Failure staging

Preserve the earliest stage:

```text
preflight
reference-initialization
router-info
listener-bind
connect
session-request
session-created
session-confirmed
peer-identity
frame-write
frame-authentication
i2np-block-decode
delivery-status-correlation
network-boundary
cleanup
timeout
```

Each failure record must include:

- direction;
- run ID;
- source/binary digests;
- local and peer Router Hash digests;
- expected message ID;
- earliest typed reason from each process where available;
- sanitized counters and event sequence positions;
- no raw keys, Noise state, payloads, or arbitrary external logs.

## Bounded correction policy

When a real failure occurs:

1. reproduce it once from fresh state;
2. identify the first divergent protocol stage;
3. compare against the official NTCP2 specification and pinned reference source;
4. add one focused local regression reproducing the defect;
5. change only the owning i2pr or test-driver surface;
6. rerun relevant local tests;
7. rerun only the failed direction;
8. preserve failed receipts as history;
9. stop if the required change expands into NetDB, tunnels, SAM/I2CP, SSU2, or public-network behavior.

Ownership guide:

| Failure | Likely owner |
| --- | --- |
| RouterInfo signature/address/static-key mismatch | RouterInfo codec/config or driver |
| TCP bind/connect | execution lane or runtime adapter |
| SessionRequest/Created/Confirmed | NTCP2 handshake state/codec |
| peer identity mismatch | RouterInfo/SessionConfirmed validation |
| frame length/AEAD | NTCP2 data frame state |
| block conversion | NTCP2 block/I2NP codec |
| wrong/duplicate message ID | launcher/runner correlation |
| external destination | lane/configuration defect |
| residual process/socket | runner/runtime ownership |

Do not add evidence fields to hide or normalize a protocol failure.

## Required records

Create one immutable Level 1 record per attempted direction. Passing records must satisfy the corrected smoke schema. Failed records must remain valid typed records with exact failure stage.

Create `plans/078-status.md` with:

- selected lane and qualification digest;
- exact commands;
- exact source/binary digests;
- one result per direction;
- corrections made from observed failures;
- focused regression commands/results;
- control comparison;
- explicit statement that Level 2 and Level 3 remain open.

## Acceptance criteria

Plan 078 closes only when:

- one real instrumented pass exists for `i2pr-to-i2pd-ipv4`;
- one real instrumented pass exists for `i2pd-to-i2pr-ipv4`;
- both use fresh state and exact message/identity correlation;
- structured events prove authentication, frame write, decryption, and I2NP decode;
- the selected lane proves no public network interface/route;
- cleanup is clean;
- control build shows no observer-dependent protocol success;
- every correction is backed by a focused regression;
- no Java, Emissary, release certificate, or support advertisement claim is made.

If a genuine protocol incompatibility remains after bounded correction, Plan 078 may close only as `blocked-protocol-defect` with exact stage and reproduction. Plan 079 must not start.

## Validation commands

Use exact Plan 077 lane entry commands. Also run focused relevant checks:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-vectors.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

Run only relevant fuzz targets when the observed defect involves parser/frame/handshake input handling.

## Non-goals

Plan 078 does not:

- require three repetitions;
- run the full negative-control matrix;
- run Java or Emissary;
- add broad CI;
- advertise or normally enable NTCP2;
- produce release-grade evidence.

## Stop rules

Stop and record a blocker when:

- the lane loses no-public-network guarantees;
- the tested binary differs from its verified digest;
- a real reference event cannot be distinguished from a synthetic/test fixture;
- the observer changes protocol behavior;
- required correction exceeds the bounded NTCP2/driver/runner scope;
- cleanup cannot own all spawned state.

## Small-model execution guidance

- Run one direction at a time.
- Preserve the first failing stage before editing.
- Correct one defect per commit.
- Do not refactor the runner while debugging protocol state unless the failure proves runner ownership.
- Do not proceed to reverse direction until the first direction passes or is precisely blocked.
- Do not mark a handshake-only run as partial success sufficient for closure.
