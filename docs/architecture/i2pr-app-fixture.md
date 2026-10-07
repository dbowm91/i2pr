# `i2pr-app-fixture` — the managed-app black-box fixture (evidence tooling)

`i2pr-app-fixture` (Plan 369 WP5) is **evidence tooling, not product code**. It
holds the two binaries Plan 369 needs to exercise the managed-app chain as real
processes:

| Binary | Role |
| --- | --- |
| `i2pr-app-fixture` | the native fixture **application** |
| `i2pr-app-fixture-manager` | a fixture **manager** that runs the real `Appd` against a test launch catalog |

No production crate may name it or depend on it.
`scripts/check-managed-app-process-boundary.py` rule 2 enforces both, and
`check-dependency-direction.sh` carries the crate's allowlist so that a future
`i2pr-appd -> i2pr-app-fixture` edge is a hard failure.

## Why the manager is a separate binary

The shipped `i2pr-appd` refuses **all** arguments (rule 3 asserts this), which
is deliberate: an argument that could select an executable, a root, or a launch
profile would reopen exactly the user-configurable-program hole Plan 369 closes.
Adding a fixture flag to it would therefore have undone the property it exists
to protect.

The fixture manager is a different program that runs the **real**
`i2pr_appd::Appd::with_catalog(FixtureCatalog)` over the **real**
`i2pr_appd::inherited()` transport, and gates every authority through the real
`LaunchAuthority::new`. What is qualified is the product manager, not a
stand-in that behaves the way the tests want.

Its launch catalog is the *only* production-shaped thing in this crate that
holds authority, and it exists solely to reach the real gate.

## The application depends on nothing but the wire

`i2pr-app-fixture`'s application binary depends only on `i2pr-app-proto` and
`i2pr-app-manager-proto` (plus its own shared lib). It has no route into router
state, which is what makes it a genuine separate program rather than an
in-process fake: if it could reach router internals, the qualification would be
proving nothing about containment.

## Scenarios

`Scenario::ALL` is the enumerable list — WP5 asserts against it, so removing a
scenario is a deliberate, visible edit rather than a silent loss of coverage:

`sam-happy-path`, `i2cp-happy-path`, `hello-wrong-identity`,
`hello-not-first`, `denied-capability`, `denied-service`, `oversized-frame`,
`malformed-frame`, `close-early`, `shutdown-hang`, `stderr-flood`,
`sibling-stream-id-misuse`, `duplicate-sam-session-id`, `data-before-open`,
`duplicate-stream-id`.

`Scenario::parse` treats unknown input as an **error**, not a fallback. A fixture
that silently dropped to its happy path would turn a typo in an evidence run
into a green transcript for a behaviour nobody asked about.

`hangs_until_stopped()` is what distinguishes the shutdown-hang case from the
rest: the manager's bounded shutdown deadline only matters for a scenario that
does not end on its own.

## Deliberately misbehaving, within bounds

Three scenarios are meant to be wrong, and each is still bounded — an unbounded
mistake would prove nothing except that the host can be OOM-killed:

- **oversized-frame** writes past the product's own
  `i2pr_app_proto::MAX_FRAME_PAYLOAD_BYTES` (up to `FIXTURE_MAX_FRAME_PAYLOAD`,
  128 KiB) to prove the oversize path is actually reached.
- **malformed-frame** is well formed in every respect except the one that
  matters. A hello that were merely *malformed* would be refused for the wrong
  reason and would prove nothing about identity checking.
- **stderr-flood** writes `STDERR_FLOOD_BYTES` (512 KiB), well past the
  apphost's 8 KiB retained snapshot, to prove the drain is bounded.

## Evidence channel

The fixture writes a JSONL transcript whose path arrives through launch
`argv`. That is the only channel available that is inert, per-run, and outside
every product path — so evidence collection adds no coupling to the thing being
qualified.

The manager completes the application's argv itself rather than accepting it:
the instance id is a manager-created value and the catalog re-derives it per
launch, so accepting an application-supplied instance id would let a launch name
an identity the manager never created.

## Building before testing

`cargo check`, `cargo test --all-targets` and `cargo clippy` emit only test
harnesses under `target/debug/deps` — never the plain `target/debug/<name>`
binaries this crate exists to produce. Run

```text
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost
```

before any focused `-p i2pr-daemon` qualification run. It is line 3 of the
`AGENTS.md` routine floor and a CI step for exactly this reason. The
qualification harness asserts binary freshness itself and fails closed rather
than silently testing last week's code.