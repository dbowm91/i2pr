# ELS2 external-qualification reference freeze — Plans 374 and 375

Status: **reference freeze executed; Java source proof complete; live matrix not executed.**

This file is the "before execution" section both Plans 374 and 375 require. It
records what was verified about the reference routers and the host substrate,
so the next execution pass does not repeat the discovery work and so a future
reader can tell exactly how far the external lane actually got.

It is **not** evidence that any Java ELS2 direction passed. Plan 374's i2pd
source proof and Plan 375's Java source proof are recorded below. The i2pd
matrix was subsequently completed under Plan 406; Plan 375's live matrix
remains open.

Produced: 2026-10-07; Java source proof extended 2026-10-09. Host: Linux
x86_64, unprivileged, loopback-only.

## 1. Pins and builds

Both references were built **unmodified** at their exact pins. Nothing was
patched, vendored, or copied into this repository.

| Reference | Pin | Version | Build result |
|---|---|---|---|
| i2pd | `PurpleI2P/i2pd@635b013a612ff47278ef02acf8580a28e10e26c5` | 2.61.0 (0.9.70) | built clean from source; binary 105 649 768 bytes |
| Java I2P | `i2p/i2p@9134f808337b401e8e53c73734c81fab04280c9d` | 2.13.0 | `:core:jar` + `:router:jar` built clean (`BUILD SUCCESSFUL`, 7 tasks) |

Build notes, both of which cost real time and would cost it again:

- **i2pd** builds with its stock `Makefile` (`make -j`). It needs `cmake`,
  `g++`, `openssl`, `boost`, and `zlib`. The binary lands at the **repository
  root** (`./i2pd`), not under `daemon/`.
- **Java I2P** must be built with **JDK 21**, not the host default. Gradle 8.5
  fails with `Unsupported class file major version 69` under JDK 25. The
  project's own `sourceCompatibility`/`targetCompatibility` are both 17.
  `--offline` also fails, because the `me.champeau.jmh` plugin still has to be
  resolved from the plugin portal.

The repo's own fetch/build tooling was used for the i2pd cache:
`I2PR_I2PD_SRC=<pinned checkout> bash scripts/interop/fetch-ssu2-reference.sh`.
The Java fetch in that script clones a default branch and then fails its pin
check (`expected 9134f808…, got a629ec7c…`); a correctly pinned checkout must be
supplied as an override, as with i2pd.

## 2. The controlled mesh substrate runs on this host

This is the single most useful fact in this file, and it was not known before
it was tested.

The Plan 303/306 controlled loopback mesh — the substrate Plans 374 and 375
both require — **passes here**:

```text
bash tests/integration/floodfill/run-i2pd.sh
==> i2pd reference: 2.61.0 (635b013a612ff47278ef02acf8580a28e10e26c5)
==> phase 1: controlled identity + controlled activation
    published 681 bytes
==> reference floodfills up on 127.0.0.1:43831,127.0.0.1:43832
==> seeded controlled RouterInfo into the reference client's netDb layout
==> reference client up on 127.0.0.1:43833
==> phase 2: qualification matrix
==> boundary probe (diagnostic-only): reference rejected a versioned controlled RouterInfo
Plan 278 lane passed; sanitized evidence: target/interop/m12-floodfill-evidence
```

Three stock i2pd processes (two floodfills plus one client) and a controlled
i2pr floodfill form a real multi-process SSU2 mesh on loopback with no reseed
and no public-network dependency. That mesh is the topology Plan 374 names as
R / F / D, and its floodfill already stores and serves records to a stock
reference client.

So the external ELS2 lane is **not** blocked by the environment. It is blocked
by work that has not been written.

## 3. Source proof: stock i2pd 2.61.0 ELS2 surfaces

Plan 374 requires this source proof before execution, so that a mode can only
be `reference-not-applicable` with pinned evidence. Recorded here so it is not
repeated.

### 3.1 Consumer side (Direction A is reference-feasible)

i2pd can resolve an ELS2 destination by blinded key and open a stream to it:

- `libi2pd/Destination.cpp:778` `RequestDestinationWithEncryptedLeaseSet`
  derives the **storage key** from the `BlindedPublicKey`
  (`dest->GetStoreHash()`), checks its lease-set cache, and otherwise issues
  `RequestLeaseSet(storeHash, …)` — a real blinded-key `DatabaseLookup`.
- `libi2pd/Destination.cpp:1276` wires that into stream creation, and gates the
  result on `ls && !ls->IsIncompatibleCrypto()` before calling `CreateStream`.
- `libi2pd_client/SAM.cpp:1369` documents the addressing rule from a client's
  point of view: a `.i2p` host is "either a plain b32 (ident hash) or a
  **blinded b33 (encrypted leaseset)**", and the b33 path resolves the
  lease set before sending.

**Direction A is therefore reference-feasible at the pin.** It is not blocked
by a missing i2pd capability.

### 3.2 Publisher side (Direction B is reference-feasible)

`libi2pd/Destination.cpp:1548` builds the outer record through
`LocalEncryptedLeaseSet2(ls2, m_Keys, GetAuthType(), m_AuthKeys)`, and
`libi2pd_client/I2CP.cpp:867` parses `CreateLeaseSet2Message` carrying the outer
layer. i2pd publishes ELS2 and stores it under `GetStoreHash()`.

**Direction B is therefore reference-feasible at the pin.**

### 3.3 Authorization modes: all three are implemented

`libi2pd/LeaseSet.h:292-294` defines the complete closed vocabulary, and
`LeaseSet.cpp:990-991` shows all of them reaching the wire:

```c++
const int ENCRYPTED_LEASESET_AUTH_TYPE_NONE = 0;
const int ENCRYPTED_LEASESET_AUTH_TYPE_DH   = 1;
const int ENCRYPTED_LEASESET_AUTH_TYPE_PSK  = 2;
```

`layer1Flags |= 0x01` for DH (scheme 0, auth bit 1) and `layer1Flags |= 0x03`
for PSK (scheme 1, auth bit 1). `Destination.h:80-83` exposes the I2CP
configuration keys: `i2cp.leaseSetPrivKey` (PSK decryption key),
`i2cp.leaseSetAuthType`, and `i2cp.leaseSetClient.psk` for the per-client
PSK key. `Destination.cpp:82` bounds the accepted type to
`NONE..PSK`.

**Corrected by Plan 381, then corrected again by Plan 381 §WP1.** The original
line read `i2cp.leaseSetClient.psk[.nnn]`, implying an indexed group. Plan 381
first "corrected" that to a single bare key on the strength of reading strings
out of the pinned 2.61.0 binary
(`target/interop/cache/ssu2/i2pd/635b013a…/bin/i2pd`), which contains the
literal `i2cp.leaseSetClient.psk` once and never a `.nnn` spelling.

**That correction was wrong, and it is corrected forward here rather than left
to mislead.** `strings` could not have shown otherwise: the implementation
never contains an indexed spelling as a literal, because the reader is a
**prefix match**, not a keyed lookup. `libi2pd_client/ClientContext.cpp:465-473`:

```c++
void ClientContext::ReadI2CPOptionsGroup (const Section& section, const std::string& group,
    i2p::util::Mapping& options) const
{
    for (auto it: section.second)
    {
        if (it.first.length () >= group.length () && !it.first.compare (0, group.length (), group))
            options.Insert (it.first, it.second.get_value (""));
    }
}
```

`libi2pd/Destination.cpp:1607-1622` matches the same way. So **both** the bare
`i2cp.leaseSetClient.psk` **and** the indexed `i2cp.leaseSetClient.psk.0` are
accepted; the `[.nnn]` in the original line was right all along. The source
comment at `libi2pd/Destination.h:82` says so outright: `// group of
i2cp.leaseSetClient.psk.nnn`.

**The real constraint is the colon, and it is much worse than a wrong key
name.** `ReadAuthKey` takes everything *after* the first `:` as the base64
key and **silently skips any entry whose value has no colon**:

```c++
auto pos = it.second.find (':');
if (pos != std::string::npos)
{ /* ... AuthPublicKey::FromBase64(it.second.substr (pos + 1)) ... */ }
// no else: a colon-less value is dropped with no diagnostic at all
```

Verified by running the pinned binary against a generated `tunnels.conf`, one
variant per key spelling, reading `Destination: <N> auth keys read` out of the
log:

| `tunnels.conf` line | i2pd 2.61.0 result |
|---|---|
| `i2cp.leaseSetClient.psk = <base64>` | **no auth keys read** |
| `i2cp.leaseSetClient.psk = 0:<base64>` | 1 auth key read |
| `i2cp.leaseSetClient.psk.0 = <base64>` | **no auth keys read** |
| `i2cp.leaseSetClient.psk.0 = 0:<base64>` | 1 auth key read |
| *(no client key line)* | no auth keys read |
| `i2cp.leaseSetAuthType = 0`, no key line | no auth-key line at all |

The `<n>:` prefix is discarded; only the part after the colon is the key. A
plain base64 value is therefore accepted by the config, dropped without
diagnostic, and the destination publishes with an empty auth-key set — which
surfaces later as an authentication failure indistinguishable from a crypto
defect. That is the failure this freeze document exists to prevent, and it is
a *worse* trap than a misspelled key, because a misspelled key at least
changes the file.

**The plan-number group is also mode-coupled and must be paired correctly.**
`libi2pd/Destination.cpp:1088-1091` selects the group *by auth type*, so a PSK
key must be paired with `i2cp.leaseSetAuthType = 2` (`PSK`); paired with `1`
(`DH`) i2pd reads `i2cp.leaseSetClient.dh` instead and finds nothing. The
modes are `NONE = 0`, `DH = 1`, `PSK = 2` (`libi2pd/LeaseSet.h:290-292`).

**Source-level, not `strings`-level.** The pinned tree **is** readable at
`635b013a612ff47278ef02acf8580a28e10e26c5` (`git describe` → `2.61.0`), so
Plan 381's other recorded caveat — that the source tree is not retained and a
source-level claim would need a re-fetch — is **retired**. Every line number
above is a source citation.

The stock configuration surface is `i2cp.leaseSetType` = 5 with
`i2cp.leaseSetAuthType` in {0, 1, 2} plus the lookup-secret (subcredential)
input.

**Consequence for Plan 374's capability matrix.** All three modes — no-auth,
DH/X25519, and PSK — are implemented by the reference, so *none* of them may be
marked `reference-not-applicable`. Each is mandatory in both directions, with
authorized success, missing-credential failure, and wrong-credential failure.
That is six direction×mode success rows and twelve credential-failure rows
before the nine negative rows, the persistence rows, and the rollover rows are
counted.

### 3.4 Seven facts the live lane had to execute to discover

Added by Plan 381 §WP2. None of these is guessable from the configuration
document, and every one of them presents as something that looks like a crypto
or topology defect. Each was found by running the lane and is now asserted in
`run-i2pd-els2.sh`, `els2-tunnels-conf.sh` or `sam_b33_connect.py` so it cannot
be re-learned by the next executor.

1. **`keys` must be a bare filename.** `i2pd::fs::DataDirPath` *prepends* the
   data dir (`libi2pd/FS.h:175-181`) and `ClientContext::LoadPrivateKeys` opens
   the concatenation (`libi2pd_client/ClientContext.cpp:280`). An absolute path
   becomes `<datadir>//abs/…`, fails to open, and i2pd **silently creates a
   brand-new key pair there** — so the destination gets a different identity
   than the lane configured and every later address derivation is wrong.
   `els2-tunnels-conf.sh` now refuses a `keys` containing a path separator.

2. **The destination `.dat` is not a publication signal.** It is key material,
   written at startup even with zero peers. A peerless first cycle produces one.
   Publication is `NetDb: LeaseSet2 updated` / `Publishing LeaseSet confirmed`.

3. **Seed one direction, never both.** Mutual seeding makes both routers start
   a SessionRequest simultaneously; the AEAD state machines cross and the retry
   fails `Retry AEAD verification failed`. The client initiates; the floodfill
   learns the client from the inbound session — the pattern
   `tests/integration/floodfill/run-i2pd.sh` already uses. Identities must exist
   before either can be seeded, hence two cycles.

4. **A blinded address needs a `.b32.i2p` suffix to reach i2pd.**
   `AddressBook::GetAddress` matches `.b32.i2p` literally and has no `.b33.i2p`
   branch (`libi2pd_client/AddressBook.cpp:454-461`), so `.b33.i2p` falls through
   to the base64 branch and SAM answers `INVALID_KEY`. What makes an address
   blinded is its 35-byte body, not the suffix — and i2pd's own console renders
   the blinded form as `.b32.i2p` (`daemon/HTTPServer.cpp:479-496`).
   `parse_i2pd_els2_destination.py` therefore emits `dest_b33` (i2pr's
   vocabulary) and `dest_b33_i2pd` (the reference's) side by side.

5. **SAM 3.1 `SESSION CREATE` requires `DESTINATION`** — base64 or the literal
   `TRANSIENT` — or it answers `INVALID_KEY`
   (`libi2pd_client/SAM.cpp:427-441`).

6. **`SESSION CREATE` and `STREAM CONNECT` need separate connections.** A
   successful create marks *that connection's* socket type `Session`
   (`SAM.cpp:449`) and `ProcessStreamConnect` then refuses it with
   `Socket already in use` (`SAM.cpp:529-533`). Sessions are bridge-wide, so the
   two commands go on two connections. Writing both on one connection is the
   obvious thing to do and fails with a message that reads like a port clash.

7. **A connect is gated on the client's own tunnel pool.**
   `LeaseSetDestination::IsReady()` is `m_LeaseSet && !expired &&
   m_Pool->GetOutboundTunnels().size() > 0` (`libi2pd/Destination.h:152`), and
   `RequestDestinationWithEncryptedLeaseSet` returns false — which SAM turns into
   `INVALID_KEY` — until then. Minting a `TRANSIENT` destination builds that
   pool, so the `SESSION CREATE` read is legitimately slow.

**Stop condition 2 of Plan 381 is retired by execution.** A stock i2pd client
in the controlled mesh does resolve a blinded address, fetch and decrypt an ELS2
LeaseSet2, build a Streaming session, and carry an application payload to a
stock i2pd publisher's server tunnel. The direction is not blocked on topology.

### 3.5 Java I2P 2.13.0 — Plan 375 source proof

Source was read from the clean checkout at
`i2p/i2p.i2p@9134f808337b401e8e53c73734c81fab04280c9d` on 2026-10-09. The
unmodified stock build completed with JDK 21 (`ant updater preppkg` through
`scripts/interop/fetch-m6-java.sh --rebuild`). This is source feasibility, not
live interoperability evidence.

* **ELS2 modes:** `core/java/src/net/i2p/data/BlindData.java:38-48` defines
  `AUTH_NONE=0`, `AUTH_DH=1`, `AUTH_PSK=3`. These are the blinded-data auth
  identifiers; Java's tunnel configuration surface uses a different selector:
  `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/ui/GeneralHelper.java:685-711`
  maps `i2cp.leaseSetAuthType=0` to none, `1` to DH, and `2` to PSK. For PSK,
  `i2cp.leaseSetClient.psk.0` selects per-client PSK mode; without it Java uses
  shared PSK mode. For DH, `getClientAuths(..., true)` reads consecutive
  `i2cp.leaseSetClient.dh.N` values; PSK uses `i2cp.leaseSetClient.psk.N`.
* **Publisher:** `router/java/src/net/i2p/router/client/ClientMessageEventListener.java`
  accepts I2CP lease-set type 5 for EdDSA and RedDSA destinations and applies
  `i2cp.leaseSetSecret` and `i2cp.leaseSetPrivKey` before creating/publishing
  the encrypted record. `core/java/src/net/i2p/data/EncryptedLeaseSet.java`
  implements NONE/DH/PSK encryption and authorized decryption. Thus all three
  modes are implemented at this pin; none is `reference-not-applicable`.
* **Consumer lookup:** `router/java/src/net/i2p/router/client/LookupDestJob.java:88-145`
  decodes extended Base32 names with a `.b32.i2p` suffix, recognizes a body of
  at least 35 bytes as encrypted LeaseSet2, decodes `BlindData`, derives the
  blinded hash, and issues the ordinary NetDB lookup. It fails closed when a
  required secret or auth private key is unavailable. Its Java-facing suffix
  is `.b32.i2p`; the encoded blinded body is the same B33 address material.
* **Stock SAM configuration:** `apps/sam/java/src/net/i2p/sam/SAMv3Handler.java:420-478`
  preserves unrecognized `SESSION CREATE` parameters in the session properties
  after consuming `ID`, `STYLE`, and `DESTINATION`. This permits the stock SAM
  surface to pass the `i2cp.*` type-5/auth properties without changing Java.
  Java's built-in I2PTunnel UI independently exposes these same settings.

This sizes the authorization portion of Plan 375: NONE, shared PSK, per-client
PSK, and DH configuration are source-supported. Required execution includes
the overlapping supported NONE/PSK/DH modes, authorized success and
wrong/missing credentials, and the separate lookup-secret behavior. The
source-only presence of these branches does not establish live store, lookup,
decryption, tunnel construction, or payload success.

The stock build cache records Java `21.0.12.1` (Eclipse Temurin), Ant `1.10.14`,
build command `ant updater preppkg`, and installed-tree SHA-256
`87a284800507ccc88fd53c7d1b4a0178d0168c5e16dac3a8587dd01af857e5df`. At the
source-proof baseline, i2pr HEAD was `d616f0868db4e56bd311d74095347209d69f5aa0`
and `Cargo.lock` SHA-256 was
`5389da3fa5dcd74e13d4c6421c3a2079c98b4e9ee117a8b575ca9c80e4a92bce`.

## 3.6 Plan 411 Java SAM session-create diagnosis

Plan 411 performed one fresh Java 2.13.0 loopback run, separate from the spent
Plan 279 runner. Its single DATAGRAM `SESSION CREATE` returned `I2P_ERROR` in
the `session-create` stage. Before starting Java, an ephemeral bind probe found
that UDP `127.0.0.1:7655` was unavailable. The exact-pinned source trace
attributes the response to `SAMv3Handler.execSessionMessage`: its DATAGRAM
branch obtains the `SAMv3DatagramServer` before constructing
`SAMv3DatagramSession`; the server binds the default UDP port, and the caught
`IOException` becomes the SAM error reply. Therefore I2CP session creation was
never reached. The sanitized response digest and source-fact digests are
packaged in `tests/integration/els2/evidence/plan411/`; raw response and router
logs were not retained.

This is a host port collision in the SAM helper setup, not an ELS2 result or a
Java protocol defect. A Java driver that needs a DATAGRAM helper must pass an
available loopback `sam.udp.host` and `sam.udp.port` in `SESSION CREATE`.
Plan 411's first post-request runner exit occurred in its source-trace
postprocessor. The source trace was then generated offline from the exact pin
and the evidence checker passed; the SAM command was not repeated.

## 4. Remaining Plan 375 work

Plan 406 delivered the i2pd scope. Plan 375 remains blocked because its Java
ELS2 driver and directions have not been executed. Plan 411 attributed the
earlier SAM setup failure. Plan 412 found that the Java requester direction
also depends on Plan 437 proving exact-pinned selection of controlled F under
a truthful bandwidth tier; Plan 413 owns that handoff and remains blocked on
Plans 433 and 436 through 437. No Java ELS2 direction has passed.

> **Historical Plan 381 note (2026-10-08).** The i2pd-direction half now exists:
> `tests/integration/els2/run-i2pd-els2.sh` plus the driver
> `crates/i2pr-daemon/tests/els2_i2pd_external.rs` carry NONE/PSK/DH payload
> rows, two live negatives, and a mesh authority control, with packaged
> `evidence.json`/`evidence.md` and `scripts/check-els2-live-lane-evidence.sh`
> as the guard. At that point the reverse direction and authority row remained;
> Plan 406 subsequently delivered the i2pd scope. Java remains independently open.

Available substrates include:

- the Plan 303/306 controlled floodfill mesh, which proves *type-1/3/7*
  RouterInfo publication, lookup, and serve against stock references;
- Plan 350/351's i2pr-internal type-5 store/serve/consume, proven
  i2pr-to-i2pr;
- Plan 346's crypto-boundary cross-verification of the deployed type-11
  transcript against executed Java and i2pd signature output.

These substrates alone do not satisfy a Java ELS2 row. Plan 375 forbids
satisfying a row by inserting a decoded LeaseSet into a consumer, sharing an
in-process NetDB between R and F, invoking the resolver with test bytes, or
modifying Java I2P. The lane must:

1. Use stock SAM/I2CP to publish a Java type-5 service and consume an external
   B33 address, exposing a loopback application.
2. Compose with the controlled queried-floodfill topology: F is a separate
   controlled i2pr floodfill, while JC builds tunnels through Java relays JD1
   and JD2. Prove that F is queried and Java does not need R as a tunnel peer.
3. Execute both directions across NONE/PSK/DH and lookup-secret modes, with
   wrong/missing credential, malformed/stale/tampered/wrong-key, restart, and
   adjacent-day rollover cases. Package bounded evidence and a checker that
   rejects local-record injection and topology bypass.

Plan 377 remains blocked until Plan 375's Java artifact passes its checker.

## 5. What this changes for the next pass

- **The environment is proven.** A future pass does not need to re-derive that
  the controlled mesh runs here, that i2pd builds unmodified at the pin, or
  that Java needs JDK 21.
- **The i2pd capability question is answered.** Neither direction is
  `reference-not-applicable`, so Plan 374's matrix is the full one. That is the
  single biggest input to sizing the work, and it was previously unknown.
- **Plan 375's source matrix is sized.** Java implements NONE, DH, and PSK at
  this pin. The stock live driver and complete matrix remain open.

Nothing in this file may be cited as ELS2 interoperability evidence. Type 5
remains `advertised = false`, and no ELS2 capability claim is made.
