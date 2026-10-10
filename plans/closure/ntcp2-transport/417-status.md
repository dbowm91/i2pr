# Plan 417 status: stopped — stock handshake markers were filtered at the default log level

Closure token: `stopped-stock-debug-milestones-not-enabled-at-log-source`.

Plan: `plans/implementation/ntcp2-transport/417-sanitized-stock-ntcp2-handshake-stage-observer.md`.
Implementation commit: `0a6a76a` — capture sanitized NTCP2 handshake milestones.

## Scope and disposition

Plan 417 extended the existing ephemeral-log observer with post-baseline,
allowlisted counts for SessionRequest, SessionCreated, SessionConfirmed, AEAD
failures, decrypted frames, I2NP blocks, and decoded DeliveryStatus messages.
The runner atomically records only those bounded counts and fixed outcomes; raw
logs, message IDs, Router Hashes, paths, process output, and log digests are not
retained. Tests cover baseline exclusion, malformed/oversized input, duplicate
and missing observations, stage counting, and evidence redaction.

All pre-attempt gates passed. The one permitted forward attempt failed with the
same helper/launcher results as Plan 415. Its post-baseline stage counts were
all zero. Source inspection then showed why: pinned i2pd `Log` initializes
`m_MinLevel` to `eLogInfo`, but the handshake milestone lines are emitted at
`eLogDebug`; the i2pr-owned helper starts the logger without calling
`SetLogLevel("debug")`. Therefore the observer had no debug milestones to
count. The attempt tree was cleaned and the first-failure rule prevented a
reverse attempt. This is a qualification-tool configuration issue; it provides
no protocol conclusion.

Plan 417 is stopped under its stated attempt budget. Plan 418 is registered to
enable the pinned logger's existing debug output in the private helper process,
guard that configuration, and then use its own fresh bounded attempt budget.
No i2pd source or binary was modified. Normal-daemon NTCP2 remains disabled and
non-advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Fixed-name handshake-stage parser | Passed. Observer counts only exact allowlisted stock log markers after a runner-captured byte offset. |
| Bounded, sanitized output | Passed. Evidence contains only stage counts and fixed outcome fields; no log digest, raw line, path, message ID, Router Hash, stdout, or stderr. |
| Negative controls | Passed. Observer self-test covered baseline exclusion, missing/duplicate/malformed/oversized input; runner self-test covered count retention and private-value exclusion; checker mutations covered marker removal and baseline bypass. |
| Pinned helper rebuild | Passed against pristine i2pd 2.61.0 revision `635b013a612ff47278ef02acf8580a28e10e26c5`. |
| Forward wire result | Rejected. Helper return code 66 (`control-listening-peer-connect-timeout`); launcher return code 2 (`receiver-frame-read-failed`); all observed handshake and I2NP counts were zero. |
| Reverse wire result | Not run because forward failed. |
| Cleanup | Passed (`cleanup_result=passed`); no `plan417-*` directory remained. |
| Protocol and product posture | No protocol success/failure inferred. No production code, default, bind, advertisement, dependency, or reference source changed. |

## Pin and artifact evidence

- i2pd revision: `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Source tree SHA-256: `dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98`.
- Helper source SHA-256: `3cdbc969bed546ecb3b3d4ab56a7f226b10052c382a7389fb9ec2e27e50b4c65`.
- Helper binary SHA-256: `cdb1a5325e20f9d53acb0fd44ed0fad24039b04f698a18dfc67f9f6b988f5783`.
- Sanitized attempt record: [`417-evidence.json`](417-evidence.json). No raw logs or private state were retained.

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk cargo build --locked -p i2pr-interop` — passed.
- `rtk bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd` — passed; rebuilt pinned helper.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `rtk bash scripts/check-ntcp2-vectors.sh` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as historical Plan 099 boundary check only.
- `rtk python3 scripts/check-tooling-inventory.py` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests; the synthetic fixture emitted its expected missing-workflows-directory notice.
- `rtk git diff --check` — passed.
- Forward invocation exited 2 and wrote sanitized evidence; reverse was not invoked:

  ```text
  rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --launcher target/debug/i2pr-interop --driver target/interop/plan410-i2pd/i2pd-current-ntcp2-driver --build-manifest target/interop/plan410-i2pd/build-manifest.txt --observer tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --work-parent target/interop --evidence plans/closure/ntcp2-transport/417-evidence.json
  ```

- Full workspace routine floor — not run; only non-production interop tooling and planning changed.

## Findings and unblock audit

- **Medium — helper debug-stage logging was not enabled.** The observer correctly reported zero post-baseline debug milestones because the helper never raised the pinned logger's default `info` threshold to `debug`. Plan 418 owns this private-helper configuration correction. No protocol failure is inferred.
- Plan 434 remains blocked pending authoritative two-way authenticated I2NP evidence.
- Plan 433 remains blocked because Plan 431 stopped without an authorized independent non-loopback topology; Plan 432 passed.
- Plan 435 remains blocked on 433/434; Plan 436 on 433; Plan 437 on 433/436; Plan 438 on 437; Plan 439 on 433/435/436/438.
- Proposal 170 Plans 412/413 remain blocked on Plan 437. No other work line became dependency-ready.
- Plan 418 is registered as the bounded helper-log configuration corrective. No blocked successor was silently unblocked.

