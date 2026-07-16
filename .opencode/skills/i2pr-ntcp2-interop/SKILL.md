---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's Ubuntu 24.04 reference-router NTCP2 interoperability harness, including host preflight, pinned Java I2P and i2pd preparation, isolated scenario execution, typed evidence validation, cleanup, and fail-closed result interpretation. Use when Codex is asked to run Plan 038, prepare its reference routers, add scenarios or adapters, inspect interoperability outcomes, or update this apparatus.
---

# I2PR NTCP2 interoperability

Use this skill from the repository root for the manual, opt-in Plan 038/040/041/042/043/044/046
harness. Read `AGENTS.md`, `plans/043-ubuntu-build-system-interop-gates.md`,
`plans/044-ntcp2-interop-final-integration-corrective-pass.md`,
`plans/046-rootless-sealed-namespace-evidence-lane.md`,
`plans/038-ubuntu-reference-router-interoperability-harness.md`,
`tests/integration/ntcp2/README.md`, and the relevant architecture/ADR files
before changing the apparatus.

The canonical reference identifiers are `java_i2p` and `i2pd`. The locked
source objects are Java I2P
`2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`; abbreviated revisions are not
valid cache or evidence inputs.

## Safety boundary

Treat the harness as experimental infrastructure, not an anonymity or security
tool. Never enable `i2pr-daemon`, use public egress, perform DNS/bootstrap or
reseed, retain identities/keys/RouterInfo/raw logs/packet captures, or turn a
local self-handshake, loopback run, vector, or testkit result into Java I2P or
i2pd interoperability evidence. Keep support rows experimental and
non-advertised unless sanitized evidence satisfies `specs/CONFORMANCE.md`.

Run only on an authorized disposable Ubuntu 24.04 amd64 host. The namespace
and firewall checks are mandatory and fail closed. Do not bypass a host,
privilege, route, cleanup, or evidence validation error.

The exact host contract is Ubuntu 24.04 amd64/x86_64, Bash 4+, a UTF-8 locale,
non-interactive `sudo` when not root, Linux namespace/nftables capability, and
at least 4 GiB free under `target/`. The declared package set and all cache
identity inputs are authoritative in
`tests/integration/ntcp2/references.lock.toml`.

## Plan 042 runtime and launcher boundary

The NTCP2 wire driver is a runtime-owned composition. `i2pr-runtime` owns
Tokio sockets and tasks, action deadlines, cancellation, replay/admission,
authenticated frame state, bounded queues, and child joins. The
`i2pr-transport-ntcp2` state machines remain runtime-neutral and receive only
complete bounded actions. `tools/i2pr-interop` is a non-production launcher
seam: it validates bounded non-secret scenario input and composes the runtime
driver, but it must never activate `i2pr-daemon`.

The launcher status protocol has separate meanings. A completed `listen` emits
listener readiness and then a distinct authenticated terminal result; `dial`
emits one terminal typed result; and `inspect` emits redacted state metadata.
Listener readiness is not authentication. The current checkout composes the
runtime-owned handshake executor, authenticated link owner, listener/dial
promotion, and DeliveryStatus smoke. State, handshake, data-phase, timeout,
and cleanup failures are typed terminal results; none is evidence by itself.

Plan 042 selects the existing fixed-size DeliveryStatus message (I2NP type 10)
for the first data smoke: 12-byte body, 21-byte NTCP2/SSU2 short transport
encoding, and 24-byte NTCP2 block before frame overhead and padding. A positive
gate requires one authenticated outbound and one authenticated inbound
DeliveryStatus per direction plus orderly cleanup. Reference acceptance or
echo behavior is not yet verified; do not claim interoperability or substitute
padding/TCP readiness for the message exchange.

## Plan 044 mixed-runner composition

The checkout now contains the four directional mixed-scenario definitions under
`tests/integration/ntcp2/mixed-scenarios/`: `i2pr-to-java-ipv4`,
`java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`, and `i2pd-to-i2pr-ipv4`. Each
direction has a unique execution ID, one declared initiator and responder, and
one terminal typed result.

The mixed runner composes `I2prAdapter` with each reference adapter through
a strict launcher scenario renderer. The renderer populates the exact launcher
schema with execution-specific scenario ID, role, address family, synthetic
endpoints, private network ID 99, confined state directory, deadlines, padding
profile, smoke-message profile, and expected-result class.

The data-phase oracle does not rely on an echo assumption. It uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued
records fail the gate. No completed mixed-router i2pr record is present;
these are explicit blockers, not skipped successes.

## Plan 043 workflow

The semantic gates are ordered and later gates are ineligible when required
inputs are missing or invalid:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

1. Inspect the lock, scenario definitions, and current workflow status. Do not
   change source revisions, package assumptions, scenario IDs, or the IzPack
   hash without updating the plan and conformance documentation.
2. Run the contract checks without starting routers. Preparation then runs
   `check-host.sh --pre-install`, the declared `setup-host.sh`, and
   `check-host.sh --post-install`.
3. Build exact reference caches with `build-references.sh --force-rebuild`.
   This is the only network-enabled phase and records source/tool/artifact/tree
   hashes. Resolve only through `target/interop/cache/current-cache.json`.
4. Restore the verified cache and run `build-references.sh --offline`.
   Re-hash the complete runtime tree. A cache miss or metadata mismatch is a
   hard failure; never fetch or choose an arbitrary cache.
5. Run `environment-smoke`, then `reference-crosscheck-ipv4`. The latter uses
   separate Java/i2pd namespaces, private network ID 99, staged strict
   RouterInfo validation/import, controlled directions, and dual authenticated
   observations. It is harness control evidence only.
6. Only after reference control passes, build the current launcher and run
   `handshake-smoke`; require four independent i2pr/reference directions,
   authenticated handshake, bounded DeliveryStatus exchange, typed counters,
   sanitized finalization, and clean state. Run `full` only afterward; it adds
   bounded adversarial/resource cases and never unbounded fuzzing.
7. Validate every record and the aggregate manifest with
   `validate-evidence.py` and `check-ntcp2-interoperability.sh`. Empty evidence,
   placeholders, forbidden content, missing scenarios, extra passed records,
   hash mismatches, or incomplete direction coverage fail the gate.
8. Record the clean-host baseline before privileged execution with
   `sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline`.
   Always run `cleanup.sh`, then verify with
   `sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline
   target/interop/build/clean-host-baseline.json`. Reject residual namespaces,
   veths, child processes, secret-bearing run roots, forbidden retained files,
   or attributable host nftables/routes/forwarding changes. Cleanup
   verification failure overrides protocol success.

The workflow and helper apparatus expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation, but no completed
successful aggregate run is present. Treat that as an explicit Plan 043
blocker, not a skipped pass. Retain only sanitized typed records and approved
hashes under `target/interop/evidence/`.

Consult [operations.md](references/operations.md) for command routing,
profiles, typed outcomes, and implementation-specific stop conditions.

## Development rules

Keep production ownership boundaries intact: runtime owns Tokio tasks and
sockets; transport contracts remain runtime-neutral; the launcher crate under
`tools/i2pr-interop` is a non-production seam and must not activate the daemon.
Add negative-path tests for new configuration, topology, process, parser, or
evidence behavior. Prefer deterministic local checks and never add raw network
fixtures or secrets.

Before handoff, run the repository's required Rust, boundary, fixture/vector,
interoperability, Python harness, and shell syntax checks. Record commands,
results, host constraints, and any blocked stop condition in a closure record;
do not report a blocked profile as a passing interoperability result.

## Plan 046 rootless sealed-namespace lane

Plan 046 replaces the host-global namespace requirement for the primary NTCP2
interoperability evidence path with a **rootless, process-scoped sandbox**.
The primary mixed-router evidence topology is `rootless-sealed-single-netns`
with privilege model `unprivileged-userns`. The legacy
`privileged-dual-netns-veth` topology is renamed and reserved for explicit
later qualification work; it is never the default and never a silent
fallback.

The lane introduces:

- `tests/integration/ntcp2/harness/interop_topology.py` — topology contract
  with `InteropTopology`, `ProcessPlacement`, `select_topology`, and a
  registry locked to the two allowed topology kinds.
- `tests/integration/ntcp2/harness/rootless_topology.py` —
  `RootlessSealedTopology` backend (`topology_kind =
  "rootless-sealed-single-netns"`, `privilege_model = "unprivileged-userns"`).
  Adapter `placement()` returns an empty prefix; the inner process is
  required to set `I2PR_INTEROP_ROOTLESS_INNER=1`; the structural checks
  (`_can_bind`, `_external_connect_attempt`) gate passed records.
- `tests/integration/ntcp2/harness/rootless_supervisor.py` — inner
  supervisor that verifies single-ID UID/GID maps, `no_new_privs`, distinct
  user/network/mount/PID namespaces, `lo` readiness, synthetic bind,
  absence of default/external routes, and a bounded external connect probe.
  Writes a sanitized `IsolationAttestation` whose sha256 is bound to every
  passed mixed-router evidence record. Emits typed `probe-status` JSON.
- `scripts/interop/rootless-enter.sh` — outer entrypoint that creates the
  sandbox via `unshare --user --net --mount --pid --fork --propagation
  private --mount-proc --map-root-user`, accepts only allowlisted
  operations, has no shell `eval`, and never falls back to the privileged
  backend.
- `scripts/interop/probe-rootless-sandbox.sh` — capability probe that
  validates host capability to run the sandbox without `sudo`/host capability
  grants and emits a typed JSON blocker on failure.
- `scripts/check-rootless-interop-boundary.sh` — static boundary checker
  that fails the change when rootless-owned files contain prohibited
  patterns (sudo, `ip netns add`, `nft`, `setcap`, `--privileged`,
  `--network host`, fallback to privileged backend) or omit required
  contracts (gate catalog entries, sandbox attestation requirement).
- `.github/workflows/ntcp2-interop-rootless.yml` — manual no-escalation
  workflow with `permissions: contents: read` and `workflow_dispatch`
  trigger only.

The new command surface is:

```text
bash scripts/interop/probe-rootless-sandbox.sh                       # typed blocker or usable
bash scripts/interop/rootless-enter.sh --probe                       # sandbox-only verify
bash scripts/interop/rootless-enter.sh --scenario <id> --reference <ref> \
    --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile handshake-smoke-rootless
```

Mixed-router evidence records now carry `topology_kind`, `privilege_model`,
`sandbox_attestation_sha256`, and `parent_network_state_unchanged`. A passed
record that violates any of these is rejected. The lane forbids automatic
fallback to the privileged topology; a missing rootless capability is a
typed blocker, not a skipped success. NTCP2 remains experimental and
non-advertised; Milestone 3 is still open.
