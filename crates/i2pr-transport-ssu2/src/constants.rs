//! SSU2 v2 constants derived from the pinned protocol dossier.
//!
//! Normative traceability: `specs/protocols/09-ssu2.md` and the
//! Milestone 8 source refresh in `specs/SOURCES.md` (I2P website
//! commit `88596022920bdf99f27db27688faf4f204792fcd`; SSU2 page
//! `Completed`, accurate for 0.9.69; re-verified 2026-09-03).
//! Proposal 159/165 are historical/design context only.

/// The SSU2 protocol version implemented by this crate (classical v2).
pub const SSU2_VERSION: u8 = 2;
/// The production I2P network ID required in every long header.
pub const SSU2_NETWORK_ID: u8 = 2;

/// PQ-hybrid SSU2 versions are deferred compatibility-watch debt, not
/// malformed v2. They are classified as unsupported, never parsed.
pub const SSU2_DEFERRED_VERSIONS: [u8; 2] = [3, 4];

/// The I2P-specific Noise protocol name for the initial KDF state.
/// Recorded here so later handshake plans avoid a magic literal; no
/// handshake is performed by this crate.
pub const NOISE_PROTOCOL_NAME: &[u8] = b"Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256";
/// Expected length of [`NOISE_PROTOCOL_NAME`] in bytes.
pub const NOISE_PROTOCOL_NAME_LENGTH: usize = 52;

/// X25519 static/intro/ephemeral key length in bytes (RFC 7748).
pub const KEY_LENGTH: usize = 32;
/// ChaCha20-Poly1305 authentication tag length in bytes.
pub const AUTH_TAG_LENGTH: usize = 16;
/// ChaCha20 nonce length in bytes (4 zero bytes + 8-byte LE counter).
pub const NONCE_LENGTH: usize = 12;
/// Largest packet-number counter value that may be transmitted.
pub const MAX_PACKET_NUMBER: u64 = u64::MAX - 1;

/// Connection-ID length in bytes (both header forms).
pub const CONNECTION_ID_LENGTH: usize = 8;
/// Packet-number field length in bytes (both header forms).
pub const PACKET_NUMBER_LENGTH: usize = 4;
/// Address-validation token length in bytes (long header).
pub const TOKEN_LENGTH: usize = 8;
/// Long-header length in bytes (pre-handshake and out-of-session).
pub const LONG_HEADER_LENGTH: usize = 32;
/// Short-header length in bytes (SessionConfirmed and Data).
pub const SHORT_HEADER_LENGTH: usize = 16;
/// First bytes of any header sharing the connection-ID/packet-number/type prefix.
pub const HEADER_COMMON_PREFIX_LENGTH: usize = CONNECTION_ID_LENGTH + PACKET_NUMBER_LENGTH + 1;

/// Minimum UDP datagram length accepted as SSU2 (spec §Messages).
pub const MIN_DATAGRAM_LENGTH: usize = 40;
/// Maximum UDP datagram length over IPv4 (spec §Messages).
pub const MAX_DATAGRAM_IPV4_LENGTH: usize = 1472;
/// Maximum UDP datagram length over IPv6 (spec §Messages).
pub const MAX_DATAGRAM_IPV6_LENGTH: usize = 1452;
/// Minimum authenticated payload (plaintext + MAC tail) required after
/// the packet header so header protection has its 24-byte IV window
/// (16-byte MAC plus at least 8 payload bytes).
pub const MIN_POST_HEADER_BYTES: usize = 24;
/// Extra ephemeral-key bytes preceding the authenticated payload in
/// SessionRequest/SessionCreated messages.
pub const HANDSHAKE_EPHEMERAL_LENGTH: usize = KEY_LENGTH;

/// SessionRequest message type.
pub const MESSAGE_SESSION_REQUEST: u8 = 0;
/// SessionCreated message type.
pub const MESSAGE_SESSION_CREATED: u8 = 1;
/// SessionConfirmed message type.
pub const MESSAGE_SESSION_CONFIRMED: u8 = 2;
/// Data message type.
pub const MESSAGE_DATA: u8 = 6;
/// PeerTest message type.
pub const MESSAGE_PEER_TEST: u8 = 7;
/// Retry message type.
pub const MESSAGE_RETRY: u8 = 9;
/// TokenRequest message type.
pub const MESSAGE_TOKEN_REQUEST: u8 = 10;
/// HolePunch message type.
pub const MESSAGE_HOLE_PUNCH: u8 = 11;

/// Minimum SSU2 MTU accepted in a RouterAddress (`mtu` option).
pub const SSU2_MIN_MTU: u16 = 1280;
/// Conservative upper bound for the advisory RouterAddress `mtu`
/// option. The option is advisory only; datagram caps above are
/// enforced independently at the packet layer.
pub const SSU2_MAX_MTU: u16 = 9000;
/// Maximum introducers retained from one RouterAddress.
pub const MAX_SSU2_INTRODUCERS: usize = 3;
/// Maximum bytes accepted for one RouterAddress `caps` option.
pub const MAX_SSU2_CAPS_BYTES: usize = 16;

/// Encoded size of a block type byte plus its big-endian length.
pub const BLOCK_HEADER_LENGTH: usize = 3;
/// DateTime block type.
pub const BLOCK_DATETIME: u8 = 0;
/// Options block type.
pub const BLOCK_OPTIONS: u8 = 1;
/// RouterInfo block type.
pub const BLOCK_ROUTER_INFO: u8 = 2;
/// Complete I2NP message block type.
pub const BLOCK_I2NP_MESSAGE: u8 = 3;
/// First I2NP fragment block type.
pub const BLOCK_FIRST_FRAGMENT: u8 = 4;
/// Follow-on I2NP fragment block type.
pub const BLOCK_FOLLOW_ON_FRAGMENT: u8 = 5;
/// Termination block type.
pub const BLOCK_TERMINATION: u8 = 6;
/// RelayRequest block type.
pub const BLOCK_RELAY_REQUEST: u8 = 7;
/// RelayResponse block type.
pub const BLOCK_RELAY_RESPONSE: u8 = 8;
/// RelayIntro block type.
pub const BLOCK_RELAY_INTRO: u8 = 9;
/// PeerTest block type.
pub const BLOCK_PEER_TEST: u8 = 10;
/// NextNonce block type (spec: TODO, only for key rotation).
pub const BLOCK_NEXT_NONCE: u8 = 11;
/// ACK block type.
pub const BLOCK_ACK: u8 = 12;
/// Address block type.
pub const BLOCK_ADDRESS: u8 = 13;
/// Reserved block type (v2 has no assigned meaning).
pub const BLOCK_RESERVED_14: u8 = 14;
/// RelayTagRequest block type.
pub const BLOCK_RELAY_TAG_REQUEST: u8 = 15;
/// RelayTag block type.
pub const BLOCK_RELAY_TAG: u8 = 16;
/// NewToken block type.
pub const BLOCK_NEW_TOKEN: u8 = 17;
/// PathChallenge block type.
pub const BLOCK_PATH_CHALLENGE: u8 = 18;
/// PathResponse block type.
pub const BLOCK_PATH_RESPONSE: u8 = 19;
/// FirstPacketNumber block type.
pub const BLOCK_FIRST_PACKET_NUMBER: u8 = 20;
/// Congestion block type.
pub const BLOCK_CONGESTION: u8 = 21;
/// First reserved experimental block type.
pub const BLOCK_EXPERIMENTAL_MIN: u8 = 224;
/// Last reserved experimental block type.
pub const BLOCK_EXPERIMENTAL_MAX: u8 = 253;
/// Padding block type.
pub const BLOCK_PADDING: u8 = 254;
/// Reserved future-extension block type.
pub const BLOCK_FUTURE: u8 = 255;

/// Maximum authenticated blocks accepted in one payload. The datagram
/// cap (~1440 payload bytes) bounds real payloads far below this;
/// the count ceiling additionally bounds parser iteration.
pub const MAX_BLOCK_COUNT: usize = 64;
/// Maximum aggregate bytes skipped for unknown/reserved blocks in one
/// payload (forward-compatibility budget). The datagram cap bounds
/// real payloads near 1412 bytes, so this ceiling is reachable and
/// tested, unlike a frame-scale budget.
pub const MAX_UNKNOWN_BLOCK_BYTES: usize = 1024;
/// Maximum RouterInfo bytes retained in one RouterInfo block. Wire-fit
/// against the path MTU is a Plan 156 establishment concern; this is
/// the per-block parser ceiling.
pub const MAX_ROUTER_INFO_BLOCK_BYTES: usize = 4096;
/// Maximum I2NP fragments tracked per message (spec practical limit is
/// 63 or fewer; the wire fragment number allows 1..=127). Reassembly
/// state machines belong to Plan 157; this bounds block metadata.
pub const MAX_I2NP_FRAGMENTS: usize = 64;
/// Largest wire fragment number accepted in a follow-on fragment.
pub const MAX_FRAGMENT_NUMBER: u8 = 127;
/// Maximum ACK ranges retained in one ACK block. Loss interpretation
/// belongs to Plan 157; this bounds block structure.
pub const MAX_ACK_RANGES: usize = 128;
/// Maximum additional termination bytes accepted and then discarded.
pub const MAX_TERMINATION_ADDITIONAL_BYTES: usize = 256;
/// Maximum relay/peer-test trailing signature bytes retained as opaque
/// bounded evidence (64 covers Ed25519; headroom covers larger future
/// signature types without an unbounded allocation).
pub const MAX_SIGNATURE_BYTES: usize = 1024;
/// Maximum path challenge/response data bytes (spec: well under 1280
/// to avoid amplification).
pub const MAX_PATH_DATA_BYTES: usize = 1024;
/// Maximum congestion-block extension bytes beyond the flags byte.
pub const MAX_CONGESTION_EXTENSION_BYTES: usize = 64;
/// Maximum RouterInfo fragments accepted during establishment
/// (Plan 156 owns the policy; the constant is declared here so no
/// later plan invents a magic number).
pub const MAX_ROUTER_INFO_FRAGMENTS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_protocol_name_length_matches_spec() {
        assert_eq!(NOISE_PROTOCOL_NAME.len(), NOISE_PROTOCOL_NAME_LENGTH);
        assert_eq!(
            NOISE_PROTOCOL_NAME,
            b"Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256"
        );
    }

    #[test]
    fn header_lengths_match_spec_layouts() {
        assert_eq!(
            LONG_HEADER_LENGTH,
            CONNECTION_ID_LENGTH + PACKET_NUMBER_LENGTH + 4 + CONNECTION_ID_LENGTH + TOKEN_LENGTH
        );
        assert_eq!(
            SHORT_HEADER_LENGTH,
            CONNECTION_ID_LENGTH + PACKET_NUMBER_LENGTH + 4
        );
        assert_eq!(HEADER_COMMON_PREFIX_LENGTH, 13);
    }

    #[test]
    fn datagram_bounds_match_spec() {
        const {
            assert!(MIN_DATAGRAM_LENGTH <= SHORT_HEADER_LENGTH + MIN_POST_HEADER_BYTES);
            assert!(MAX_DATAGRAM_IPV6_LENGTH < MAX_DATAGRAM_IPV4_LENGTH);
        }
    }
}
