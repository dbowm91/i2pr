# Plan 347 — Live bidirectional Encrypted LeaseSet2 cross-router qualification

Status: **registered-live-els2-java-i2pd-qualification-blocked-on-plan346**

Classification: external interoperability + capability closure.

Hard dependency: Plan 346 passed.

Historical context:
- Plan 335 proved a cryptographic-boundary incompatibility between i2pr's former strict-only ELS2
  signature and the deployed Java/i2pd transcript.
- Plan 346 corrects the ELS2 transcript policy while preserving standalone Proposal-146 Red25519.
- Plan 326 remains blocked until this plan supplies end-to-end external evidence.

## Objective

Prove that corrected i2pr Encrypted LeaseSet2 works through the real type-5 lifecycle with stock,
unmodified Java I2P and i2pd in both publisher and consumer roles.

A signature harness is not sufficient. The acceptance path must include:

```text
service identity
 -> daily blinding / B33
 -> type-5 record build
 -> DatabaseStore publication
 -> independent-router NetDB storage/lookup
 -> outer type-11 verification
 -> layer-1/layer-2 decrypt
 -> inner LeaseSet2 validation
 -> destination/streaming use
 -> application payload round-trip
```

## Reference freeze

Before execution, record:
- exact i2pr SHA;
- exact Java I2P SHA/version;
- exact i2pd SHA/version;
- Proposal 146 / standalone Red25519 spec hash;
- Encrypted LeaseSet spec hash;
- Proposal 123/149/B33 spec hashes;
- Cargo.lock hash;
- Plan-346 ADR/profile revision.

Reference routers must be stock/unmodified. Harness configuration is allowed; source patching is not.

## Controlled topology

Prefer a local/Ubuntu controlled topology with deterministic evidence collection.

Required roles:
- one i2pr router;
- one Java I2P router;
- one i2pd router;
- enough stock/reference floodfill reachability to ensure a publisher's record is actually stored
  and a consumer performs a real lookup.

The harness must prove it did not satisfy a row by injecting a decoded LeaseSet directly into the
consumer.

At minimum record:
- publisher;
- consumer;
- floodfill/store peer used;
- blinded storage key;
- B33/extended B32 address;
- store acknowledgement/result;
- lookup result;
- transcript profile accepted;
- final streaming/application result.

Never record lookup secrets, PSKs, DH private keys, destination signing private keys, or proxy
credentials.

## Mandatory direction matrix

Each of these four directions is a closure gate:

| Publisher/service | Consumer/client | Required |
|---|---|---|
| i2pr | Java I2P | pass |
| Java I2P | i2pr | pass |
| i2pr | i2pd | pass |
| i2pd | i2pr | pass |

Also execute Java -> i2pd and i2pd -> Java as control rows where the stock harness exposes the
required configuration. A failure in a control row is investigated before attributing a failure to
i2pr.

## Capability matrix

### No-auth

Mandatory in all four i2pr/reference directions.

Evidence must prove a real type-5 store/lookup and an application payload exchange, not only B33
resolution.

### Lookup secret

Mandatory in all four directions if the stock reference exposes the current specification feature.
A consumer with the wrong/missing secret must fail before it can use the inner LeaseSet.

### PSK client authorization

Run bidirectionally against each reference implementation that exposes compatible PSK configuration.

Required rows:
- authorized client succeeds;
- wrong PSK fails;
- unconfigured client fails;
- restart preserves the authorized service identity/address.

### DH/X25519 client authorization

Run bidirectionally against each reference implementation that exposes compatible DH authorization.

Required rows:
- authorized client succeeds;
- wrong private key fails;
- unconfigured client fails;
- multiple authorized clients remain bounded and independently usable.

If a pinned reference does not implement one auth mode, prove that from pinned source/docs and mark
that specific row `reference-not-applicable`; do not silently skip it.

## Rollover and persistence

The external lane does not need to wait for UTC midnight.

Use the deterministic/day-controlled local owners to prove adjacent-day key/address/storage-key
rotation, then execute at least one live current-day reference lookup.

For i2pr-owned services additionally prove:
- daemon restart;
- unchanged unblinded service identity;
- expected current-day B33;
- republished type-5 record;
- reference client reconnects after restart.

## Negative rows

At minimum:
- wrong lookup secret;
- wrong PSK/DH credential;
- expired/stale type-5 record;
- tampered outer signature;
- tampered ciphertext;
- malformed B33;
- lookup under the wrong storage key;
- wrong blinded public key;
- reference receives a deliberately strict-only Proposal-146 signature and rejects it (diagnostic,
  preserving the Plan-335 distinction);
- i2pr accepts a valid strict-form ELS2 record only through its bounded compatibility verifier,
  not the generic signature API.

## Evidence format

Commit a bounded machine-readable evidence artifact with:
- exact SHAs/versions;
- row ID;
- topology role;
- store/lookup/application result;
- hashes of public wire artifacts;
- transcript profile;
- failure classification.

Provide a checker that fails if:
- a mandatory row is missing;
- a required external binary was not the pinned hash/version;
- a reference was patched;
- an application-success row lacks preceding store+lookup evidence;
- any secret appears in the artifact;
- only cryptographic-boundary evidence is present.

## Closure consequences

On pass:
- write the successor closure stating that Plan 326's external acceptance requirement is satisfied;
- mark Plan 335 as historical measured-negative evidence superseded by Plans 346–347 for forward
  execution;
- update the Red25519/ELS2 roadmap to complete;
- update `specs/support.toml` and `specs/CONFORMANCE.md` to claim the exact externally qualified
  ELS2 subset while retaining experimental/non-default status;
- do not rewrite Plan 326/335/336 closure files.

On failure:
- preserve the exact boundary;
- distinguish signature, store, lookup, decrypt, inner-LS2, streaming, and harness failures;
- register a narrow corrective rather than changing transcript policy ad hoc.

## Acceptance criteria

Plan 347 passes only if all four mandatory i2pr<->Java/i2pd directions complete the real type-5
publication/lookup/application path for no-auth, and the overlapping lookup-secret/client-auth
matrix passes or is explicitly source-proven not applicable.

Passing Plan 347 closes the encrypted-LeaseSet external blocker for the Proposal-170 continuation
and unblocks Plan 348 together with Plan 342.
