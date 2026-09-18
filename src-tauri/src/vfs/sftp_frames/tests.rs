use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fragmented {
    bytes: Vec<u8>,
    offset: usize,
    pending: bool,
}
impl AsyncRead for Fragmented {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.pending {
            this.pending = false;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if output.remaining() > 0 && this.offset < this.bytes.len() {
            output.put_slice(&this.bytes[this.offset..this.offset + 1]);
            this.offset += 1;
            this.pending = true;
        }
        Poll::Ready(Ok(()))
    }
}
fn stream(bytes: Vec<u8>, limit: u32) -> BoundedSftpStream<Fragmented> {
    BoundedSftpStream::new(
        Fragmented {
            bytes,
            offset: 0,
            pending: false,
        },
        limit,
    )
}
fn packet(body: &[u8]) -> Vec<u8> {
    let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(body);
    bytes
}
#[test]
fn fragmented_frames_and_exact_boundaries_preserve_bytes() {
    crate::net::session::runtime().block_on(async {
        let mut input = packet(b"12345678");
        input.extend(packet(b"a"));
        input.extend(packet(b"final"));
        for capacity in [1, 3, 4, 7, 128] {
            let mut guarded = stream(input.clone(), 8);
            let mut output = Vec::new();
            let mut buffer = vec![0; capacity];
            loop {
                let count = guarded.read(&mut buffer).await.unwrap();
                if count == 0 {
                    break;
                }
                output.extend_from_slice(&buffer[..count]);
            }
            assert_eq!(output, input);
        }
    });
}
#[test]
fn invalid_lengths_never_reach_the_downstream_reader_or_consume_body_bytes() {
    crate::net::session::runtime().block_on(async {
        for length in [0_u32, 9, u32::MAX] {
            let mut input = length.to_be_bytes().to_vec();
            input.extend_from_slice(b"unconsumed body");
            let mut guarded = stream(input, 8);
            let mut output = Vec::new();
            assert_eq!(
                guarded.read_to_end(&mut output).await.unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert!(output.is_empty());
            assert_eq!(guarded.inner.offset, 4);
            assert_eq!(guarded.read(&mut [0]).await.unwrap(), 0);
            assert_eq!(guarded.inner.offset, 4);
        }
    });
}
#[test]
fn partial_prefixes_and_bodies_are_terminal_errors() {
    crate::net::session::runtime().block_on(async {
        for input in [vec![0], vec![0, 0], vec![0, 0, 0], vec![0, 0, 0, 3, 1, 2]] {
            let mut guarded = stream(input, 8);
            let mut output = Vec::new();
            assert_eq!(
                guarded.read_to_end(&mut output).await.unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
            assert_eq!(guarded.read(&mut [0]).await.unwrap(), 0);
        }
    });
}
#[test]
fn outgoing_bytes_are_unchanged_and_framing_errors_refuse_later_writes() {
    crate::net::session::runtime().block_on(async {
        let (client, mut peer) = tokio::io::duplex(64);
        let mut guarded = BoundedSftpStream::new(client, 8);
        guarded.write_all(b"outgoing request").await.unwrap();
        guarded.flush().await.unwrap();
        let mut sent = [0; 16];
        peer.read_exact(&mut sent).await.unwrap();
        assert_eq!(&sent, b"outgoing request");
        peer.write_all(&9_u32.to_be_bytes()).await.unwrap();
        assert!(guarded.read(&mut [0]).await.is_err());
        assert_eq!(
            guarded.write(b"later").await.unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        guarded.shutdown().await.unwrap();
    });
}
#[test]
fn actual_sftp_handshake_accepts_valid_version_and_rejects_oversized_prefix_without_body() {
    crate::net::session::runtime().block_on(async {
        for oversized in [false, true] {
            let (client, mut peer) = tokio::io::duplex(64);
            let (release, wait) = tokio::sync::oneshot::channel::<()>();
            let (closed, observed_close) = tokio::sync::oneshot::channel::<()>();
            let server = tokio::spawn(async move {
                let mut init = [0; 9];
                peer.read_exact(&mut init).await.unwrap();
                assert_eq!(init, [0, 0, 0, 5, 1, 0, 0, 0, 3]);
                let response = if oversized {
                    9_u32.to_be_bytes().to_vec()
                } else {
                    packet(&[2, 0, 0, 0, 3])
                };
                for byte in response {
                    peer.write_all(&[byte]).await.unwrap();
                    tokio::task::yield_now().await;
                }
                if oversized {
                    assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
                    closed.send(()).unwrap();
                }
                // Keep the peer open until the client has finished its request.
                let _ = wait.await;
            });
            let handshake = tokio::spawn(async move {
                let config = russh_sftp::client::Config {
                    request_timeout_secs: 1,
                    ..Default::default()
                };
                russh_sftp::client::SftpSession::new_with_config(
                    BoundedSftpStream::new(client, 8),
                    config,
                )
                .await
            });
            if oversized {
                tokio::time::timeout(std::time::Duration::from_millis(800), observed_close)
                    .await
                    .expect("Invalid frame did not close the stream before request timeout")
                    .unwrap();
            }
            let result = tokio::time::timeout(std::time::Duration::from_secs(3), handshake).await;
            release.send(()).unwrap();
            let result = result.expect("Handshake exceeded request timeout").unwrap();
            if oversized {
                assert!(result.is_err());
            } else {
                result.unwrap().close().await.unwrap();
            }
            server.await.unwrap();
        }
    });
}
