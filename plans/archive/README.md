# Archive

Completed, superseded, or frozen interim planning retained for traceability. Archive moves
MUST preserve traceability via `git mv` and MUST NOT rewrite historical conclusions.

- `legacy-flat-registry-2026-09-18.md` — byte-identical snapshot of the pre-migration
  `plans/README.md` (76 KB authority registry for flat plans 000–215). Its internal
  relative links resolve against the old flat layout; use `plans/registry.md` plus the
  `plans/subsystems/` roadmaps for live authority. Kept byte-identical so the migration
  itself is auditable as a pure rename.

Canonical direction (`GUARDRAILS.md`, `specs/`, accepted ADRs) MUST NOT be archived merely
because an initial implementation completed.

- `managed-native-app-runtime/385-managed-app-private-persistent-data-and-launch-workspace.md` and `386-linux-secured-sandbox-and-resource-enforcement.md` — superseded planning drafts consolidated into Plan 407 after global-number reconciliation; the Proposal 170 line already owns global Plans 385 and 386.
- `managed-native-app-runtime/387-host-owned-local-service-ingress.md` and `388-external-rust-managed-app-sdk-and-package-builder.md` — deferred planning drafts retained for traceability; their old global numbers belong to Proposal 170, and each needs a fresh uniquely numbered plan before execution.
