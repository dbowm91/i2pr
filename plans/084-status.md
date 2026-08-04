# Plan 084 status — precondition blocked

## Status

Plan 084 has not started. Its prerequisite Plan 083 remains the schema,
focused test matrix, and test-only runner orchestration module; no live
`i2pr → i2pd` wire probe or development decision exists on this host.
Plan 082 is implemented and closed: the launcher prepares authentic
endpoint-bound i2pr state, the runner validates both peer identities and
freezes real correlation fields, and the Rust `validate-scenario`
command parses the strict live scenario without opening a peer.

Plan 072 is not activated. The repository has no retained TCP-stage
divergence, no reference disagreement at a precise protocol stage, and no
written diagnostic question that would satisfy the Plan 084 ambiguity gate.

## Evidence boundary

The latest retained execution is the Plan 080/078 attempt, which stopped at
the i2pr pre-protocol RouterInfo stage. It did not establish TCP, NTCP2
authentication, authenticated frame transfer, or I2NP DeliveryStatus decode.
Plan 082 corrected that pre-protocol preparation path but did not run a peer.

The following are therefore intentionally absent:

- a Plan 083 compact probe record;
- a Plan 084 reverse-direction record;
- a Plan 084 exact development decision;
- an Emissary source lock, driver, or differential record.

## Next step

Execute Plan 083 only after its lane and artifact prerequisites are validated.
Do not begin Plan 084, Plan 079, or Plan 072 from preparation-only or
pre-protocol results.
