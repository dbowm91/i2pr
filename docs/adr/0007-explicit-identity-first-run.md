# ADR 0007: Explicit router identity first-run policy

- Status: Accepted
- Date: 2026-07-15

## Context

Creating an identity is a durable security decision. The bootstrap CLI must
remain safe during configuration validation and dry runs, and identity
corruption must not be hidden by a new key pair.

## Decision

Identity creation is explicit through:

```text
i2pr identity generate --config <path>
i2pr identity inspect --config <path>
```

`identity generate` may create and harden the configured data directory, then
creates the identity file only if it does not already exist. `identity inspect`
loads, revalidates, and summarizes only public algorithm identifiers. It never
prints private bytes or silently regenerates missing/corrupt state. `run
--dry-run` and `check-config` do not create directories or identity files.

The live `run` path remains unimplemented and does not publish a RouterInfo,
open a listener, perform reseeding, or advertise capabilities.

## Consequences

Operators have a clear point at which identity material is created and can
back it up according to the security model. Automated first-run convenience is
deferred until startup ordering, recovery, rotation, and operator prompts have
an explicit design.

## Review triggers

Review when the daemon gains a live supervised runtime, identity rotation or
backup commands, noninteractive deployment requirements, or a passphrase
provider.
