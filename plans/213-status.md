# Plan 213 status — M10 router-backed generic external qualification

Status: **`registered-executable-after-plan212-source-closure`**.

Plan of record: [`213-m10-router-backed-generic-external-qualification-and-evidence-corrective.md`](213-m10-router-backed-generic-external-qualification-and-evidence-corrective.md).

Source floor: `232be0f87469175a1f01152a7488ecf026b27eeb`.

## Why this plan exists

Plan 212 source architecture landed, but its ignored generic Direction A/B driver is still a structural scaffold rather than an executable external proof: the Direction A/B success booleans remain immutable `false`, the counted loops do not perform application byte I/O, the current M10 runner normally skips provisioning the generic reference destination, and several mandatory evidence rows are written as unconditional success literals.

Plan 213 owns only the qualification correction and execution:

```text
complete generic A/B application driver
  -> independent exact-pinned i2pd SAM STREAM reference service
  -> independent i2pd initiator for reverse direction
  -> command/state-derived evidence only
  -> standalone fail-closed Plan 213 runner
  -> hosted exact-head run twice
```

No new router architecture is authorized unless the real external run proves a product defect.

## Authority

```text
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = registered-executable-after-plan212-source-closure
plan_214 = registered-blocked-by-plan213

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 213 may become passed only after its generic Direction A + Direction B hosted qualification succeeds twice on the same exact source SHA with all mandatory evidence rows command/state-derived.
