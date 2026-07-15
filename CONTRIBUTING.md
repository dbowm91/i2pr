# Contributing to i2pr

`i2pr` is an experimental, clean-room Rust router project. Contributions must
preserve `GUARDRAILS.md`, the active plan, and the architecture decisions in
`docs/adr/`.

## Before changing code

Read, in order:

1. `README.md`.
2. `GUARDRAILS.md`.
3. The relevant roadmap or detailed plan under `plans/`.
4. Relevant ADRs and protocol dossiers.

Protocol changes require a plan covering acceptance criteria, limits, negative
tests, dependency changes, security implications, sources, and documentation.
Do not add empty future crates or claim interoperability without evidence.
The current common-structure subset in `i2pr-proto` is structural only: keep
signed byte regions intact, use the pinned source ledger, and leave signing,
freshness policy, transport interpretation, and LeaseSet2-family behavior to
their later plans.

## Local quality checks

Run from the repository root:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The CI matrix covers Linux and macOS. Dependency downloads may require network
access in restricted environments; do not weaken checks to work around that.

## Security and testing

Treat all external input as hostile. Add malformed, boundary, cancellation,
cleanup, and resource-exhaustion tests alongside successful-path tests. Use the
manual clock and reproducibility seeds from `i2pr-testkit` for deterministic
state-machine tests. Public-network testing must be passive and ordinary;
stress, mutation, malformed-traffic, and adversarial tests belong only in an
authorized isolated testnet.

Report security issues privately to the project owner rather than publishing
exploit details in an issue or pull request.

## Dependencies, provenance, and commits

Keep dependencies focused, centralize workspace versions, review transitive
impact and unsafe-code exposure, and record why a new dependency is needed.
Do not copy implementation code or test vectors from another router until
license and provenance review is complete. The project license is intentionally
not selected yet.

Make focused commits that explain behavior and tests. Handoffs should list
changed files, commands and results, dependency changes, security-relevant
decisions, deviations, and remaining risks.
