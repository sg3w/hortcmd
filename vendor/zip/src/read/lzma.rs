use std::collections::VecDeque;
use std::io::{BufRead, Error, ErrorKind, Read, Result, Write};
use xz2::{stream::Stream, write::XzDecoder};

pub struct LzmaDecoder<R> {
    compressed_reader: R,
    stream: Option<XzDecoder<BoundedOutput>>,
    output: BoundedOutput,
    size: u64,
    eos: bool,
    initialized: bool,
    failed: bool,
}

impl<R> std::fmt::Debug for LzmaDecoder<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LzmaDecoder")
            .field("size", &self.size)
            .field("eos", &self.eos)
            .field("initialized", &self.initialized)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl<R: Read> LzmaDecoder<R> {
    pub fn new(inner: R, size: u64, eos: bool) -> Self {
        Self {
            compressed_reader: inner,
            stream: None,
            output: BoundedOutput::new(size),
            size,
            eos,
            initialized: false,
            failed: false,
        }
    }

    pub fn into_inner(self) -> R {
        self.compressed_reader
    }

    fn initialize(&mut self) -> Result<()> {
        // APPNOTE 5.8: SDK version (informational), property length, properties.
        // ZIP does not carry an LZMA-alone uncompressed-size field here.
        let mut prefix = [0; 4];
        self.compressed_reader.read_exact(&mut prefix)?;
        if u16::from_le_bytes([prefix[2], prefix[3]]) != 5 {
            return Err(invalid("Unsupported ZIP LZMA properties length"));
        }
        let mut header = [0; 13];
        self.compressed_reader.read_exact(&mut header[..5])?;
        if header[0] >= 225 {
            return Err(invalid("Invalid ZIP LZMA properties"));
        }
        let dictionary = u32::from_le_bytes(header[1..5].try_into().unwrap());
        if dictionary as u64 > archive_budget::DICTIONARY as u64 {
            return Err(archive_budget::fail());
        }
        // Adapt only the framing to liblzma. An unknown size requires an EOS
        // marker; otherwise the ZIP metadata supplies the exact decoded size.
        let size = if self.eos { u64::MAX } else { self.size };
        header[5..].copy_from_slice(&size.to_le_bytes());
        let codec = Stream::new_lzma_decoder(archive_budget::DICTIONARY as u64)?;
        let mut stream = XzDecoder::new_stream(BoundedOutput::new(self.size), codec);
        stream.write_all(&header)?;
        self.stream = Some(stream);
        self.initialized = true;
        Ok(())
    }
}

impl<R: BufRead> LzmaDecoder<R> {
    fn read_inner(&mut self, buf: &mut [u8]) -> Result<usize> {
        archive_budget::check()?;
        if !self.initialized {
            self.initialize()?;
        }
        loop {
            archive_budget::check()?;
            let Some(stream) = self.stream.as_mut() else {
                return self.output.read(buf);
            };
            let count = stream.get_mut().read(buf)?;
            if count > 0 {
                return Ok(count);
            }
            let compressed = self.compressed_reader.fill_buf()?;
            if compressed.is_empty() {
                self.output = self.stream.take().unwrap().finish()?;
                if self.output.written != self.size {
                    return Err(invalid("ZIP LZMA uncompressed size mismatch"));
                }
            } else {
                // Bound each call even when the caller supplies a large buffer.
                let consumed = compressed.len().min(8192);
                stream.write_all(&compressed[..consumed])?;
                self.compressed_reader.consume(consumed);
            }
        }
    }
}

impl<R: BufRead> Read for LzmaDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.failed {
            return Err(invalid("Invalid ZIP LZMA stream"));
        }
        self.read_inner(buf).map_err(|error| {
            self.failed = true;
            if error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<xz2::stream::Error>())
                == Some(&xz2::stream::Error::MemLimit)
            {
                archive_budget::fail()
            } else {
                error
            }
        })
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

#[derive(Debug)]
struct BoundedOutput {
    bytes: VecDeque<u8>,
    written: u64,
    size: u64,
}
impl BoundedOutput {
    fn new(size: u64) -> Self {
        Self {
            bytes: VecDeque::new(),
            written: 0,
            size,
        }
    }
}
impl Read for BoundedOutput {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.bytes.read(buf)
    }
}
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        archive_budget::check()?;
        let written = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("ZIP LZMA uncompressed size overflow"))?;
        if written > self.size {
            return Err(invalid("ZIP LZMA uncompressed size mismatch"));
        }
        if self.bytes.len().saturating_add(bytes.len()) > 8 * 1024 * 1024 {
            return Err(archive_budget::fail());
        }
        self.bytes.write_all(bytes)?;
        self.written = written;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
