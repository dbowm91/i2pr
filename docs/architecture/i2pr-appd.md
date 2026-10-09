# `i2pr-appd` — the trusted application manager process

`i2pr-appd` (Plans 369 and 383) owns the **manager** side of the managed native
application runtime: the separately supervised process the router starts, which
holds manager-created launch authority and is the only component that starts an
`i2pr-apphost`.

It is a **separate runtime trust zone**. Its only production `i2pr-*`
dependencies are the two wire contracts, `i2pr-app-manager-proto` and
`i2pr-app-proto`, plus `i2pr-app-state` for persistent local policy. It may not
reach `i2pr-daemon`, `i2pr-runtime`, or any router
crate, and that is asserted by `scripts/check-dependency-direction.sh` rather
than left to review.

## What it is not

The shipped binary refuses **all** arguments and uses
`PersistentLaunchCatalog`. After the authenticated inherited-pipe handshake,
the catalog reads the daemon-bound `I2PR_APP_STATE_ROOT`, holds `runtime.lock`,
and loads one validated policy snapshot. It launches only explicitly selected,
trusted, autostart applications, after re-verifying their package. The manager
protocol still has no manager-receivable launch request, so daemon messages
cannot select an executable or create authority. With no operator-approved
autostart state, the catalog yields no authority.

## Process and transport

The daemon launches this binary as its own child with two inherited anonymous
pipes on file descriptors 0 and 1. There is no listener, no port, no discovery
endpoint, and no way to run the process standalone and have it mean anything.
`inherited()` maps fd 0 to the manager's read half and fd 1 to its write half;
stderr is reserved for the manager's own diagnostics and is never protocol and
never interpreted as control.

The executable is resolved as a **sibling** of the router's own
`current_exe()`. Nothing in configuration, argv, or `PATH` can substitute a
different program. `scripts/check-managed-app-process-boundary.py` rule 1b
asserts the `current_exe()` lookup, and rules 1b/1b-path forbid a shell
launcher and any `PATH` lookup outright.

`i2pr-appd` takes no arguments, and `Appd::with_catalog` remains confined to
the fixture manager. The process checker also requires the daemon to clear the
manager environment and bind the canonical state root only.

The v1.1 `local_service` capability does not change this process boundary:
appd sends publication requests and logical-stream events over the inherited
pipe. The router daemon binds `127.0.0.1` and owns the listener and accepted
sockets; no descriptor crosses into appd or apphost.

## Modules

| Module | Owns |
| --- | --- |
| `transport` | `DuplexTransport` (both halves in one object, so `tokio::io::split` cannot separate them) and `inherited()` |
| `authority` | sealed `LaunchAuthority` / `AuthorityRequest`, **no decoder** |
| `catalog` | `LaunchCatalog`, test `EmptyCatalog`, and production `PersistentLaunchCatalog` |
| `manager_link` | the concurrent manager client: one reader, one writer, a single-writer queue, a bounded in-flight ledger, per-session routing |
| `session` | the app v1 session: greeting, hello-first, capability presentation, permission denial, SAM/I2CP mapping, stream-id mapping |
| `apphost_launch` | resolving and starting the `i2pr-apphost` sibling |
| `runtime` | the bounded instance registry and launch pipeline |

## Authority is sealed, not merely private

`LaunchAuthority` and `AuthorityRequest` have private fields and **no decoder**.
Effective capabilities can only be assembled through
`GrantedCapability::from_administrator_policy`, so `BrokeredTcp` is
ungrantable, and there is no `&mut` path to the capability set at all. Every
gate that can be evaluated without a filesystem runs in `authority`. Host
support and enforcement are then checked by apphost before it sends readiness.

This is asserted by **method resolution**, not by scanning for a
`#[derive(Deserialize)]`: adding a derive makes the crate stop compiling rather
than merely failing a test.

## Direction matters

`manager_link` decodes inbound frames as `DaemonToManagerMessage`. WP2 read this
backwards — using the *outbound* control vocabulary — and WP2's test peer also
sent outbound-vocabulary frames, so the two mistakes cancelled and the test
asserted the inverted contract. The two vocabularies share no tags, so a
direction mistake is not a mis-parse but `InvalidControl`: against the real
bridge the first daemon reply would have ended the transport. This was
invisible to every test that shared the implementation's assumption.

The link also **fails closed on any correlated reply for a request the manager
never sent**, which is the property a confused or hostile daemon would violate.

### A defect only a real process could find

`writer_task` originally wrote each frame with `write_all` and never flushed.
Every WP2–WP4 test drives a `tokio::io::duplex` transport, which is unbuffered
and therefore always "flushed". The real transport's daemon end is
`tokio::io::Stdout`, which is *line-buffered*, and a length-prefixed control
frame contains no newline — so every manager→daemon frame sat in the buffer and
every `CreateSession` timed out. The handshake escaped only because
`Appd::write_handshake` flushes on its own. Found by WP5's black-box
qualification; `every_frame_is_flushed_without_waiting_for_a_newline` now
asserts the flush directly rather than inferring it from a timeout.

## Bounded resources

| Constant | Value |
| --- | --- |
| `MAX_APP_INSTANCES` | `MAX_MANAGER_SESSIONS` (32) — one shared ceiling, not a third number |
| `MAX_QUEUED_OUTBOUND_FRAMES` / `MAX_SESSION_EVENT_QUEUE` | 64 / 64 |
| `MANAGER_REPLY_GRACE` | 15 s |
| `APP_HELLO_GRACE` | 30 s |
| `SESSION_CLOSE_GRACE` | 2 s |
| `APPHOST_BOOTSTRAP_GRACE` / `APPHOST_EXIT_GRACE` / `APPHOST_REAP_GRACE` | 10 s / 5 s / 500 ms |
| `MAX_APPHOST_STDERR_SNAPSHOT_BYTES` | 8 KiB retained (totals still counted) |

Every spawned task has explicit ownership and cancellation; channel and socket
close are lifecycle events, not retry conditions.

## Teardown

EOF on the manager's read half, or a daemon-side transport failure, tears down
every manager session. Teardown mirrors the daemon's `terminate_manager`:
cancel, drop/abort the bridge future, a bounded grace, then `start_kill` and
reap. `AppManagerBridge::cancel()` alone does **not** end the read loop, and the
manager's only exit signal is EOF on a write half the daemon holds — so the
manager is always killed. Assertions say "terminated and reaped", never "exited
cleanly".

## Limits

- Secured is qualified only for static native ELF applications on supported
  Linux x86_64/aarch64 hosts. It denies subprocess creation; other hosts refuse
  before exec.
- No package store, signature verification, grant persistence, or policy engine.
  The next milestone owns those.
- No administrator or general control credential is exposed to this process.
- Restart begins empty: no application or grant recovery is claimed.
