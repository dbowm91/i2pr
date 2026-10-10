# Plan 443 status: blocked — normal-process product topology gate open

Closure token: `blocked-normal-process-composition-and-stock-control-unproven`

Plan: `plans/implementation/core-router-recovery/443-controlled-router-product-integration-without-public-ssu2.md`

## Controlled one-family evidence completed

The exact-pinned, unmodified i2pd 2.61.0 loopback lanes passed on i2pr commit
`ce36345b06e4c8ee40bc0b20a62fe35cf51e2184`. Every i2pr and reference bind was
`127.0.0.1`; the i2pr profile kept `advertise=false` and no introducer. The
current-head evidence is under `target/interop/m6-{preflight,tunnel,netdb,destination,streaming}-evidence/`.

The executed lanes established daemon-owned SSU2 authentication and inbound /
outbound I2NP, real exploratory tunnel installation, live RouterInfo lookup
and publication through TunnelData, LeaseSet2 lookup and publication, real
inbound/outbound destination tunnels and bidirectional ECIES/Garlic delivery,
and byte-exact bidirectional Streaming. Streaming also exercised sibling
streams, listener acceptance and orderly shutdown. All five lane summaries
report their workspace-gates slice passed.

Commands and results:

```text
rtk bash tests/integration/m6-interop/run-preflight.sh   passed (Plan 184)
rtk bash tests/integration/m6-interop/run-tunnels.sh     passed (Plan 185)
rtk bash tests/integration/m6-interop/run-netdb.sh       passed (Plan 186)
rtk bash tests/integration/m6-interop/run-destination.sh passed (Plan 187)
rtk bash tests/integration/m6-interop/run-streaming.sh   passed (Plan 193)
```

Reference pin: i2pd `2.61.0` at
`635b013a612ff47278ef02acf8580a28e10e26c5`. The runners validated the pin
before launch and retained sanitized evidence only. No reference source was
modified and no public network was used.

## Remaining acceptance gate

These drivers build and start daemon-owned SSU2/runtime components inside the
integration test process. They do not start the normal `i2pr run` process and
drive its externally owned SAM/I2CP interfaces through the whole multi-router
composition. They also do not establish a stock-to-stock control topology
before the candidate path. The positive mixed-router results therefore cannot
be reported as Plan 443's normal-process acceptance, and no stock selection,
peer-failure/backoff, or cross-process restart claim is made.

The bounded next step is to add a fail-closed controlled runner that first
proves the selected pristine stock references can exchange on the same
network-ID/loopback policy, then starts the ordinary daemon process with
isolated data and signs its RouterInfo from the Plan 442 persistent SSU2 store.
It must drive real NetDB/tunnel/SAM or I2CP traffic through process interfaces
and retain only identity-redacted event counts/digests. If stock control does
not complete, stop before candidate traffic and classify the fixture.

Plan 443 remains blocked at this process/topology gate. The passed component
lanes are retained as infrastructure evidence and are not substitutes for
Plan 433's independent-router, externally qualified path. No support,
conformance or public-address claim changed.
