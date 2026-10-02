# Plan 287 — Secure base I2PControl JSON-RPC, authentication, and TLS server

Status: registered-prop170-secure-base-server-blocked-on-plan286

Classification: infrastructure + minimum control-plane capability.

Hard dependency: Plan 286 closed.

## Objective

Expose the runtime-neutral i2pr-i2pcontrol contract through one optional, bounded, supervised daemon-owned I2PControl endpoint with exact base API-1 authentication and JSON-RPC semantics. This plan deliberately stops before RouterInfo/AddressBook/TunnelManager/ClientServicesInfo gain substantive router behavior.

## Architecture

i2pr-daemon owns:
- listener and accepted sockets;
- TLS identity/loading;
- token generation/storage/expiry;
- source-IP authentication throttle;
- connection/request permits;
- request body and batch admission;
- request deadline/cancellation;
- typed dispatch into later capability handles.

i2pr-i2pcontrol owns only decode/validation/envelope/error semantics.

No HTTP framework may obtain unrestricted router state. A small focused server implementation or narrowly featured dependency is acceptable after dependency review.

## Configuration

Add a dedicated [i2pcontrol] block:
- enabled = false by default;
- bind default 127.0.0.1:7650, with IPv6 loopback supported explicitly;
- password has no insecure factory default in i2pr; enabling with empty/missing password fails validation before bind;
- optional certificate/private_key paths;
- optional request/resource limits only if bounded by hard compile-time maxima.

Managed self-signed TLS is allowed only for loopback identities and must cover localhost, 127.0.0.1, and ::1. Any non-loopback or wildcard bind requires a complete explicit certificate/private-key pair and fails validation before listener bind or managed-certificate side effects.

There is no plaintext fallback and no fallback from bad explicit TLS material to managed TLS.

## Authentication semantics

Implement API version 1 Authenticate with the standard I2PControl error inventory:
- invalid password -32001;
- missing token -32002;
- invalid/unknown token -32003;
- first use after expiry -32004 followed by removal;
- missing API version -32005;
- unsupported API version -32006.

Use the Emissary fork implementation as the donor/reference for bounded behavior unless Plan 286's manifest marks a file otherwise:
- 32 random bytes per opaque token;
- finite one-day monotonic lifetime;
- maximum 1024 live tokens with deterministic bounded eviction;
- maximum presented token length 256 bytes;
- constant-time bounded password comparison;
- source-IP, not source-port, failed-auth accounting;
- fixed-capacity throttle state and bounded delay;
- tokens in memory only and invalidated on restart.

Credentials/tokens are never logged or included in Debug/Display output.

## JSON-RPC semantics and admission

Implement:
- JSON-RPC exactly 2.0;
- string, integer, and explicit null request ids;
- absent id means notification; explicit null remains a request id;
- named-object params only;
- standard parse/invalid request/method/params/internal errors;
- protected token in params.Token;
- optional X-I2PControl-Token compatibility only if the Plan 286 contract records it; when both token forms are present they must agree;
- single requests and non-empty batches;
- empty batch -> one invalid-request response;
- invalid batch elements -> per-element invalid-request response without corrupting valid siblings;
- all-notification valid batch -> no response body;
- input-order response preservation;
- no token propagation between Authenticate and sibling batch elements.

Adopt the proven initial ceilings unless the implementation documents a stricter equivalent:
- 1 MiB HTTP body hard cap;
- 32 elements per batch;
- 64 concurrent in-flight requests;
- bounded accepted connection count;
- one held request permit for sequential batch execution;
- explicit per-request deadline.

No task-per-batch-element fanout.

## Dispatch floor

Authenticate executes.

All other known methods may decode/authenticate and then return a typed not-yet-available/internal capability error; unknown methods return standard method-not-found. Do not fabricate RouterInfo or service state merely to exercise dispatch.

## Security and lifecycle

- listener is supervised by the daemon graph;
- shutdown cancels accept and all child requests under hard deadlines;
- partial reads, slow bodies, client disconnects, TLS failures, and cancellation release permits/resources;
- disabled config allocates no listener, managed cert, token table background task, or Proposal state;
- remote source addresses are taken from the real socket, never forwarding headers.

## Evidence

Tests must cover:
- literal Authenticate positive/negative vectors;
- all six auth errors;
- expiry transition expired -> unknown with deterministic clock;
- token table ceiling/eviction;
- oversized password/token/body/batch +1 cases;
- JSON parse/request/id/params errors;
- notification side-effect/response suppression;
- mixed valid/invalid batches and order;
- simultaneous failed auth from the same source to prove atomic throttle reservation;
- loopback managed TLS creation/reuse and restrictive permissions where supported;
- non-loopback no-cert, half-configured TLS, bad cert/key, wildcard bind fail-before-side-effect;
- disabled mode no listener/filesystem mutation;
- graceful and forced shutdown resource baseline.

## Acceptance criteria

The plan closes only when an independent test client can:
1. establish loopback TLS;
2. Authenticate with API 1;
3. receive an opaque token;
4. issue a protected known-method request and reach typed dispatch;
5. observe exact JSON-RPC/authentication failure behavior;
6. restart the daemon and confirm the old token is invalid.

Routine workspace checks and focused adversarial server tests must pass. Closure makes Plans 288, 289, and 294 dependency-ready in parallel.
