//! Bounded SAM 3.x UDP bridge framing for Datagram1/2/3 and Raw.

use i2pr_api::sam::{MAX_SAM_OPTION_VALUE_BYTES, base64};
use i2pr_client::datagram::{
    DATAGRAM1_PROTOCOL, DATAGRAM2_PROTOCOL, DATAGRAM3_PROTOCOL, DatagramReceiveEvent,
    RAW_DATAGRAM_PROTOCOL,
};

pub const MAX_DATAGRAM_HEADER_BYTES: usize = MAX_SAM_OPTION_VALUE_BYTES;
pub const MAX_DATAGRAM_PACKET_BYTES: usize = 65_507;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatagramPacket<'a> {
    pub id: &'a str,
    pub destination: &'a str,
    pub from_port: Option<u16>,
    pub to_port: Option<u16>,
    pub protocol: Option<u8>,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatagramPacketError {
    MissingHeaderTerminator,
    HeaderTooLarge,
    InvalidHeader,
    DuplicateOption,
    InvalidPort,
    InvalidProtocol,
    /// Optional message controls are not representable in the current
    /// canonical datagram delivery request and are rejected.
    UnsupportedMessageControl,
    EmptyPayload,
}

pub fn parse_packet(bytes: &[u8]) -> Result<DatagramPacket<'_>, DatagramPacketError> {
    let newline = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or(DatagramPacketError::MissingHeaderTerminator)?;
    if newline > MAX_DATAGRAM_HEADER_BYTES {
        return Err(DatagramPacketError::HeaderTooLarge);
    }
    let header = bytes[..newline]
        .strip_suffix(b"\r")
        .unwrap_or(&bytes[..newline]);
    if !header.is_ascii() {
        return Err(DatagramPacketError::InvalidHeader);
    }
    let header = core::str::from_utf8(header).map_err(|_| DatagramPacketError::InvalidHeader)?;
    let mut fields = header.split_ascii_whitespace();
    let version = fields.next().ok_or(DatagramPacketError::InvalidHeader)?;
    if !(version.starts_with("3.")
        && version[2..].len() == 1
        && version.as_bytes()[2].is_ascii_digit())
    {
        return Err(DatagramPacketError::InvalidHeader);
    }
    let id = fields.next().ok_or(DatagramPacketError::InvalidHeader)?;
    let destination = fields.next().ok_or(DatagramPacketError::InvalidHeader)?;
    if id.is_empty()
        || id.len() > MAX_SAM_OPTION_VALUE_BYTES
        || destination.is_empty()
        || destination.len() > MAX_SAM_OPTION_VALUE_BYTES
    {
        return Err(DatagramPacketError::InvalidHeader);
    }
    let mut from_port = None;
    let mut to_port = None;
    let mut protocol = None;
    for field in fields {
        let (key, value) = field
            .split_once('=')
            .ok_or(DatagramPacketError::InvalidHeader)?;
        match key {
            "SEND_TAGS" | "TAG_THRESHOLD" | "EXPIRES" | "SEND_LEASESET" => {
                return Err(DatagramPacketError::UnsupportedMessageControl);
            }
            "FROM_PORT" => {
                if from_port.is_some() {
                    return Err(DatagramPacketError::DuplicateOption);
                }
                from_port = Some(parse_number(value).ok_or(DatagramPacketError::InvalidPort)?);
            }
            "TO_PORT" => {
                if to_port.is_some() {
                    return Err(DatagramPacketError::DuplicateOption);
                }
                to_port = Some(parse_number(value).ok_or(DatagramPacketError::InvalidPort)?);
            }
            "PROTOCOL" => {
                if protocol.is_some() {
                    return Err(DatagramPacketError::DuplicateOption);
                }
                protocol = Some(
                    value
                        .parse::<u8>()
                        .map_err(|_| DatagramPacketError::InvalidProtocol)?,
                );
            }
            _ => return Err(DatagramPacketError::InvalidHeader),
        }
    }
    let payload = &bytes[newline + 1..];
    if payload.is_empty() {
        return Err(DatagramPacketError::EmptyPayload);
    }
    Ok(DatagramPacket {
        id,
        destination,
        from_port,
        to_port,
        protocol,
        payload,
    })
}

fn parse_number(value: &str) -> Option<u16> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

pub fn encode_received(
    event: &DatagramReceiveEvent,
    raw_session: bool,
    raw_header: bool,
) -> Option<Vec<u8>> {
    let mut packet = Vec::with_capacity(event.payload.len().saturating_add(3072));
    if raw_session {
        if raw_header {
            packet.extend_from_slice(
                format!(
                    "FROM_PORT={}\nTO_PORT={}\nPROTOCOL={}\n\n",
                    event.source_port, event.destination_port, event.protocol
                )
                .as_bytes(),
            );
        }
        packet.extend_from_slice(&event.raw_payload);
        return (packet.len() <= MAX_DATAGRAM_PACKET_BYTES).then_some(packet);
    }
    match event.protocol {
        RAW_DATAGRAM_PROTOCOL => packet.extend_from_slice(&event.payload),
        DATAGRAM1_PROTOCOL | DATAGRAM2_PROTOCOL => {
            let destination = event.from_destination.as_deref()?;
            packet.extend_from_slice(base64::encode(destination).as_bytes());
            packet.extend_from_slice(
                format!(
                    " FROM_PORT={} TO_PORT={}\n",
                    event.source_port, event.destination_port
                )
                .as_bytes(),
            );
            packet.extend_from_slice(&event.payload);
        }
        DATAGRAM3_PROTOCOL => {
            packet.extend_from_slice(base64::encode(&event.from_hash).as_bytes());
            packet.extend_from_slice(
                format!(
                    " FROM_PORT={} TO_PORT={}\n",
                    event.source_port, event.destination_port
                )
                .as_bytes(),
            );
            packet.extend_from_slice(&event.payload);
        }
        _ => return None,
    }
    (packet.len() <= MAX_DATAGRAM_PACKET_BYTES).then_some(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binary_payload_and_port_overrides() {
        let packet = parse_packet(b"3.3 child dest FROM_PORT=65535 TO_PORT=0\n\0\xff")
            .expect("packet parses");
        assert_eq!(packet.id, "child");
        assert_eq!(packet.destination, "dest");
        assert_eq!(packet.from_port, Some(u16::MAX));
        assert_eq!(packet.to_port, Some(0));
        assert_eq!(packet.payload, b"\0\xff");
    }

    #[test]
    fn raw_protocol_is_bounded_to_u8() {
        let packet = parse_packet(b"3.0 raw dest PROTOCOL=18\npayload").expect("packet");
        assert_eq!(packet.protocol, Some(18));
        assert_eq!(
            parse_packet(b"3.0 raw dest PROTOCOL=256\npayload"),
            Err(DatagramPacketError::InvalidProtocol)
        );
    }

    #[test]
    fn malformed_and_unbounded_headers_are_rejected() {
        for input in [
            &b"3.3 id dest FROM_PORT=1 FROM_PORT=2\nx"[..],
            &b"3.3 id dest TO_PORT=65536\nx"[..],
            &b"3.3 id dest UNKNOWN=1\nx"[..],
            &b"3.3 id dest\n"[..],
        ] {
            assert!(parse_packet(input).is_err());
        }
        let mut too_long = vec![b'x'; MAX_DATAGRAM_HEADER_BYTES + 2];
        *too_long.last_mut().expect("nonempty") = b'\n';
        assert_eq!(
            parse_packet(&too_long),
            Err(DatagramPacketError::HeaderTooLarge)
        );
    }

    #[test]
    fn unsupported_optional_send_controls_are_rejected_explicitly() {
        for control in [
            "SEND_TAGS=2",
            "TAG_THRESHOLD=4",
            "EXPIRES=1000",
            "SEND_LEASESET=true",
        ] {
            let packet = format!("3.3 id destination {control}\npayload");
            assert_eq!(
                parse_packet(packet.as_bytes()),
                Err(DatagramPacketError::UnsupportedMessageControl),
                "{control}"
            );
        }
    }

    #[test]
    fn received_formats_distinguish_authenticated_destinations_and_hashes() {
        let authenticated = DatagramReceiveEvent {
            from_hash: [0x11; 32],
            from_destination: Some(vec![0x22, 0x33]),
            source_port: 12,
            destination_port: 34,
            protocol: DATAGRAM2_PROTOCOL,
            payload: vec![0xFF],
            raw_payload: vec![0xFF],
            sender_authenticated: true,
            options: None,
            received_at_ms: 0,
        };
        assert_eq!(
            encode_received(&authenticated, false, false).expect("Datagram2 output"),
            [
                base64::encode(&[0x22, 0x33]).as_bytes(),
                b" FROM_PORT=12 TO_PORT=34\n",
                &[0xFF]
            ]
            .concat()
        );
        let unauthenticated = DatagramReceiveEvent {
            from_hash: [0x11; 32],
            from_destination: None,
            source_port: 12,
            destination_port: 34,
            protocol: DATAGRAM3_PROTOCOL,
            payload: vec![0xFF],
            raw_payload: vec![0xFF],
            sender_authenticated: false,
            options: None,
            received_at_ms: 0,
        };
        let packet = encode_received(&unauthenticated, false, false).expect("Datagram3 output");
        assert!(packet.starts_with(base64::encode(&[0x11; 32]).as_bytes()));
        assert!(packet.ends_with(&[0xFF]));
    }

    #[test]
    fn raw_forwarding_header_preserves_i2cp_metadata() {
        let event = DatagramReceiveEvent {
            from_hash: [0x11; 32],
            from_destination: None,
            source_port: 12,
            destination_port: 34,
            protocol: RAW_DATAGRAM_PROTOCOL,
            payload: b"raw".to_vec(),
            raw_payload: b"raw".to_vec(),
            sender_authenticated: false,
            options: None,
            received_at_ms: 0,
        };
        assert_eq!(
            encode_received(&event, true, true).expect("RAW header packet"),
            b"FROM_PORT=12\nTO_PORT=34\nPROTOCOL=18\n\nraw"
        );
        assert_eq!(
            encode_received(&event, true, false).expect("RAW packet"),
            b"raw"
        );
    }

    #[test]
    fn raw_subsession_forwards_custom_and_reserved_listen_protocol_payloads_as_raw() {
        for protocol in [17, 42] {
            let event = DatagramReceiveEvent {
                from_hash: [0x11; 32],
                from_destination: None,
                source_port: 12,
                destination_port: 34,
                protocol,
                payload: b"raw child payload".to_vec(),
                raw_payload: b"raw wire envelope".to_vec(),
                sender_authenticated: false,
                options: None,
                received_at_ms: 0,
            };
            assert_eq!(
                encode_received(&event, true, false).expect("RAW child output"),
                b"raw wire envelope"
            );
        }
    }
}
