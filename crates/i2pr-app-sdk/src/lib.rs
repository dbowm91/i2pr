//! Application-side Rust API for the managed-app wire contract.
//!
//! The core crate has no runtime or socket dependency. Enable `tokio-adapter`
//! to use the generic framed session with any Tokio `AsyncRead`/`AsyncWrite`.
//! It never grants capabilities or creates host network connections.

#![forbid(unsafe_code)]

pub use i2pr_app_proto as protocol;

#[cfg(feature = "tokio-adapter")]
mod tokio_session {
    use std::collections::{BTreeSet, VecDeque};
    use std::future::poll_fn;
    use std::sync::atomic::{AtomicU32, Ordering};

    use i2pr_app_proto::{
        AppId, AppRequestOutcome, AppService, AppToHostMessage, Capability, Frame, FrameKind,
        Handshake, HostToAppMessage, MAX_FRAME_PAYLOAD_BYTES, MAX_STREAMS, PROTOCOL_MAJOR,
        PROTOCOL_MINOR, RequestId, Role, decode_host_to_app_control, encode_app_to_host_control,
    };
    use thiserror::Error;
    use tokio_app::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);
    const MAX_PENDING_CONTROLS: usize = 128;
    const MAX_PENDING_FRAMES: usize = 128;

    #[derive(Debug, Error)]
    pub enum SdkError {
        #[error("managed-app I/O failed")]
        Io(#[from] std::io::Error),
        #[error("managed-app protocol rejected the frame")]
        Protocol,
        #[error("managed-app frame exceeded its bound")]
        FrameLimit,
        #[error("managed-app stream limit reached")]
        StreamLimit,
        #[error("managed-app session is closed")]
        Closed,
        #[error("managed-app host refused the request")]
        Refused,
        #[error("the host did not grant the required capability")]
        CapabilityDenied,
    }

    /// Owns the application side of the inherited byte channel. Callers own
    /// runtime, cancellation, task scheduling, and any host-specific pipes.
    pub struct AppSession<R, W> {
        reader: R,
        writer: W,
        next_stream: u32,
        streams: BTreeSet<u32>,
        pending_controls: VecDeque<HostToAppMessage>,
        pending_data: VecDeque<Frame>,
        capabilities: BTreeSet<Capability>,
        pending_write: Option<Vec<u8>>,
        pending_write_offset: usize,
        pending_flush: bool,
    }

    impl<R, W> AppSession<R, W>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        pub async fn connect(
            reader: R,
            mut writer: W,
            app_id: AppId,
            instance_id: String,
        ) -> Result<Self, SdkError> {
            let handshake = Handshake {
                role: Role::Application,
                major: PROTOCOL_MAJOR,
                minor: PROTOCOL_MINOR,
            };
            writer.write_all(&handshake.encode()).await?;
            writer.flush().await?;
            let instance_id = i2pr_app_proto::AppInstanceId::parse(&instance_id)
                .map_err(|_| SdkError::Protocol)?;
            let request_id_value = request_id()?;
            let message = AppToHostMessage::Hello {
                request_id: request_id_value,
                app_id,
                instance_id,
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
            };
            write_control(&mut writer, &message).await?;
            let mut session = Self {
                reader,
                writer,
                next_stream: 1,
                streams: BTreeSet::new(),
                pending_controls: VecDeque::new(),
                pending_data: VecDeque::new(),
                capabilities: BTreeSet::new(),
                pending_write: None,
                pending_write_offset: 0,
                pending_flush: false,
            };
            match session.recv_control().await? {
                HostToAppMessage::Reply {
                    request_id: response,
                    outcome: i2pr_app_proto::AppRequestOutcome::Succeeded,
                } if response == request_id_value => {}
                _ => return Err(SdkError::Protocol),
            }
            let HostToAppMessage::Capabilities { capabilities } = session.recv_control().await?
            else {
                return Err(SdkError::Protocol);
            };
            session.capabilities = capabilities.into_iter().collect();
            Ok(session)
        }

        /// Returns the effective capability set supplied by the host.
        pub fn capabilities(&self) -> impl Iterator<Item = Capability> + '_ {
            self.capabilities.iter().copied()
        }

        pub async fn open(&mut self, service: AppService) -> Result<u32, SdkError> {
            let required = match service {
                AppService::Sam => Capability::Sam,
                AppService::I2cp => Capability::I2cp,
                AppService::ControlScoped => Capability::ControlScoped,
            };
            if !self.capabilities.contains(&required) {
                return Err(SdkError::CapabilityDenied);
            }
            if self.streams.len() >= MAX_STREAMS {
                return Err(SdkError::StreamLimit);
            }
            let stream_id = self.next_stream;
            self.next_stream = self
                .next_stream
                .checked_add(1)
                .ok_or(SdkError::StreamLimit)?;
            let request_id = request_id()?;
            self.write_control(&AppToHostMessage::Open {
                request_id,
                stream_id,
                service,
            })
            .await?;
            if !matches!(
                self.wait_reply(request_id).await?,
                AppRequestOutcome::Succeeded
            ) {
                return Err(SdkError::Refused);
            }
            self.streams.insert(stream_id);
            Ok(stream_id)
        }

        pub async fn publish_local_service(
            &mut self,
            service_name: String,
            preferred_port: Option<u16>,
        ) -> Result<(u32, u16), SdkError> {
            if !self.capabilities.contains(&Capability::LocalService) {
                return Err(SdkError::CapabilityDenied);
            }
            let request_id = request_id()?;
            self.write_control(&AppToHostMessage::PublishLocalService {
                request_id,
                service_name,
                preferred_port,
            })
            .await?;
            loop {
                match self.next_control().await? {
                    HostToAppMessage::LocalServicePublished {
                        request_id: response,
                        service_id,
                        port,
                    } if response == request_id => return Ok((service_id, port)),
                    HostToAppMessage::Reply {
                        request_id: response,
                        outcome: AppRequestOutcome::Failed(_),
                    } if response == request_id => return Err(SdkError::Refused),
                    other => self.queue_control(other)?,
                }
            }
        }

        pub async fn unpublish_local_service(&mut self, service_id: u32) -> Result<(), SdkError> {
            if !self.capabilities.contains(&Capability::LocalService) {
                return Err(SdkError::CapabilityDenied);
            }
            let request_id = request_id()?;
            self.write_control(&AppToHostMessage::UnpublishLocalService {
                request_id,
                service_id,
            })
            .await?;
            loop {
                match self.next_control().await? {
                    HostToAppMessage::LocalServiceUnpublished {
                        request_id: response,
                        ..
                    } if response == request_id => return Ok(()),
                    HostToAppMessage::Reply {
                        request_id: response,
                        outcome: AppRequestOutcome::Failed(_),
                    } if response == request_id => return Err(SdkError::Refused),
                    other => self.queue_control(other)?,
                }
            }
        }

        pub async fn send(&mut self, stream_id: u32, bytes: &[u8]) -> Result<(), SdkError> {
            if !self.streams.contains(&stream_id) {
                return Err(SdkError::Closed);
            }
            let frame = Frame {
                kind: FrameKind::Data,
                stream_id,
                payload: bytes.to_vec(),
            }
            .encode()
            .map_err(|_| SdkError::Protocol)?;
            self.write_frame(frame).await?;
            Ok(())
        }

        pub async fn receive(&mut self) -> Result<Frame, SdkError> {
            while let Some(frame) = self.pending_data.pop_front() {
                if self.streams.contains(&frame.stream_id) {
                    return Ok(frame);
                }
            }
            loop {
                let frame = read_frame(&mut self.reader).await?;
                match frame.kind {
                    FrameKind::Data if self.streams.contains(&frame.stream_id) => return Ok(frame),
                    FrameKind::Data => return Err(SdkError::Protocol),
                    FrameKind::Control => {
                        let message = decode_host_to_app_control(&frame.payload)
                            .map_err(|_| SdkError::Protocol)?;
                        self.queue_control(message)?;
                    }
                }
            }
        }

        pub async fn recv_control(&mut self) -> Result<HostToAppMessage, SdkError> {
            if let Some(message) = self.pending_controls.pop_front() {
                return Ok(message);
            }
            self.next_control().await
        }

        async fn next_control(&mut self) -> Result<HostToAppMessage, SdkError> {
            loop {
                let frame = read_frame(&mut self.reader).await?;
                match frame.kind {
                    FrameKind::Control => {
                        let message = decode_host_to_app_control(&frame.payload)
                            .map_err(|_| SdkError::Protocol)?;
                        self.apply_stream_event(&message)?;
                        return Ok(message);
                    }
                    FrameKind::Data if self.streams.contains(&frame.stream_id) => {
                        if self.pending_data.len() >= MAX_PENDING_FRAMES {
                            return Err(SdkError::StreamLimit);
                        }
                        self.pending_data.push_back(frame);
                    }
                    FrameKind::Data => return Err(SdkError::Protocol),
                }
            }
        }

        async fn wait_reply(&mut self, id: RequestId) -> Result<AppRequestOutcome, SdkError> {
            loop {
                let message = self.next_control().await?;
                if let HostToAppMessage::Reply {
                    request_id,
                    outcome,
                } = message
                {
                    if request_id == id {
                        return Ok(outcome);
                    }
                    self.queue_control(HostToAppMessage::Reply {
                        request_id,
                        outcome,
                    })?;
                } else {
                    self.queue_control(message)?;
                }
            }
        }

        fn queue_control(&mut self, message: HostToAppMessage) -> Result<(), SdkError> {
            if self.pending_controls.len() >= MAX_PENDING_CONTROLS {
                return Err(SdkError::StreamLimit);
            }
            self.apply_stream_event(&message)?;
            self.pending_controls.push_back(message);
            Ok(())
        }

        fn apply_stream_event(&mut self, message: &HostToAppMessage) -> Result<(), SdkError> {
            match message {
                HostToAppMessage::LocalServiceIncoming { stream_id, .. } => {
                    if !self.streams.contains(stream_id) && self.streams.len() >= MAX_STREAMS {
                        return Err(SdkError::StreamLimit);
                    }
                    self.streams.insert(*stream_id);
                }
                HostToAppMessage::StreamClosed { stream_id }
                | HostToAppMessage::StreamReset { stream_id, .. } => {
                    self.streams.remove(stream_id);
                }
                _ => {}
            }
            Ok(())
        }

        pub async fn close_stream(&mut self, stream_id: u32) -> Result<(), SdkError> {
            if !self.streams.contains(&stream_id) {
                return Err(SdkError::Closed);
            }
            self.write_control(&AppToHostMessage::Close { stream_id })
                .await?;
            self.streams.remove(&stream_id);
            Ok(())
        }

        async fn write_control(&mut self, message: &AppToHostMessage) -> Result<(), SdkError> {
            let payload = encode_app_to_host_control(message).map_err(|_| SdkError::Protocol)?;
            let frame = Frame {
                kind: FrameKind::Control,
                stream_id: 0,
                payload,
            }
            .encode()
            .map_err(|_| SdkError::Protocol)?;
            self.write_frame(frame).await
        }

        async fn write_frame(&mut self, frame: Vec<u8>) -> Result<(), SdkError> {
            // A cancelled write keeps its frame and byte offset in the session.
            // The next operation must finish it before writing a later frame.
            if self.pending_write.is_some() {
                self.drain_pending_frame().await?;
            }
            if self.pending_flush {
                self.writer.flush().await?;
                self.pending_flush = false;
            }
            self.pending_write = Some(frame);
            self.pending_write_offset = 0;
            self.drain_pending_frame().await?;
            if self.pending_flush {
                self.writer.flush().await?;
                self.pending_flush = false;
            }
            Ok(())
        }

        async fn drain_pending_frame(&mut self) -> Result<(), SdkError> {
            let writer = &mut self.writer;
            let pending_write = &self.pending_write;
            let offset = &mut self.pending_write_offset;
            poll_fn(|cx| {
                let Some(bytes) = pending_write.as_ref() else {
                    return std::task::Poll::Ready(Err(std::io::Error::other(
                        "missing pending frame",
                    )));
                };
                while *offset < bytes.len() {
                    match std::pin::Pin::new(&mut *writer).poll_write(cx, &bytes[*offset..]) {
                        std::task::Poll::Ready(Ok(0)) => {
                            return std::task::Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::WriteZero,
                                "zero-length managed-app write",
                            )));
                        }
                        std::task::Poll::Ready(Ok(count)) => *offset += count,
                        std::task::Poll::Ready(Err(error)) => {
                            return std::task::Poll::Ready(Err(error));
                        }
                        std::task::Poll::Pending => return std::task::Poll::Pending,
                    }
                }
                std::task::Poll::Ready(Ok(()))
            })
            .await?;
            self.pending_write = None;
            self.pending_write_offset = 0;
            self.pending_flush = true;
            Ok(())
        }
    }

    async fn write_control<W: AsyncWrite + Unpin>(
        writer: &mut W,
        message: &AppToHostMessage,
    ) -> Result<(), SdkError> {
        let payload = encode_app_to_host_control(message).map_err(|_| SdkError::Protocol)?;
        let frame = Frame {
            kind: FrameKind::Control,
            stream_id: 0,
            payload,
        }
        .encode()
        .map_err(|_| SdkError::Protocol)?;
        writer.write_all(&frame).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Frame, SdkError> {
        let mut header = [0_u8; 12];
        reader.read_exact(&mut header).await?;
        let len =
            u32::from_be_bytes(header[8..12].try_into().map_err(|_| SdkError::Protocol)?) as usize;
        if len > MAX_FRAME_PAYLOAD_BYTES {
            return Err(SdkError::FrameLimit);
        }
        let total = 12_usize.checked_add(len).ok_or(SdkError::FrameLimit)?;
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&header);
        bytes.resize(total, 0);
        reader.read_exact(&mut bytes[12..]).await?;
        let (frame, consumed) = Frame::decode(&bytes).map_err(|_| SdkError::Protocol)?;
        if consumed != total {
            return Err(SdkError::Protocol);
        }
        Ok(frame)
    }

    fn request_id() -> Result<RequestId, SdkError> {
        let id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        RequestId::new(id).map_err(|_| SdkError::Closed)
    }

    pub use AppSession as Session;
    pub use SdkError as Error;
}

#[cfg(feature = "tokio-adapter")]
pub use tokio_session::{Error, Session};

#[cfg(all(test, feature = "tokio-adapter"))]
mod tests {
    use super::*;
    use i2pr_app_proto::{
        AppRequestOutcome, AppToHostMessage, Capability, Frame, FrameKind, Handshake,
        HostToAppMessage, RequestId, Role, decode_app_to_host_control, encode_host_to_app_control,
    };
    use tokio_app::io::{AsyncReadExt, AsyncWriteExt};

    async fn read_frame<T: AsyncReadExt + Unpin>(io: &mut T) -> Frame {
        let mut header = [0_u8; 12];
        io.read_exact(&mut header).await.unwrap();
        let len = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
        let mut bytes = header.to_vec();
        bytes.resize(12 + len, 0);
        io.read_exact(&mut bytes[12..]).await.unwrap();
        Frame::decode(&bytes).unwrap().0
    }

    async fn write_control<T: AsyncWriteExt + Unpin>(io: &mut T, message: HostToAppMessage) {
        let payload = encode_host_to_app_control(&message).unwrap();
        let frame = Frame {
            kind: FrameKind::Control,
            stream_id: 0,
            payload,
        }
        .encode()
        .unwrap();
        io.write_all(&frame).await.unwrap();
    }

    #[test]
    fn generic_session_uses_versioned_hello_and_logical_streams() {
        futures_executor::block_on(async {
            let (app, mut host) = tokio_app::io::duplex(4096);
            let (reader, writer) = tokio_app::io::split(app);
            let server = std::thread::spawn(move || {
                futures_executor::block_on(async move {
                    let mut greeting = [0; 9];
                    host.read_exact(&mut greeting).await.unwrap();
                    let decoded = Handshake::decode(&greeting).unwrap();
                    assert_eq!(decoded.role, Role::Application);
                    let hello = read_frame(&mut host).await;
                    let AppToHostMessage::Hello {
                        request_id,
                        protocol_minor,
                        ..
                    } = decode_app_to_host_control(&hello.payload).unwrap()
                    else {
                        panic!("hello expected")
                    };
                    assert_eq!(protocol_minor, i2pr_app_proto::PROTOCOL_MINOR);
                    write_control(
                        &mut host,
                        HostToAppMessage::Reply {
                            request_id,
                            outcome: AppRequestOutcome::Succeeded,
                        },
                    )
                    .await;
                    write_control(
                        &mut host,
                        HostToAppMessage::Capabilities {
                            capabilities: vec![Capability::Sam],
                        },
                    )
                    .await;
                    let open = read_frame(&mut host).await;
                    let AppToHostMessage::Open {
                        request_id,
                        stream_id,
                        ..
                    } = decode_app_to_host_control(&open.payload).unwrap()
                    else {
                        panic!("open expected")
                    };
                    write_control(
                        &mut host,
                        HostToAppMessage::Health {
                            state: "ready".into(),
                            detail: None,
                        },
                    )
                    .await;
                    write_control(
                        &mut host,
                        HostToAppMessage::Reply {
                            request_id,
                            outcome: AppRequestOutcome::Succeeded,
                        },
                    )
                    .await;
                    let data = Frame {
                        kind: FrameKind::Data,
                        stream_id,
                        payload: b"reply".to_vec(),
                    }
                    .encode()
                    .unwrap();
                    host.write_all(&data).await.unwrap();
                    let request = read_frame(&mut host).await;
                    assert_eq!(request.kind, FrameKind::Data);
                    assert_eq!(request.payload, b"request");
                    let close = read_frame(&mut host).await;
                    assert!(
                        matches!(decode_app_to_host_control(&close.payload).unwrap(), AppToHostMessage::Close { stream_id: id } if id == stream_id)
                    );
                })
            });
            let app_id = i2pr_app_proto::AppId::parse("sdk.fixture").unwrap();
            let mut session = Session::connect(reader, writer, app_id, "1".into())
                .await
                .unwrap();
            let stream = session.open(i2pr_app_proto::AppService::Sam).await.unwrap();
            assert!(
                matches!(session.recv_control().await.unwrap(), HostToAppMessage::Health { state, .. } if state == "ready")
            );
            session.send(stream, b"request").await.unwrap();
            let frame = session.receive().await.unwrap();
            assert_eq!(frame.stream_id, stream);
            assert_eq!(frame.payload, b"reply");
            session.close_stream(stream).await.unwrap();
            server.join().unwrap();
        });
    }

    #[test]
    fn request_ids_are_nonzero_and_typed() {
        assert!(RequestId::new(1).is_ok());
    }
}
