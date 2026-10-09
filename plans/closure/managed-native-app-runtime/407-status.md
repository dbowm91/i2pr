# Plan 407 closure — Linux Secured apphost sandbox and private data root

Status: **passed-linux-secured-apphost-sandbox-and-private-persistent-app-data**.

Plan: `plans/implementation/managed-native-app-runtime/407-linux-secured-apphost-sandbox.md`.

Implementation commits:

- `810425fb` — Linux secured apphost and persistent data implementation.
- `b334a255` — use architecture-native aarch64 syscall names.
- `715aa5dd` — gate sandbox dependencies and implementation on supported Linux hosts.

Exact-head ordinary CI: [run 37961378206](https://github.com/dbowm91/i2pr/actions/runs/37961378206) on `715aa5ddfee81d654ea97767c62cc661f2017285`; all five jobs passed, including native Linux aarch64 secured qualification, macOS, Ubuntu quality, MSRV, and dependency policy.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| Secured app launches only after full platform enforcement | `i2pr-apphost` resolves the signed package entrypoint, checks a static ELF of the current architecture, installs hard resource limits, Landlock ABI v3 rules with hard compatibility, `no_new_privs`, and a default-kill seccomp allowlist, then sends the complete validated attestation and replaces itself with the app. Setup errors are typed and returned before exec. Production executable behavior is exercised by the Linux apphost black-box tests on both x86_64 and native aarch64. |
| All eight required sandbox properties have executed evidence | `linux_secured_exec_enforces_filesystem_environment_fds_network_and_fork` positively proves execution and the broker channel, and negatively probes filesystem boundaries, environment/descriptor state, networking, process creation/inspection, and resource-limit mutation. ELF interpreter refusal proves unsupported binaries fail before Ready/exec. `sandbox::tests::allowlist_excludes_network_process_creation_and_inspection` pins the syscall set; the secured checker and six mutation self-tests pass. |
| Private persistent application data | `i2pr-app-state` gives each publisher/AppId a stable private root, rejects symlink/non-directory leaf substitution, applies private permissions, and keeps the data path stable across package version changes and manager restarts. Production catalog qualification reaches private SAM and I2CP before and after an appd restart using the persisted data root. |
| Package and unrelated host state remain isolated | Landlock grants package read/traverse/execute and data read/write/create/remove without execute; the hostile fixture demonstrates package mutation and sibling/router/user-private reads fail. Secured launches reject dynamic/interpreter ELF so the filesystem rules do not require ambient loader directories. |
| Direct network, child creation, process inspection, and resource-limit mutation are denied | The executed fixture attempts public, LAN, loopback, fork/clone, process-inspection, and `prlimit64` mutation paths; each is denied. Static syscall negative coverage also asserts the architecture-native allowlist and positive file/channel operations. |
| Resource ceilings are effective and immutable | Requested memory and open-file counts are clamped by the policy ceiling and installed as hard/soft RLIMIT_AS and RLIMIT_NOFILE values before seccomp. The executable probes observe the exact limits; attempts to raise them fail. |
| Environment and descriptor authority are minimized | The host clears its environment and supplies only the declared managed-app inputs. The fixture verifies absent ambient variables and the bounded inherited app channel; descriptor census confirms no unexpected host descriptors. |
| Private SAM/I2CP capability remains usable | Exact-head native x86_64 and aarch64 workflows execute the production appd/apphost catalog qualification and reach both private client paths, including after manager restart. The app never receives a router listener or direct socket. |
| Unsupported hosts and binaries fail closed | The sandbox backend is compiled only for Linux x86_64/aarch64. All other targets use a typed unavailable result. Dynamic, malformed, and foreign-architecture executables cannot receive a complete attestation. macOS workspace checks pass with no Linux sandbox dependency compiled. |
| Dependency and unsafe review | Added exact-pinned `landlock = 0.4.7` and `bux-seccomp = 0.2.0`, plus the existing workspace `libc`/`rustix` crates scoped to Linux x86_64/aarch64. Landlock provides the safe ruleset abstraction; bux-seccomp installs a custom local BPF program with default KILL, not its permissive convenience profile. No local unsafe code or new production network/process dependency was added. `cargo deny check advisories bans sources` and license metadata checks pass. |
| Durable documentation and recurrence guards | ADR 0038, `AGENTS.md`, security model, crate architecture pages, managed-app protocol/policy references, tooling inventory, CI, and `scripts/check-managed-app-secured-sandbox.py` document/enforce the contract. Checker normal and six-mutation self-test pass. `specs/support.toml` is unchanged. |

## Verification

Local Linux verification:

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed.
- Static secured fixture build, copy to `i2pr-app-fixture-secured`, then rebuild of the ordinary fixture — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed, 4,685 passed and 36 ignored.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- `cargo +1.89.0 check --locked --workspace --all-targets` — passed.
- `cargo deny check advisories bans sources` — passed.
- Exact-head native Linux aarch64 qualification — passed (workflow above).
- Exact-head Linux and macOS quality suites, MSRV, and dependency policy — passed (workflow above).
- `python3 scripts/check-managed-app-secured-sandbox.py` and `--self-test` — passed, six mutations.
- Managed-app private-client, manager, gateway, package, policy, process, and their required self-tests; dependency direction; runtime and console boundaries; workflow validity; tooling inventory; global plan/ADR uniqueness; planning unit suite (51 tests); service-tunnel, protocol-vector, fixture-manifest, evidence-integrity, constrained-host, and cargo-deny checks — passed locally and where listed in exact-head CI.

The aarch64 hosted job is native kernel qualification, not cross-compilation. Local cross-compilation also passed for `aarch64-unknown-linux-gnu` and `x86_64-apple-darwin`; those results do not substitute for the native hosted execution.

## Compatibility, security, and limitations

`Secured` is now available only on Linux x86_64/aarch64 when Landlock ABI v3 is fully enforced and the executable is a static ELF for the current architecture. The no-subprocess profile is intentional; threads, child creation, direct network, filesystem access outside package/data, and resource-limit changes are denied. App data persists across restart and package updates under a publisher/AppId-owned private root.

Other operating systems and unsupported kernel configurations continue to fail closed before exec. No macOS/Windows sandbox claim, sandbox escape resistance claim, production readiness claim, anonymity/privacy claim, protocol support promotion, or public advertisement is made. `UnsafeDirect` retains ordinary host networking and has no containment claim. The apphost itself is outside the app's sandbox before exec; a broken manager remains isolated from router startup per the existing optional-service policy.

Findings: critical none; high none; medium none; low none.

## Unblock audit and roadmap disposition

The repository registry had no active managed-app consumer registered against the archived draft numbers. Their global numbers belong to Proposal 170 and remain untouched. The archived local-service-ingress and Rust SDK/package-builder drafts are now reconciled as new Plans 408 and 409. Plan 408 is ready because its hard prerequisites (Plans 369–371, 382–383, and 407) are closed; Plan 409 remains blocked on Plan 408 because the SDK must consume the resulting protocol minor-version extension. The registry and roadmap are updated with these explicit states.

The external [`dbowm91/i2pr-mail` M006 registry](https://github.com/dbowm91/i2pr-mail/blob/main/plans/registry.md) remains blocked: the local runtime/sandbox prerequisites are now complete, but the public SDK/interface milestone and downstream consumer adoption are not. Its 2026-10-06 registry audit still identifies the old “app-side runtime unregistered” gap; Plans 369/382/383/407 now close that local portion, while Plan 409 must still close the publishable consumer interface. The external owner must re-audit M006 after Plan 409 before changing its state. This closure does not edit that external repository or claim its plan is unblocked.

No router protocol milestone, M12, anonymity, or transport plan is newly unblocked. No support or advertisement claim changes.

Disposition: **closed**. The Linux secured runtime and private persistent data-root scope of the reconciled managed-app drafts is complete. The two remaining imported capability drafts continue under Plans 408 and 409.
