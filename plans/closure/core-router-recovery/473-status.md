# Plan 473 status: blocked — stacked PR review and integration pending

Closure token: `blocked-stack-review-and-owner-approval-pending`

Plan: [`473-macos-ntcp2-governor-and-recovery-stack-integration.md`](../../implementation/core-router-recovery/473-macos-ntcp2-governor-and-recovery-stack-integration.md)

## Outcome

The reported paused-clock restart test passes locally on Linux:

```text
cargo test --locked -p i2pr-runtime \
  ntcp2_link::tests::authenticated_socket_budget_cancellation_releases_queue_across_restart \
  -- --exact --test-threads=1
```

The historical failure is real: on candidate `7b9f05f` two macOS CI runs failed
the quality test step at `restart frame receive deadline: Elapsed(())`. A
later exact-head CI run on `4005bfa` reproduced that same test failure on
macOS, while the simultaneous push-triggered macOS run passed. This confirms
the intermittent failure belongs to this test.

Source review localized the missing test event: `send_blocks` returns after
bounded queue admission, but the test immediately started a paused-clock
receive timeout without observing the supervised writer's socket write or the
reader's receipt. Tokio can advance paused time to the deadline while that
queued work has not yet been polled. The test now uses bounded scheduler yields
to wait for both existing owner-tracked counters (`written_frames` and
`received_frames`) before starting the receive assertion. It adds no sleep,
changes no governor or runtime semantics, and retains the receive timeout.
After this correction, the exact test passed 20/20 consecutive Linux runs and
the complete `i2pr-runtime --all-targets` suite passed (135 passed, 1 ignored).
On final SHA `2dd16c3e3f954132f2d7c626bb58b4ee34f42f57`, full macOS and Ubuntu
quality jobs passed, including the corrected test on macOS. The exact
PR-triggered run's first attempt had one unrelated aarch64 managed-app
qualification failure: the persistent-catalog test recorded a SAM fixture
`managed-app I/O failed` transcript. The same-SHA push-triggered aarch64 job
passed, and rerunning only the failed PR job passed; the failure did not
reproduce.

On exact SHA `4005bfa`, both aarch64 secured-apphost jobs passed. MSRV and
dependency policy passed in both workflows. Ubuntu quality failed once in an
unrelated I2PControl tunnel manager test, then passed in the duplicate PR run;
the failure was `manager configuration rejected` and the focused test passed
six local attempts. The PR-triggered macOS quality job failed the NTCP2 restart
test while the push-triggered macOS job passed.

On final SHA `2dd16c3`, push-triggered CI run `38317798130` passed all jobs.
PR-triggered run `38317801177` attempt 1 passed Ubuntu/macOS quality, MSRV, and
dependency policy but failed the aarch64 secured-apphost qualification at the
persistent-catalog SAM fixture transport step. Attempt 2 reran the failed job
and passed; the PR check rollup is green, and GitGuardian security checks passed.

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
| 44 | PR 43 branch | `2dd16c3e3f954132f2d7c626bb58b4ee34f42f57` | Current branch head; exact-head CI green after failed-job rerun |

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
| Linux regression for the exact governor restart test | Passed 20/20 consecutive exact runs after adding the writer/reader event barrier; full runtime all-targets passed, 135 passed and 1 ignored. |
| macOS regression and root cause | CI reproduced the exact failure on `4005bfa`; localized to missing writer/reader progress synchronization in the paused-clock test and corrected with owner-counter event waits. The corrected test passed on macOS at final SHA `2dd16c3`. |
| Exact-head full CI | On `2dd16c3`, push run `38317798130` passed all jobs. PR run `38317801177` attempt 1 had one non-reproducible aarch64 persistent-catalog SAM fixture transport failure; failed-job attempt 2 passed. Ubuntu/macOS quality, MSRV, dependency policy, native aarch64, and GitGuardian are green. |
| PR stack audit | Current refs, bases, draft/merge states, main-only commit, and review absence recorded. |
| Stack integration | Pending normal owner review and approval. No PR merged. |
| Public NTCP2/SSU2, default role, or support promotion | None. |

## Verification and disposition

No bandwidth-governor semantics, queue limits, timing tolerances, or CI
exclusions were changed. The test now waits for actual write/receive progress
before its paused-clock timeout. The Plan 472 offline runner adds a separate CI
self-test and does not execute live traffic. The full `AGENTS.md` routine floor
passed on the implementation worktree before the test synchronization change: 4,851 workspace tests passed, 39 were
ignored across 187 suites, and formatting, workspace check/build, Clippy,
rustdoc/doc tests, planning tests, all listed boundary/evidence guards, and
dependency policy passed. The Plan 472 runner and Plan 449 preflight self-tests
also passed. After the synchronization change, the exact Linux test passed 20
times and runtime all-targets passed. On exact SHA `2dd16c3`, full push CI
passed; PR CI passed after rerunning the sole failed aarch64 job, and
GitGuardian passed. The one-run SAM fixture transport failure is recorded above
and was not reproduced by the same-SHA rerun.

Disposition: **blocked only before approved stacked-PR integration**. Code,
local regressions and exact-head CI are complete. PRs 37–44 have no
recorded reviews; do not merge or retarget the unreviewed stack. Continue the
audited PR 37-to-44 order only after normal owner review and approval.
