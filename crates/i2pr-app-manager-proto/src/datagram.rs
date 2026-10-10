//! Bounded typed datagram operations for a dedicated managed-app service stream.
//!
//! The existing `sam` service remains an exact-byte SAM stream. This separate
//! vocabulary carries no host address or host UDP port; all ports are I2P ports.

use crate::{MAX_DATA_FRAME_BYTES, ManagerProtocolError};

pub const MAX_DATAGRAM_ID_BYTES: usize = 256;
pub const MAX_DATAGRAM_PAYLOAD_BYTES: usize = 60 * 1024 - 1024;
pub const MAX_DATAGRAM_OPTIONS_BYTES: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatagramRequest {
    Send {
        primary_id: String,
        child_id: String,
        destination_hash: [u8; 32],
        source_port: u16,
        destination_port: u16,
        protocol: u8,
        options: Vec<u8>,
        payload: Vec<u8>,
    },
    Receive {
        primary_id: String,
        child_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramEvent {
    pub from_hash: [u8; 32],
    pub sender_authenticated: bool,
    pub source_port: u16,
    pub destination_port: u16,
    pub protocol: u8,
    pub received_at_ms: u64,
    pub options: Vec<u8>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatagramReply {
    Sent,
    Event(DatagramEvent),
    Empty,
    Rejected,
}

impl DatagramRequest {
    pub fn encode(&self) -> Result<Vec<u8>, ManagerProtocolError> {
        let mut out = Vec::new();
        out.extend_from_slice(b"IDG1");
        match self {
            Self::Send {
                primary_id,
                child_id,
                destination_hash,
                source_port,
                destination_port,
                protocol,
                options,
                payload,
            } => {
                validate_id(primary_id)?;
                validate_id(child_id)?;
                if payload.len() > MAX_DATAGRAM_PAYLOAD_BYTES
                    || options.len() > MAX_DATAGRAM_OPTIONS_BYTES
                {
                    return Err(ManagerProtocolError::LimitExceeded("managed datagram"));
                }
                out.push(1);
                encode_id(&mut out, primary_id)?;
                encode_id(&mut out, child_id)?;
                out.extend_from_slice(destination_hash);
                out.extend_from_slice(&source_port.to_be_bytes());
                out.extend_from_slice(&destination_port.to_be_bytes());
                out.push(*protocol);
                encode_bytes(&mut out, options)?;
                encode_bytes(&mut out, payload)?;
            }
            Self::Receive {
                primary_id,
                child_id,
            } => {
                validate_id(primary_id)?;
                validate_id(child_id)?;
                out.push(2);
                encode_id(&mut out, primary_id)?;
                encode_id(&mut out, child_id)?;
            }
        }
        check_frame_bound(&out)?;
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ManagerProtocolError> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(4)? != b"IDG1" {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        let request = match cursor.u8()? {
            1 => Self::Send {
                primary_id: cursor.id()?,
                child_id: cursor.id()?,
                destination_hash: cursor
                    .take(32)?
                    .try_into()
                    .map_err(|_| ManagerProtocolError::MalformedFrame)?,
                source_port: cursor.u16()?,
                destination_port: cursor.u16()?,
                protocol: cursor.u8()?,
                options: cursor.bytes(MAX_DATAGRAM_OPTIONS_BYTES)?,
                payload: cursor.bytes(MAX_DATAGRAM_PAYLOAD_BYTES)?,
            },
            2 => Self::Receive {
                primary_id: cursor.id()?,
                child_id: cursor.id()?,
            },
            _ => return Err(ManagerProtocolError::MalformedFrame),
        };
        cursor.finish()?;
        Ok(request)
    }
}

impl DatagramReply {
    pub fn encode(&self) -> Result<Vec<u8>, ManagerProtocolError> {
        let mut out = b"IDR1".to_vec();
        match self {
            Self::Sent => out.push(1),
            Self::Event(event) => {
                if event.payload.len() > MAX_DATAGRAM_PAYLOAD_BYTES
                    || event.options.len() > MAX_DATAGRAM_OPTIONS_BYTES
                {
                    return Err(ManagerProtocolError::LimitExceeded("managed datagram"));
                }
                out.push(2);
                out.extend_from_slice(&event.from_hash);
                out.push(u8::from(event.sender_authenticated));
                out.extend_from_slice(&event.source_port.to_be_bytes());
                out.extend_from_slice(&event.destination_port.to_be_bytes());
                out.push(event.protocol);
                out.extend_from_slice(&event.received_at_ms.to_be_bytes());
                encode_bytes(&mut out, &event.options)?;
                encode_bytes(&mut out, &event.payload)?;
            }
            Self::Empty => out.push(3),
            Self::Rejected => out.push(4),
        }
        check_frame_bound(&out)?;
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ManagerProtocolError> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(4)? != b"IDR1" {
            return Err(ManagerProtocolError::MalformedFrame);
        }
        let reply = match cursor.u8()? {
            1 => Self::Sent,
            2 => Self::Event(DatagramEvent {
                from_hash: cursor
                    .take(32)?
                    .try_into()
                    .map_err(|_| ManagerProtocolError::MalformedFrame)?,
                sender_authenticated: match cursor.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(ManagerProtocolError::MalformedFrame),
                },
                source_port: cursor.u16()?,
                destination_port: cursor.u16()?,
                protocol: cursor.u8()?,
                received_at_ms: cursor.u64()?,
                options: cursor.bytes(MAX_DATAGRAM_OPTIONS_BYTES)?,
                payload: cursor.bytes(MAX_DATAGRAM_PAYLOAD_BYTES)?,
            }),
            3 => Self::Empty,
            4 => Self::Rejected,
            _ => return Err(ManagerProtocolError::MalformedFrame),
        };
        cursor.finish()?;
        Ok(reply)
    }
}

fn validate_id(value: &str) -> Result<(), ManagerProtocolError> {
    if value.is_empty()
        || value.len() > MAX_DATAGRAM_ID_BYTES
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(ManagerProtocolError::InvalidIdentifier);
    }
    Ok(())
}

fn encode_id(out: &mut Vec<u8>, value: &str) -> Result<(), ManagerProtocolError> {
    let bytes = value.as_bytes();
    let len = u16::try_from(bytes.len()).map_err(|_| ManagerProtocolError::InvalidIdentifier)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn encode_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), ManagerProtocolError> {
    let len = u16::try_from(value.len())
        .map_err(|_| ManagerProtocolError::LimitExceeded("managed datagram"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn check_frame_bound(bytes: &[u8]) -> Result<(), ManagerProtocolError> {
    if bytes.len() > MAX_DATA_FRAME_BYTES {
        Err(ManagerProtocolError::LimitExceeded(
            "managed datagram frame",
        ))
    } else {
        Ok(())
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], ManagerProtocolError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ManagerProtocolError::MalformedFrame)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ManagerProtocolError::MalformedFrame)?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, ManagerProtocolError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ManagerProtocolError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| ManagerProtocolError::MalformedFrame)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, ManagerProtocolError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| ManagerProtocolError::MalformedFrame)?,
        ))
    }
    fn bytes(&mut self, maximum: usize) -> Result<Vec<u8>, ManagerProtocolError> {
        let len = usize::from(self.u16()?);
        if len > maximum {
            return Err(ManagerProtocolError::LimitExceeded("managed datagram"));
        }
        Ok(self.take(len)?.to_vec())
    }
    fn id(&mut self) -> Result<String, ManagerProtocolError> {
        let len = usize::from(self.u16()?);
        if len == 0 || len > MAX_DATAGRAM_ID_BYTES {
            return Err(ManagerProtocolError::InvalidIdentifier);
        }
        let value = std::str::from_utf8(self.take(len)?)
            .map_err(|_| ManagerProtocolError::InvalidIdentifier)?
            .to_owned();
        validate_id(&value)?;
        Ok(value)
    }
    fn finish(self) -> Result<(), ManagerProtocolError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ManagerProtocolError::MalformedFrame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_datagram_requests_and_events_round_trip() {
        let request = DatagramRequest::Send {
            primary_id: "primary".to_owned(),
            child_id: "child".to_owned(),
            destination_hash: [7; 32],
            source_port: 10,
            destination_port: 20,
            protocol: 19,
            options: vec![0, 0],
            payload: b"bounded".to_vec(),
        };
        assert_eq!(
            DatagramRequest::decode(&request.encode().unwrap()).unwrap(),
            request
        );

        let reply = DatagramReply::Event(DatagramEvent {
            from_hash: [8; 32],
            sender_authenticated: false,
            source_port: 20,
            destination_port: 10,
            protocol: 20,
            received_at_ms: 99,
            options: vec![],
            payload: b"reply".to_vec(),
        });
        assert_eq!(
            DatagramReply::decode(&reply.encode().unwrap()).unwrap(),
            reply
        );
    }

    #[test]
    fn malformed_and_over_limit_datagram_frames_fail_closed() {
        assert_eq!(
            DatagramRequest::decode(b"IDG1\x02\0\0\0"),
            Err(ManagerProtocolError::InvalidIdentifier)
        );
        let too_large = DatagramRequest::Send {
            primary_id: "p".to_owned(),
            child_id: "c".to_owned(),
            destination_hash: [0; 32],
            source_port: 0,
            destination_port: 0,
            protocol: 19,
            options: vec![],
            payload: vec![0; MAX_DATAGRAM_PAYLOAD_BYTES + 1],
        };
        assert_eq!(
            too_large.encode(),
            Err(ManagerProtocolError::LimitExceeded("managed datagram"))
        );
    }
}
