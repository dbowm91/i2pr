//! Plan 369 §3 — the inherited anonymous apphost transport.
//!
//! `i2pr-appd` gives `i2pr-apphost` exactly one logical duplex byte stream,
//! realized as **two inherited anonymous pipes**. There is no listening socket,
//! no loopback connect, no filesystem-discoverable Unix-domain path, and no
//! globally named pipe endpoint.
//!
//! # The transport is the authentication fact
//!
//! The apphost bootstrap handshake carries no credential, nonce, or identity
//! binding. Its entire trust contribution is that the bytes arrived over a
//! channel the manager itself created and handed to the host it forked. That is
//! why this module refuses to offer anything discoverable: the moment the
//! transport becomes a name another process could connect to, ADR 0035's
//! inherited-authority model no longer holds.
//!
//! # Lifetime
//!
//! A process whose only channel is an inherited pipe has no other lifetime
//! signal: when the manager closes or drops its ends, this transport reaches EOF
//! and the host exits. There is deliberately no shutdown socket, control file,
//! or signal-based teardown, because each would be a second, discoverable
//! authority channel.
//!
//! # Why this is duplicated rather than shared
//!
//! `i2pr-appd` carries an identical module. Linking the two crates together to
//! share it would make the trust-zone boundary a naming convention; Plan 369 §6
//! requires it to be a build constraint. The duplication is deliberate and is
//! the smaller cost.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// One owned duplex byte stream presented as a single logical stream.
///
/// The bootstrap codec and the transparent relay both split this with
/// `tokio::io::split`, so the two halves must live in **one** object rather than
/// being passed around as a pair.
#[derive(Debug)]
pub struct DuplexTransport<R, W> {
    read: R,
    write: W,
}

impl<R, W> DuplexTransport<R, W> {
    pub fn new(read: R, write: W) -> Self {
        Self { read, write }
    }

    /// Splits into the owned halves used by a bidirectional codec.
    pub fn into_halves(self) -> (R, W) {
        (self.read, self.write)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncRead for DuplexTransport<R, W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.read).poll_read(context, buffer)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncWrite for DuplexTransport<R, W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.write).poll_write(context, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.write).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.write).poll_shutdown(context)
    }
}

/// The inherited transport of a real `i2pr-apphost` process.
///
/// File descriptor 0 is the manager→host pipe and file descriptor 1 is the
/// host→manager pipe, so the host's read half is stdin and its write half is
/// stdout. Plan 369 §8 reserves those two descriptors for protocol bytes; the
/// host keeps stderr for its own diagnostics and never writes protocol to it.
pub fn inherited() -> DuplexTransport<tokio::io::Stdin, tokio::io::Stdout> {
    DuplexTransport::new(tokio::io::stdin(), tokio::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn one_object_carries_bytes_in_both_directions() {
        let (left, right) = tokio::io::duplex(64);
        let (left_read, left_write) = tokio::io::split(left);
        let (right_read, right_write) = tokio::io::split(right);

        let mut host = DuplexTransport::new(left_read, left_write);
        let mut manager = DuplexTransport::new(right_read, right_write);

        host.write_all(b"ping").await.expect("write");
        let mut buffer = [0_u8; 4];
        timeout(Duration::from_secs(10), manager.read_exact(&mut buffer))
            .await
            .expect("host bytes must reach the manager rather than hang")
            .expect("read");
        assert_eq!(&buffer, b"ping");

        manager.write_all(b"pong").await.expect("write");
        timeout(Duration::from_secs(10), host.read_exact(&mut buffer))
            .await
            .expect("manager bytes must reach the host rather than hang")
            .expect("read");
        assert_eq!(&buffer, b"pong");
    }

    #[tokio::test]
    async fn eof_is_the_only_shutdown_signal() {
        let (host_side, manager_side) = tokio::io::duplex(64);
        let (host_read, host_write) = tokio::io::split(host_side);

        let mut host = DuplexTransport::new(host_read, host_write);
        // The manager drops its whole side: the host must observe EOF, not hang.
        // There is no shutdown message and no separate control channel, so if
        // this does not terminate, `i2pr-apphost` can never be stopped.
        drop(manager_side);

        let mut buffer = Vec::new();
        let read = timeout(Duration::from_secs(10), host.read_to_end(&mut buffer))
            .await
            .expect("host must reach eof rather than hang")
            .expect("read to eof");
        assert_eq!(read, 0);
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn halves_can_be_taken_back_for_a_splitting_codec() {
        let (host_side, mut manager_side) = tokio::io::duplex(64);
        let (host_read, host_write) = tokio::io::split(host_side);
        let transport = DuplexTransport::new(host_read, host_write);
        let (mut read, mut write) = tokio::io::split(transport);
        write.write_all(b"x").await.expect("write");
        // A duplex is bidirectional, not a loopback: reading the host's read half
        // returns what the *manager* wrote, never what the host wrote. Every
        // assertion is bounded so a direction mistake fails rather than hangs.
        let mut buffer = [0_u8; 1];
        timeout(
            Duration::from_secs(10),
            manager_side.read_exact(&mut buffer),
        )
        .await
        .expect("manager must observe host bytes rather than hang")
        .expect("read");
        assert_eq!(buffer, [b'x']);

        manager_side.write_all(b"y").await.expect("manager write");
        let mut back = [0_u8; 1];
        timeout(Duration::from_secs(10), read.read_exact(&mut back))
            .await
            .expect("host must observe manager bytes rather than hang")
            .expect("read");
        assert_eq!(back, [b'y']);
    }
}
