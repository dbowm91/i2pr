# Plan 251 status — Java source-lock test environment gating and ordinary-CI corrective

Status: **registered-ready-java-source-lock-test-environment-gating-and-ordinary-ci-corrective**

Plan of record:
`plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md`

Baseline: `958c06171a6d60dc3d1866ed8b7d93937d001d6f`

## Registration basis

Current and immediately preceding GitHub Actions runs fail ordinary Ubuntu/macOS workspace
tests because Plan-246/247 source-lock tests unconditionally read an external exact-pinned
Java source checkout that CI does not provision.

This is retained Java-lane test infrastructure only. It does not reopen M6 progression.

## Required disposition

Closure must contain the final source-lock inventory, explicit gating/runner contract,
missing/wrong environment failure evidence, current-SHA GitHub Actions results, and proof
that production code and reference pins were untouched.
