//! Validate inbound SFTP lengths before russh-sftp allocates a packet body.
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Includes the packet type and payload, excludes the four-byte length prefix.
pub const MAX_PACKET_BYTES: u32 = 1024 * 1024;

pub struct BoundedSftpStream<S> {
    inner: S,
    limit: u32,
    prefix: [u8; 4],
    received: usize,
    emitted: usize,
    remaining: usize,
    failed: bool,
}
impl<S> BoundedSftpStream<S> {
    pub fn new(inner: S, limit: u32) -> Self {
        Self {
            inner,
            limit,
            prefix: [0; 4],
            received: 0,
            emitted: 0,
            remaining: 0,
            failed: false,
        }
    }
    fn fail(&mut self, kind: io::ErrorKind, message: &'static str) -> Poll<io::Result<()>> {
        self.failed = true;
        Poll::Ready(Err(io::Error::new(kind, message)))
    }
}
impl<S: AsyncRead + Unpin> AsyncRead for BoundedSftpStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        // An error is terminal: subsequent EOF makes the dependency stop its reader.
        if output.remaining() == 0 || this.failed {
            return Poll::Ready(Ok(()));
        }
        while this.received < 4 {
            let mut prefix = ReadBuf::new(&mut this.prefix[this.received..]);
            match Pin::new(&mut this.inner).poll_read(cx, &mut prefix) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    this.failed = true;
                    return Poll::Ready(Err(error));
                }
                Poll::Ready(Ok(())) => {
                    let count = prefix.filled().len();
                    if count == 0 {
                        if this.received == 0 {
                            return Poll::Ready(Ok(()));
                        }
                        return this
                            .fail(io::ErrorKind::UnexpectedEof, "Truncated SFTP packet length");
                    }
                    this.received += count;
                }
            }
        }
        if this.emitted == 0 {
            let length = u32::from_be_bytes(this.prefix);
            if length == 0 || length > this.limit {
                return this.fail(
                    io::ErrorKind::InvalidData,
                    "Incoming SFTP packet exceeds the allowed length",
                );
            }
            this.remaining = length as usize;
        }
        if this.emitted < 4 {
            let end = (this.emitted + output.remaining()).min(4);
            output.put_slice(&this.prefix[this.emitted..end]);
            this.emitted = end;
            return Poll::Ready(Ok(()));
        }
        let mut bytes = [0; 8192];
        let count = bytes.len().min(output.remaining()).min(this.remaining);
        let mut body = ReadBuf::new(&mut bytes[..count]);
        match Pin::new(&mut this.inner).poll_read(cx, &mut body) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => {
                this.failed = true;
                Poll::Ready(Err(error))
            }
            Poll::Ready(Ok(())) => {
                let count = body.filled().len();
                if count == 0 {
                    return this.fail(io::ErrorKind::UnexpectedEof, "Truncated SFTP packet body");
                }
                output.put_slice(body.filled());
                this.remaining -= count;
                if this.remaining == 0 {
                    this.received = 0;
                    this.emitted = 0;
                }
                Poll::Ready(Ok(()))
            }
        }
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for BoundedSftpStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.failed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        Pin::new(&mut this.inner).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests;
