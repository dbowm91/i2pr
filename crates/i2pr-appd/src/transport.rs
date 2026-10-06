//! Plan 369 §3 — the inherited anonymous manager transport.
//!
//! The daemon gives `i2pr-appd` exactly one logical duplex byte stream, realized
//! as **two inherited anonymous pipes** (daemon→manager and manager→daemon).
//! There is no listening socket, no loopback connect, no filesystem-discoverable
//! Unix-domain path, and no globally named pipe endpoint.
//!
//! # The transport is the authentication fact
//!
//! The manager-protocol handshake carries no credential, nonce, or identity
//! binding. Its entire trust contribution is that the bytes arrived over a
//! channel the daemon itself created and handed to the child it forked. That is
//! why this module refuses to offer anything discoverable: the moment the
//! transport becomes a name another process could connect to, ADR 0035's
//! inherited-authority model no longer holds.
//!
//! # Lifetime
//!
//! A process whose only channel is an inherited pipe has no other lifetime
//! signal: when the daemon closes or drops its ends, this transport reaches EOF
//! and the manager exits. There is deliberately no shutdown socket, control
//! file, or signal-based teardown, because each of those would be a second,
//! discoverable authority channel.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// One owned duplex byte stream presented as a single logical stream.
///
/// The Plan 368 codec splits this with `tokio::io::split`, so the two halves
/// must live in **one** object rather than being passed around as a pair.
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

/// The inherited transport of a real `i2pr-appd` process.
///
/// File descriptor 0 is the daemon→manager pipe and file descriptor 1 is the
/// manager→daemon pipe, so the manager's read half is stdin and its write half
/// is stdout. Plan 369 §8 reserves those two descriptors for protocol bytes; the
/// manager keeps stderr for its own diagnostics and never writes protocol to it.
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

        let mut appd = DuplexTransport::new(left_read, left_write);
        let mut daemon = DuplexTransport::new(right_read, right_write);

        appd.write_all(b"ping").await.expect("write");
        let mut buffer = [0_u8; 4];
        let read_len = timeout(Duration::from_secs(10), daemon.read_exact(&mut buffer))
            .await
            .expect("manager bytes must reach the daemon rather than hang")
            .expect("read");
        assert_eq!(read_len, 4);
        assert_eq!(&buffer, b"ping");

        daemon.write_all(b"pong").await.expect("write");
        let read_len = timeout(Duration::from_secs(10), appd.read_exact(&mut buffer))
            .await
            .expect("daemon bytes must reach the manager rather than hang")
            .expect("read");
        assert_eq!(read_len, 4);
        assert_eq!(&buffer, b"pong");
    }

    #[tokio::test]
    async fn eof_is_the_only_shutdown_signal() {
        let (appd_side, daemon_side) = tokio::io::duplex(64);
        let (appd_read, appd_write) = tokio::io::split(appd_side);

        let mut appd = DuplexTransport::new(appd_read, appd_write);
        // The daemon drops its whole side of the transport: the manager must
        // observe EOF, not hang. This is the entire teardown contract -- there
        // is no shutdown message and no separate control channel, so if this
        // does not terminate, `i2pr-appd` can never be stopped.
        drop(daemon_side);

        let mut buffer = Vec::new();
        let read = timeout(Duration::from_secs(10), appd.read_to_end(&mut buffer))
            .await
            .expect("manager must reach eof rather than hang")
            .expect("read to eof");
        assert_eq!(read, 0);
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn halves_can_be_taken_back_for_a_splitting_codec() {
        let (appd_side, mut daemon_side) = tokio::io::duplex(64);
        let (appd_read, appd_write) = tokio::io::split(appd_side);
        let transport = DuplexTransport::new(appd_read, appd_write);
        let (mut read, mut write) = tokio::io::split(transport);
        // The Plan 368 bridge splits exactly like this.
        write.write_all(b"x").await.expect("write");
        // A duplex is bidirectional, not a loopback: reading the manager's read
        // half returns what the *daemon* wrote, never what the manager wrote.
        // Every assertion here is bounded so a direction mistake fails the test
        // instead of hanging the suite.
        let mut buffer = [0_u8; 1];
        let read_len = timeout(Duration::from_secs(10), daemon_side.read_exact(&mut buffer))
            .await
            .expect("daemon must observe manager bytes rather than hang")
            .expect("read");
        assert_eq!(read_len, 1);
        assert_eq!(buffer, [b'x']);

        daemon_side.write_all(b"y").await.expect("daemon write");
        let mut back = [0_u8; 1];
        timeout(Duration::from_secs(10), read.read_exact(&mut back))
            .await
            .expect("manager must observe daemon bytes rather than hang")
            .expect("read");
        assert_eq!(back, [b'y']);
    }
}
