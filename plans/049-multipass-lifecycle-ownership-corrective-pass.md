# Plan 049: Multipass lifecycle ownership and collision-recovery corrective pass

## Objective

Correct the Plan 048 Multipass lifecycle so a normal stale, interrupted, retained, deleted, or concurrently running instance cannot prevent the permissive guest evidence path from being exercised.

The current Plan 048 implementation reached the host-side orchestration boundary but did not launch or validate a fresh permissive guest because the fixed instance name already existed. The resulting blocker, `blocked_instance_name_collision`, is a valid fail-closed outcome, but it exposes an incomplete lifecycle contract rather than a limitation of the rootless evidence design.

The completed phase must provide a repeatable Multipass lifecycle in which:

- an authoritative evidence run uses a collision-resistant concrete instance name by default;
- the stable environment contract is separate from the ephemeral Multipass instance name;
- every managed instance has a cryptographically linked ownership record;
- interrupted runs can be inspected and resumed safely;
- compatible owned instances may be adopted only through an explicit operation;
- incompatible or unowned colliding instances are never deleted automatically;
- deleted-but-unpurged instances are recognized distinctly from active instances;
- a fresh run can always select a new safe name without mutating unrelated instances;
- host-side state is written before instance creation so ownership survives interruption;
- the guest contract is verified after launch or adoption rather than inferred from its name;
- the host `blocked_unprivileged_user_namespace` result remains a negative baseline and never blocks the Multipass guest path;
- the guest rootless probe is the only user-namespace capability result that controls guest scenario execution;
- no router process starts until the guest policy, ownership, source, cache, execution-user, and rootless-probe contracts all pass;
- sanitized environment and protocol evidence records identify the concrete instance and stable environment contract without retaining sensitive host details;
- cleanup and evidence export remain fail-closed;
- Plan 045, Plan 046, Plan 047, and Plan 048 protocol and evidence predicates are not weakened.

Plan 049 is an orchestration-correctness pass. It does not advertise NTCP2 support, does not reinterpret the host blocker as a protocol result, and does not close Milestone 3 by itself.

## Starting repository state

This plan starts from `main` commit:

```text
b6a034968e619293f1c978958871771e0a98988b
interop: add Multipass rootless evidence environment
```

The Plan 048 attempt recorded:

```text
Multipass outcome = blocked_instance_name_collision
host probe outcome = blocked_unprivileged_user_namespace
external evidence closure = unclaimed
```

The host probe is expected for the current `host.apparmor-restrict-on` baseline. The instance collision prevented evaluation of the Plan 048 recovery guest and therefore prevented any guest probe or mixed-router direction from running.

Relevant files include:

- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `plans/046-rootless-sealed-namespace-evidence-lane.md`
- `plans/046-closure.md`
- `plans/047-cross-host-rootless-lane-expansion.md`
- `plans/048-multipass-permissive-rootless-evidence-environment.md`
- `plans/048-status.md`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`
- `docs/adr/0018-multipass-rootless-interop-environment.md`
- `scripts/interop/multipass/environment.toml`
- `scripts/interop/multipass/cloud-init.yaml`
- `scripts/interop/multipass/run-evidence-lane.sh`
- `scripts/interop/multipass/create.sh`
- `scripts/interop/multipass/status.sh`
- `scripts/interop/multipass/prepare.sh`
- `scripts/interop/multipass/probe.sh`
- `scripts/interop/multipass/run-matrix.sh`
- `scripts/interop/multipass/export-evidence.sh`
- `scripts/interop/multipass/destroy.sh`
- `scripts/interop/multipass/snapshot.sh`
- `scripts/interop/multipass/restore.sh`
- `tests/integration/ntcp2/harness/test_multipass.py`

The implementing agent must inventory the exact checked-in Plan 048 script names before editing. Where this plan uses an illustrative name that differs from the repository, update the existing canonical implementation rather than creating parallel lifecycle tools.

## Controlling documents

Read and preserve the boundaries in:

- `plans/000-mvp-roadmap.md`
- `plans/030-milestone-3-overview.md`
- `plans/043-ubuntu-build-system-interop-gates.md`
- `plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`
- `plans/046-rootless-sealed-namespace-evidence-lane.md`
- `plans/046-closure.md`
- `plans/047-cross-host-rootless-lane-expansion.md`
- `plans/048-multipass-permissive-rootless-evidence-environment.md`
- `plans/048-status.md`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`
- `docs/adr/0018-multipass-rootless-interop-environment.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`

Where code, scripts, tests, and documentation disagree, fail closed and reconcile them in the same corrective pass.

## Non-negotiable boundaries

1. Do not alter the target host's AppArmor or user-namespace policy.
2. The host `blocked_unprivileged_user_namespace` result remains a negative baseline only.
3. The Multipass guest probe controls whether guest rootless execution may proceed.
4. Do not delete, purge, stop, suspend, restore, snapshot, or mutate an instance unless ownership is proven.
5. Do not infer ownership from the instance name alone.
6. Do not silently adopt an existing instance.
7. Do not silently recreate or destroy an existing instance.
8. Do not use arbitrary shell fragments, `eval`, or user-controlled Multipass command options.
9. Do not use host mounts as authoritative source, cache, state, or evidence inputs.
10. Do not weaken source archive, cache manifest, offline execution, evidence sanitation, or cleanup requirements.
11. The guest evidence lane still runs as `i2ptest` without sudo, ambient capabilities, file capabilities, privileged groups, or setuid helpers.
12. Do not introduce automatic fallback to the privileged dual-netns topology.
13. Cleanup failure overrides protocol success.
14. Instance collision, ownership mismatch, contract mismatch, and stale-state ambiguity are blockers, never skipped success.
15. A successful VM lifecycle remains environment evidence until all four mixed-router records pass existing interoperability validation.

## Defect statement

Plan 048 currently treats the canonical instance name as both:

- the stable environment identity; and
- the concrete global Multipass resource name.

Those are different concepts.

A stable environment identity describes the reviewed contract:

```text
environment schema
Ubuntu image
CPU/memory/disk
cloud-init digest
guest policy
execution user
source revision
reference-cache manifest
workflow version
```

A concrete Multipass name identifies one ephemeral realization of that contract on one host.

Using one fixed global name and failing on any pre-existing instance makes the supposedly repeatable lane fragile under ordinary states such as interrupted execution, retained debugging instances, deleted-but-unpurged resources, concurrent agents, or previous test campaigns.

Plan 049 separates these identities and introduces explicit ownership and recovery semantics.

## Architectural decision

Define three distinct identifiers.

### Stable environment ID

A reviewed, versioned identifier for the environment contract:

```text
i2pr-plan048-rootless-v1
```

This value changes only when the reviewed environment contract changes incompatibly.

### Run ID

A safe operator-supplied or generated execution identifier:

```text
plan049-<date-or-sequence>-<random-suffix>
```

Requirements:

- lowercase ASCII letters, digits, and hyphens only;
- 8 to 48 characters;
- no path separators, whitespace, shell metacharacters, or leading hyphen;
- uniqueness checked against host state and existing evidence directories.

### Concrete instance name

Derived from a bounded prefix and the run ID:

```text
i2pr-interop-<run-id>
```

The derivation must respect Multipass name limits. If truncation is required, append a digest suffix so distinct run IDs cannot collapse to the same instance name.

Example:

```text
i2pr-interop-plan049-a7d31c
```

The stable environment ID, run ID, and concrete instance name must be recorded separately.

## Deliverable 1: Define a canonical lifecycle state schema

Create a versioned host-side lifecycle record under a non-sensitive repository-local state root, for example:

```text
target/interop/multipass/state/<run-id>/lifecycle.json
```

The record must be created atomically before `multipass launch` is attempted.

Required fields:

```text
schema_version
environment_id
run_id
instance_name
state
source_commit
source_archive_sha256
environment_manifest_sha256
cloud_init_sha256
reference_cache_manifest_sha256 or typed pending value
owner_token_sha256
created_at_utc
updated_at_utc
last_operation
last_typed_outcome
host_multipass_version
```

Allowed lifecycle states must be explicit, for example:

```text
reserved
launching
provisioning
provisioned
source_ready
cache_ready
source_and_cache_ready
probe_passed
offline_ready
running
exporting
exported
stopped
destroying
destroyed
blocked
abandoned
```

State transitions must be validated. Unknown, backward, or impossible transitions fail closed.

Do not retain:

- host username;
- home-directory path;
- SSH private material;
- raw Multipass authentication material;
- raw RouterInfo or router state;
- private reference-cache contents.

## Deliverable 2: Add an ownership token and guest ownership record

Generate a cryptographically random ownership token before instance launch.

Store only its SHA-256 in the host lifecycle record. Transfer the full token to the guest during provisioning through a root-owned file such as:

```text
/var/lib/i2pr-interop/ownership-token
```

The guest must also contain a root-owned contract record:

```text
/var/lib/i2pr-interop/environment.json
```

Required guest fields:

```text
schema_version
environment_id
run_id
instance_name
source_commit_expected
environment_manifest_sha256
cloud_init_sha256
owner_token_sha256
```

Permissions:

```text
root:root
0600 for token
0644 or stricter for non-secret contract record
```

The `i2ptest` user must not be able to modify either file.

Ownership proof requires all of:

- host state record exists;
- instance name matches;
- environment ID matches;
- run ID matches;
- guest ownership-token hash matches host state;
- cloud-init/environment digest matches;
- guest contract file is root-owned and not writable by `i2ptest`;
- source/cache contract matches the requested operation phase.

A name match without this proof is unowned.

## Deliverable 3: Implement collision-resistant default naming

Change the default `--all` behavior so it generates a fresh safe run ID and concrete instance name when none is supplied.

The default path must not use the legacy fixed name.

Support explicit:

```text
--run-id <safe-id>
--instance-name <safe-name>
```

An explicit instance name is allowed only when:

- it passes the name validator;
- it does not target `primary` or another reserved name;
- it is linked to the supplied run ID;
- ownership/adoption rules are satisfied.

Generated names must be checked against:

- active Multipass instances;
- stopped/suspended Multipass instances;
- deleted-but-unpurged instances if visible through the installed Multipass version;
- local lifecycle state directories;
- existing exported evidence run IDs.

On collision, generate another bounded suffix rather than failing immediately, up to a fixed maximum such as 16 attempts.

If uniqueness cannot be established, emit:

```text
blocked_instance_name_allocation_exhausted
```

## Deliverable 4: Add authoritative Multipass state discovery

Implement one parser for Multipass instance state. Prefer structured output such as:

```text
multipass list --format json
multipass info <instance> --format json
```

Do not parse human-formatted columns when structured output is available.

Normalize Multipass states into the repository taxonomy:

```text
absent
running
stopped
suspended
starting
restarting
delayed-shutdown
unknown
deleted-unpurged
```

The parser must tolerate additional unrelated instances without exposing their details in retained evidence.

Unknown state values fail closed with:

```text
blocked_unknown_multipass_instance_state
```

Add test fixtures for multiple Multipass versions where output differs.

## Deliverable 5: Define collision outcomes precisely

Replace the generic `blocked_instance_name_collision` result with typed outcomes.

Required outcomes:

```text
blocked_instance_name_owned_by_other_workflow
blocked_existing_instance_contract_mismatch
blocked_existing_instance_state_ambiguous
blocked_deleted_instance_requires_purge
blocked_host_state_without_instance
blocked_instance_without_host_state
blocked_ownership_token_mismatch
blocked_instance_name_allocation_exhausted
```

Retain `blocked_instance_name_collision` only as a compatibility alias if existing evidence validators require it. New records must use the precise taxonomy.

Every blocker must include a sanitized remediation class, not a raw command containing private paths.

Examples:

```text
remediation = choose-new-run-id
remediation = inspect-owned-instance
remediation = explicit-recreate-required
remediation = operator-purge-required
remediation = ownership-reconciliation-required
```

## Deliverable 6: Add explicit adoption semantics

Add an explicit operation:

```text
--adopt-owned
```

Adoption is permitted only when ownership proof succeeds and the requested environment contract matches the guest.

Adoption must verify:

- exact environment ID;
- exact environment manifest digest;
- exact cloud-init digest;
- exact source commit when source preparation is complete;
- exact cache manifest digest when cache preparation is complete;
- expected guest sysctls;
- expected `i2ptest` user and groups;
- zero effective/permitted/inheritable/ambient capabilities for `i2ptest`;
- no unexpected Multipass mounts;
- only allowlisted snapshots;
- no active router processes;
- no stale run directory that would contaminate evidence;
- parent host lifecycle state agrees with guest phase.

Do not adopt an instance merely because it is stopped or has the expected name.

An adopted instance must rerun status and rootless probe checks before any router process.

## Deliverable 7: Add explicit recreate semantics

Add:

```text
--recreate-owned
```

Recreation may destroy and purge only an instance whose ownership proof succeeds.

Before destruction:

- export any already validated sanitized evidence;
- reject recreation if unexported passing evidence exists;
- record the transition to `destroying`;
- verify no unrelated mounts or snapshots are attached;
- stop the owned instance if required;
- delete it;
- purge only the owned deleted instance where the installed Multipass interface permits safe attribution;
- verify absence;
- launch a fresh instance under the same run ID or a newly derived generation suffix;
- update the lifecycle record atomically.

Never use a global purge operation when it could affect unrelated deleted instances.

If selective purge is unavailable, emit:

```text
blocked_deleted_instance_requires_operator_purge
```

and do not run global `multipass purge` automatically.

## Deliverable 8: Add safe resume semantics

Add:

```text
--resume-owned
```

Resume uses the lifecycle state machine and guest contract to continue from the earliest safe incomplete phase.

Examples:

- `reserved` with no instance: continue launch;
- `launching` with matching running instance: verify ownership and provisioning;
- `provisioned`: transfer source;
- `source_ready`: transfer cache;
- `source_and_cache_ready`: run guest probe;
- `probe_passed`: apply or verify offline policy;
- `offline_ready`: run matrix;
- `exporting`: revalidate and retry export;
- `exported`: no-op success unless explicit destroy requested.

Never resume from:

- ownership mismatch;
- unknown guest contract;
- source/cache digest mismatch;
- unexpected router process;
- dirty scenario state without cleanup proof;
- failed cleanup;
- unknown Multipass state.

These must become typed blockers requiring explicit recreation or operator review.

## Deliverable 9: Separate host baseline probing from guest gating

The top-level orchestrator must record two independent results:

```text
host_baseline_probe
guest_rootless_probe
```

The host baseline result may be:

```text
blocked_unprivileged_user_namespace
rootless_sandbox_available
other typed host outcome
```

It is informational for the Multipass lane and must not gate guest launch.

The guest result must be:

```text
rootless_sandbox_available
```

before any router process starts.

Add a regression test proving:

```text
host baseline = blocked_unprivileged_user_namespace
guest probe = rootless_sandbox_available
result = guest lane may proceed
```

Add the inverse test proving a permissive host does not allow execution when the guest probe fails.

## Deliverable 10: Move guest probing earlier

After guest provisioning and ownership verification, run the guest rootless probe before expensive source/cache transfer where practical.

Recommended order:

```text
reserve host state
allocate instance name
launch guest
verify cloud-init complete
verify ownership contract
verify guest sysctls and execution user
run guest rootless probe
transfer exact source
transfer and verify cache
snapshot source-and-cache-ready
apply offline policy
rerun guest probe under final execution context
run four directions
validate/export evidence
```

The final pre-router probe remains mandatory even if the early capability probe passed.

This reduces wasted preparation when the guest kernel or policy is incompatible.

## Deliverable 11: Add lifecycle locking and concurrency control

Add a host-side lock scoped to the run ID and concrete instance name.

Requirements:

- use an atomic filesystem lock or `flock` where supported;
- include PID/start metadata only in transient lock state, not retained evidence;
- detect stale locks safely;
- never remove an active lock owned by another process;
- serialize state-file updates;
- prevent two agents from adopting or destroying the same instance;
- allow unrelated run IDs to proceed concurrently when host resources permit.

Typed blockers:

```text
blocked_lifecycle_lock_held
blocked_stale_lock_ambiguous
```

A stale lock may be recovered only when the owning process is provably absent and the state/instance ownership contract remains consistent.

## Deliverable 12: Add generation tracking

One run ID may have multiple instance generations after explicit recreation.

Record:

```text
generation = 1, 2, 3, ...
```

The concrete instance name may include the generation suffix:

```text
i2pr-interop-<run-id>-g2
```

Evidence must identify the generation that produced it. A passing evidence bundle must not combine records from different generations unless the manifest explicitly treats them as separate campaigns.

Snapshots must be generation-scoped and never restored across environment-contract changes.

## Deliverable 13: Reconcile snapshot ownership

Only allow the reviewed snapshots:

```text
provisioned
source-and-cache-ready
```

Internally bind snapshot ownership to:

- instance generation;
- environment manifest digest;
- cloud-init digest;
- source commit where applicable;
- cache digest where applicable.

Before restore:

- verify instance ownership;
- verify snapshot is allowlisted;
- verify snapshot contract metadata;
- verify no secret-bearing scenario state was captured;
- restore;
- rerun guest ownership, policy, process, and rootless-probe checks.

Unknown snapshots block adoption and recreation until explicitly resolved.

## Deliverable 14: Correct `--all` lifecycle behavior

The one-command path must become interruption-safe and collision-safe.

Expected behavior:

```text
run-evidence-lane.sh --all
  -> generate safe run ID
  -> reserve lifecycle state
  -> allocate unique instance name
  -> launch and provision
  -> verify ownership and guest contract
  -> run guest capability probe
  -> prepare exact source/cache
  -> enforce offline execution
  -> rerun guest probe
  -> run four directions
  -> validate/export sanitized evidence
  -> optionally destroy owned instance
```

Support:

```text
--run-id <id>
--resume-owned
--adopt-owned
--recreate-owned
--destroy-owned
--destroy-after-export
--keep-on-blocker
```

`--keep-on-blocker` may retain an owned instance for debugging but must:

- mark state `blocked`;
- stop router processes;
- remove secret-bearing run state or mark the instance non-snapshotable;
- print the safe resume/recreate identifier;
- never export raw diagnostics.

## Deliverable 15: Add an inspection command

Add a read-only operation such as:

```text
run-evidence-lane.sh --inspect --run-id <id>
```

It must report a sanitized summary:

```text
environment ID
run ID
instance name
instance normalized state
ownership verified yes/no
contract verified yes/no
lifecycle state
generation
source/cache prepared yes/no
guest probe status
offline status
matrix status
export status
recommended next operation
```

Do not print:

- ownership token;
- host private paths;
- raw Multipass authentication details;
- raw router logs;
- RouterInfo or endpoint-bearing diagnostics.

## Deliverable 16: Harden evidence attribution

Extend the Multipass environment evidence record with:

```text
environment_id
run_id
instance_generation
instance_name_digest
lifecycle_schema_version
ownership_record_sha256
environment_manifest_sha256
cloud_init_sha256
host_baseline_probe_outcome
guest_rootless_probe_outcome
adoption_mode = fresh | adopted | resumed | recreated
```

Retain only a digest of the concrete instance name if the sanitation policy treats raw names as host metadata.

Every directional protocol record must reference the environment evidence hash.

A passing manifest must prove:

- all four directions came from the same run ID and generation;
- all four reference the same environment evidence hash;
- guest probe outcome was `rootless_sandbox_available`;
- host baseline outcome did not control the pass decision;
- ownership and contract verification passed;
- cleanup passed;
- parent-host state remained acceptable under Plan 048 requirements.

## Deliverable 17: Add precise failure records

For every pre-router blocker, write a sanitized environment result under:

```text
target/interop/evidence/multipass/<run-id>/environment-blocker.json
```

Required fields:

```text
schema
run_id
environment_id
instance_generation
phase
outcome
remediation_class
host_baseline_probe_outcome
guest_probe_outcome or not-reached
environment_manifest_sha256
cloud_init_sha256
```

Do not create a protocol evidence record when no router started.

Do not let a blocker record satisfy external interoperability closure.

## Deliverable 18: Static boundary checks

Extend the Multipass boundary checker to reject:

- fixed authoritative instance names in the default evidence path;
- automatic deletion of unowned instances;
- global `multipass purge` in normal lifecycle scripts;
- ownership checks based only on name;
- host probe gating guest launch;
- arbitrary `multipass exec` command input;
- missing lifecycle lock;
- missing atomic state writes;
- passing evidence without environment attribution;
- silent adoption or recreation;
- reuse of snapshots without contract verification.

Allow a fixed legacy/debug name only in clearly non-authoritative development paths and documentation.

## Deliverable 19: Unit and simulated lifecycle tests

Expand `test_multipass.py` or the canonical Multipass test module.

Required tests:

### Name allocation

- generated name passes validator;
- collision causes bounded retry;
- exhausted retries produce typed blocker;
- truncation retains digest uniqueness;
- reserved names are rejected.

### Ownership

- matching host/guest token proves ownership;
- name-only match does not prove ownership;
- token mismatch blocks;
- missing host state blocks;
- missing guest state blocks;
- incorrect ownership-file permissions block;
- `i2ptest` writable ownership record blocks.

### State discovery

- running/stopped/suspended/starting/unknown normalization;
- multiple unrelated instances ignored;
- deleted-but-unpurged classification;
- malformed structured output blocks.

### Adoption

- compatible owned instance adopts;
- unowned collision blocks;
- contract mismatch blocks;
- unexpected snapshot blocks;
- active router process blocks;
- unexpected mount blocks.

### Recreation

- owned instance may be recreated explicitly;
- unowned instance is never deleted;
- unexported passing evidence blocks recreation;
- selective-purge unavailable produces typed blocker;
- generation increments.

### Resume

- resume from every safe state;
- unsafe state blocks;
- state/guest disagreement blocks;
- export retry is idempotent.

### Probe separation

- restricted host plus permissive guest proceeds;
- permissive host plus blocked guest stops;
- host probe missing is recorded but does not substitute for guest probe;
- router start cannot occur before final guest probe.

### Concurrency

- second process sees held lock;
- unrelated run IDs can proceed;
- stale-lock recovery requires proof;
- concurrent state writes remain valid JSON.

### Evidence

- environment hash linked to four directions;
- mixed generations rejected;
- mixed run IDs rejected;
- instance-name digest stable;
- blocker record cannot satisfy interoperability closure.

## Deliverable 20: Shell-level fake-Multipass tests

Create a fake Multipass executable used by tests to simulate:

- no instances;
- matching running owned instance;
- unowned name collision;
- stopped instance;
- suspended instance;
- deleted/unpurged instance;
- launch interruption;
- cloud-init timeout;
- ownership token mismatch;
- malformed JSON;
- failed delete;
- unavailable selective purge;
- concurrent commands;
- snapshot mismatch;
- successful full lifecycle.

Tests must assert exact commands and prove scripts never issue destructive operations against unowned resources.

## Deliverable 21: Real target-host execution sequence

After local gates pass, run on the target host in this order.

### Phase A: Inspect existing collision

```text
--inspect using the legacy/fixed instance identifier
```

Classify it as:

- owned and compatible;
- owned but incompatible;
- unowned;
- deleted/unpurged;
- ambiguous.

Do not mutate it during inspection.

### Phase B: Fresh unique instance

Run a fresh generated run ID rather than reusing the colliding name.

Required result:

```text
new unique instance launched
guest provisioning complete
guest sysctls correct
i2ptest privilege contract correct
guest rootless probe = rootless_sandbox_available
```

This phase is the first proof that Plan 048 reaches the intended guest.

### Phase C: Input preparation

Transfer and verify:

- exact source commit;
- source archive digest;
- canonical reference cache;
- cache manifest;
- source-and-cache-ready snapshot contract.

### Phase D: Offline and four-direction run

Run the existing Plan 048 matrix as `i2ptest`.

Required directions:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

### Phase E: Export and cleanup

Validate and export sanitized evidence, then destroy only the owned fresh instance when requested.

### Phase F: Recovery exercise

Create a second fresh run and deliberately interrupt after `source_and_cache_ready`. Verify `--resume-owned` continues safely and does not rebuild unrelated phases.

### Phase G: Collision exercise

Create or retain an unrelated instance with the legacy name. Verify a new authoritative run allocates another name and proceeds without mutating the unrelated instance.

## Acceptance criteria

Plan 049 is implementation-complete only when all of the following hold:

1. The authoritative default path no longer depends on one fixed Multipass name.
2. Stable environment ID, run ID, and concrete instance name are distinct.
3. Host state is reserved atomically before launch.
4. Every managed instance carries a guest ownership contract linked to host state.
5. Ownership cannot be established from name alone.
6. Existing unowned instances are never automatically mutated.
7. Compatible owned instances require explicit adoption.
8. Recreation requires explicit operator intent and proven ownership.
9. Resume follows a validated lifecycle state machine.
10. Deleted-but-unpurged instances have a distinct typed outcome.
11. Global purge is not used automatically.
12. The host baseline probe is recorded separately and does not gate guest launch.
13. The guest rootless probe gates every router start.
14. A restricted host and permissive guest combination is covered by tests.
15. Lifecycle locking prevents concurrent mutation of the same run.
16. Evidence identifies run, generation, environment contract, and ownership record hashes.
17. Mixed run IDs or generations cannot form a passing manifest.
18. Static, unit, fake-Multipass, and repository gates pass.
19. A fresh unique target-host guest reaches `rootless_sandbox_available` or produces a new guest-specific typed blocker.
20. The pre-existing colliding instance remains unmodified unless proven owned and explicitly selected.

## External evidence closure criteria

External closure remains unclaimed unless the target-host pass produces one of these outcomes.

### Environment recovery success

At minimum:

```text
fresh unique instance launched
ownership verified
guest policy verified
guest rootless probe = rootless_sandbox_available
host baseline retained separately
```

This closes the Plan 049 orchestration defect but does not by itself prove NTCP2.

### Full interoperability success

The exported manifest additionally contains all four passing directional records under the existing Plan 045/046/048 predicates.

### New typed guest blocker

If the fresh guest fails after launch, retain a precise guest-specific blocker such as:

```text
blocked_guest_user_namespace
blocked_guest_policy_mismatch
blocked_guest_execution_user_contract
blocked_guest_offline_enforcement
blocked_guest_cache_contract
```

A host blocker or instance collision cannot substitute for a guest result.

## Stop conditions

Stop immediately and retain sanitized blocker evidence if:

- ownership cannot be proven;
- an operation would mutate an unowned instance;
- structured Multipass state cannot be interpreted;
- host and guest lifecycle records disagree;
- the guest ownership token hash mismatches;
- `i2ptest` has sudo or unexpected capabilities;
- the guest sysctls differ from the permissive contract;
- the guest rootless probe does not pass;
- source or cache verification fails;
- offline enforcement cannot be proved;
- an unexpected router process is present;
- cleanup cannot remove secret-bearing state;
- evidence attribution mixes run IDs or generations;
- export sanitation fails.

## Recommended implementation order

1. Lifecycle schema, validators, and atomic state writer.
2. Collision-resistant name allocator.
3. Structured Multipass state parser.
4. Guest ownership token and environment record.
5. Ownership verification and inspection command.
6. Lifecycle lock and state transition enforcement.
7. Explicit adopt, resume, recreate, and destroy-owned operations.
8. Host/guest probe separation.
9. Early and final guest probes.
10. Environment evidence attribution.
11. Static boundary updates.
12. Unit and fake-Multipass tests.
13. Documentation and ADR amendment.
14. Target-host fresh-name execution.
15. Interrupted-run recovery exercise.
16. Unrelated-collision non-mutation exercise.
17. Four-direction evidence run when the guest probe passes.
18. Status and closure record.

## Commit discipline

Use focused commits, for example:

1. `interop: add Multipass lifecycle state and ownership contract`
2. `interop: allocate collision-safe Multipass instance names`
3. `interop: add owned instance inspect adopt resume and recreate`
4. `interop: separate host baseline from guest rootless gating`
5. `interop: link Multipass environment ownership to evidence`
6. `test: cover Multipass collision and recovery lifecycle`
7. `docs: reconcile Plan 049 Multipass lifecycle semantics`
8. `docs: record Plan 049 target-host status`

Do not combine external evidence artifacts with unrelated protocol implementation changes.

## Required handoff record

The implementing agent must report:

- exact starting and ending commit SHAs;
- files changed;
- lifecycle schema and state transitions;
- name-allocation algorithm;
- ownership proof design;
- commands executed;
- static/unit/fake-Multipass test results;
- repository gate results;
- classification of the pre-existing colliding instance;
- whether that instance was left untouched;
- fresh instance run ID and generation in sanitized form;
- host baseline probe outcome;
- guest rootless probe outcome;
- whether any router process started;
- per-direction outcomes if reached;
- evidence export and cleanup outcomes;
- remaining blockers;
- explicit statement that NTCP2 remains experimental and Milestone 3 remains open unless separately closed through the controlling conformance process.
