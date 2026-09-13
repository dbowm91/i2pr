# Plan 197 — M6 PQ SSU2 option support corrective (tolerant parse only)

Status: **registered, executable next plan**. Plan 196's §10.B stop
condition has fired; this plan is the only registered follow-up
before the Plan 196 external Java lane can re-run. Plan 194 remains
the actual Java second-family qualification/closure gate; Plan 195
remains blocked by Plan 194.

## 1. Goal

Make `Ssu2RouterAddress::parse` tolerate the SSU2 `pq` KEM-scheme
option the exact-pinned Java I2P 2.13.0 reference router publishes
in every RouterInfo SSU2 address. i2pr does not implement ML-KEM
key exchange and never publishes the option; Plan 197 is a parse
tolerance only, not a crypto change.

The Plan 196 §10.B stop recorded
`Ssu2RouterAddress::parse` returning `Ssu2AddressError::UnknownOption`
at `crates/i2pr-transport-ssu2/src/address.rs:885` for
`pq=4,3`, which propagates to
`crates/i2pr-daemon/src/router_i2np.rs:798` as
`Ssu2ServiceError::InvalidIdentity` and breaks the
`destination_message_plane_against_java` driver in
`crates/i2pr-daemon/tests/java_tunnel_external.rs:183` before any
authenticated SSU2 transcript begins.

Plan 197 must:

1. Recognize the `pq` option as a bounded comma-separated list of
   KEM-scheme identifiers (Java convention: `4` = ML-KEM-768,
   `3` = ML-KEM-512; the field's KEM id space is *not* the same as
   `i2pr_proto::CryptoKeyType` codes 5/6/7 because Java omits the
   X25519 hybrid).
2. Surface the parsed schemes as a typed, bounded
   `Ssu2PqKem`/`PqCapabilities` value on `Ssu2RouterAddress`.
3. Continue to never publish `pq` on the i2pr side; ML-KEM
   negotiation is not implemented and never will be silently
   enabled by this corrective.
4. Keep the Plan 193 first-family i2pd 2.61.0 lane green (i2pd
   2.61.0 does not publish `pq` and Plan 161 has no PQ surface).
5. Lint the existing Plan 196 controlled-topology harness only
   where the parser change can regress it; do not extend the
   harness.

Plan 197 closes only when the existing Plan 196 external driver
records an authenticated SSU2 preflight pass against the exact-pinned
Java I2P 2.13.0 cache and the retained i2pd 2.61.0 evidence remains
green. It does **not** claim ML-KEM key exchange, a third PQ KEM,
PQ session negotiation, a plan-level PQ crypto milestone, or any
tunnel/NetDB/Destination/Streaming interop. Those remain Plan 194.

## 2. Starting authority

Starting repository authority (snapshot at the time Plan 197 is
registered; current state in
[`plans/196-status.md`](196-status.md) and
[`plans/README.md`](README.md)):

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product       = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_195 = registered-blocked-by-plan194
plan_196 = in-progress-corrective-implementation-landed-static-checks-green-stopped-at-§10B-authenticated-ssu2-pq-option-rejection
plan_197 = registered-m6-pq-ssu2-option-support-corrective
```

Reference pins retained unchanged:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
role   = mandatory Plan 161 first-family SSU2 reference (no pq option)

Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
role   = mandatory Plan 194 second-family reference (unconditionally publishes pq=4,3)
```

The Plan 196 §10.B recorded evidence (verbatim from
`target/interop/m6-java-evidence/external-driver.log`) is retained
as the bug-stop provenance and the Plan 197 closure proof must
replay it as `passed` after a pure-parser rerun.

```text
thread 'destination_message_plane_against_java' panicked at crates/i2pr-daemon/tests/java_tunnel_external.rs:183:54:
verify java RouterInfo: InvalidIdentity
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

## 3. Research findings that define the correction

### 3.1 Java 2.13.0 SSU2 `pq` publishing is unconditional

Pinned `router/java/src/net/i2p/router/transport/udp/UDPTransport.java`
of the Java I2P 2.13.0 pin (`9134f808337b401e8e53c73734c81fab04280c9d`)
unconditionally writes the SSU2 `pq` option for every SSU2 address it
publishes (IPv4 and IPv6, direct, and firewalled/introducer-only):

```text
145:    static final int SSU2_INT_VERSION = 2;
146:    /** "2" */
147:    static final String SSU2_VERSION = Integer.toString(SSU2_INT_VERSION);
148:    /** 0 to disable in/out, or 4/3 for preferred enctypes 6/5 */
149:    static final int PQ_INT_VERSION = 4;
150:    /** inbound support 4,3 or 4 or 3 */
151:    static final String PQ_VERSION = "4,3";
152:    /** inbound support if low MTU */
153:    static final String PQ_VERSION_LOW_MTU = "3";
...
1011:    private void addSSU2Options(Properties props, int mtu, boolean isIPv6) {
1012:        // Unlike in NTCP2, we need the intro key whether firewalled or not
1013:        props.setProperty("i", _ssu2B64StaticIntroKey);
1014:        props.setProperty("s", _ssu2B64StaticPubKey);
1015:        props.setProperty("v", SSU2_VERSION);
1016:        if (PQ_INT_VERSION != 0) {
1017:            int min = isIPv6 ? PeerState2.MIN_MLKEM768_IPV6_MTU : PeerState2.MIN_MLKEM768_IPV4_MTU;
1018:            String ver = (mtu == 0 || mtu >= min) ? PQ_VERSION : PQ_VERSION_LOW_MTU;
1019:            props.setProperty("pq", ver);
1020:        }
1021:    }
```

Pin and source:

```text
https://github.com/i2p/i2p.i2p/blob/9134f808337b401e8e53c73734c81fab04280c9d/router/java/src/net/i2p/router/transport/udp/UDPTransport.java
```

Required observations:

1. `pq=4,3` is the canonical high-MTU publish; `pq=3` is the
   low-MTU publish (when `mtu < MIN_MLKEM768_IPV4_MTU`). Java never
   emits an empty value at runtime.
2. Java's identifier space for SSU2 `pq` is *not* the same as
   `i2pr_proto::CryptoKeyType` (5/6/7). Java's `4` corresponds to
   ML-KEM-768 (the protocol's KEM-768 indicator for the inbound KEM
   negotiation); Java's `3` corresponds to ML-KEM-512. The comment
   `4/3 for preferred enctypes 6/5` confirms the relationship to
   LeaseSet2 enctypes without affecting the SSU2 wire format.
   Plan 197 must keep the two identifier spaces independent.
3. The option is comma-separated plain decimal integers, no
   whitespace, no padding, no leading sign, no hex.
4. The exact-pinned Java reference publishes the option for every
   SSU2 address regardless of firewalled/direct form, so the
   parser tolerance must apply to all four
   `Ssu2AddressClass` variants.

### 3.2 i2pd 2.61.0 does not publish `pq`

The first-family pin in `635b013a612ff47278ef02acf8580a28e10e26c5`
predates SSU2 PQ publishing; its `SSU2.cpp` does not emit a `pq`
option. Plan 161's retained i2pd evidence, the
`scripts/check-ssu2-acceptance-evidence.sh` fixture, and the
`verify_reference_router_info` consumer path therefore never see
`pq`. Plan 197 must keep every i2pd 2.61.0 fixture and external
evidence row passing unchanged: a regression in either direction
forces a re-run of Plan 161.

### 3.3 Current parser behavior is the i2pr defect

`crates/i2pr-transport-ssu2/src/address.rs:885` rejects unknown
options as `Ssu2RouterAddress::parse`'s default arm:

```text
865:    fn from_entries<I>(entries: I) -> Result<Self, Ssu2AddressError>
866:    where
867:        I: IntoIterator<Item = (&'a str, &'a str)>,
868:    {
869:        let mut parsed = Self::default();
870:        for (key, value) in entries {
871:            if let Some(index) = parse_introducer_index(key) {
872:                parsed.introducer_fields.store(index, value)?;
873:                continue;
874:            }
875:            match key {
876:                HOST_OPTION       => store(&mut parsed.host, value, HOST_OPTION)?,
877:                PORT_OPTION       => store(&mut parsed.port, value, PORT_OPTION)?,
878:                STATIC_KEY_OPTION => store(&mut parsed.static_public_key, value, STATIC_KEY_OPTION)?,
879:                INTRO_KEY_OPTION  => store(&mut parsed.intro_key, value, INTRO_KEY_OPTION)?,
880:                VERSION_OPTION    => store(&mut parsed.version, value, VERSION_OPTION)?,
881:                CAPS_OPTION       => store(&mut parsed.capabilities, value, CAPS_OPTION)?,
882:                MTU_OPTION        => store(&mut parsed.mtu, value, MTU_OPTION)?,
883:                // (no PQ_OPTION arm here)
884:                _ => return Err(Ssu2AddressError::UnknownOption),
885:            }
886:        }
...
```

`ParsedOptions` itself does not retain a `pq` slot. The fix is a
narrow additive seam that does not bend any other vocabulary
member.

### 3.4 Module doc already names the NTCP2 `pq` precedent

`crates/i2pr-transport-ssu2/src/address.rs:11-19` calls out the
NTCP2 `pq` precedent explicitly:

```text
11: //! address conventions shared with NTCP2 (`v`, `s`, `i`, `host`,
12: //! `port`, `mtu`, `caps`) plus SSU2 introducer groups
13: //! (`ihostN`/`iportN`/`ikeyN`/`itagN`). Where the spec leaves
14: //! publication details implementation-defined, this parser is strict:
15: //! unknown options are rejected (no deployed extra option is
16: //! currently accepted; any future one needs an explicit allowlist
17: //! entry, following the NTCP2 `pq` precedent), hostnames are refused,
18: //! and PQ-hybrid `v=3`/`v=4` values are classified as unsupported,
19: //! never as malformed v2.
```

The NTCP2 path already implements the precedent:

```text
crates/i2pr-transport-ntcp2/src/address.rs:38:    const PQ_OPTION: &str = "pq";
crates/i2pr-transport-ntcp2/src/address.rs:714-718:
                // The ``pq`` option carries the I2P padding-queue protocol
                // version; i2pr does not negotiate padding but other
                // routers (notably I2P 2.12.0+) publish it, so accept and
                // ignore it rather than rejecting the whole address.
                PQ_OPTION => continue,
```

The NTCP2 implementation is type-free ("accept-and-ignore"). Plan
197 takes the slightly stronger step: the parsed schemes are
surfaced as a typed value because the i2pr-daemon orchestration
layer can already route typed metadata, and a future PQ crypto
plan of record will need the parsed IDs without re-parsing the
string. The behaviour contract — accept the option, never use it —
remains the same.

### 3.5 No router-role change implied

`Ssu2RouterAddress::address_class` is computed from the endpoint /
introducers tuple only (lines ~574-581 today). Plan 197 must not
change that classifier. The `pq` capability is purely advisory in
the current SSU2 v2 specification: a router that cannot perform
ML-KEM simply ignores inbound KEM material in the handshake and
proceeds with classical X25519. The i2pr session layer is already
classical-X25519-only by the Plan 156/160/161 establishment
contract; Plan 197 does not propose to add or alter a single
session-layer path.

### 3.6 Publication is the only behavioral risk on the i2pr side

`crates/i2pr-transport-ssu2/src/publication.rs` writes the
canonical SSU2 option set (`caps`, `host`, `i`, `ihost*`,
`ikey*`, `iport*`, `itag*`, `mtu`, `port`, `s`) and never `pq`.
Plan 197 must preserve that invariant and add a hard regression
that asserts the option set never contains `pq` on the outbound
path. The Plan 193 first-family i2pd 2.61.0 ping/publish fixtures
in `crates/i2pr-transport-ssu2/tests/handshake.rs` (and any
`publication` snapshot tests already exercised in the SSU2 suite)
must remain green.

## 4. Architecture lock

The bounded parser contract:

```text
i2pr SSU2 v2 session layer    = classical X25519 only (Plan 156/160/161 unchanged)
i2pr SSU2 address parser      = tolerant of pq=X,Y,Z option; surface as typed metadata
i2pr SSU2 publication path    = never emits pq
i2pr RouterInfo RouterAddress = no pq option today; never adds pq under Plan 197
ML-KEM-512 / ML-KEM-768 crypto = NOT implemented; NOT claimed; NOT silently enabled
```

Plan 197 is parser-only. It is not allowed to:

1. introduce or import an ML-KEM implementation;
2. alter the Noise XK transcript, the `Ssu2Transcript`,
   `Ssu2CryptoError`, or any handshake codec;
3. write a `pq` option in any i2pr-side publication path or test
   fixture that semantically counts as i2pr policy;
4. change the `Ssu2RouterAddress::address_class` discriminator;
5. change any wire format, including the pinned Java inbound
   KEM-handling path (we do not process it);
6. widen Plan 193 i2pd 2.61.0 evidence semantics by re-parsing an
   i2pd RouterInfo with `pq` and pretending it is PQ;
7. re-use a returned `pq_capabilities()` value for outbound
   routing decision anywhere in the tree.

The implementation reuses the existing `Ssu2AddressError`
taxonomy; only the `UnknownOption` arm is removed for `pq` and a
single new `InvalidOptionValue { option: PQ_OPTION }` arm is added
for malformed values.

## 5. Required implementation

### 5.1 Add `PQ_OPTION` and a typed PQ surface to `Ssu2RouterAddress`

Primary file:

```text
crates/i2pr-transport-ssu2/src/address.rs
```

Add at the existing constant block (next to `STATIC_KEY_OPTION`,
`INTRO_KEY_OPTION`, etc., lines 44-50):

```text
const PQ_OPTION: &str = "pq";

/// Maximum number of PQ KEM-scheme identifiers accepted from one
/// `pq` option. The pinned Java 2.13.0 router publishes two (`"4,3"`).
/// Eight covers any plausible production variant without unbounded
/// parser allocation.
pub const MAX_SSU2_PQ_SCHEMES: usize = 8;
```

Add a bounded PQ KEM-scheme vocabulary next to
`Ssu2TransportStyle` (around lines 136-157):

```rust
/// A KEM-scheme identifier carried by the SSU2 `pq` option.
///
/// Java I2P 2.13.0 publishes identifiers drawn from the
/// UDPTransport.java PQ-id space (`3` = ML-KEM-512, `4` = ML-KEM-768,
/// document comment: "preferred enctypes 6/5"). The identifier
/// space is intentionally separate from
/// `i2pr_proto::CryptoKeyType` (which uses 5/6/7 for the
/// ML-KEM-*+X25519 hybrids) because Java's value does not imply
/// the X25519 hybrid. Future revisions of the pinned reference
/// may widen the id space; unknown identifiers are retained as
/// `Unknown(u8)` for forward compatibility without a hidden
/// ML-KEM implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ssu2PqKem {
    /// ML-KEM-512 (Java's PQ id `3`).
    MlKem512,
    /// ML-KEM-768 (Java's PQ id `4`).
    MlKem768,
    /// Any other PQ id the parser encountered. i2pr does not
    /// understand the KEM; the value is retained for diagnostic
    /// parity with the wire.
    Unknown(u8),
}

impl Ssu2PqKem {
    /// Parses one scheme identifier from a numeric prefix.
    pub const fn from_code(value: u8) -> Self {
        match value {
            3 => Self::MlKem512,
            4 => Self::MlKem768,
            other => Self::Unknown(other),
        }
    }
}

/// Bounded KEM-scheme list extracted from the SSU2 `pq` option.
///
/// Empty list = `pq` option absent or `pq=` value. The byte string
/// is never re-derived; callers may inspect `as_wire()` to log the
/// canonical form. The list order follows the wire order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PqCapabilities {
    schemes: Vec<Ssu2PqKem>,
    wire: String,
}
```

Public surface that must end up in `crates/i2pr-transport-ssu2/src/lib.rs`:

```rust
pub use address::{
    ConfiguredListenAddress, PqCapabilities, ResolvedDialTarget, Ssu2AddressClass,
    Ssu2AddressError, Ssu2AddressMaterial, Ssu2Capabilities, Ssu2Endpoint, Ssu2Introducer,
    Ssu2PqKem, Ssu2RouterAddress, Ssu2TransportStyle, MAX_SSU2_PQ_SCHEMES,
};
```

`Ssu2PqKem` is bounded by `MAX_SSU2_PQ_SCHEMES`; `PqCapabilities`
carries the canonical wire string for transparent logging.

### 5.2 Parse `pq=X,Y,Z` into `PqCapabilities`

Extend `ParsedOptions` (lines 843-853):

```rust
struct ParsedOptions<'a> {
    host: Option<&'a str>,
    port: Option<&'a str>,
    static_public_key: Option<&'a str>,
    intro_key: Option<&'a str>,
    version: Option<&'a str>,
    capabilities: Option<&'a str>,
    mtu: Option<&'a str>,
    introducer_fields: IntroducerFields<'a>,
    pq: Option<&'a str>, // NEW
}
```

Extend `ParsedOptions::from_entries` (lines 864-889) so the
`PQ_OPTION` arm parses the wire value rather than `continue`-ing
out. Reject malformed values as
`Ssu2AddressError::InvalidOptionValue { option: PQ_OPTION }` (the
single new error arm Plan 197 introduces; do not add new
variants). Use comma-splitting:

```rust
PQ_OPTION => store(&mut parsed.pq, value, PQ_OPTION)?,
```

Then in `from_parsed` (lines 614-681) convert the stored value
into the typed field on `Ssu2RouterAddress`:

```rust
let pq = match options.pq {
    Some(value) => parse_pq_capabilities(value)?,
    None => PqCapabilities::empty(),
};
```

`parse_pq_capabilities` (new) lives next to `parse_capabilities`:

```rust
fn parse_pq_capabilities(value: &str) -> Result<PqCapabilities, Ssu2AddressError> {
    if value.is_empty() {
        return Ok(PqCapabilities::empty());
    }
    let mut schemes = Vec::new();
    for chunk in value.split(',') {
        let trimmed = chunk; // no whitespace allowed by Java; reject if present
        let bytes = trimmed.as_bytes();
        if bytes.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
            return Err(Ssu2AddressError::InvalidOptionValue {
                option: PQ_OPTION,
            });
        }
        // Bounded parse: only the first 8 id codes are retained;
        // any extras are rejected as malformed.
        let numeric = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => unreachable!("ascii_digit filter"),
        };
        let value = match numeric.parse::<u8>() {
            Ok(value) => value,
            Err(_) => {
                return Err(Ssu2AddressError::InvalidOptionValue {
                    option: PQ_OPTION,
                });
            }
        };
        schemes.push(Ssu2PqKem::from_code(value));
        if schemes.len() > MAX_SSU2_PQ_SCHEMES {
            return Err(Ssu2AddressError::InvalidOptionValue {
                option: PQ_OPTION,
            });
        }
    }
    Ok(PqCapabilities::from_parts(schemes, value.to_owned()))
}
```

`PqCapabilities` API:

```rust
impl PqCapabilities {
    /// Empty capabilities (option absent or empty).
    pub const fn empty() -> Self {
        Self { schemes: Vec::new(), wire: String::new() }
    }
    /// Builds from a parsed scheme vector and the canonical wire
    /// string. The wire string is retained for diagnostics; it is
    /// never re-derived from the schemes.
    pub fn from_parts(schemes: Vec<Ssu2PqKem>, wire: String) -> Self {
        Self { schemes, wire }
    }
    /// Returns the parsed scheme list. Empty when the option is
    /// absent or empty.
    pub fn schemes(&self) -> &[Ssu2PqKem] { &self.schemes }
    /// Returns the canonical wire string verbatim. Empty when the
    /// option is absent. Never empty when the value carried a list.
    pub fn as_wire(&self) -> &str { &self.wire }
    /// Returns whether any scheme is recognised by i2pr.
    pub fn is_supported(&self) -> bool {
        self.schemes
            .iter()
            .any(|s| matches!(s, Ssu2PqKem::MlKem512 | Ssu2PqKem::MlKem768))
    }
}
```

Wire-form invariants:

1. `pq=` (empty) parses to `empty()` and is valid.
2. `pq=4` is valid (`[MlKem768]`).
3. `pq=3` is valid (`[MlKem512]`).
4. `pq=4,3` is valid (the canonical Java high-MTU publish).
5. `pq=4,3,2,1,0,...` past 8 identifiers is rejected as
   `InvalidOptionValue { option: PQ_OPTION }`.
6. `pq=4,` (trailing comma) is rejected
   (`InvalidOptionValue { option: PQ_OPTION }`).
7. `pq=,4` (leading comma) is rejected.
8. `pq= 4` (whitespace) is rejected.
9. `pq=4x` is rejected.
10. `pq=999` is valid and surfaces as `[Unknown(999)]`.

### 5.3 Surface `pq_capabilities()` on `Ssu2RouterAddress`

Add a `pq_capabilities: PqCapabilities` field to
`Ssu2RouterAddress` (lines 456-467) and an accessor:

```rust
pub fn pq_capabilities(&self) -> &PqCapabilities { &self.pq_capabilities }
```

The field is initialised in `from_parsed` (lines 614-681) via
`pq` from the parsed options. `Default::default()` for
`PqCapabilities` is `empty()`. No other public function signature
in the crate changes.

### 5.4 Publication must remain `pq`-free

`crates/i2pr-transport-ssu2/src/publication.rs` is unchanged.
Add a regression that verifies the outbound option-set never
includes `pq`:

```rust
#[test]
fn publication_options_never_emit_pq() {
    let snapshot = Ssu2PublicationSnapshot::direct(...);
    for (key, _value) in snapshot.option_entries() {
        assert_ne!(key, "pq", "i2pr must never publish pq");
    }
}
```

The Plan 161 retained-evidence fixtures in
`crates/i2pr-transport-ssu2/tests/handshake.rs`,
`crates/i2pr-transport-ssu2/tests/data_phase.rs`,
`crates/i2pr-transport-ssu2/tests/path_validation.rs`, and
`crates/i2pr-transport-ssu2/tests/peer_relay.rs` must remain
green without modification. The i2pd first-family SSU2 external
lane continues to exercise i2pd RouterInfo that has no `pq`
option, so the empty list naturally flows through the new path.

### 5.5 Regression tests

Primary files:

```text
crates/i2pr-transport-ssu2/src/address.rs        (unit, in-file `#[cfg(test)]` module)
crates/i2pr-transport-ssu2/src/publication.rs    (publication pq-free test)
crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs (i2pd RouterInfo still parses to empty caps)
crates/i2pr-daemon/tests/java_tunnel_external.rs (Java RouterInfo parses to `[MlKem768, MlKem512]`)
```

Required new unit-test rows (in
`crates/i2pr-transport-ssu2/src/address.rs`'s existing
`#[cfg(test)] mod tests` block):

1. `parses_java_high_mtu_pq_options`: address carries
   `pq=4,3` → `parsed.pq_capabilities().schemes()`
   == `[Ssu2PqKem::MlKem768, Ssu2PqKem::MlKem512]`,
   `as_wire() == "4,3"`, `is_supported() == true`.
2. `parses_java_low_mtu_pq_option`: `pq=3` →
   `[Ssu2PqKem::MlKem512]`.
3. `parses_mlkem768_only_pq_option`: `pq=4` →
   `[Ssu2PqKem::MlKem768]`.
4. `parses_empty_pq_option_as_empty_capabilities`: `pq=` →
   empty schemes, empty `as_wire`.
5. `parses_absent_pq_option_as_empty_capabilities`: no `pq`
   key → empty schemes (the i2pd 2.61.0 path).
6. `parses_unknown_pq_scheme_as_unknown`: `pq=999` →
   `[Ssu2PqKem::Unknown(999)]`, `is_supported() == false`.
7. `parses_mixed_known_and_unknown_pq_schemes`: `pq=4,99,3`
   → `[MlKem768, Unknown(99), MlKem512]`,
   `is_supported() == true`.
8. `rejects_too_many_pq_schemes`: `pq=1,2,3,4,5,6,7,8,9`
   (9 ids) → `InvalidOptionValue { option: PQ_OPTION }`.
9. `rejects_pq_option_with_leading_comma`: `pq=,4` →
   `InvalidOptionValue { option: PQ_OPTION }`.
10. `rejects_pq_option_with_trailing_comma`: `pq=4,` →
    `InvalidOptionValue { option: PQ_OPTION }`.
11. `rejects_pq_option_with_whitespace`: `pq= 4` →
    `InvalidOptionValue { option: PQ_OPTION }`.
12. `rejects_pq_option_with_non_digit_chars`: `pq=4x` →
    `InvalidOptionValue { option: PQ_OPTION }`.
13. `rejects_pq_option_with_negative_sign`: `pq=-4` →
    `InvalidOptionValue { option: PQ_OPTION }`.
14. `rejects_pq_option_with_decimal_point`: `pq=4.0` →
    `InvalidOptionValue { option: PQ_OPTION }`.
15. `parses_pq_alongside_full_direct_options`:
    `direct_entries()` plus `pq=4,3` → all canonical SSU2 options
    still parse, `pq_capabilities()` carries the schemes, no
    duplicate `pq` conflict (single `pq` key only).
16. `rejects_duplicate_pq_option`: `direct_entries()` plus two
    `pq=` keys → `DuplicateOption { option: PQ_OPTION }`.
17. `i2pd_style_address_without_pq_parses_to_empty_caps`:
    no `pq`, no other PQ-shape keys → `pq_capabilities()` empty.
18. `pq_capabilities_canonical_field_never_re_derived`:
    rewrite the `wire` field directly (only via API) and confirm
    the byte string the parser saw is preserved verbatim.

In `crates/i2pr-transport-ssu2/src/publication.rs` add:

19. `publication_never_emits_pq`: assemble a publication snapshot
    via `direct_snapshot(...)` and assert the entries contain no
    `pq` key (deterministic, no i2pd/Java reference).

In `crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs` add
behind the existing `#[ignore]` external lane (Plan 184 path):

20. `reference_i2pd_routerinfo_has_no_pq_capabilities`: pin the
    first-fixture i2pd RouterInfo bytes from Plan 161 through
    `verify_reference_router_info`, assert
    `parsed.pq_capabilities().schemes().is_empty()`.
    Use only the in-tree fixture; do not require an i2pd
    runtime. This is a CI-fast regression; the external lane
    may still ignore the test by its existing annotation.

In `crates/i2pr-daemon/tests/java_tunnel_external.rs` add a
non-panicking early sanity check that does not require an
external Java runtime:

21. `reference_java_routerinfo_fixture_has_pq_capabilities`: if a
    captured-bytestream fixture of the exact-pinned Java 2.13.0
    RouterInfo is present under the test's ignore annotation,
    parse it and assert
    `pq_capabilities().schemes() == [MlKem768, MlKem512]`. If the
    fixture is absent, the test must be `#[ignore]`d exactly
    like the surrounding lane; do not embed live router-info
    bytes from the run.

`crates/i2pr-daemon/tests/java_tunnel_external.rs:183` itself
must remain the single panic site if the harness returns a
non-Java router-info blob; Plan 197 only removes the panic when
the byte stream is a genuine Java RouterInfo. Do not weaken the
`expect` calls.

### 5.6 Static evidence/integrity checks

`scripts/check-m6-mixed-router-acceptance-evidence.sh` already
enforces the Plan 196 §7 controlled-topology invariants. Plan 197
adds a narrow §8 block:

1. `Ssu2RouterAddress::parse` must accept `pq=4,3` (positive parse
   via any in-tree unit test) — guard against accidentally
   re-introducing the `UnknownOption` default arm for `pq`.
2. `Ssu2RouterAddress` must surface a `pq_capabilities()` accessor
   (or `pq_capabilities`-named public method) — guard against
   surface regression.
3. `i2pr-transport-ssu2/src/publication.rs` must not contain a
   `"pq"` string literal in any `props.setProperty(...)`-style
   push or any matching `key == "pq"` branch (a literal in a
   comment is fine).
4. `crates/i2pr-transport-ssu2/src/lib.rs` re-exports
   `Ssu2PqKem` and `PqCapabilities` (positive re-export check).

The `scripts/check-ssu2-acceptance-evidence.sh` checker is
unchanged. The single-touch on the i2pd path remains an
empty-capabilities expected value at the new accessor; the
checker is type-stable across the change because the previous
behaviour was `UnknownOption` and the new behaviour is a typed
empty slice.

### 5.7 Surface stability

Plan 197 is additive on the public surface. Existing
`Ssu2RouterAddress` methods (`endpoint`, `host`, `port`,
`static_public_key`, `intro_key`, `mtu`, `capabilities`,
`introducers`, `address_class`, `address_material`,
`configured_listen`, `resolved_dial_target`) all keep their
signatures and semantics. Add `pq_capabilities()` next to them
with the same shape. Add `MAX_SSU2_PQ_SCHEMES`, `Ssu2PqKem`, and
`PqCapabilities` to the `pub use` re-exports of
`crates/i2pr-transport-ssu2/src/lib.rs`.

## 6. First-family regression

The retained first-family i2pd 2.61.0 evidence must remain green
without modification. Specifically:

1. `cargo test --locked -p i2pr-transport-ssu2 --all-targets`
   must continue to pass Plan 155–161 unit/vector rows.
2. `bash scripts/check-ssu2-acceptance-evidence.sh` must
   continue to pass without edits.
3. `cargo test --locked -p i2pr-runtime --test ssu2_peer_relay`
   and the Plan 161 host-peer lane must remain green.
4. The Plan 184 preflight test
   `crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs`
   `reference_i2pd_routerinfo_has_no_pq_capabilities` new row
   (test 20) must record empty capabilities for the in-tree
   i2pd 2.61.0 fixture.
5. `crates/i2pr-transport-ssu2/src/publication.rs`
   `publication_never_emits_pq` (test 19) must pass on every
   supported publication snapshot path.

## 7. Second-family external lane proof

Plan 197 passes only when the **existing**
`crates/i2pr-daemon/tests/java_tunnel_external.rs::destination_message_plane_against_java`
driver reaches the bounded authenticated SSU2 preflight gate
against the **exact-pinned Java I2P 2.13.0** cache set up by
Plan 196 §5.4. No edit to the driver, harness, or
`tests/integration/m6-interop/run-java.sh` is required or
allowed by Plan 197. Concretely:

1. `bash scripts/interop/fetch-m6-java.sh --rebuild` already
   produces a clean staged `lib/` jars + `clientApp` config
   under the exact-pinned commit; the Plan 196 controlled
   `ControlledRouter.java` launcher compiled from
   `tests/integration/m6-interop/java/ControlledRouter.java`
   against the staged jars must remain the launch contract.
2. `bash tests/integration/m6-interop/run-java.sh` is the
   authoritative lane. It must produce a non-panicking
   `external-driver.log` that records `session-established`
   rather than the Plan 196 §10.B panic.
3. The `external-session-established-java` row (bound through
   `scripts/check-m6-mixed-router-acceptance-evidence.sh`'s
   `cross_family_row` helper) must transition from `failed`
   with stop provenance `plan194-java-stop` to `passed` with
   evidence key `session-established`.
4. Sanitized evidence must contain only public keys,
   public-router-hash counts, and digest keys; no private
   router key, signing seed, SSU2 session secret, raw token, or
   application plaintext.
5. `scripts/check-ssu2-acceptance-evidence.sh` does not gain a
   new `check-m6-mixed-router-...` cross-link; the SSU2
   checker remains first-family-clean and the cross-family
   ledger records the new evidence through the existing
   Plan 196 §7 harness.
6. The full workspace floor on the closing implementation head
   must pass:
   ```text
   cargo fmt --all --check
   cargo check --locked --workspace --all-targets
   cargo test --locked --workspace --all-targets \
     -- --test-threads=1
   cargo clippy --locked --workspace --all-targets \
     --all-features -- -D warnings
   RUSTDOCFLAGS="-D warnings" cargo doc --locked \
     --workspace --no-deps
   bash scripts/check-m6-mixed-router-acceptance-evidence.sh
   bash scripts/check-ssu2-acceptance-evidence.sh
   bash scripts/check-sam-acceptance-evidence.sh
   bash scripts/check-i2cp-acceptance-evidence.sh
   bash scripts/check-service-tunnel-acceptance-evidence.sh
   bash scripts/check-streaming-tunnel-evidence.sh
   bash scripts/check-netdb-tunnel-evidence.sh
   bash scripts/check-exploratory-tunnel-evidence.sh
   bash scripts/check-destination-tunnel-evidence.sh
   cargo deny check advisories bans sources
   ```
7. Routine Linux CI on the closing head must be green; the
   manual external lane is the only consumer of the
   `#[ignore]`-annotated Java driver per Plan 196.

## 8. Validation commands

Focused local floor:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
```

External first-family gate:

```bash
bash scripts/check-ssu2-acceptance-evidence.sh
```

External second-family gate (the same lane that fired Plan 196
§10.B):

```bash
bash scripts/interop/fetch-m6-java.sh --rebuild
bash tests/integration/m6-interop/run-java.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

Then the repository floor used by current authority:

```bash
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
```

## 9. Acceptance criteria

Plan 197 passes only when all are true:

1. `Ssu2RouterAddress::parse` accepts `pq=4,3`, `pq=3`, and the
   empty/absent `pq` form without changing the document
   behaviour for every other SSU2 option.
2. `Ssu2RouterAddress` retains a `PqCapabilities` value via
   `pq_capabilities()` for every parsed address; absent `pq`
   means an empty slice (this is the i2pd 2.61.0 path).
3. The malformed-value rows (leading/trailing comma, whitespace,
   non-digit, decimal, leading sign, >MAX_SSU2_PQ_SCHEMES ids)
   produce `Ssu2AddressError::InvalidOptionValue { option: PQ_OPTION }`,
   never `UnknownOption` and never a panic.
4. `pub use` in `crates/i2pr-transport-ssu2/src/lib.rs` re-exports
   `Ssu2PqKem`, `PqCapabilities`, and `MAX_SSU2_PQ_SCHEMES`
   alongside the existing SSU2 surface; no other public signature
   changes.
5. The Plan 161 retained i2pd 2.61.0 external matrix continues
   to pass locally and the in-tree `ssu2_daemon_preflight` row
   `reference_i2pd_routerinfo_has_no_pq_capabilities` records
   empty capabilities against the pinned i2pd RouterInfo fixture.
6. The Plan 196 `ControlledRouter.java` driver compiles + runs
   the controlled topology unchanged and the
   `destination_message_plane_against_java` external driver
   records `session-established` against the exact-pinned Java
   I2P 2.13.0 cache.
7. `publication_never_emits_pq` row passes against every
   supported snapshot; no test or fixture path gains a `pq`
   key in the i2pr publication side.
8. `scripts/check-m6-mixed-router-acceptance-evidence.sh`
   Plan 197 §8 invariants pass; the Plan 196 §7 invariants
   remain in force.
9. The full workspace/static/dependency floor and exact-head
   routine CI on the closing head pass green.
10. `plans/197-status.md` records the exact-head implementation
    SHA, exact hosted CI run, exact Java external lane result,
    and the demonstrated next lane boundary.
11. No ML-KEM implementation is added, depended on, advertised,
    or claimed at any layer; the
    `pq_capabilities().is_supported()` flag is purely a
    diagnostic and is not branched on by any production code
    path. Confirm by inspection: `pq_capabilities` is read only
    in tests and the `verify_reference_router_info` consumer at
    most for evidence logging.
12. No assertion ever claims `milestone6_interoperable = passed-via-plan197`.

## 10. Stop conditions

Stop this corrective and register a new narrow plan if any of
the following is proven after the parser-tolerant re-run:

- the controlled Java topology still fails to come up
  (re-confirm `router.config` + `clients.config.d` invariants
  and the cache fingerprint — this is a Plan 196 topology
  regression, not a Plan 197 issue);
- Java RouterInfo parses cleanly with the new tolerances but
  the daemon-owned SSU2 runtime fails to establish an
  authenticated SessionRequest/SessionCreated handshake
  against the exact-pinned reference (a genuine protocol
  defect, owned by a future narrow corrective);
- the i2pd 2.61.0 first-family lane or any Plan 161 retained
  evidence row regresses;
- an i2pd pin later than `635b013a612ff47278ef02acf8580a28e10e26c5`
  publishes `pq` and the new typed surface becomes the only
  path that distinguishes a KEM-id space from the protocol
  `CryptoKeyType` codes; in that case the Plan 197 §5.1
  `Ssu2PqKem` vocabulary may need widening, owned by a
  separate future plan.

In each case preserve the parser tolerance (do not revert the
`pq` arm) and escalate only the newly demonstrated layer.

## 11. Explicitly ruled-out work for this corrective

Do **not**:

1. add an ML-KEM-512 or ML-KEM-768 implementation anywhere;
2. add a PQ X25519 hybrid KeyExchange or modify the Noise XK
   transcript (`Ssu2Transcript`, `derive_data_keys`,
   `HANDSHAKE_DEADLINE_MS`, etc.);
3. write a `pq=` option in any i2pr publication path or test
   fixture used as i2pr policy evidence;
4. extend the `Ssu2AddressClass` discriminator or rewire the
   `address_class()` classifier;
5. mutate `Ssu2Capabilities` or `Ssu2RouterAddress::capabilities`;
6. introduce a wire-emit helper for `pq`;
7. introduce or widen third-party dependency surface;
8. claim `milestone6_interoperable` or any new Milestone;
9. start Plan 194's tunnel/NetDB/LeaseSet2/Streaming
   qualification rows (those resume only after Plan 196 flips
   to `passed`);
10. parse raw i2pd or Java router logs as evidence; only
    command-derived facts count;
11. rerun Plan 193's full 33-row i2pd Streaming matrix unless
    the parser change breaks a Plan 161 row, which itself
    would mean stop condition §10 fired.

## 12. Handoff / authority transition

Before execution (current state):

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_195 = registered-blocked-by-plan194
plan_196 = in-progress-corrective-implementation-landed-static-checks-green-stopped-at-§10B-authenticated-ssu2-pq-option-rejection
plan_197 = registered-m6-pq-ssu2-option-support-corrective

m6_second_family_java              = controlled-launcher-landed-pending-pq-ssu2-corrective
milestone6_i2pd_streaming_interop  = passed-via-plan193
milestone6_interoperable           = not-yet-claimed

next_executable_plan = 197
```

After Plan 197 passes (authenticated SSU2 preflight against
Java 2.13.0 is recorded as `passed`):

```text
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_196 = in-progress-resume-external-execution-against-landed-corrective
          (re-runs the existing Plan 196 lane against the same cache;
           external session-established-java row must flip)
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194

m6_second_family_java              = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
milestone6_interoperable           = not-yet-claimed

next_executable_plan   = 196 (re-run external lane)
remaining_sequence     = 196-execute -> resume-194 -> 195
```

If Plan 196's external re-run alone closes every M6 Java
qualification layer (i2pr-194 §5.1 through §5.5), Plan 194 may
flip to `passed-m6-java-second-family-mixed-router-closure` in
the same evidence pass, advancing:

```text
plan_194 = passed-m6-java-second-family-mixed-router-closure
plan_195 = registered-now-executable-m10-remote-service-final-closure
milestone6_interoperable = passed-via-plan193-and-plan194 (bounded to
  the controlled MVP path actually demonstrated)
next_executable_plan     = 195
```

In every transition, do not claim `milestone6_interoperable`
without a full Plan 194 external two-family workflow pass per
§12 of the original Plan 194 plan of record.

## 13. Plan 196 / 197 interaction

Plan 197 is the only registered follow-up that can flip Plan 196
to `passed-m6-java-controlled-first-run-topology-corrective`.
Plan 196 §10.B stops here; Plan 197 owns the SSU2 `pq` parser
tolerance; Plan 196 retains ownership of the controlled-topology
harness, `ControlledRouter.java`, `run-java.sh`, the static
checker, and the §6 acceptance criteria for the topology itself.
On Plan 197 pass, the Plan 196 closure record is updated by
appending the Java external lane result to the existing §6 row
set and flipping the status; no Plan 196 source file is rewritten
beyond a single §11 status append.

Plan 197 does not own `local-rows-passed`, the `blocked` Java
qualification rows of Plan 194, the M6 mixed-router Streaming
re-run, or any Milestone 10 dependency. Those belong to Plan 194
when it resumes.
