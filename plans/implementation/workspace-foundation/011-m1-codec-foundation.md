# Milestone 1 plan A: bounded codec foundation

## Purpose

Establish the low-level decoding, encoding, error, and test conventions used by all Milestone 1 protocol structures. This plan must land before common identity and RouterInfo codecs.

## Scope

Implement in `i2pr-proto`:

- bounded byte cursor;
- checked fixed-width integer reads and writes;
- length-prefixed byte and UTF-8 string helpers required by pinned I2P specifications;
- exact-consumption top-level decode entry points;
- bounded encoder with exact-length accounting;
- stable error taxonomy;
- codec traits or functions limited to concrete current use;
- reusable boundary and truncation test helpers.

Do not implement RouterIdentity, Destination, RouterInfo, LeaseSet, or I2NP messages in this plan except minimal private fixtures needed to test codec mechanics.

## Required design decisions

### Decoder shape

Use a cursor over a borrowed byte slice. It should track offset without copying input by default. Required operations should include:

- remaining length;
- current offset;
- checked `take(n)`;
- fixed-width unsigned integers in protocol byte order;
- bounded length-prefixed byte slices;
- bounded UTF-8 strings where the protocol requires text;
- exact end-of-input assertion.

Avoid generic parser-combinator architecture unless a concrete benchmark and maintenance case justifies it. The default implementation should be understandable under manual security review.

### Allocation policy

Parsing helpers must not allocate until a caller-approved bound has been checked. Where an owned value is required, allocation occurs only after length validation. Borrowed parsing is preferred for intermediate verification, but do not create complex lifetime-heavy public APIs merely to eliminate small bounded copies.

### Error model

Errors must distinguish at least:

- truncated input;
- declared length exceeding caller or protocol maximum;
- checked arithmetic overflow;
- invalid UTF-8;
- invalid field value;
- noncanonical encoding;
- unsupported type or algorithm;
- trailing bytes;
- duplicate field/key where applicable;
- semantic policy rejection.

Errors should contain bounded structural context such as field category and offset, but must not retain or print full attacker-controlled input.

### Encoder shape

Provide deterministic output with checked size accumulation. The implementation must reject values that do not fit protocol widths instead of truncating. Prefer writing into a caller-provided `Vec<u8>` or bounded writer abstraction only where this yields a simpler and auditable API.

Do not introduce an async writer abstraction or filesystem I/O into `i2pr-proto`.

## Implementation phases

### Phase A: limits and errors

1. Define protocol-wide limit types or constants only for genuinely shared codec mechanics.
2. Define the structured codec error enum.
3. Add display text that is stable and safe for logs.
4. Add unit tests for all error categories.

### Phase B: read cursor

1. Implement checked offset and remaining-byte handling.
2. Add fixed-width integer reads.
3. Add bounded slice and length-prefixed reads.
4. Add UTF-8 validation helpers.
5. Add exact-consumption support.
6. Test truncation at every field boundary and offset overflow behavior.

### Phase C: encoder

1. Implement exact checked length accumulation.
2. Implement fixed-width integer emission.
3. Implement bounded length-prefixed bytes and strings.
4. Add canonical ordering helper only if required by Mapping work in Plan 012.
5. Verify maximum and maximum-plus-one behavior.

### Phase D: codec contract

Introduce only the smallest useful public contract, for example separate `decode_exact` and `encode` functions or narrow traits. Avoid forcing every type to implement a speculative universal trait if associated context or limits differ.

The contract must make caller-supplied limits visible. Hidden unlimited defaults are prohibited.

### Phase E: test support

Add helpers that generate:

- every truncation prefix of a fixed vector;
- maximum and maximum-plus-one lengths;
- appended trailing bytes;
- one-bit mutations;
- invalid UTF-8 cases;
- deterministic random byte inputs bounded by test size.

These helpers belong in test modules or `i2pr-testkit` only if they are shared across crates. Production code must not depend on testkit.

## Testing requirements

At minimum:

- unit tests for every cursor operation;
- no panic on empty input;
- no panic on offsets near `usize::MAX` through synthetic checked-arithmetic tests;
- exact error classification for truncation versus oversized declaration;
- deterministic encoding tests with fixed expected bytes;
- trailing-byte rejection for top-level strict decode;
- allocation bounds verified where practical;
- property tests may be introduced if dependency cost is reviewed and centrally declared.

## Fuzz preparation

Add a first fuzz target or documented harness entry for the codec cursor if the repository fuzz infrastructure is established in this plan. The harness must cap input size and assert no panic or infinite loop. Full structure fuzzing is handled in Plan 014.

## Documentation

- Document byte order, length semantics, ownership behavior, and exact-consumption rules.
- Link module documentation to `specs/CONFORMANCE.md`.
- Record why any third-party parsing or property-testing dependency is introduced.

## Acceptance criteria

- Common protocol primitives can be decoded and encoded without hidden allocation or unbounded length behavior.
- All arithmetic is checked.
- Strict decode rejects trailing bytes.
- Maximum and maximum-plus-one tests exist for every length-prefixed helper.
- Error messages do not echo arbitrary payloads.
- `i2pr-proto` remains free of runtime, filesystem, CLI, and tracing-subscriber dependencies.
- Workspace quality and MSRV checks pass.

## Stop conditions

Stop and report rather than continue if:

- pinned specifications disagree on basic integer or string encoding;
- a desired generic API creates a dependency cycle or requires broad runtime abstractions;
- a dependency introduces significant unsafe code or unsupported MSRV without review;
- limits cannot be derived or bounded for a planned public helper.

## Handoff

Report public API signatures, limit decisions, error variants, dependencies, test count/categories, commands, and unresolved questions. Identify any API intentionally kept private pending Plan 012.