# Plan 407 — Linux Secured apphost sandbox + private persistent data root

Status: **in-progress-linux-secured-apphost-sandbox**

Plan: `plans/implementation/managed-native-app-runtime/407-linux-secured-apphost-sandbox.md`

Execution has started on branch `plans/407-linux-secured-apphost-sandbox`. The implementation will first freeze the Linux mechanism and dependency review, then thread a trusted persistent app-data root, implement the apphost enforcement path, and qualify it through the production manager/apphost/fixture chain. No Secured capability claim is made while this record is in progress.

Initial research found candidate safe wrappers for Landlock and seccomp. The seccomp candidate is new and its default profile is deliberately broad; only a custom fail-closed allowlist could qualify. Exact dependency/MSRV/architecture review remains open.
