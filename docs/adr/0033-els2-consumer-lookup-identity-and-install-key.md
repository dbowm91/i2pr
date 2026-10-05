# ADR 0033: The ELS2 consumer lookup identity and install key

- Status: Accepted
- Date: 2026-10-05
- Amends, for the ELS2 **consumer** path only: the Plan 349/351 framing recorded under ADR 0032.
  ADR 0032 owns the type-11 signature profile and is unchanged by this decision.
- Implements: Plan 351.
- Does not amend: ADR 0028 §7 (Emissary Red25519 production source stays excluded), ADR 0030
  (no bandwidth-tier claim is fabricated), ADR 0031 (one shared service-tunnel manager).

## Context

Plan 349 built a bounded `EncryptedServiceResolver` — the owner that derives a day's blinded
storage key, hands the caller a lookup target, and unwraps a type-5 reply into an ordinary
`LeaseSet2`. It had **zero production callers**. Plan 351 supplies one.

Supplying a caller forced three questions that no amount of review of Plan 349's isolated owner
could answer, because each of them is a statement about the layers *around* it.

### 1. What is the lookup identity for an encrypted record?

Every ordinary destination lookup in the product derives its lookup key from a
`DestinationHash`:

```rust
LookupId::new(request_id, LookupKind::LeaseSet2, router_hash_from_destination(target))
```

That derivation is correct for a Standard `LeaseSet2`, because such a record is filed under its
own destination hash. An encrypted record is filed under the **day's blinded storage key**, which
is not a destination hash — and it cannot be derived from one, because at lookup time no
`Destination` exists anywhere in the caller's possession. The `Destination` is inside the sealed
record.

Two of the three production callers of the router-material provisioning pass shut the manager
down, cancel the operator token, and shut the SSU2 handle down on *any* resolution failure, so
this could not be sidestepped with a tolerance.

### 2. What key is the fetched record installed under?

The obvious answer — the destination hash of the `.b33`'s public key — is not available. The
protocol layer's own documentation is explicit:

> The value carries the **unblinded** signing public key plus both signature types, so a client
> can derive the daily blinded key from the address alone. It is not a Destination hash and must
> never be treated as one.

That is correct and load-bearing. A `Destination` hash is the SHA-256 of a canonical
`Destination` encoding, which needs the ECIES public key, the signing key, the certificate, and
the padding. A `.b33` publishes exactly one of those four.

### 3. Does the type-11 answer to question 2 generalize?

No, and this is the finding that shaped the decision.

Plan 349's publisher path publishes **type 7** (`EdDsaSha512Ed25519`): the unblinded Red25519
scalar is `CONVERT_ED25519_PRIVATE(seed)` — SHA-512, clamp, low 32 bytes — and `DERIVE_PUBLIC` of
that scalar reproduces the destination's ordinary Ed25519 public key. For a type-7 `.b33`,
therefore, `address.public_key()` **is** the signing key of the inner `LeaseSet2`'s
`Destination`.

A type-11 `.b33` has no such relationship. Its unblinded key is a distinct Red25519 identity that
signs the *outer* record only. A consumer that assumed the type-7 relationship would refuse every
type-11 address; a consumer that assumed there was no relationship at all would accept any valid
`LeaseSet2` for any destination.

## Decision

### Decision 1 — the lookup key is supplied verbatim; the wire type is unchanged

`NetDbSeam::begin_lease_set2_lookup_for_key_with_store` takes the lookup key as a parameter
instead of re-deriving it from a `DestinationHash`.
`DestinationTunnelCoordinator::begin_encrypted_lease_lookup` is its sibling, and the shared body
is `begin_lease_lookup_inner`.

The lookup **kind** stays `LookupKind::LeaseSet2`, wire code `1`. A reference client issues that
same lookup type when resolving an encrypted service, precisely because the record is filed under
its blinded storage key. **No new wire lookup type is introduced**, so nothing a peer can observe
changes.

`PendingLeaseLookup` splits into `lookup_key: RouterHash` (what the lookup is filed under, on both
paths) and `destination: Option<DestinationHash>` (what a Standard record must install under,
`None` on the encrypted path). The ordinary path's behaviour is unchanged: `lookup_key` is exactly
`router_hash_from_destination(target)` and `destination` is exactly that same hash.

### Decision 2 — the install key is the inner record's own destination hash, gated by a signature binding

`encrypted_service_resolver::bind_inner_to_address` returns the inner record's destination hash,
and only after two checks pass:

```text
b33 signing key     ==  inner Destination.signing_key       else refuse
b33 unblinded sigtype ==  inner signing key type             else refuse
```

The trust argument is transitivity through a signature. Without the binding, the hash would be
whatever any valid `LeaseSet2` claimed to be, and a publisher could hand a consumer a working
record for a service its address does not name. With it, the hash is trusted because the record's
signature verifies against a key the operator obtained out of band from the address — not because
the record said so.

Both checks are fail-closed and neither has a fallback. The policy lives with the rest of the ELS2
consumer policy rather than in the product layer, so a second consumer cannot reimplement it wrong.

**Ordering is load-bearing.** `bind_inner_to_address` compares header metadata and therefore does
*not* verify the signature; `ValidatedLeaseSet2::from_lease_set2` runs immediately after it, with
the bound hash as its expected value. A row in `encrypted_service_consumer_wiring.rs` pins this
ordering explicitly rather than leaving it to inspection.

### Decision 3 — a `.b33` is its own reference kind, with three projection outcomes

`DestinationRef` gains `EncryptedService`, dispatched by
`EncryptedServiceAddress::is_encrypted_service_address` **before** the `.b32.i2p` branch. Without
that ordering a valid `.b33` is rejected as "Base32 label must be exactly 52 characters" — true of
the length and useless as a diagnosis.

`RemoteTargetProjection` replaces the `Option<[u8; 32]>` Plan 212 §8 returned, with three
outcomes: `LocalCoOwned`, `Remote(DestinationHash)`, `EncryptedService(EncryptedServiceAddress)`.
Both wrong answers are dangerous and for opposite reasons: `Remote` would require fabricating a
destination hash, and `LocalCoOwned` would route an unreachable remote endpoint to the local
bridge, which works in exactly the way that makes a misconfiguration invisible.

### Decision 4 — containment, as three gates

Adding a caller to a path whose failure mode is product-wide shutdown requires the failure to be
bounded before it exists. Each gate answers a measured defect:

| Gate | Rule | Defect it answers |
| --- | --- | --- |
| 1 | An encrypted remote target requires a `DelayOpen` client. Enforced in `ServiceTunnelSpec::validate`; an alias may not name a `.b33`. | Two of three provisioning callers tear the product down on any error. `DelayOpen` groups already have proven per-destination isolation. |
| 2 | The consumer secret arrives through I2PControl definition options, in the same `leaseset_password` slot the publisher uses. | `delay_open` is unsettable from TOML, so a TOML field would be dead on arrival. The publisher's slot already exists; a second channel would be a second thing to leak. |
| 3 | No inline secret in any config struct; resolve-on-demand only. | A TOML parse error prints the whole offending source line and a type mismatch prints the value, both reaching stderr through `ConfigError::Parse`. |

Gate 1's alias refusal is the non-obvious half. The rule is a *per-service* property and an alias
is global; if an alias could name a `.b33`, the rule would be enforced on the alias spelling rather
than on the address the service resolves, and an eager service could reach the encrypted path by
naming the alias. Making the pair unrepresentable closes the bypass instead of trusting every
caller to re-check after resolution.

### Decision 5 — the failure is recorded, never propagated, and never carries a foreign string

`resolve_encrypted_destination_for_service` returns `Result<(), EncryptedTargetStatus>`, never a
`ServiceProductError`. That is a deliberate narrowing: this path runs where a propagated error
shuts the product down, so the set of things that can escape it is a closed enum whose reasons are
`&'static str` and which cannot carry a secret, a derived key, or a fetched payload.

`EncryptedServiceResolver::cancel` runs on every early return, and `ingest_store` removes the
request from its table before it can fail, so the in-flight lease is released on every outcome.

## Consequences

- The NetDB layer gains `LookupResult::EncryptedLeaseSet2Success` and a type-5 response arm that
  performs the key match and the record-type check **only**. It does not validate, decrypt, or
  install: the closed type-11 profile is ADR 0032's policy, and the daily material and any
  per-client credential belong to the owner. A caller matching only `LeaseSet2Success` treats a
  type-5 reply as a non-match and keeps waiting, which is the correct fail-closed outcome.
- `record_observation`'s `_ => {}` fallthrough becomes a counted `observations_rejected_unknown`.
  A typo'd label was previously indistinguishable from a deliberately suppressed one, so a counter
  that was never wired up looked identical to one that was deliberately rejected.
- `remote_target_hash_for_reference` → `project_remote_target` is a signature change. Its single
  production caller and its in-crate tests were updated; the ordinary path's behaviour is
  unchanged.
- Plan 289's "every secret-classified key is rejected on a client" row changes its **error variant**
  for `leaseset_password` only. It is still rejected before storage; the cross-field rule now names
  the real objection instead of claiming the key is out of scope for the kind, which was never
  true.

## Non-claims

This decision does not claim: PSK/DH consumer authorization; daily rollover re-resolution; a
cross-router interoperability result; or any Java or i2pd direction of Plan 347. The blinding
rotates daily and there is no periodic re-resolution today, so a resolution computed before a
midnight boundary addresses the **wrong DHT key**; that is a recorded limitation, not rollover
support. **Type 5 remains `advertised = false`.**

## Alternatives considered

**A TOML configuration field.** Rejected: `delay_open` has no TOML surface, so every
TOML-configured service is eager and Gate 1 would reject all of them. Dead on arrival.

**A SAM client surface.** Rejected in favour of the service-tunnel surface on the user's
preference, conditioned on truthful containment. The containment is the three gates above; without
them the risk is not contained, and saying so would be the easier thing to do.

**Installing under the blinded storage key.** Rejected: nothing looks a Standard `LeaseSet2` up
under that key, so the record would be installed where it can never be found — a silent success.

**Deriving a destination hash from the `.b33` public key.** Rejected: the address does not carry
the ECIES public key, the certificate, or the padding, so no canonical `Destination` encoding can
be formed. Any 32 bytes produced this way would look plausible and match nothing.

**A generic dual-transcript type-11 verifier.** Not applicable and explicitly out of scope; ADR
0032 forbids it and its guard enforces that.