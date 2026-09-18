# Plan 215 — hosted Plan 214 tunnel-config generation corrective and exact-head re-verification

Status: **registered-executable narrow corrective**.

Plan of record for the hosted Plan 214 failure observed on source head
`b08d4977dbd43605f47bdea01ef8bd142f8b1c7f`.

This plan is intentionally narrow. It corrects the shell/config-generation defect
that prevented the hosted Plan 214 lane from reaching either counted HTTP or IRC
application test. It MUST NOT reopen the router-backed service-Destination,
Streaming, SSU2, NetDB, HTTP-profile, or IRC-profile implementation unless a
post-corrective counted run produces independent evidence that one of those
layers is actually failing.

## 1. Why this plan exists

Plan 214 landed a black-box product driver and passed locally, but the first two
hosted `full` service-tunnels workflow attempts failed before the application
matrix could run.

Authoritative failed hosted attempts:

- workflow run `35245848091`
- workflow run `35245869000`
- exact source SHA for both:
  `b08d4977dbd43605f47bdea01ef8bd142f8b1c7f`

Both runs established the same important boundary:

1. exact-pinned i2pd and jaraco/irc acquisition succeeded;
2. the service-tunnel static evidence checker succeeded;
3. Plan 213 generic router-backed Direction A+B succeeded;
4. the retained local M10 rows succeeded up to the delegated remote lane;
5. Plan 214 fixture startup and reference-router startup succeeded;
6. the Plan 214 HTTP/IRC server destination files were never produced;
7. the lane therefore stopped before either counted HTTP or IRC application
   operation;
8. the terminal Plan 214 classification was
   `P214-C-public-destination-extraction`.

The source cause is in
`tests/integration/service-tunnels/run-plan214-applications.sh`.
The runner currently generates `${I2PD_HOME}/tunnels.conf` with an **unquoted
heredoc**:

```bash
cat > "${I2PD_HOME}/tunnels.conf" <<EOF
...
EOF
```

The heredoc body includes explanatory prose containing shell command-substitution
syntax, notably Markdown backticks such as:

```text
`type = server`
`type = irc`
```

Backticks are active command substitution inside an unquoted heredoc. Therefore
what was intended to be inert configuration commentary was interpreted by the
shell while constructing `tunnels.conf`. The hosted logs contain the resulting
shell-command errors. i2pd subsequently ran without the intended valid server
configuration and never generated the HTTP/IRC destination `.dat` files.

This is a qualification-runner defect. The two hosted attempts did **not** reach
the counted application boundary and therefore do not demonstrate a product
regression.

## 2. Current authority before execution

The implementation must preserve this authority until the hosted re-verification
requirements in this plan are actually satisfied:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = local-pass-proven-hosted-requalification-blocked-by-plan215
plan_215 = registered-executable

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = locally-passed-only-hosted-proof-pending
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Do not reinterpret the hosted `P214-C` failures as a product failure. Do not
promote M10 on the basis of the prior local pass alone.

## 3. Scope lock

### 3.1 In scope

This plan may change only what is required to make the Plan 214 hosted
qualification runner deterministic and shell-safe, plus the evidence checker and
status/authority records required to prove the correction.

Expected implementation files are primarily:

- `tests/integration/service-tunnels/run-plan214-applications.sh`
- `scripts/check-service-tunnel-acceptance-evidence.sh`
- focused test/checker fixtures only if strictly necessary
- `plans/closure/service-tunnels/214-status.md`
- `plans/closure/service-tunnels/215-status.md`
- final authority documentation after external evidence exists

### 3.2 Out of scope without new evidence

Do **not** modify any of the following merely to make the hosted lane green:

- `ServiceProduct` router composition
- `ServiceTunnelManager` routing semantics
- router-backed per-service Destination material
- tunnel build protocol or `DestinationOutboundRole`
- inbound tunnel ownership
- NetDB lookup/publication logic
- Garlic/ECIES dispatch
- `StreamingManager` or `StreamingDestinationAdapter`
- SSU2 protocol/runtime
- HTTP proxy semantics
- IRC client semantics
- fixture semantics that weaken an assertion
- i2pd source or jaraco/irc source
- public-I2P/reseed policy

If the corrected runner reaches one of these boundaries and fails, stop and use
Plan 214's existing evidence/classification surface to attribute the failure
before making product changes.

## 4. Corrective design requirement

The generated i2pd tunnel configuration must not execute shell syntax contained
in explanatory prose.

The preferred implementation is to remove prose from the generated configuration
entirely and generate the small `tunnels.conf` deterministically with `printf`
or an equivalently shell-inert helper.

Recommended shape:

```bash
write_plan214_tunnels_conf() {
  local path="$1"
  local http_port="$2"
  local irc_port="$3"

  {
    printf '%s\n' \
      '[HTTP-Server]' \
      'type = http' \
      'host = 127.0.0.1'
    printf 'port = %s\n' "${http_port}"
    printf '%s\n' \
      'keys = plan214-http-server.dat' \
      'inbound.length = 0' \
      'outbound.length = 0' \
      '' \
      '[IRC-Server]' \
      'type = server' \
      'host = 127.0.0.1'
    printf 'port = %s\n' "${irc_port}"
    printf '%s\n' \
      'keys = plan214-irc-server.dat' \
      'inbound.length = 0' \
      'outbound.length = 0'
  } > "${path}"
}
```

The exact helper name/format is not mandatory. The properties are mandatory:

- no unquoted heredoc is allowed for the Plan 214 `tunnels.conf` block;
- fixed configuration text is shell-inert;
- only the intended dynamic values are interpolated;
- HTTP retains `type = http`;
- IRC retains transparent `type = server`;
- fixture ports are supplied from the current run's actual `HTTP_TARGET` and
  `IRC_TARGET`;
- key filenames remain the expected Plan 214 filenames;
- zero-hop `inbound.length = 0` / `outbound.length = 0` semantics remain
  unchanged;
- no new dependency is introduced.

The existing rationale for transparent IRC `type = server` belongs in ordinary
shell comments immediately above the config-writer helper/call, not inside text
that is interpreted while generating the config.

### 4.1 Alternative implementation

A quoted heredoc may be used only if dynamic fields are injected through a
separate explicit mechanism that cannot execute arbitrary template prose.
Avoid introducing `eval`, `envsubst`, ad-hoc `sed` programs, temporary template
languages, or another scripting dependency merely to preserve a tiny config
file. The correction should reduce, not increase, harness complexity.

## 5. Add a pre-launch generated-config sanity gate

Do not rely on eventual `.dat` absence to discover malformed configuration.
Immediately after writing `tunnels.conf` and before starting i2pd, validate the
small public configuration surface.

A helper such as `validate_plan214_tunnels_conf` should fail closed unless all of
the following are true:

1. the file exists and is non-empty;
2. exactly one `[HTTP-Server]` section exists;
3. exactly one `[IRC-Server]` section exists;
4. the HTTP section contains `type = http`;
5. the IRC section contains `type = server`;
6. the configured HTTP port equals `${HTTP_TARGET}`;
7. the configured IRC port equals `${IRC_TARGET}`;
8. the HTTP key file is `plan214-http-server.dat`;
9. the IRC key file is `plan214-irc-server.dat`;
10. both profiles retain zero-hop inbound/outbound lengths;
11. no unintended third tunnel section exists;
12. there is no unresolved template token/place-holder in the generated file.

Use simple POSIX/GNU tools already present in the runner environment. Do not
build a general INI parser.

Record a sanitized command-derived row such as:

```text
plan214-reference-tunnel-config-sanity = passed
```

The row may report only bounded public facts such as section count, profile
names, and configured port equality. Do not upload private key bytes or raw
application payloads.

If this validation fails, stop before launching i2pd. The failure belongs to the
reference/config startup boundary, not to destination extraction. Reuse the
existing fail-closed Plan 214 classification model; do not manufacture a passed
application row.

## 6. Add a static regression guard for shell-active config generation

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` with a narrow Plan
215 source invariant.

At minimum the checker must prove:

1. the Plan 214 runner no longer writes `tunnels.conf` using the vulnerable
   unquoted `<<EOF` form;
2. the deterministic config-writer/helper is present;
3. the pre-launch sanity validation is present and called before the i2pd
   `setsid` launch;
4. the generated IRC profile remains `type = server`;
5. the HTTP and IRC fixture ports remain dynamic and distinct inputs;
6. the expected key filenames remain present;
7. the runner records the config-sanity evidence row;
8. no `eval` was introduced into the runner.

Prefer source-shape checks that are specific to this bug class. Do not turn the
checker into a general shell parser.

The checker should specifically reject a reintroduction of an unquoted
`tunnels.conf` heredoc even if the current comments happen not to contain
backticks. That prevents the same class of failure from returning when someone
later edits prose.

## 7. Shell syntax validation is necessary but not sufficient

Add:

```bash
bash -n tests/integration/service-tunnels/run-plan214-applications.sh
```

to the focused validation sequence.

However, `bash -n` alone is **not** accepted as proof of this corrective. The
original vulnerable heredoc is syntactically valid shell. The static source
invariant and an actual generated-config/full-lane execution are both required.

## 8. Preserve Plan 214 fail-closed destination extraction

Do not weaken the existing `.dat`/public-Destination checks to get past the
failure.

The corrected runner must still require:

- both HTTP and IRC destination files to appear;
- public destination extraction through the existing bounded helper;
- nonzero destinations;
- distinct HTTP and IRC destinations;
- hash/Base32 consistency;
- no private key material in evidence;
- LeaseSet2 publication/lookup readiness as already required by Plan 214.

If the `.dat` files remain absent after the config writer and sanity gate are
correct, preserve the failure. At that point inspect i2pd startup/config logs and
classify a new narrow issue rather than bypassing the gate.

## 9. Preserve the transparent IRC tunnel decision

The existing Plan 214 local investigation established why the external
transport endpoint uses an i2pd transparent server tunnel rather than i2pd's
IRC-transforming tunnel type.

This corrective MUST retain:

```text
[IRC-Server]
type = server
```

Do not revert to `type = irc` as a workaround for hosted behavior. That would
reintroduce the previously attributed upstream line/chunk transformation and
would change the application byte-stream semantics under test.

## 10. Preserve Plan 213 as the transport prerequisite

The hosted failures already demonstrated that Plan 213 remains green on the
same source head where Plan 214's shell bug appears.

The corrected `full` workflow must continue to run in this order:

```text
Plan 213 generic router-backed A+B
  -> retained/local M10 matrix
  -> Plan 214 HTTP remote application qualification
  -> Plan 214 IRC remote application qualification
  -> final evidence-integrity checker
```

Do not remove or skip Plan 213 to make the total run shorter.

## 11. Focused implementation tests

Before running the expensive external lane, execute at least:

```bash
bash -n tests/integration/service-tunnels/run-plan214-applications.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
cargo test --locked -p i2pr-daemon \
  --test service_tunnels_application_product_only_remote_qualification -- \
  --test-threads=1
```

If a small dedicated shell test is added, it should test only the generated
config contract, for example:

- HTTP target port X appears in HTTP section;
- IRC target port Y appears in IRC section;
- changing X/Y changes only the intended fields;
- IRC remains `type = server`;
- generated output contains no unresolved placeholders.

Do not create a broad new shell-testing framework for this patch.

## 12. Required source validation floor

On the corrective source tree, before hosted verification, run the normal
source floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
```

If an unrelated pre-existing warning appears, document it. Do not expand the
scope into unrelated cleanup unless it prevents the required run.

## 13. Required local re-verification

The patch changes the external harness, so the prior local Plan 214 pass is not
sufficient by itself for the new source SHA.

On the exact corrective SHA, require:

### 13.1 Retained local rows

```bash
bash tests/integration/service-tunnels/run-independent.sh --local-only
```

Expected: retained local matrix remains green.

### 13.2 Plan 213 generic prerequisite

Where the existing controlled i2pd environment is available:

```bash
bash tests/integration/service-tunnels/run-plan213-generic.sh
```

Expected: terminal `P213-N-passed`, with the existing Direction A+B evidence
unchanged.

### 13.3 Full delegated lane

```bash
bash tests/integration/service-tunnels/run-independent.sh
```

Expected on the corrective SHA:

- generated tunnel-config sanity row passes;
- both HTTP/IRC destination files appear;
- destination extraction passes;
- Plan 214 reaches the counted product driver;
- HTTP aggregate passes;
- IRC aggregate passes;
- exactly one terminal classification is emitted;
- terminal classification is `P214-N-passed`;
- no `remote-stop` appears;
- retained rows remain green;
- resource baseline remains clean.

One complete local exact-SHA `P214-N-passed` is required after the patch. A
second local pass is useful but the authoritative stability gate is the hosted
double-pass below.

## 14. Commit boundary before hosted verification

After the patch and local source/full-lane validation are green, create a single
corrective implementation commit (or a tightly bounded series ending in one
well-defined verification head).

Record that exact SHA in `plans/closure/service-tunnels/215-status.md` as:

```text
corrective_verification_sha = <sha>
hosted_pass_1 = pending
hosted_pass_2 = pending
```

No runtime, harness, fixture, checker, or workflow source may change between the
two hosted verification runs.

Documentation-only evidence closure after the two runs is allowed and is
handled in §17.

## 15. Hosted re-verification: two consecutive `full` passes

Trigger the existing hosted workflow with lane `full` **twice sequentially** on
the same exact corrective verification SHA.

Do not launch the two acceptance runs concurrently. Run #2 begins only after
run #1 has completed successfully. This removes ambiguity and makes
"consecutive pass" literal.

For each run independently require all of the following:

1. checkout/source head equals the corrective verification SHA;
2. exact i2pd 2.61.0 pin
   `635b013a612ff47278ef02acf8580a28e10e26c5` is verified clean;
3. exact jaraco/irc pin
   `90e10e690da2c7bf60de21be4e36d24c9ffd7474` is verified clean;
4. static service-tunnel evidence checker passes;
5. Plan 213 completes with `P213-N-passed`;
6. retained local M10 rows remain green;
7. Plan 214 config-sanity row passes;
8. both independent server destination `.dat` files are generated;
9. public-Destination extraction succeeds for both profiles;
10. HTTP and IRC destinations are nonzero and distinct;
11. HTTP remote aggregate passes from command + target + production-counter
    evidence;
12. IRC remote aggregate passes from command + target + production-counter
    evidence;
13. Plan 214 records exactly one terminal classification;
14. terminal classification is `P214-N-passed`;
15. no required Plan 214 row is failed or blocked;
16. resource baseline passes;
17. workflow conclusion is `success`;
18. `service-tunnels-external-evidence-<run-id>` artifact is uploaded;
19. `plan213-generic-evidence-<run-id>` artifact is uploaded;
20. `plan214-applications-evidence-<run-id>` artifact is uploaded and non-empty;
21. the Plan 214 artifact's `source-head.txt` equals the corrective SHA;
22. the final service-tunnel evidence-integrity check passes.

A workflow whose application step is skipped, whose Plan 214 artifact is
missing, or whose terminal classification is not `P214-N-passed` is not a pass.

## 16. Hosted failure/reset rule

The required result is **two consecutive successful hosted `full` runs on one
unchanged source SHA**.

Therefore:

- pass then fail = requirement not met;
- fail then pass = requirement not met until another pass follows;
- pass on SHA A then pass on SHA B = requirement not met;
- one pass only = requirement not met;
- cancelled/skipped run does not count as a pass;
- any patch after run #1 resets the count to zero.

If a hosted run fails after the config correction, use the terminal evidence to
attribute the new boundary. Do not keep rerunning indefinitely without
classification.

## 17. Final evidence/authority commit

Only after the same corrective SHA has two qualifying hosted passes:

1. update `plans/closure/service-tunnels/215-status.md` with:
   - corrective verification SHA;
   - both hosted workflow run IDs;
   - both conclusions;
   - both `P214-N-passed` terminal facts;
   - artifact names;
   - local exact-head result;
2. update `plans/closure/service-tunnels/214-status.md` to the passed final application authority;
3. update the current plan/roadmap authority documentation that still says the
   hosted double-pass is pending;
4. do **not** modify runtime/harness/checker/workflow source in this final
   evidence-only commit.

The evidence commit may be a documentation-only descendant of the tested
corrective SHA. The external qualification remains attributable to the exact
corrective SHA because the final commit changes only authority records. Run
routine CI/checkers on the documentation-only closure commit, but another full
external double-pass is not required unless executable qualification source is
changed.

## 18. Final authority transition after successful hosted double-pass

When and only when §15 is satisfied, the authority may become:

```text
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = passed-m10-product-only-remote-http-and-irc-application-closure
plan_215 = passed-hosted-plan214-tunnel-config-corrective-and-reverification

m10_local_rows = passed
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = passed-via-plan214-after-plan215
milestone10_remote_service_interop = passed-via-plan213-and-plan214-after-plan215
milestone10_final_acceptance = closed-via-plan214-after-plan215
```

This transition does not close the independent Java M6 second-family work. Do
not claim Java M6 cross-family closure as a side effect of M10 success.

## 19. Evidence that must be retained

Retain sanitized, public evidence sufficient to diagnose future regressions:

- corrective verification SHA;
- hosted workflow run IDs;
- Plan 213 terminal classification for each run;
- Plan 214 terminal classification for each run;
- Plan 214 aggregate HTTP/IRC rows;
- generated-config sanity row;
- public destination hashes/Base32 forms if already within the existing bounded
  evidence policy;
- exact pin facts;
- resource-baseline result;
- artifact names/digests where available.

Do not retain:

- private Destination key bytes;
- complete `.dat` private files;
- raw application payloads where a digest is sufficient;
- secret GitHub values;
- arbitrary runner environment dumps.

## 20. Stop conditions

Stop and write a new narrow classification/corrective rather than broadening
this plan if any of the following occurs:

1. the shell-safe config writer produces the expected validated config but i2pd
   still does not create either destination file;
2. Plan 213 stops passing on the corrective SHA;
3. public destination extraction fails despite both `.dat` files existing;
4. the HTTP leg reaches the product and fails with a new command/target/counter
   mismatch;
5. the IRC leg reaches the product and fails with a new
   registration/PING/PRIVMSG/ACTION/DCC/privacy/counter mismatch;
6. the only proposed solution requires patching i2pd or jaraco/irc;
7. the only proposed solution requires public I2P/reseed access;
8. a proposed fix weakens an application assertion;
9. a proposed fix reintroduces a shadow router/Streaming stack;
10. product code changes are proposed without post-corrective product evidence;
11. the two hosted passes require different executable source SHAs.

## 21. Explicit acceptance criteria

Plan 215 is complete only when **all** of the following are true:

1. `run-plan214-applications.sh` no longer uses the vulnerable unquoted
   `tunnels.conf` heredoc.
2. Explanatory Markdown/backtick prose is no longer interpreted during config
   generation.
3. The generated config remains minimal and deterministic.
4. HTTP server tunnel remains `type = http`.
5. IRC server tunnel remains transparent `type = server`.
6. Dynamic HTTP/IRC target ports are preserved exactly.
7. Existing Plan 214 key filenames are preserved.
8. Existing zero-hop tunnel lengths are preserved.
9. A pre-launch generated-config sanity check exists.
10. That sanity check is recorded as command-derived evidence.
11. The static checker rejects reintroduction of the vulnerable heredoc shape.
12. The static checker requires the pre-launch sanity gate.
13. No `eval` or new templating dependency is introduced.
14. `bash -n` passes for the runner.
15. Plan 214 focused unit floor passes.
16. Workspace source validation floor passes.
17. Retained local-only M10 lane remains green.
18. Plan 213 generic qualification remains green.
19. The corrected full local lane reaches destination extraction.
20. Both HTTP/IRC destination files are generated locally.
21. Both public destinations extract and validate locally.
22. Local HTTP aggregate passes.
23. Local IRC aggregate passes.
24. Local terminal Plan 214 classification is `P214-N-passed` on the corrective
    verification SHA.
25. Hosted `full` run #1 checks out that same corrective SHA.
26. Hosted run #1 Plan 213 is `P213-N-passed`.
27. Hosted run #1 Plan 214 is `P214-N-passed`.
28. Hosted run #1 uploads non-empty Plan 213, Plan 214, and service-tunnel
    evidence artifacts.
29. Hosted `full` run #2 is started only after run #1 succeeds.
30. Hosted run #2 checks out the identical corrective SHA.
31. Hosted run #2 Plan 213 is `P213-N-passed`.
32. Hosted run #2 Plan 214 is `P214-N-passed`.
33. Hosted run #2 uploads non-empty Plan 213, Plan 214, and service-tunnel
    evidence artifacts.
34. Both hosted workflow conclusions are `success`.
35. No executable qualification source changed between the hosted runs.
36. `plans/closure/service-tunnels/215-status.md` records both run IDs and the tested SHA.
37. Plan 214 is not promoted before criteria 25–35 are satisfied.
38. M10 final acceptance is not closed before criteria 25–35 are satisfied.
39. Final authority normalization is evidence-only and does not modify the
    executable qualification source.
40. Java M6 second-family authority remains independent and unchanged.

## 22. Recommended implementation sequence for handoff

Keep this work small enough for one implementation model/session.

### Commit A — runner correction + regression guard

- replace vulnerable Plan 214 `tunnels.conf` generation;
- move explanatory IRC rationale outside generated config;
- add pre-launch config sanity gate;
- add sanitized config-sanity evidence row;
- extend static checker with Plan 215 invariants;
- run focused shell/checker/unit tests.

### Commit B — exact-head local qualification

Prefer no source changes here. Run the full source floor and local Plan 213/214
qualification. If a source correction is required, amend/follow with a narrow
commit and restart exact-head local verification on the new SHA.

Once green, designate the resulting executable SHA as the
`corrective_verification_sha`.

### Hosted gate

- dispatch `full` once on `corrective_verification_sha`;
- require complete success + artifacts;
- only then dispatch `full` again on the same SHA;
- require complete success + artifacts.

### Commit C — evidence/authority normalization

Only after both hosted passes, update Plan 214/215 and current authority docs.
This commit is documentation/evidence only.

## 23. Handoff summary

The implementation model should treat the existing router/product code as
already qualified through Plan 213 and the prior local Plan 214 pass. The first
task is not to debug I2P protocol behavior. It is to make the tiny i2pd server
`tunnels.conf` writer shell-safe and self-validating, prove that the hosted lane
now reaches the intended application boundary, and then rerun the exact Plan 214
closure gate twice on one immutable source SHA.

Successful completion means the hosted environment independently reproduces the
existing local `P214-N-passed` result twice. Anything less remains pending and
must not close Milestone 10.
