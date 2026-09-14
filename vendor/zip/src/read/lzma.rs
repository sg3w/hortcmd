use lzma_rs::decompress::{Options, Stream, UnpackedSize};
use std::collections::VecDeque;
use std::io::{BufRead, Error, ErrorKind, Read, Result, Write};

const OPTIONS: Options = Options {
    unpacked_size: UnpackedSize::ReadFromHeader,
    memlimit: Some(archive_budget::DICTIONARY),
    allow_incomplete: true,
};

#[derive(Debug)]
pub struct LzmaDecoder<R> {
    compressed_reader: R,
    stream: Option<Stream<BoundedOutput>>,
    output: BoundedOutput,
}

impl<R: Read> LzmaDecoder<R> {
    pub fn new(inner: R) -> Self {
        LzmaDecoder {
            compressed_reader: inner,
            stream: Some(Stream::new_with_options(
                &OPTIONS,
                BoundedOutput(VecDeque::new()),
            )),
            output: BoundedOutput(VecDeque::new()),
        }
    }

    pub fn into_inner(self) -> R {
        self.compressed_reader
    }
}

impl<R: BufRead> Read for LzmaDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            archive_budget::check()?;
            let Some(stream) = self.stream.as_mut() else {
                return self.output.read(buf);
            };
            let count = stream
                .get_output_mut()
                .ok_or(Error::new(ErrorKind::InvalidData, "Invalid LZMA stream"))?
                .read(buf)?;
            if count > 0 {
                return Ok(count);
            }
            let compressed = self.compressed_reader.fill_buf()?;
            if compressed.is_empty() {
                self.output = self
                    .stream
                    .take()
                    .unwrap()
                    .finish()
                    .map_err(|error| Error::new(ErrorKind::InvalidData, error.to_string()))?;
            } else {
                let consumed = compressed.len();
                stream.write_all(compressed)?;
                self.compressed_reader.consume(consumed);
            }
        }
    }
}

#[derive(Debug)]
struct BoundedOutput(VecDeque<u8>);
impl Read for BoundedOutput {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.read(buf)
    }
}
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        archive_budget::check()?;
        if self.0.len().saturating_add(bytes.len()) > 8 * 1024 * 1024 {
            return Err(archive_budget::fail());
        }
        self.0.write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
