# Corrective plan: Milestone 0 closure

## Purpose

Close the remaining repository-foundation issues before protocol implementation begins. This is a small corrective pass, not an extension of Milestone 0 and not authorization to begin Milestone 1 protocol code.

## Required reading

1. `README.md`
2. `GUARDRAILS.md`
3. `plans/000-mvp-roadmap.md`
4. `plans/001-preplan-workspace-skeleton.md`
5. `docs/architecture.md`
6. `specs/README.md`
7. `specs/CONFORMANCE.md`

## Scope

### 1. Verify CI actually runs and passes

- Confirm GitHub Actions is enabled for the private repository.
- Run the complete workflow on Linux and macOS.
- Verify formatting, check, tests, Clippy, rustdoc, dependency-direction, and `cargo-deny` jobs.
- Do not weaken checks to obtain a green run.
- Record any environment-specific exception in the closure note.

### 2. Resolve the MSRV/toolchain mismatch

The root manifest currently declares `rust-version = "1.85"` while normal development and CI use Rust 1.95.0.

Preferred correction:

- Retain Rust 1.85 as the explicit MSRV.
- Add an Ubuntu MSRV CI job using Rust 1.85.
- Run at least `cargo check --workspace --all-targets` under the MSRV.
- Keep formatting, Clippy, rustdoc, and ordinary tests on the pinned current toolchain.
- If any selected dependency no longer supports 1.85, either constrain it to a compatible release with justification or raise the declared MSRV and document the decision.

Do not claim an MSRV that is not continuously tested.

### 3. Close the bootstrap plan

Add `plans/001-closure.md` or append a clearly marked closure section to the existing pre-plan. It must record:

- final files and crates created;
- deviations from the proposed structure;
- commands executed and results;
- CI result or workflow URL/reference;
- dependency additions;
- security-relevant implementation choices;
- known limitations;
- confirmation that no network or persistent router behavior was introduced.

The original pre-plan should remain as historical intent and must not be rewritten to pretend that implementation exactly matched every proposal.

### 4. Add a machine-readable protocol-support ledger

Create a small tracked file suitable for later automated checks, for example `specs/support.toml`.

Initial requirements:

- Every Milestone 1 protocol surface starts as `not-implemented`.
- Entries include protocol/structure identifier, support status, evidence references, and advertised status.
- Allowed statuses must align with `specs/README.md` and `specs/CONFORMANCE.md`.
- The file must not imply interoperability or capability advertisement.
- Add a schema note or parser test if the format is consumed by code or CI.

Do not build a large support-management subsystem in this pass.

### 5. Clarify cancellation scope

Document that the current atomic cancellation token is a runtime-neutral bootstrap primitive and does not provide async wake semantics. Add this limitation to the relevant module documentation or architecture document. Do not replace it with a generalized runtime abstraction during this corrective pass.

## Validation

Run:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

Also run the new MSRV check locally or in CI.

## Exit criteria

- Latest `main` commit has a successful complete CI run.
- MSRV and normal toolchain policy are consistent and tested.
- Milestone 0 has an explicit closure record.
- The protocol-support ledger exists and truthfully reports no implemented wire support.
- Cancellation limitations are documented.
- No Milestone 1 protocol behavior was introduced.

## Handoff requirements

The implementation handoff must list changed files, CI run outcome, MSRV decision, command results, deviations, and any issue that should block Milestone 1. If CI cannot be made green without weakening policy, stop and report the blocker.