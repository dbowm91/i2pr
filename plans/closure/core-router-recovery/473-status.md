# Plan 473 status: blocked — final exact-head checks and PR integration pending

Closure token: `blocked-final-exact-head-ci-and-stack-approval-pending`

Plan: [`473-macos-ntcp2-governor-and-recovery-stack-integration.md`](../../implementation/core-router-recovery/473-macos-ntcp2-governor-and-recovery-stack-integration.md)

## Outcome

The reported paused-clock restart test passes locally on Linux:

```text
cargo test --locked -p i2pr-runtime \
  ntcp2_link::tests::authenticated_socket_budget_cancellation_releases_queue_across_restart \
  -- --exact --test-threads=1
```

The historical failure is real: on candidate `7b9f05f` two macOS CI runs failed
the quality test step at `restart frame receive deadline: Elapsed(())`. On
candidate `a10654c8cbd82cd7fc37dd7cf7d517a6d18b2657`, the full macOS quality
test step passed. Source review found no proven governor refill, waiter wakeup,
or cancellation defect, and the test body is unchanged between those
candidates. No runtime or test change was justified from the available
evidence. This is one successful macOS rerun, not proof that the intermittent
failure has a source-rooted correction.

The latest CI on candidate `a10654c8` passed Ubuntu quality, macOS quality,
MSRV, and dependency policy. The native Linux aarch64 secured-apphost job failed
in `persisted_autostarts_reach_sam_and_i2cp_again_after_manager_restart`: the
persistent-catalog assertion timed out while waiting for all signed app
transcripts to complete. This is separate from the NTCP2 governor test and
remains an exact-head acceptance failure to investigate or reproduce. The
current implementation branch has additional Plan 470/471 commits and Plan
472 changes, so those results do not qualify the eventual final SHA.

## Stacked pull-request audit

GitHub reports the following open chain, oldest to newest:

| PR | Base | Head SHA | State |
| --- | --- | --- | --- |
| 37 | `main` | `1ceab5fcae76ab293bfdf7e9c718c6d9d6436075` | Draft, merge state unstable |
| 38 | PR 37 branch | `40510d4b882662d601a46b65eafc2a172978f6f0` | Draft, clean |
| 39 | PR 38 branch | `a15a63a1082d7e8646830961723e56f72df091cb` | Draft, clean |
| 40 | PR 39 branch | `58c364b6c1863f05747240de983b266380be0923` | Not draft, unstable |
| 41 | PR 40 branch | `a3173774b9037266eece08d84ec63dc0edd1f3d8` | Not draft, unstable |
| 42 | PR 41 branch | `2cc7594273898607d6e25c3b19bca5e6b48fb587` | Not draft, clean |
| 43 | PR 42 branch | `7b9f05fda11dc5ae990f2059a7a42ecaa9c1313f` | Not draft, unstable |
| 44 | PR 43 branch | `a10654c8cbd82cd7fc37dd7cf7d517a6d18b2657` | Current work branch |

None of PRs 37–44 has a GitHub review decision or recorded review. The current
`main` SHA is `51f7426080906044b2bb99934c0336489b6b6ecd`; its merge base with the
PR 43 branch is `8fd5eb10824909ddebbd99c8b3800dac71eae211`, and `main` has one
additional commit (`chore(deps): bump actions/upload-artifact...`). It must be
preserved when PR 37 is reviewed and reconciled. The finite integration order
is PR 37 through PR 44, updating each dependent base to the actual accepted
head and checking for duplicate plan/registry changes after every merge. The
open review state and unstable checks prevent autonomous integration; no
branch was retargeted, force-updated, or merged.

## Requirement matrix

| Requirement | Result |
| --- | --- |
| Linux regression for the exact governor restart test | Passed locally, one exact run. |
| macOS regression and root cause | Historical failure reproduced by CI; a later full macOS test step passed. Root cause remains unproven; no source change. |
| Exact-head full CI | Candidate `a10654c8` passed Ubuntu/macOS quality, MSRV, and dependency policy, but failed native aarch64 secured-apphost qualification. Final worktree SHA is not yet qualified. |
| PR stack audit | Current refs, bases, draft/merge states, main-only commit, and review absence recorded. |
| Stack integration | Pending normal owner review and approval. No PR merged. |
| Public NTCP2/SSU2, default role, or support promotion | None. |

## Verification and disposition

No bandwidth-governor semantics, queue limits, timing tolerances, or CI
exclusions were changed. The Plan 472 offline runner adds a separate CI
self-test and does not execute live traffic. The full `AGENTS.md` routine floor
passed on the implementation worktree: 4,851 workspace tests passed, 39 were
ignored across 187 suites, and formatting, workspace check/build, Clippy,
rustdoc/doc tests, planning tests, all listed boundary/evidence guards, and
dependency policy passed. The Plan 472 runner and Plan 449 preflight self-tests
also passed. Final candidate CI, including macOS and native aarch64, remains
required after commit/push.

Disposition: **blocked before final exact-head qualification and approved PR
integration**. The next bounded actions are to reproduce the aarch64 persistent
catalog timeout on the exact final head, observe the new macOS exact test result,
and proceed through PR 37–44 only after normal review approval.
