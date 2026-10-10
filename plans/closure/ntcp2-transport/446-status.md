# Plan 446 status: blocked — normal i2pd control fixture did not establish isolation

Closure token: `blocked-normal-i2pd-control-fixture-datadir-isolation-not-established`

Plan: [`446-real-i2pd-daemon-control-and-ntcp2-requalification.md`](../../implementation/ntcp2-transport/446-real-i2pd-daemon-control-and-ntcp2-requalification.md)

## Outcome

The pinned i2pd 2.61.0 source and executable were available and matched the frozen
revision. The normal-daemon control did **not** reach an admissible NTCP2 attempt.
No authenticated session, decoded I2NP request, response, or i2pr wire attempt is
claimed. Plan 446 is blocked at safe, independent process/data-directory setup.

Source inspection found why the initial runner setup was unsafe: in pinned
`daemon/Daemon.cpp`, `Daemon_Singleton::init` selects and initializes the data
directory before it parses the config file. A `datadir` key in that config is
therefore too late to isolate a process. The first launch omitted the required
`--datadir` command-line argument. The two attempted processes were stopped, but
they may have used the account's default i2pd directory. That directory was not
inspected, modified, or cleaned by this work. No i2pd process remained after the
attempts. Because the process isolation guarantee failed, the attempt is not
usable as protocol or fixture evidence, and there was no follow-on i2pr attempt.

The next execution must use an explicit, unique `--datadir` for every normal
i2pd process, prove the directories are disjoint before launch, and demonstrate
that only the intended loopback listener is opened before any peer traffic. The
runner must stop before protocol execution if any of these checks fail. The
prior Plan 441/445 helper outcomes remain unchanged.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact pinned pristine reference | Pass for source identity: `635b013a612ff47278ef02acf8580a28e10e26c5`; tracked source was clean. The existing binary reported i2pd 2.61.0. |
| Real normal-daemon control with isolated identities and data | Blocked. The command-line data-directory isolation was omitted; no admissible process pair was established. |
| Authenticated stock-to-stock NTCP2 in both directions | Not run to an admissible protocol boundary; no session evidence. |
| Decoded I2NP request and response | Not run; no I2NP evidence. |
| Substitute i2pr only after passing stock control | Not run, correctly gated. |
| Keep normal/public NTCP2 disabled and preserve reference source | Preserved in tracked files; no i2pr production or reference source changes. |
| Owned-process cleanup | The launched processes were stopped and no i2pd process remained in the post-run process check. Temporary directories created by the probes were removed. |

## Commands and outcomes

Executed locally on Linux:

- `target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd --version` — passed; reported i2pd 2.61.0 (0.9.70), Boost 1.83.0, OpenSSL 3.0.13.
- `git -C target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 rev-parse HEAD` — passed; exact frozen pin.
- `git -C target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 status --porcelain --untracked-files=no` — passed; no tracked source changes.
- `target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd --help` — passed; confirms `--datadir` is a supported command-line option.
- A bounded two-process startup probe with distinct loopback ports and empty reseed URLs — **invalid fixture**; `--datadir` was omitted. One process exited before producing its configured log and one remained running briefly. No protocol result is inferred.
- A single-process startup probe with the same invalid data-directory setup — remained alive until the 8-second bounded check timed out; this confirms startup only and is not qualification evidence.
- A source-order probe of `daemon/Daemon.cpp` — confirms data-directory selection precedes config parsing.
- `ps -eo pid,ppid,comm,args | rg '[i]2pd'` — no matching process remained after cleanup.

The probes did not retain raw router logs or identity material in the repository.
The account's default i2pd directory was not inspected. No hosted CI result is
claimed.

## Security, compatibility, and findings

- **High — data-directory isolation was not established for the first probe.**
  The normal daemon may have selected the account's default i2pd data directory.
  The processes were stopped and no matching process remains, but this record
  cannot claim that the account directory was untouched. Do not repeat the
  control until the runner passes `--datadir` before startup and proves the
  effective directories are unique.
- No i2pr NTCP2 or normal-daemon configuration was changed. No reference source
  was patched. No support, reachability, RouterInfo, or capability claim is
  promoted.
- The attempted configuration disabled reseed URLs, but the data-directory
  defect means it is not used as proof that no existing peer state was loaded.

## Unblock audit and disposition

Plan 446 remains blocked. Plans 434 and 435 remain blocked on their independent
authenticated-link, I2NP, and product requirements; Plan 446 provides none of
those results. Plans 447–449 are independent registered work and are not
unblocked or blocked by this result. No historical status is rewritten.

Normal-daemon NTCP2 remains disabled and non-advertised under Plan 101. A new
attempt requires an isolated normal-process runner with an explicit command-line
data directory and source-backed proof of its egress and cleanup boundaries.
