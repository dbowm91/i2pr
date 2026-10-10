# Initial security model

Milestone 0 establishes boundaries and validation behavior. It does not claim
anonymity, privacy, censorship resistance, production readiness, or protocol
security.

## Assets

Current and future assets include router identity keys, peer and destination metadata,
RouterInfo/LeaseSet data, tunnel state, client traffic, configuration, local
service endpoints, resource capacity, and diagnostic output. The bootstrap has
an explicit persistent identity path but does not create one during validation,
dry-run, or live runtime startup.

## Adversaries and trust boundaries

The design treats the following as untrusted:

- Remote unauthenticated peers and authenticated-but-malicious peers.
- Malicious or malformed SAM/I2CP and application clients once those adapters exist.
- Malformed command-line arguments and local TOML configuration.
- Corrupted or stale persisted network state.
- Malicious or misleading reseed material.
- Resource-exhaustion attempts and oversized inputs.
- Browser-executed script on the operator's machine, and any page that may be
  induced to issue requests to a loopback listener (Plans 356–358).
- Dependencies, build tools, CI actions, and other supply-chain inputs.

Trust boundaries are the protocol decoder, configuration parser, persisted-state
loader, client adapters, local service boundary, the browser console surface,
and daemon composition root.
Each future boundary must validate input before handing a narrower capability to
the next subsystem.

Plan 345 adds the `i2pr-app-proto` contract boundary for future managed native
applications. App, publisher, launch-instance, router, and I2P Destination
identities remain separate; app permission requests are inert; and the secured
network policy defaults to deny, including loopback. The crate does not launch
or contain processes, perform DNS/network I/O, or verify an OS sandbox. See
ADR 0032 and `specs/references/managed-native-app-runtime-v1.md`.

Service-tunnel HTTP/IRC profiles remove router-specific branding from their
router-generated application bytes. HTTP remote `Host` uses the resolved
Destination's canonical Base32 name, independent of a local address-book alias.
Opaque generic, SOCKS, and CONNECT forwarding can still expose application and
TLS fingerprints. These local boundary properties do not establish anonymity,
privacy, or resistance to traffic analysis.

## Router console browser boundary (Plans 356–358)

The console is a browser-facing HTTP surface, which introduces a threat class
the other local adapters do not have: **the client is running attacker-influenced
code on the operator's machine**. A loopback-only listener is not a browser
boundary. The console is experimental, loopback-only, disabled by default, and
non-advertised, and the following controls are what make enabling it defensible
rather than merely possible.

**DNS rebinding.** The accepted `Host` set is derived from the bound address and
the **resolved** port, and matching is exact apart from ASCII case. A page
hosted on an attacker-controlled name cannot resolve to `127.0.0.1` and be
accepted, because the name is not in the set. Because the policy depends on the
real port, the router is constructed after `bind` rather than before. No
`X-Forwarded-*` header is consulted and there is no trusted-proxy mode; a
forwarded header is not a fact the console has any way to verify.

**Cross-site requests.** Unsafe methods require an `Origin` (or, only when
`Origin` is absent, a `Referer`) that matches the allowed origin *including its
terminating `/`*, which is what refuses `localhost:7070.evil.test`.
Authenticated unsafe actions additionally require a per-session CSRF token.
`/logout` is the one documented exemption: its token travels as a form field,
which middleware cannot read, so the handler verifies it with the same
`verify_csrf` before the state change.

**Script execution.** CSP is `default-src 'none'` with `script-src 'self'`,
`style-src 'self'`, `img-src 'self'`, `font-src 'self'`, `connect-src 'self'`,
`frame-ancestors 'none'`, `form-action 'self'`, `base-uri 'none'`, and
`object-src 'none'`. There is no `unsafe-inline` and no `unsafe-eval`; the shell
ships no inline script, so the escape hatch is never needed. `console.js` is
dependency-free and uses `AbortController` with a single in-flight request, so a
hidden or backgrounded tab cannot fan out requests.

**Credentials.** Optional Argon2id at 16 MiB / 2 passes / 1 lane — deliberately
not the desktop default, because a small SBC must not allocate hundreds of
megabytes to log in. At most two verifications run concurrently; saturation is
refused rather than queued, because a queue is an unbounded memory amplifier. A
plaintext `[console] password` is converted to a PHC hash during configuration
parsing and does not survive into `Config`; `auth = true` without credential
material is a configuration error, so there is no dormant default password.

**Login throttling is console-wide, deliberately.** The substrate does not
surface an unforgeable per-client identity and no proxy header is trusted, so
there is no sound key to throttle on — deriving one from `Host` would trust the
attacker. The failure mode is therefore a bounded delay that expires with the
window, not a lockout. This is a deliberate trade, recorded rather than hidden.

**Cookies** are opaque, `HttpOnly`, `SameSite=Strict`, `Path=/`, and have no
`Domain`. `Secure` is **deliberately omitted**: the listener is plain HTTP on
loopback, and a `Secure` cookie over `http://localhost` would silently never be
sent. The localhost exception is documented rather than assumed, and this
console does not terminate TLS.

**What the console can reach.** It is read-only. Its `ControlClient` trait has
two methods and no method-name parameter, so "a browser cannot select arbitrary
daemon methods" is a property of the type rather than a runtime check. The
daemon-side principal's allow-set is `RouterInfo` and `ClientServicesInfo`; it
bypasses only the external bearer-token transport step, while method, parameter,
selector, availability, bounds, and redaction checks all still run in the shared
dispatcher. **No configuration on the console path can reach `TunnelManager`,
`AddressBook`, or any mutating method**, and enabling the console requires no
I2PControl listener, password, or token.

**Rendering.** Every value interpolated into HTML goes through the `Text`
newtype, which escapes and offers no unescaped constructor, and the object-leaf
renderer is bounded in depth and count. A value that is unavailable renders as
an explicit `unavailable` with a null value, never as zero or empty — the page
cannot be made to lie about router state.

None of this establishes anonymity, privacy, or resistance to traffic analysis.
The console is a local operator surface, not a privacy feature.

## SAM local adapter boundary

The SAM listener is experimental and loopback-only. Plan 139's `STREAM
FORWARD` target is restricted to numeric loopback addresses or the literal
`localhost`; omitted hosts use the loopback peer address of the SAM socket.
The adapter performs no system DNS lookup, does not expose Unix sockets or TLS
forwarding, and does not provide a general TCP pivot. A forward registration is
owned by its control socket and is removed on EOF, session teardown, or
shutdown.

Forwarded streams use a bounded read-then-write bridge and a three-second local
connect deadline. SAM client, session, attachment, pending-ACCEPT, line, and
buffer ceilings are checked independently and against router-wide task and
buffer budgets. Invalid naming input fails before any lookup; `NAME=ME` is
session-scoped, complete public Destinations are canonicalized, and unknown
`.i2p`/b32 names do not trigger clearnet DNS or create a second address book.
Normal diagnostics may identify bounded protocol results and session lifecycle
categories, but must not contain private destinations, keys, or raw payloads.
Plan 150's localhost evidence confirms the external-client SAM application
surface, including raw CONNECT/ACCEPT, SILENT, private-destination, FORWARD,
and NAMING behavior. It does not establish router-to-router interoperability;
SAM remains experimental, loopback-only, and non-advertised.

## Objectives and required properties

Future implementations must use explicit size/count/time/nesting limits,
complete-consumption parsing where canonical encodings require it, bounded
queues, deadlines, cancellation, and cleanup. Resource exhaustion must cause
rejection, deferral, backoff, or shedding rather than unbounded memory growth.
Persisted network data must be revalidated on load, and security-sensitive
writes must be atomic or recoverable.

The initial I2NP codec applies a 62,708-byte payload ceiling before body
allocation, checks standard-header lengths and checksums, rejects unknown
message identifiers, caps DatabaseLookup exclusions at 512 and search replies
at 16 peers, and validates fixed tunnel/build framing. Compressed, encrypted,
and cryptographic body semantics remain explicitly deferred. Deferred payload
wrappers redact bytes from `Debug`; callers must opt into accessing their
contents. No parser performs clock, routing, NetDB, tunnel, garlic, or
transport side effects.

Secret-bearing types must avoid accidental `Debug`, `Display`, unrestricted
serialization, and unnecessary cloning. Production cryptography must use
reviewed implementations; deterministic randomness belongs only to tests and
explicit reproducibility tooling. Plan 013's private Ed25519 and X25519
wrappers zeroize on drop and expose bytes only to explicit storage methods.

**The configuration path (Plan 352).** The rule above is not satisfied by the
config file on its own, so two specific gates apply.

*Parse errors are position-only.* A TOML decode failure is rendered by
`RedactedTomlError`, which reports the **line and column** and nothing else. It
exists because both upstream renderers leak: `toml::de::Error`'s `Display`
prints the entire offending source line, and its `message()` embeds the rejected
key *and value* on a `deny_unknown_fields` failure. Filtering the message would
therefore not be a fix — the redaction is structural, and the marker
`[source content redacted]` is always present. The upstream error is still
reachable through `Error::source` for a caller that legitimately needs it, so
nothing is silently destroyed; what is forbidden is rendering the source chain
(`{error:#}` / `{error:?}`) on this path.

*At rest.* `Config::load` refuses a file that holds a `[i2pcontrol] password`
and is group- or world-readable, using the same `& 0o077` idiom as every other
secret file in the tree (`i2pr-storage`, `i2pcontrol_tunnels.rs`,
`addressbook.rs`). The check is **conditional on a password being present**, so
an ordinary config is not affected, and it runs *after* parsing for that reason.
The refusal names the path and the required mode and never echoes content.

*Non-POSIX platforms.* Windows exposes no POSIX mode, and a gate that cannot
run must not report success. A password-bearing config is therefore **refused**
there rather than accepted unchecked. This is a deliberate behaviour change, not
an oversight: enabling `[i2pcontrol]` with a password on Windows requires moving
the secret out of the config (`outbound_secret.rs` holds the sealing mechanism)
or designing a real platform gate. `scripts/check-config-secret-hygiene.sh`
asserts all of this, and both outcomes of the platform decision are exercised on
any host so the non-POSIX branch is never untested dead code.

Plan 015 extends that boundary to transient ownership: generated and
reconstructed seed buffers, serialized identity write buffers, file-read
buffers, and decoded DatabaseLookup reply keys/tags use zeroizing owners where
they are retained. Reply-secret wrappers are non-cloneable and redact their
debug output. This is memory hygiene for ordinary success and failure paths,
not encrypted-reply implementation or a guarantee against a process that is
already compromised.

The initial identity storage threat model is permission hardening rather than
encryption at rest. On Unix, the data directory must have no group/world mode
bits and generated identity files are mode 0600. Storage uses a versioned
fixed-format record, SHA-256 integrity, strict revalidation, and atomic
create-only installation. A checksum cannot protect against an attacker with
write access to both the file and its directory, so operators must protect
ownership and backups; passphrase encryption is deferred to a separate ADR.
Corrupt or unsupported identity state fails closed and is never silently
replaced.

New Unix identity directories are created with mode `0700` at creation time;
the standard-library implementation requires an existing parent and does not
recursively create missing intermediate components. Existing directories and
symlink paths are rejected when unsafe. The parent directory remains an
operator responsibility: an attacker who can write the parent can replace the
identity path even if the child mode is private.

Zeroization does not defeat allocator copies, compiler/platform retention,
swap, hibernation images, core dumps, crash reporters, process snapshots, or
an attacker with process memory access. Non-Unix permission and directory
durability semantics are limited by the platform and are not a production
security claim.

`identity inspect` reports only the storage path and public algorithm IDs. It
does not print private seeds, a private serialization, or a full router hash.

## NTCP2 cryptographic foundation threats and controls

Plan 032 adds local cryptographic composition but no network activation. The
NTCP2 static X25519 key and its published AES obfuscation IV are independent
from the RouterIdentity and are persisted in a separate versioned, checksummed,
create-only record. Immediate restart reuses both values; silent replacement
or rotation would invalidate cached RouterAddresses and is rejected. The
record is still plaintext private material protected by restrictive directory
and file permissions, not encryption at rest.

The transcript binds the exact I2P Noise name, empty prologue, responder
static key, role, and message ordering. SessionRequest/Created/Confirmed
stages are consuming owners; SessionConfirmed part one uses the retained
SessionRequest cipher state at nonce 1, and split invalidates the handshake
owner. This prevents accidental state reuse and transcript confusion but does
not protect a process whose memory, swap, core dump, or crash artifacts are
compromised.

X25519 all-zero shared secrets are rejected before KDF use. ChaCha20-
Poly1305 counters are checked before use and never emit `2^64 - 1`; a nonce
reuse or counter wrap would compromise authenticated encryption and therefore
terminates the bounded state operation. AES-CBC ephemeral obfuscation is DPI
obfuscation using public RouterHash/IV inputs, not authentication or secrecy.
SipHash material is directional and used only for the two-byte length mask;
it is not a substitute for AEAD authentication.

Private keys, chaining keys, cipher keys, split keys, and shared secrets use
non-cloneable zeroizing owners with no `Debug`, `Display`, serde, or payload
formatting. Public keys and transcript hashes use typed wrappers with redacted
diagnostics. Fixed vectors contain synthetic test values only; the validator
and support ledger prevent local vectors from becoming interoperability or
capability evidence.

## Runtime supervision threats and controls

Plan 021 adds a concrete non-networked runtime boundary without changing the
protocol support claim. Tokio is confined to `i2pr-runtime`; protocol, crypto,
storage, and runtime-neutral core crates remain executor-free.

Long-lived task leaks are treated as resource-exhaustion vulnerabilities. A
supervisor owns every service manager through an owned `JoinSet`, and each
service owns a bounded child scope. Shutdown cancels all scopes, joins within a
bounded deadline, aborts remaining managers, and joins aborted handles before
returning. Child scopes abort their remaining children on drop as a final
guard, while normal service completion explicitly joins them.

Plan 025 closes the false-zero cleanup risk by retaining each active manager's
bounded child-scope owner in the supervisor. A forced manager abort is followed
by abort-and-drain of that exact child collection; child counters decrement
only after join results, and a bounded failed drain is reported as typed
`FailedCleanup` evidence. A synchronous scope drop can still request abort as a
last resort, but it cannot claim termination. Service-selected
`RequestedShutdown` without observed cancellation is classified as an
unexpected clean exit, preventing an essential service from disappearing
while the graph remains ready.

Cancellation races are handled by Tokio's hierarchical cancellation primitive:
registration and cancellation are wake-safe, cancellation is idempotent, a
bounded static reason is recorded once, parent cancellation reaches children,
and child cancellation cannot reach a parent. The runtime-neutral atomic token
is retained only for synchronous contracts and is not used as async service
cancellation.

Restart policy is explicit and bounded. Only services classified
`Restartable` may restart; attempts, exponential delay, service timeouts, and
the router-wide service count are capped. Zero-delay hot loops are rejected.
Restart exhaustion must choose degradation or coordinated shutdown. Dependency
failure marks dependent snapshots degraded and cancels their owned managers so
they cannot remain ready after a hard dependency is gone.

Panic and join failures become static completion categories. Panic payloads,
raw errors, secrets, peer data, addresses, and arbitrary user text are not
formatted into health snapshots or normal diagnostics. Health uses a bounded
latest-state watch snapshot rather than an unbounded event log.

Forced abort is cleanup evidence, not a claim that arbitrary code can be made
graceful. A non-cooperative service is stopped at the configured deadline and
reported as forced. No Plan 021 service binds sockets, connects to peers,
performs DNS, touches NetDB, constructs tunnels, exposes client listeners, or
advertises protocol capabilities.

## Managed-application process and policy threats (Plans 369–374)

Plans 369–374 add a managed-application runtime that can start applications
as real processes after explicit local policy. It is **disabled by default**
and remains experimental, unsupported, and unadvertised. The controls below
define the narrow authority path and its remaining non-claims.

### The three process edges are the whole trust model

```text
  i2pr-daemon  --spawn-->  i2pr-appd  --spawn-->  i2pr-apphost  --exec-->  application
   (supervisor)              (manager)              (direct host)
```

Three spawn sites exist and no more.
`scripts/check-managed-app-process-boundary.py` rule 1 pins that set, so a
fourth edge — any component other than `i2pr-apphost` able to exec an
application — is a hard failure.

The two **distribution-owned** edges (daemon → manager, manager → apphost)
resolve their executable as a `current_exe()` sibling. Nothing in configuration,
argv, or `PATH` can substitute a different program, so possession of the
inherited pipes is meaningful: a swapped binary would have to be a swapped
*sibling*, which is an installation problem rather than a remote one. Rule 1b
forbids a shell launcher and any `PATH` lookup outright, so no edge can be
routed through `sh -c`.

`i2pr-apphost`'s target is deliberately **not** a sibling — it is the
root/entrypoint of a launch request the manager already validated — so it is
governed by the containment rules below instead.

### No discoverable endpoint

The daemon ↔ manager link is two **inherited anonymous pipes** on file
descriptors 0 and 1. There is no listener, no port, no discovery endpoint, and
no way to run `i2pr-appd` standalone and have it mean anything: without an
inherited transport it has nothing to talk to, and without a daemon on the other
end it holds no authority. A local attacker cannot connect because there is
nothing to connect to.

### Only persistent local policy can select an autostart

Three independent facts hold together, and removing any one of them would open
a path:

1. The shipped `i2pr-appd` **refuses all arguments** and loads
   `PersistentLaunchCatalog` only after the authenticated inherited-pipe
   handshake. It consumes one local policy snapshot under a lifetime lock.
2. The Plan-368 manager protocol has **no manager-receivable launch request**,
   so the daemon cannot ask the manager to select an executable or grant a
   capability.
3. `LaunchAuthority` / `AuthorityRequest` have **no decoder** and private
   fields. Effective capabilities can only be assembled through
   `GrantedCapability::from_administrator_policy`, so `BrokeredTcp` is
   ungrantable and there is no `&mut` path to the capability set at all.

The seal is asserted by **method resolution**, not by scanning for a
`#[derive(Deserialize)]`: adding a derive makes the crate stop compiling rather
than merely fail a test.

The daemon derives `<data-dir>/managed-apps`, canonicalizes it and clears the
manager environment before binding only `I2PR_APP_STATE_ROOT`. Appd still needs
the inherited anonymous pipes; setting that environment variable alone cannot
give a standalone process router authority.

### Signed content is not trusted content

The Plan-373 package verifier checks exact signed manifest/inventory bytes,
Ed25519 publisher-key fingerprint, archive structure, and every payload hash
before the store commit. It copies an untrusted source once into private
staging and parses only that copy. Package paths are validated and files are
materialized manually; there is no install hook, generic extract-all, network
fetch, or exec path. Appd re-verifies selected packages before constructing
authority.

These checks prove key possession and content integrity only. Publisher trust,
selection, grants, profile, and autostart are distinct persistent decisions.
They are stored outside package trees in strict immutable policy generations.
A malformed highest committed generation fails closed. Offline mutations take
the runtime lock nonblocking, so they cannot race appd's snapshot; untrust
clears grants, profile, and autostart so retrust cannot resurrect authority.
Only Sam and I2cp can be granted, and only when the exact selected manifest
requests them.

### Containment is checked twice, and `Secured` fails closed

`LaunchRequest::validate` rejects `..`, `.`, absolute, and backslash forms
*structurally*, on strings, so it is testable without a filesystem. That is
necessary but not sufficient — a symlink inside the root can still point outside
it — so `resolve_command` canonicalises both paths and re-checks containment on
the resolved result. A single check would leave either the obvious escape or the
disguised one open.

`LaunchProfile::Secured` is **refused before any exec**, and refused again at
the exec site so no future refactor can quietly skip the gate. No sandbox
backend is qualified in Plan 369, so reporting a successful `Secured` launch
would be a forged containment claim. The failure mode is a refusal, not an
approximation.

The child environment is **constructed**, not inherited, so the router's
environment does not leak into an application.

### Failure degrades one feature, and children are bounded

A manager that crashes, hangs, or answers wrongly takes down **the app runtime
and nothing else**: Plan 371's `StartupRequirement::Optional` lets an optional
subsystem degrade its own feature instead of aborting router startup, and
`RestartExhaustion::Degrade` is honoured during startup as well as after. A
dependency edge constrains start *order*, not *availability*, so a degraded
service still lets its dependents start.

Every wait is bounded: bootstrap grace, reply grace, hello grace, close grace,
and forced-kill grace each have an explicit ceiling. The direct child is owned
from `spawn` until it is reaped and is **killed rather than leaked** if its
manager goes away first — "the pipe closed" must not mean "an application keeps
running with nobody to talk to". stderr is drained continuously with a bounded
retained snapshot and an uncapped byte total, so a flooding application cannot
drive unbounded allocation.

**No grandchild containment is claimed.** Only the direct child is owned. An
application that forks is outside the guarantee, and saying otherwise would be
a claim this plan cannot support.

### Non-claims

Stated plainly because each is a plausible misreading:

- **No secured sandbox.** `Secured` remains refused. `UnsafeDirect` provides
  ordinary host networking, and resource ceilings are descriptive only.
- **No automatic update or rollback ordering.** Operators explicitly select an
  exact artifact; version strings are not ordered.
- **No app crash auto-restart.** Exit is terminal for that application during
  the current appd lifetime; approved autostarts run after a later manager
  restart with fresh random instance ids.
- **No live administrator endpoint.** Filesystem access to the managed-app
  state root is the local administrator boundary; mutations apply after
  restart.
- **No remote repository/TUF claim.** Package verification is local and
  offline; repository rollback/freeze security belongs to a future plan.
- **No administrator or general control credential** is exposed to a manager or
  an application.
- **No restart recovery.** Restart begins empty: no application or grant
  recovery is claimed.
- **No protocol support or advertisement change.** Managed-app v1 remains
  unreleased; the router console does not gain an application principal.

## Transport contract threats and controls

Plan 031 adds only the ownership vocabulary needed before NTCP2 wire work. The
transport manager owns bounded link candidates, authenticated-link admission,
delivery queue ownership, lifecycle observations, and typed outcomes. The
runtime owns every future socket, Tokio channel, timer, reader/writer task, and
cancellation scope. `i2pr-transport-ntcp2` remains a pure protocol crate and
cannot open sockets or perform filesystem, NetDB, tunnel, or client work.

## NTCP2 handshake threats and controls

Plan 033 adds bounded handshake parsing and consuming state transitions without
activating a network adapter. SessionRequest and SessionCreated accept only a
fixed 32-byte obfuscated ephemeral field, a fixed authenticated 16-byte
options payload, and bounded cleartext padding. SessionConfirmed requires the
fixed 48-byte static-key frame plus the negotiated second frame; its plaintext
may contain only RouterInfo, Options, then Padding. Lengths are checked before
allocation; fixed regions are exact and trailing bytes in the part-two block
sequence are rejected.

The responder and initiator use injected timestamp values with a documented
±60-second policy and no wall-clock access in the protocol crate. Replay
tokens are bounded SHA-256 values derived from encrypted ephemeral material;
the reference cache expires entries deterministically and fails closed on
replay, capacity, or unavailable decisions. Neither timestamps nor replay
tokens enter default logs.

RouterInfo is structurally decoded, its retained signed region is verified,
and the NTCP/NTCP2 version-2 `s` option must match the X25519 static key
authenticated by SessionConfirmed. RouterIdentity hash and static-key
mismatches are distinct typed failures. Structural parsing never becomes an
authentication claim, and no accepted RouterInfo mutates NetDB.

Consuming states prevent retransmission or resumption after a failed
transition. Actions retain only bounded owned handshake bytes or redacted
typed decisions; secret transcript/cipher owners are consumed into the final
split-key result. Random production padding, partial-stream adaptation,
timeouts, cancellation, and probing-resistance behavior remain the
runtime-owned follow-up boundary.

## NTCP2 data-phase threats and controls

Plan 034 deobfuscates the length prefix with a direction-specific SipHash
state and rejects the clear length before allocating ciphertext. The wire
prefix itself may be any two bytes, so validation is deliberately performed
after XOR rather than on the attacker-controlled obfuscated value. Ciphertext
is authenticated with empty associated data before any block header or unknown
type is inspected; tag failure, malformed blocks, invalid ordering, and
counter exhaustion put the receive/transmit owner into a terminal state.

The authenticated plaintext has explicit limits for total bytes, block count,
unknown bytes, options, RouterInfo, I2NP messages, padding, and termination
metadata. General data-phase non-padding blocks may repeat where permitted;
Termination is accepted after earlier valid blocks but must be the final
non-padding block, and Padding remains single and final. Invalid blocks after
Termination, trailing headers, and oversized fields fail closed. RouterInfo
signature and authenticated-link static-key checks produce candidates only;
they do not update NetDB. Unknown blocks are treated as bounded padding only
after authentication and cannot bypass the aggregate budget.

Transmit and receive owners contain independent cipher and length counters.
Counters advance once for an accepted frame; failed authentication cannot be
reused because the owner is terminal. The protocol dossier defines no
periodic in-session rekey threshold, so this layer never invents one: a fresh
Noise handshake is required after exhaustion or static-key/IV rotation. The
forbidden nonce value `2^64 - 1` is never emitted.

Partial length/ciphertext reads are retained by the future runtime adapter in
bounded owners, not inferred as frame alignment. Deterministic testkit cases
cover split prefixes, one-byte writes, truncation, duplicate frames,
backpressure, cancellation, and teardown. Debug and error values expose only
lengths, counts, typed categories, and terminal state; they never expose
payloads, keys, tags, identities, addresses, or remote text.

Transport payloads cross the manager boundary as bounded owned encoded-I2NP
messages. The owner validates nonzero and maximum lengths at construction,
preserves authenticated bytes, exposes no implicit large-payload clone, and
uses explicit consuming handoff. Delivery requests carry only a redacted peer
reference, payload owner, bounded monotonic expiry, and a runtime-owned
response capability. Typed outcomes distinguish no-link, queue/resource
denial, deadline, cancellation, replacement, closure, protocol termination,
and identity mismatch without retaining remote error text.

Pending handshakes, active links, buffered bytes, and queue items use existing
`ResourceClass` leases. Admission is immediate grant-or-deny; every accepted
lease stays with its exact candidate, link, or queued payload until handoff,
completion, rejection, cancellation, or drop. Capacity-one, exact-limit, and
limit-plus-one cases are deterministic tests, and valid teardown must return
usage to zero without an underflow signal.

Link IDs are local bounded correlators and are not derived from peer identity.
Peer references wrap the redacted public identity digest but do not expose full
bytes or mutable profile state. Default snapshots contain only bounded local
IDs, transport/direction/lifecycle categories, queue counters, rounded age,
typed termination, and resource usage. They exclude addresses, ports, hashes,
keys, transcripts, payloads, and dynamic peer labels. Duplicate-link inputs
and decisions are representable, but the winner policy remains deferred to
Plan 035 rather than being guessed here.

## Plan 035 runtime TCP threats and controls

Plan 035 is the first phase allowed to open TCP sockets, but only through
`i2pr-runtime` and only for controlled local/private scenarios. The listener is
disabled unless an explicit test configuration enables it. Every accepted
socket is immediately covered by global, per-IP, and IPv4 `/24` or IPv6 `/64`
pending-handshake admission before cryptography. Admission keys are bounded
internal counters; raw addresses never enter default events or snapshots.

Slowloris reads, stalled writes, connect storms, and duplicate candidates are
bounded by nonzero capped connect/handshake/read-idle/write/queue/drain
deadlines, bounded queues and bytes, replay capacity, active-link limits, and
expiring dial backoff records. Replay-cache capacity fails closed. The runtime
uses typed outcomes for overload, cancellation, deadline, identity mismatch,
replacement, closure, and protocol termination rather than retaining OS error
text or peer-controlled messages.

The listener/dialer owns sockets through the supervisor; each runtime-managed
link registers exactly one reader and one writer child. A child failure cancels
its sibling, and both are joined before closure. The current subset does not
yet drive authenticated NTCP2 frames or claim end-to-end I2NP delivery.
Forced shutdown aborts and
drains the owned scope, and counters/leases are released only after join or
explicit bounded cleanup. Stale close notifications carry the local link ID so
they cannot remove a replacement. Outbound I2NP owners remain consuming and
are reserved for the later authenticated data-phase driver; they are not
claimed as delivered by the Plan 035 raw link helper.

NTCP2 address parsing accepts only validated literal fields and separates
configured literals from resolved dial targets. Reachability is an observation
candidate only: one peer cannot infer or publish an external address, and the
runtime never mutates RouterInfo or NetDB. Runtime TCP and malformed/fault
tests use loopback, the deterministic testkit, or an authorized isolated
testnet; no public-network stress or mutation is permitted. Local TCP success,
self-handshakes, and synthetic vectors remain non-advertised evidence.

## Bounded communication and resource-governor threats

Plan 022 treats queue exhaustion and slow consumers as explicit denial-of-
service conditions. Command and request queues wait only under a caller-owned
deadline and wakeable cancellation scope; event queues use a documented
drop-newest policy with counters; latest-state consumers receive only the
current version and can detect closure. Shutdown cancellation is a separate
path, so a full ordinary queue cannot starve supervisor teardown.

Request response senders are one-shot and are dropped when the requester is
cancelled or its deadline expires. A service that drops its response path is
reported as response closure rather than leaving an unbounded waiter. Queue
items own resource leases through enqueue and processing, and dropping a
receiver drops queued items so their charges are released.

Resource accounting is immediate-grant or immediate-denial under one bounded
lock; it has no hidden asynchronous waiter queue. Bundle requests validate all
classes before mutating usage, reject duplicate classes, and commit atomically,
so an exhausted class cannot leave partial grants. Limits are immutable for a
budget lifetime, class counts and estimates are bounded, and high-water/denial
counters saturate rather than wrap. Lease drop, consuming release, panic
unwind, cancellation, and forced task cleanup are all release paths. Snapshot
metadata contains only static class/channel identifiers and bounded counters;
payloads, secrets, peer identities, addresses, and destinations are excluded.
Release amounts greater than current usage are treated as an internal
accounting fault: usage is bounded back to zero, a saturating typed
underflow counter is recorded, and cleanup remains non-panicking. The fault
must remain visible in resource snapshots so a double release or ownership
bug cannot be mistaken for valid cleanup.

## Deterministic simulation threats and controls

Plan 023 is a local test boundary, not an emulation of I2P transports or a
security/interoperability claim. The scheduler opens no sockets, resolves no
names, and has no public-network path. Fault scripts are executable only in
the testkit and authorized isolated testnets; malformed or stress traffic must
not be sent to live peers.

Simulation inputs have hard limits for sleepers, pending deliveries, buffered
bytes, receiver queues, datagrams, stream segments, fault rules, duplicate
expansion, peers, and idle steps. Queue admission and resource leases happen
before scheduled payload ownership. Due work stays pending under receiver
backpressure, and reset/close paths purge or release queued ownership. A
manual clock never polls wall time; dropping its final handle deterministically
fails pending sleeps.

Reproducibility uses domain-separated seed derivation rather than one shared
mutable RNG, preventing task interleaving from changing unrelated components.
Replay records contain root seed, scenario, safe sequence/rule/outcome
categories, final monotonic time, and bounded resource/queue snapshots. They
never contain payloads, private keys, destinations, real addresses, or full
RouterInfo bytes. `TestPeer` deliberately has a redacted `Debug` impl and
keeps deterministic private identity material in memory-only zeroizing crypto
owners. Link leases remain attached to live endpoint handles, so tests must
drop those handles before asserting zero active links.

The harness does not hide detached Tokio tasks: it manually pumps the
scheduler, exposes the runtime cancellation token, and leaves any supervised
service ownership with the caller's Plan 021 graph. Teardown snapshots are
therefore evidence about queued simulation resources, not proof of a live
router's network cleanup behavior.

## Observability and non-claims

Plan 024 makes the default diagnostic boundary explicit. Fixed event names are
used for service registration/start/ready/failure/restart/stop, shutdown,
channel rejection, resource denial, and testkit fault/completion events. Safe
fields are validated static service/channel identifiers, classifications,
lifecycle and typed failure categories, bounded restart/counter values,
capacity/depth/usage values with units, monotonic durations, and synthetic
simulation link/sequence/rule metadata. The daemon owns subscriber
initialization; runtime and testkit crates never install global subscribers.

`HealthDetail` remains bounded for internal control flow, but its `Debug`
implementation is redacted. Aggregate runtime snapshots use a service
projection that omits detail text and retain only sorted, bounded channel and
resource snapshots. Snapshot generation performs no await while holding a
mutable runtime lock and is documented as an eventually coherent point-in-time
observation. No default event or snapshot retains full router hashes,
destination identities, keys, session material, LeaseSet secrets, packet
bodies, user traffic, filesystem paths, arbitrary error/panic text, precise
per-peer timing histories, or unbounded identity-bearing labels.

Plan 024's integrated scenarios validate clean startup/shutdown, bounded
overload, deterministic restart recovery, essential failure with forced
cleanup, and stream/datagram fault replay. They use only synthetic services,
manual time, fixed seeds, and bounded step counts. They do not prove anonymity,
privacy against traffic analysis, resilience, transport authentication,
interoperability, or safe public-network operation.

Nothing in the bootstrap proves anonymity, resistance to traffic analysis,
correctness against hostile peers, complete identity interoperability, secure
recovery/rotation, protocol interoperability, or safe public-network
operation. Local crypto/storage tests do not replace mixed-router evidence.
Malformed and stress tests must run only in an authorized isolated testnet.

## Plan 036 evidence and artifact sanitation

The Plan 036 integration path is a manual evidence boundary, not a public
network feature. Its manifest pins Java I2P and i2pd revisions, requires a
synthetic private network with reseed/bootstrap disabled, and requires
disposable identities and static keys. The committed preflight rejects private
key markers, identity/static-key files, and packet captures from the evidence
directory. Completed runs may retain only typed outcomes and hashes of
sanitized artifacts/configuration; raw addresses, peer identities, RouterInfo,
I2NP, keys, transcripts, and remote error text must be deleted.

The current checkout has no mixed-router artifacts or results because the
complete runtime wire adapter and authorized testnet are unavailable. This is
recorded as a blocker in `plans/closure/ntcp2-transport/036-closure.md`; neither the fixed-seed testkit
matrix nor pure fuzz campaigns are treated as interoperability evidence.

## Plan 038 harness threats and controls

Plan 038 is a manual evidence boundary, not a public-network feature. The
first host contract is Ubuntu amd64. Preparation and execution are separate:
preparation may install declared packages and fetch only the locked reference
sources, while execution consumes prepared artifacts without downloads, DNS,
reseed, bootstrap, RouterInfo publication, NetDB mutation, or public egress.
Host and tool checks must fail before modifying an unsupported host.

Each scenario uses disposable run state and two Linux namespaces connected only
by a veth pair. Both endpoints leave the host namespace; each namespace
contains only loopback, its expected interface, and directly connected routes.
No default route, DNS path, host bridge, or public canary may work. Route
isolation is the primary control, with namespace-scoped nftables rules as
defense in depth. Isolation is checked before a router starts and cannot be
disabled by a scenario option. Process termination, child draining, namespace
deletion, veth cleanup, and secret-state deletion are all required; a cleanup
failure is a failed scenario.

The harness distinguishes environment smoke, the Plan 041 reference-pair
crosscheck, and i2pr mixed-router evidence. Smoke validates reference startup
and cleanup only. The crosscheck validates Java I2P against i2pd only, using
separate namespaces, explicit network ID 99, staged RouterInfo exchange, and
dual authenticated observations. Only bounded authenticated
i2pr-to-reference runs in both directions can contribute to a mixed-router
claim. Sanitation retains typed outcomes, bounded run metadata, and hashes of
sanitized artifacts/configuration; it deletes raw addresses, peer identities,
RouterInfo, I2NP, keys, transcripts, raw logs, and arbitrary remote error text.

## Plan 043 build-system trust domains and promotion controls

Plan 043 treats the build runner as two security domains with a one-way
promotion boundary. Network-enabled preparation may install only the exact
Ubuntu package set from `references.lock.toml`, fetch only the full locked
Java I2P/i2pd revisions and verified IzPack artifact, and resolve declared
build dependencies. It records host/tool/source/build/artifact metadata and
must leave no router process or namespace behind. Preparation output is input
to execution, not proof of protocol behavior.

Execution is offline and fail-closed. It restores only a verified cache,
re-hashes the complete runtime tree, and runs references in disposable
namespaces connected only by synthetic veth peers. Default routes, DNS,
forwarding, host bridges, public egress, reseed/bootstrap, updates, and
unbounded discovery are prohibited. A cache miss, metadata mismatch, network
attempt, or failed isolation check stops before router startup.

The ordered gates are `contract`, `reference-build`,
`reference-offline-reuse`, `environment-smoke`,
`reference-crosscheck-ipv4`, `i2pr-handshake-smoke-ipv4`, `full-matrix`,
`evidence-validation`, and `cleanup-verification`. Reference control is
deliberately before i2pr execution: the Java I2P/i2pd pair must use private
network ID 99, strict staged RouterInfo validation/import, controlled
directions, and dual authenticated observations. This proves harness health,
not i2pr interoperability. The i2pr gate requires four independent
authenticated i2pr/reference IPv4 directions and bounded DeliveryStatus
exchange; one direction or listener readiness cannot stand in for the others.

The evidence threat model includes both leakage and false success. The
aggregate manifest and each typed record are checked for expected scenario
coverage, exact cache/build hashes, placeholders, forbidden content, private
absolute paths, and cleanup disposition. Raw logs, packet captures, endpoints,
RouterInfo, identities, keys, payloads, and mutable run roots are deleted
before retention. Only the narrow sanitized JSON/hash allowlist may cross a
workflow artifact boundary.

Cleanup is an independent security property, not a postscript to protocol
success. An always-run cleanup path terminates and drains children, removes
owned namespaces/veths, deletes secret-bearing run roots, and records bounded
counters. A separate clean-host verifier must then reject residual interop
namespaces, veths, reference/launcher processes, forbidden retained files, or
attributable global nftables/routes/forwarding changes. A cleanup or verifier
failure remains a failed lane even if all protocol scenarios passed.

The workflow and helper apparatus now expose the complete ordered Plan 043
lane, including clean-host verification and aggregate validation, but no
successful aggregate run or mixed-router i2pr record is present. These gaps are
explicit blockers.
No support-ledger status, RouterInfo capability, or NTCP2 advertisement may
change until the conformance and promotion requirements are met.

## Plan 044 mixed-router evidence boundary

Plan 044 composes the mixed-router execution model with four directional
i2pr/reference IPv4 scenarios. Each direction has its own namespace pair,
firewall policy, startup order, and evidence record. No direction may mask
another.

The data-phase oracle must not rely on an echo assumption; it uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition. Zero-filled required hashes and required counters at
zero are rejected.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued records
fail the gate. Protocol success never overrides cleanup failure.

Negative scenarios must define injection point, responsible side, expected
typed result, maximum runtime, expected counters, cleanup expectations, and
whether the reference process is expected to remain healthy. A negative case
passes only when the expected bounded rejection occurs; an unexpected
successful session is a test failure.

No completed mixed-router i2pr record is present in this checkout. These are
explicit blockers, not skipped successes. NTCP2 remains experimental and
non-advertised.

## Plan 046 rootless sealed-namespace evidence boundary

Plan 046 replaces the host-global namespace requirement for the primary NTCP2
interoperability evidence path with a rootless, process-scoped sandbox that
an ordinary user can run without `sudo`, passwordless elevation, host
capabilities, setuid helpers, host-visible namespaces, host-visible veths, or
host nftables mutation. The primary evidence topology is
`rootless-sealed-single-netns` with privilege model `unprivileged-userns`;
the legacy `privileged-dual-netns-veth` topology is renamed and reserved
for explicit later qualification work, never the default and never a silent
fallback.

The lane refuses the entire host-privilege surface that the privileged
topology required: no `sudo`, no `setcap`, no file capability, no ambient
host capability, no `--privileged` container, no `--network host` container,
no `ip netns add`, no host link mutation, no host route mutation, no host
nftables mutation. The single-ID UID/GID mapping with `setgroups deny` and
`no_new_privs` makes the inner user namespace structurally equivalent to
the invoking host user. Any deviation is a typed blocker, not a fallback.

The mixed-router evidence schema now carries `topology_kind`,
`privilege_model`, `sandbox_attestation_sha256`, and
`parent_network_state_unchanged`. A passed record that violates any of these
is rejected. The aggregate manifest verifies that all four handshake-smoke
scenario records reference the same gate attestation, that the attestation
exists, validates, and matches the scenario-record commit, and that the
parent-network state digest pre/post is byte-equal. Cleanup failure overrides
protocol success in the rootless lane exactly as it does in the privileged
lane.

The retained evidence claim is intentionally narrower than the privileged
topology: the rootless lane proves protocol compatibility (real TCP
sockets, exact bind addresses, RouterInfo validation, NTCP2 obfuscation and
Noise handshakes, authenticated link promotion, encrypted frame write/read
paths, directional I2NP send/receive, process lifecycle/deadline/cancellation/
cleanup) inside a single process-scoped network namespace, not separate-stack
network behavior, asymmetric firewall semantics, packet loss, route mutation,
or interface-failure semantics. Those remain explicit later qualification
work for the privileged dual-namespace backend or a future rootless dual-
namespace supervisor. A typed probe blocker such as
`blocked_unprivileged_user_namespace`, `blocked_loopback_unconfigured`,
`blocked_synthetic_bind_failed`, or `blocked_external_connect_succeeded`
is a hard stop. NTCP2 remains experimental and non-advertised; Milestone 3
remains open.

### Plan 046 closure

Plan 046 is closed with a typed host-level blocker on this checkout.
`unshare -U -r --map-root-user` returns `Operation not permitted` on
`/proc/self/uid_map` because `kernel.apparmor_restrict_unprivileged_userns=1`
confines every unprivileged user namespace to a restrictive AppArmor
policy, even though `kernel.unprivileged_userns_clone=1` permits the
namespace itself. The ordinary invoking user has no `CAP_MAC_ADMIN` and
no other lever to lift that policy, and Plan 046 forbids `sudo`. The
probe and the wrapper write the canonical typed blocker
`blocked_unprivileged_user_namespace` to the `--attestation-output`
path on disk, and the on-host evidence directory
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`
carries that blocker plus a kernel/sysctl/capability snapshot. The
lane is runnable by an ordinary user on hosts where the AppArmor
restriction is `0` (or AppArmor is unloaded); cross-host recovery is
recorded in `plans/implementation/ntcp2-transport/047-cross-host-rootless-lane-expansion.md`.

## Plan 048/049 VM recovery boundary

The host-level Plan 046 AppArmor restriction remains unchanged. Plans 048 and
049 place the permissive user-namespace policy only in a disposable Ubuntu
24.04 amd64 Multipass guest. Cloud-init is administrative provisioning; the
actual probe and four-direction evidence run as the capability-free, non-sudo
`i2ptest` user. The source archive and pinned cache are transferred, never
mounted, and execution is offline after the guest-only egress policy is
installed.

Plan 049 separates the stable environment ID from the run ID and concrete
instance name/generation. Plan 050 minimizes the cloud-init unit (no
`rustup` or host toolchain inside the guest), adds a sanitized cloud-init
failure taxonomy, a `--guest-probe-only` flow, and a selective-purge
remediation that requires a verified ownership contract.
The host writes a versioned lifecycle reservation
atomically before launch and serializes operations with a per-run/per-instance
lock. A root-owned guest contract contains the environment/run/generation
identity and ownership-token hash; the host retains only the token digest.
Ownership requires matching host and guest contracts, environment/cloud-init
digests, phase-specific source/cache digests, guest policy, execution-user
privileges, mounts, snapshots, and process state. A name match is never enough.

Inspection is read-only. Adoption, resume, recreation, and destruction require
explicit operator intent and proven ownership. Existing unowned, incompatible,
ambiguous, or deleted-but-unpurged instances are not mutated. Global
`multipass purge` is forbidden in the normal lifecycle. Recreated instances
increment a generation, and snapshots are allowlisted and bound to that
generation and environment contract.

The host baseline probe and guest rootless probe are separate security facts.
The host `blocked_unprivileged_user_namespace` result remains informational;
guest execution requires `rootless_sandbox_available` after provisioning and
again before routers start. The export boundary permits only the sanitized
bundle, independently recomputes hashes, rejects links/devices/FIFOs/sockets/
hardlinks and oversized or unexpected files, and atomically places evidence
under `target/interop/evidence/multipass/<run-id>/`. VM destruction preserves
that directory. Every directional record carries environment, run/generation,
ownership, and probe attribution; a pre-router blocker writes environment
blocker evidence and cannot become a protocol pass. A source/cache mismatch,
failed rootless probe, offline-control failure, cleanup failure, or
evidence-validation failure is a typed blocker; no privileged fallback or
support-ledger advance is permitted.
