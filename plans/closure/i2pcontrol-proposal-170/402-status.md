# Plan 402 closure — blocked pending remote ELS2 secret parameter correction

Status: **blocked-reverse-requester-uses-publication-auth-options-plan-403**.

Plan: `plans/implementation/i2pcontrol-proposal-170/402-reverse-authorized-requester-key-loading-corrective.md`.

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Correct requester setup based on pinned source | Added `i2cp.leaseSetType=5` with publisher `leaseSetAuthType` and `leaseSetClient.{psk,dh}` options. Reinspection shows those options configure a local encrypted publisher, not remote LS2 consumption. | Incomplete; Plan 403 owns correction |
| Preserve the failed attempt | Absolute-path run `target/interop/els2-evidence-plan402-psk-type7-keyloaded-20261009b`: all mesh and standard authority controls passed; reverse request again returned `CANT_REACH_PEER / LeaseSet not found`. Hashes: evidence `712bdd840a5fd87e575126077016989380c4756c2b96041c07e9a84ddca45687`; results `1eb75d877892501c449736ea27ffd7c9c1e54b308c83bd4ad109ea3ab1ecaa78`; driver `07fda2cb53ed22b17bd68e8b183a67945d2990820fe75f51a300221d30f40999`. | Failed; retained |
| Determine remote consumer secret parameter | Pinned `libi2pd/Destination.cpp:88-97` parses `i2cp.leaseSetPrivKey` into `m_LeaseSetPrivKey`; `libi2pd/Destination.cpp:498-503` passes that pointer to the received encrypted LeaseSet2 parser; `libi2pd/LeaseSet.cpp:661-706` uses it for DH/PSK client authorization. By contrast `Destination.cpp:1084-1102` loads `leaseSetClient.*` only for construction of the local publisher LeaseSet. | Pass; Plan 403 owns the correct test fix |
| Keep profile and confidentiality bounds | Explicit signature type 7 remains; evidence contains no client key bytes. | Pass |

## Commands and outcomes

- `cargo fmt --all --check` — pass.
- ELS2 source checker, runner self-test, evidence guard, encrypted-consumer caller guard, and global plan-number check — pass.
- Managed-app sibling build and `cargo check --locked -p i2pr-daemon --test els2_i2pd_external` — pass.
- Relative-path PSK runner invocation failed before entering the driver because the Rust test resolves evidence paths from its package working directory. Retained at `target/interop/els2-evidence-plan402-psk-type7-keyloaded-20261009`.
- Absolute-path PSK runner invocation reached the driver, passed reference mesh/authority controls, and failed reverse lookup with `LeaseSet not found`. Retained at `target/interop/els2-evidence-plan402-psk-type7-keyloaded-20261009b`.
- No DH run: it shares the same incorrect consumer-option construction and would not provide meaningful evidence.

## Security, compatibility, disposition

No production files changed. Plan 402's option changes are confined to the external test requester and do not expose credential bytes in artifacts. The source proof now identifies the actual remote consumer credential parameter; Plan 403 removes the publication-auth options from the requester and supplies the protected private/PSK secret through `i2cp.leaseSetPrivKey`. Plan 400 NONE remains qualified; reverse PSK/DH remain unqualified.

Plan 402 is blocked on Plan 403. No other registered plan becomes dependency-ready. Plans 374/375 remain blocked and therefore 377/378 remain blocked. No support, conformance, or advertisement changes.
