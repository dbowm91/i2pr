# ELS2 external-qualification reference freeze — Plans 374 and 375

Status: **reference freeze executed; live matrix not executed.**

This file is the "before execution" section both Plans 374 and 375 require. It
records what was verified about the reference routers and the host substrate,
so the next execution pass does not repeat the discovery work and so a future
reader can tell exactly how far the external lane actually got.

It is **not** evidence that any ELS2 direction passed. No live ELS2 matrix row
has been executed. Both plans remain blocked, and the precise blocker is
recorded below and in their closure records.

Produced: 2026-10-07. Host: Linux x86_64, unprivileged, loopback-only.

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

### 3.4 Java I2P 2.13.0

`core/build/libs/i2p.jar` at the pin contains `net/i2p/data/EncryptedLeaseSet.class`
and `net/i2p/crypto/eddsa/RedDSAEngine.class`. The ELS2 type-5 record type and
the RedDSA engine are both present. Plan 375's source proof is **not** complete
here: the auth-mode vocabulary and the b33 consumer path have not been read at
this pin, and nothing has been executed.

## 4. Why Plans 374 and 375 are blocked

Both plans are blocked by **the same specific, named gap**, and it is not an
environmental one.

The repository has no ELS2 live driver. What exists is:

- the Plan 303/306 controlled floodfill mesh, which proves *type-1/3/7*
  RouterInfo publication, lookup, and serve against stock references;
- Plan 350/351's i2pr-internal type-5 store/serve/consume, proven
  i2pr-to-i2pr;
- Plan 346's crypto-boundary cross-verification of the deployed type-11
  transcript against executed Java and i2pd signature output.

None of those is a live ELS2 row. Plan 374 explicitly forbids satisfying a row
by inserting a decoded LeaseSet into a consumer, by sharing an in-process NetDB
between R and F, by invoking the resolver with test bytes, or by modifying
i2pd. So the lane has to be **written**, not extended:

1. An **R/F/D ELS2 mesh driver** that adds type-5 publication and blinded-key
   lookup to the controlled topology, with R and F as distinct processes.
2. An **i2pd service/client driver** built only from stock configuration and
   public output — creating an encrypted service, obtaining its b33, setting
   the lookup secret / PSK / DH material with stock syntax, and exposing a
   loopback application behind it.
3. The full matrix: 2 directions × 3 auth modes × 3 credential scenarios, the
   nine negative rows, the persistence and daily-rotation rows, and a bounded
   machine-readable artifact with a checker that fails when a mandatory row
   lacks `store → lookup → decrypt/validate → application` provenance.
4. The same again for Java in Plan 375, against a **different** auth-mode
   vocabulary that has not yet been read at the pin.

This is a substantial piece of new external-integration engineering. A partial
lane that "passes" would be worse than an honest block: the repo's guardrails
forbid early-return-success, and a half-executed matrix recorded as evidence is
exactly the failure mode the evidence checkers exist to prevent.

## 5. What this changes for the next pass

- **The environment is proven.** A future pass does not need to re-derive that
  the controlled mesh runs here, that i2pd builds unmodified at the pin, or
  that Java needs JDK 21.
- **The i2pd capability question is answered.** Neither direction is
  `reference-not-applicable`, so Plan 374's matrix is the full one. That is the
  single biggest input to sizing the work, and it was previously unknown.
- **Plan 375 is smaller than it looks but not small.** Java has the type-5
  record type and the RedDSA engine. Whether Java implements the DH/PSK
  authorization variants is unread at this pin and must be read before Plan
  375's matrix is sized — it is the same source-proof step this file performed
  for i2pd.

Nothing in this file may be cited as ELS2 interoperability evidence. Type 5
remains `advertised = false`, and no ELS2 capability claim is made.