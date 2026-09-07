# Plan 167 — Milestone 9 I2CP loopback server runtime

Status: **blocked until Plan 166 passes**.

## 1. Goal

Compose the runtime-neutral I2CP protocol/session state and client-owned destination capability into a real supervised TCP server owned by `i2pr-daemon`.

This pass proves real localhost protocol/session/LeaseSet2 lifecycle. Application message transport remains Plan 168.

Expected closure:

```text
plan_167 = passed-m9-i2cp-loopback-server-runtime
next_executable_plan = 168
```

## 2. Configuration surface

Add a strict daemon config section equivalent to:

```toml
[i2cp]
enabled = false
listen = "127.0.0.1:7654"
```

Policy:

- disabled by default;
- IPv4/IPv6 loopback literals allowed according to existing config conventions;
- wildcard and non-loopback bind addresses rejected during config validation;
- no DNS hostname whose resolution could change the exposure boundary unless resolved and proven loopback before bind;
- no TLS/authentication options pretended supported in M9;
- `check-config` validates without opening sockets.

Do not reuse SAM config limits implicitly; name I2CP listener/session limits separately while sharing router-wide resource ceilings where appropriate.

## 3. Runtime ownership

Create the I2CP server in `i2pr-daemon` following the proven SAM supervision pattern without copying its protocol logic.

Topology:

```text
one supervised TcpListener
  -> bounded accepted-connection admission
  -> one supervised connection task per admitted TCP connection
  -> bounded read buffer + bounded write queue
  -> i2pr-api::i2cp state/actions
  -> destination registry/client-owned operations
```

A task per connection is acceptable. No task per frame/message/timer.

Every accepted connection must hold an explicit resource/admission lease released on all exits.

## 4. Connection reader/writer behavior

Requirements:

- require protocol byte `0x2a` before frame parsing;
- handshake/idle deadlines are named and bounded;
- partial TCP reads are normal and use Plan 164 incremental framing;
- multiple frames in one read are preserved and processed in order;
- maximum buffered unread bytes is bounded near the frame ceiling;
- write queue has count and aggregate-byte ceilings;
- a slow/nonreading client cannot retain unbounded replies;
- EOF, reset, timeout, parse failure, cancellation, and write failure converge on one cleanup path;
- no raw secret/payload logging.

## 5. Action composition

Wire Plan 165 actions into Plan 166/client services transactionally.

Required trajectory:

```text
TCP connect
  -> 0x2a
  -> GetDate
  <- SetDate
  -> CreateSession(verified SessionConfig)
  -> reserve/start client-owned destination
  -> wait for required inbound/outbound tunnel readiness
  <- RequestVariableLeaseSet(actual inbound leases)
  -> CreateLeaseSet2(signed Standard LS2 + X25519 private decryption key)
  -> validate/install/publish transaction
  <- SessionStatus(success/active according to pinned behavior)
```

The exact ordering of SessionStatus vs LeaseSet request must follow the pinned normative/reference behavior discovered in Plans 164–166; tests must pin it. Do not choose ordering based on convenience.

## 6. Failure mapping

Map runtime failures to protocol responses when the connection remains safely usable; close when the protocol/security state is no longer trustworthy.

At minimum distinguish:

- invalid protocol byte/frame;
- invalid SessionConfig/signature/date;
- unsupported options/profile;
- resource exhaustion;
- duplicate destination;
- destination tunnel startup failure/timeout;
- invalid/mismatched LeaseSet2/decryption key;
- client timeout/disconnect.

No internal filesystem/socket/debug detail in replies.

## 7. Lifecycle and cleanup

Connection ownership is authoritative for all M9 resources.

On TCP closure/cancellation:

1. mark connection closing so no new actions are accepted;
2. cancel pending lease requests/reconfiguration;
3. destroy all sessions owned by the connection;
4. stop client-owned destination runtimes/tunnel pools;
5. withdraw/release client-owned LeaseSet publication state according to existing policy;
6. zeroize decryption/session secrets;
7. release registry and connection admission resources;
8. finish the task within a bounded shutdown deadline.

Daemon shutdown closes listener first, then drains/cancels admitted connections.

## 8. Real-TCP acceptance tests

Add a focused integration suite such as:

```text
crates/i2pr-daemon/tests/i2cp_loopback.rs
```

Drive the server using raw external-to-protocol bytes over TCP, but only for local product validation; independent client evidence belongs to Plan 170.

Required trajectories:

- disabled config opens no listener;
- non-loopback/wildcard config rejected;
- valid protocol/GetDate/SetDate;
- fragmented protocol byte/header/body reads;
- multiple frames in one TCP write;
- valid signed client-owned session creation through real tunnel readiness;
- real RequestVariableLeaseSet contains actual pool leases;
- valid Standard LeaseSet2 + matching X25519 key activates session;
- invalid signature/date/options produce expected result;
- bad LeaseSet2/key pair fails without leaked destination;
- disconnect before/after every major transaction stage returns baseline;
- connection/session ceiling and max+1;
- slow reader fills bounded output and is terminated/degraded without affecting sibling client;
- graceful daemon cancellation leaves zero listener/session/destination tasks.

After listener startup, positive product tests must drive behavior through TCP/I2CP only. Read-only sanitized resource counters may be inspected for baselines.

## 9. Regression requirements

Because daemon composition is shared, run:

```text
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
```

I2CP must not change SAM listener defaults or ownership.

## 10. Documentation

Update:

- daemon config docs/example;
- `docs/architecture/i2pr-daemon.md` with I2CP listener topology;
- `docs/architecture/i2pr-api.md` with runtime/action boundary;
- I2CP dossier/support/CONFORMANCE;
- root README only to say an experimental disabled loopback I2CP server exists after closure; do not claim independent compatibility yet.

## 11. Acceptance criteria

Plan 167 closes only when:

1. `[i2cp]` is disabled by default and loopback-only when enabled;
2. wildcard/non-loopback configuration fails before bind;
3. daemon exclusively owns I2CP TCP/Tokio/task lifecycle;
4. framing/read/write buffers and queues have explicit ceilings;
5. protocol/session/LeaseSet2 activation works over real localhost TCP;
6. RequestVariableLeaseSet uses actual destination tunnel material;
7. invalid config/LeaseSet/key transactions leave no committed destination state;
8. EOF/reset/timeout/cancel/shutdown converge on bounded cleanup;
9. slow reader and admission ceiling tests prove sibling isolation;
10. after listener startup positive tests use only TCP/I2CP behavior-driving interfaces;
11. SAM regressions remain green;
12. I2CP vectors/runtime/dependency boundary checks remain green;
13. workspace floor and exact-head routine CI pass;
14. no application-message or independent-client claim is introduced;
15. `plans/167-status.md` records exact evidence.

## 12. Stop conditions

Stop if real TCP composition exposes a protocol-order mismatch against pinned Java/Go behavior, if listener security would require non-loopback exposure, or if cleanup cannot be made transactional with existing destination ownership. Write a narrow corrective rather than weakening the tests.

## 13. Handoff

After Plan 167 passes, execute Plan **168**.