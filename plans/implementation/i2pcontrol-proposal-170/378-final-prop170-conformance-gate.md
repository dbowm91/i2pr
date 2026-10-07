# Plan 378 — Final Proposal 170 conformance gate after external ELS2 and outproxy resilience

Status: **registered-prop170-final-gate-blocked-on-plans373-376-377**

Classification: final conformance/evidence gate. No feature implementation should originate here.

Hard dependencies:
- Plan 373 passed.
- Plan 376 passed and closes the historical Plan-327 remainder.
- Plan 377 passed and closes the historical Plan-326/347 ELS2 external remainder.

Historical final gates:
- Plan 328 remains the first blocked full-conformance attempt.
- Plan 348 remains the later blocked attempt whose Proposal re-freeze was clean but whose
  dependencies had not closed.

## Objective

Run one current, integrated Proposal-170 gate against the code and evidence that actually exist
after the ELS2 and outproxy successor lines.

Only this plan may promote the control-plane claim to `full-proposal-conformant`.

## 1. Re-freeze the Open Proposal

Proposal 170 is Open.

Fetch/hash the current Proposal at execution time and compare it to:
- Plan-320 canonical wire freeze;
- Plan-348 clean re-freeze;
- current canonical field inventories.

If any material method/selector/field/type/result/applicability rule changed, stop and register a
wire-contract corrective. Do not silently test an old revision.

## 2. Canonical local contract gate

Require green evidence for:
- Authenticate / JSON-RPC 2.0;
- RouterInfo all 43 additions;
- ClientServicesInfo;
- AddressBook operational semantics;
- all 12 TunnelManager families;
- full applicable option matrix;
- pool/bundling/TLS residuals;
- destination SigType/EncType policy;
- all Proposal ELS2 modes and secret redaction;
- all canonical outproxy fields and request paths;
- logs/logs.clear;
- signed news;
- persistence/edit/rollback/restart semantics.

No accepted parser-only/inert cell.

## 3. Runtime capability gate

Through production composition prove:
- one shared ServiceTunnelManager;
- type-3 and type-5 publication;
- type-5 external qualification imported from Plan 377;
- AddressBook ordinary resolution and administrative state share one owner;
- RouterInfo selectors read real owners;
- outproxy request path, failover, and post-restart routing imported from Plan 376;
- no direct-clearnet fallback;
- disabled-by-default I2PControl does not alter ordinary router startup.

## 4. External differential gate

Run/re-run the canonical API corpus against:
- project-owned Emissary Proposal-170 oracle under existing provenance restrictions;
- the Java I2PControl reference for methods it implements;
- i2pd for adopted/base semantics it implements.

Import Plan-377 ELS2 application evidence by exact hash rather than replacing it with control-plane
fixtures.

Reference-not-implemented rows are classified, not counted as i2pr failures.

## 5. Security/resource gate

Re-run:
- non-loopback TLS fail closed;
- authentication before protected dispatch;
- throttle/body/batch/table/task ceilings;
- secret redaction from get/rawConfig/logs/errors/evidence;
- config-file error redaction and permissions policy from Proposal 170/352;
- no generic dual-transcript type-11 verifier;
- ELS2 deployed profile confined to typed type-5 use;
- no direct-clearnet outproxy path;
- path confinement;
- dependency/unsafe/MSRV checks;
- global plan/ADR uniqueness guards.

## 6. Final support vocabulary

A pass may set:

`full-proposal-conformant`

only for the exact Proposal revision frozen here.

The claim does not imply:
- enabled by default;
- remote exposure by default;
- stable/non-experimental API;
- every historical I2P crypto algorithm;
- frontend/router-console completion;
- transit participation enabled.

Keep those orthogonal support flags truthful.

## 7. CI and evidence integrity

Require:
- exact-head ordinary CI green on all required jobs;
- external artifacts bound to exact implementation/reference SHAs;
- no missing mandatory row;
- checker negative controls/mutations where the evidence format has a guard;
- registry, roadmaps, support.toml, CONFORMANCE and closure status agree.

## Acceptance criteria

Plan 378 passes only if the current Proposal has no unresolved contract drift, Plans 376/377 are
passed, every applicable canonical surface has a real owner/effect, required external lanes execute,
security/resource gates pass, and exact-head CI is green.

If any gate fails, close with the exact stage and register a narrow successor. Do not relax the
claim vocabulary.
