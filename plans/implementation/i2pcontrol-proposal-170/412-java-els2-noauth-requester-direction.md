# Plan 412 — stock Java no-auth ELS2 requester direction

Status: **in-progress-java-els2-noauth-requester-direction**.

Subsystem: Proposal 170 / Java ELS2 qualification.

## Objective and readiness

Deliver and execute the first real stock-Java ELS2 application direction owned
by Plan 375: an i2pr ELS2 publisher is stored at controlled floodfill F, and a
stock Java I2P 2.13.0 client resolves its B33 through F, decrypts the type-5
record, and carries an application payload to the i2pr service. This is the
NONE/no-authorization row only.

Hard prerequisites: Plans 373, 374's i2pd scope via 406, and 411's Java SAM
substrate diagnosis passed; Plan 375's pin and ELS2 source matrix are frozen.
Plan 411 proved the old DATAGRAM setup failure comes from the default
`127.0.0.1:7655` SAM UDP bind, before I2CP session construction. The Java
driver must configure a unique loopback `sam.udp.port` for any DATAGRAM
session. This plan does not execute Plan 279's spent runner.

## Classification and invariants

**Capability evidence plus test infrastructure.** This closes only the
i2pr-publisher-to-Java-consumer NONE direction.

1. Java I2P remains stock, exact-pinned at
   `9134f808337b401e8e53c73734c81fab04280c9d`; no patch, fork, or rebuilt
   reference source.
2. Every router listener and peer endpoint is IPv4 loopback. Java reseed,
   public update, UPnP, network time, and unrelated client applications remain
   disabled. Use fresh datadirs and generated identities.
3. Use the controlled queried-floodfill topology: Java reaches i2pr as F for
   NetDB lookups, while Java client tunnels use Java relay peers. Do not claim
   Java selected i2pr as an OBEP, IBGW, or tunnel participant.
4. No record injection, shared in-process NetDB, test-byte resolver call, or
   reference source mutation may satisfy a row. Java must issue a real blinded
   lookup and consume the real returned type-5 record.
5. Java SAM datagram listeners must bind an explicitly selected available
   loopback UDP port through stock SAM session properties. Never rely on host
   default port 7655.
6. Evidence contains pins, hashes, bounded counts, categorical statuses, and
   payload integrity digests only. Do not retain Java destination private
   material or raw reference logs.
7. This plan makes no Java-publisher, auth-mode, restart/rollover, whole-ELS2,
   or Proposal-170 conformance claim. Type 5 remains non-advertised.

## Scope

In scope: a new Java ELS2 driver/runner independent of Plan 279's runner;
stock SAM/I2CP Java consumer setup; a controlled F + Java relay mesh;
i2pr-owned ELS2 publication; real Java B33 lookup/decrypt; loopback application
payload round-trip; sanitized evidence checker and mutation tests; Plan 411 UDP
port lesson integrated into the driver.

Out of scope: Java publisher to i2pr consumer; PSK/DH and lookup-secret matrix;
wrong/missing credentials; restart, day rollover, stale/tampered records;
other Plan 375 negative cases; support metadata or RouterInfo advertisement
changes; running Plan 279 or modifying Java I2P.

## Ordered work packages

1. Build a new lane runner from current repository-owned test helpers, with
   exact pin/build-metadata checks, owner-only scratch, fresh datadirs, clean
   teardown, an explicit bounded process/attempt budget, and sanitized output.
   Do not invoke `tests/integration/floodfill/run-java-floodfill.sh`.
2. Construct the controlled mesh and assert source-side role facts: Java client
   JC has F as its queried floodfill; JD1/JD2 are the only Java tunnel relay
   peers; F is not used as a Java tunnel peer. Include parsed RouterInfo and
   endpoint digests in the artifact.
3. Provision an i2pr ELS2 NONE publisher with the production Plan-351/380 path
   and publish its type-5 record to F. Require the ordinary publication/store
   acknowledgement and a healthy reference mesh before proceeding.
4. Create a stock Java SAM STREAM client to the B33. If a DATAGRAM helper
   session is needed, allocate one available loopback UDP port and pass
   `sam.udp.host=127.0.0.1 sam.udp.port=<port>` explicitly. Establish Java's
   own tunnels through JD1/JD2, require a blinded-key lookup to F, type-5
   response, Java decrypt/inner-LS2 validation, and application payload
   integrity at the i2pr service.
5. Add the evidence checker and negative self-tests proving it rejects missing
   lookup/store/payload stages, local injection, topology bypass, wrong pin,
   raw/private data, and incomplete payload integrity.
6. Execute once under the frozen budget. Stop on unhealthy reference controls,
   missing F lookup, no Java decrypt, or missing payload; do not convert setup
   failures into skips. Record a typed stop and exact next evidence if needed.

## Failure, restart, and compatibility

The runner owns every process and socket and has bounded startup, mesh-settle,
SAM, lookup, and payload deadlines. Cancellation terminates the process group,
then escalates after bounded grace; scratch cleanup is unconditional. Any live
row failure stops the lane. The explicit SAM UDP bind is a host-resource fix in
the test driver, not a Java patch or protocol change. No persisted i2pr format
changes.

## Verification

```sh
bash tests/integration/els2/run-java-els2-noauth.sh
python3 scripts/check-java-els2-noauth-evidence.py
python3 scripts/check-java-els2-noauth-evidence.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Run formatting/whitespace checks and affected ELS2, package, SAM/I2CP, exact-pin,
and source-lock guards. No production workspace floor is required unless
production code changes.

## Acceptance and closure evidence

Pass only when the real Java requester stores/looks up/decrypts the i2pr type-5
record through F and delivers an integrity-checked application payload, with
Java tunnel relays independently shown and sanitized evidence accepted by the
negative-tested checker. Closure must preserve the Plan 411 diagnosis, record
the explicit UDP bind, provide command outcomes/artifact hashes, review secret
and process cleanup, and leave Plan 375, 377, and 378 gated on their remaining
directions and matrices.
