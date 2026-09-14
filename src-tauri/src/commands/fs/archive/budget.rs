//! Decoder budgets shared with the audited, pinned dependency patches.
use super::{password, safety};
use crate::errors::{code, AppError, AppResult};
use std::{
    fs::File,
    io::{self, Read},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

pub(super) fn failure() -> Option<AppError> {
    archive_budget::check()
        .err()
        .and_then(|error| {
            error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<archive_budget::Failure>())
                .copied()
        })
        .map(|error| match error {
            archive_budget::Failure::Cancelled => AppError::new(code::CANCELLED),
            archive_budget::Failure::Limit => safety::limit(),
        })
}
pub(super) fn run<T>(
    cancel: Arc<AtomicBool>,
    seconds: u64,
    work: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _scope = archive_budget::Scope::new(cancel, Duration::from_secs(seconds));
    if let Some(error) = failure() {
        return Err(error);
    }
    let result = decode(work);
    match failure() {
        Some(error) => Err(error),
        None => result,
    }
}
/// A malformed upstream parser must not skip the caller's staging cleanup.
pub(super) fn decode<T>(work: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err(AppError::new(code::ARCHIVE_CORRUPT)))
}
pub(super) fn password_limit(password: &Option<String>) -> AppResult<()> {
    if password.as_ref().is_some_and(|p| p.len() > 4096) {
        return Err(safety::limit());
    }
    Ok(())
}
pub(super) fn open(path: &str) -> io::Result<archive_budget::Reader<File>> {
    archive_budget::check()?;
    Ok(archive_budget::Reader(File::open(path)?))
}
pub(super) fn seven_meta(
    path: &str,
    pw: &Option<String>,
) -> Result<sevenz_rust::Archive, sevenz_rust::Error> {
    let mut file = open(path).map_err(sevenz_rust::Error::io)?;
    let size = file.0.metadata().map_err(sevenz_rust::Error::io)?.len();
    sevenz_rust::Archive::read(&mut file, size, password(pw).as_slice())
}
pub(super) fn seven_reader(
    path: &str,
    pw: &Option<String>,
) -> Result<sevenz_rust::SevenZReader<archive_budget::Reader<File>>, sevenz_rust::Error> {
    let file = open(path).map_err(sevenz_rust::Error::io)?;
    let size = file.0.metadata().map_err(sevenz_rust::Error::io)?.len();
    sevenz_rust::SevenZReader::new(file, size, password(pw))
}
/// Preserve liblzma's typed allocation refusal as the existing localized limit code.
pub(super) struct XzReader<R>(pub R);
impl<R: Read> Read for XzReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.0.read(out).map_err(|error| {
            if error
                .get_ref()
                .and_then(|e| e.downcast_ref::<xz2::stream::Error>())
                == Some(&xz2::stream::Error::MemLimit)
            {
                archive_budget::fail()
            } else {
                error
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Seek, SeekFrom, Write};
    use std::sync::atomic::Ordering;

    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    fn seven(header: &[u8], declared: u64) -> Vec<u8> {
        let mut start = Vec::new();
        start.extend(0u64.to_le_bytes());
        start.extend(declared.to_le_bytes());
        start.extend(crc(header).to_le_bytes());
        let mut bytes = b"7z\xbc\xaf\x27\x1c\x00\x04".to_vec();
        bytes.extend(crc(&start).to_le_bytes());
        bytes.extend(start);
        bytes.extend(header);
        bytes
    }
    fn seven_result(bytes: Vec<u8>) -> AppResult<()> {
        run(Arc::new(AtomicBool::new(false)), 30, || {
            let len = bytes.len() as u64;
            sevenz_rust::Archive::read(&mut Cursor::new(bytes), len, &[])
                .map_err(super::super::map_7z_err)?;
            Ok(())
        })
    }
    #[test]
    fn sevenz_rejects_advertised_header_and_file_counts_before_allocation() {
        assert!(seven_result(seven(&[1, 0], 1 << 40))
            .unwrap_err()
            .is(code::ARCHIVE_LIMIT));
        let mut header = vec![1, 5, 255];
        header.extend(100_001u64.to_le_bytes());
        header.extend([0, 0]);
        assert!(seven_result(seven(&header, header.len() as u64))
            .unwrap_err()
            .is(code::ARCHIVE_LIMIT));
    }
    #[test]
    fn sevenz_encoded_header_dictionary_is_bounded_before_output() {
        let header = [
            0x17, 6, 0, 1, 9, 1, 0, 7, 0x0b, 1, 0, 1, 0x21, 0x21, 1, 40, 0x0c, 1, 0, 0,
        ];
        assert!(seven_result(seven(&header, header.len() as u64))
            .unwrap_err()
            .is(code::ARCHIVE_LIMIT));
        let mut expanded = header[..17].to_vec();
        expanded.push(255);
        expanded.extend((1u64 << 40).to_le_bytes());
        expanded.extend([0, 0]);
        assert!(seven_result(seven(&expanded, expanded.len() as u64))
            .unwrap_err()
            .is(code::ARCHIVE_LIMIT));
    }
    #[test]
    fn aes_work_factor_and_cancellation_are_checked_before_payload() {
        let _scope =
            archive_budget::Scope::new(Arc::new(AtomicBool::new(false)), Duration::from_secs(30));
        assert!(
            sevenz_rust::Aes256Sha256Decoder::new(Cursor::new([]), &[30, 0], b"password").is_err()
        );
        assert!(failure().unwrap().is(code::ARCHIVE_LIMIT));
        let _scope =
            archive_budget::Scope::new(Arc::new(AtomicBool::new(true)), Duration::from_secs(30));
        assert!(
            sevenz_rust::Aes256Sha256Decoder::new(Cursor::new([]), &[20, 0], b"password").is_err()
        );
        assert!(failure().unwrap().is(code::CANCELLED));
    }
    #[test]
    fn encrypted_sevenz_headers_and_payload_still_round_trip() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = sevenz_rust::SevenZWriter::new(&mut output).unwrap();
        writer.set_content_methods(vec![
            sevenz_rust::AesEncoderOptions::new("test-password".into()).into(),
            sevenz_rust::SevenZMethodConfiguration::new(sevenz_rust::SevenZMethod::LZMA2),
        ]);
        for n in 0..20 {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = format!("directory/repeated-name-{n:03}.txt");
            entry.has_stream = true;
            writer
                .push_archive_entry(entry, Some(&b"encrypted contents"[..]))
                .unwrap();
        }
        writer.finish().unwrap();
        let bytes = output.into_inner();
        let decode = |pw: &str| {
            run(Arc::new(AtomicBool::new(false)), 30, || {
                let mut reader = sevenz_rust::SevenZReader::new(
                    Cursor::new(bytes.clone()),
                    bytes.len() as u64,
                    pw.into(),
                )
                .map_err(super::super::map_7z_err)?;
                let mut files = 0;
                reader
                    .for_each_entries(|_, reader| {
                        let mut out = Vec::new();
                        reader.read_to_end(&mut out).unwrap();
                        assert_eq!(out, b"encrypted contents");
                        files += 1;
                        Ok(true)
                    })
                    .map_err(super::super::map_7z_err)?;
                assert_eq!(files, 20);
                Ok(())
            })
        };
        decode("test-password").unwrap();
        assert!(decode("").unwrap_err().is(code::ARCHIVE_PASSWORD_REQUIRED));
        assert!(decode("wrong")
            .unwrap_err()
            .is(code::ARCHIVE_PASSWORD_WRONG));
    }
    #[test]
    fn deadline_and_live_memory_reservations_are_enforced() {
        let _scope = archive_budget::Scope::new(Arc::new(AtomicBool::new(false)), Duration::ZERO);
        assert!(archive_budget::Reader(Cursor::new([1]))
            .read(&mut [0])
            .is_err());
        assert!(failure().unwrap().is(code::ARCHIVE_LIMIT));
        let _scope =
            archive_budget::Scope::new(Arc::new(AtomicBool::new(false)), Duration::from_secs(30));
        for _ in 0..20 {
            let _live = archive_budget::reserve(64 * 1024 * 1024).unwrap();
        }
        let _live = archive_budget::reserve(archive_budget::MEMORY).unwrap();
        assert!(archive_budget::reserve(1).is_err());
    }
    #[test]
    fn cancellation_during_header_read_reaches_buffered_metadata_parser() {
        struct CancelOnHeader {
            input: Cursor<Vec<u8>>,
            cancel: Arc<AtomicBool>,
        }
        impl Read for CancelOnHeader {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.input.position() >= 32 {
                    self.cancel.store(true, Ordering::Relaxed);
                }
                self.input.read(out)
            }
        }
        impl Seek for CancelOnHeader {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                self.input.seek(pos)
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let bytes = seven(&[1, 0], 2);
        let len = bytes.len() as u64;
        let result = run(cancel.clone(), 30, || {
            sevenz_rust::Archive::read(
                &mut CancelOnHeader {
                    input: Cursor::new(bytes),
                    cancel,
                },
                len,
                &[],
            )
            .map_err(super::super::map_7z_err)?;
            Ok(())
        });
        assert!(result.unwrap_err().is(code::CANCELLED));
    }
    #[test]
    fn zip64_excessive_count_is_rejected_before_central_entries() {
        let mut bytes = b"PK\x06\x06".to_vec();
        bytes.extend(44u64.to_le_bytes());
        bytes.extend([45, 0, 45, 0]);
        bytes.extend([0; 8]);
        bytes.extend(100_001u64.to_le_bytes());
        bytes.extend(100_001u64.to_le_bytes());
        bytes.extend([0; 16]);
        bytes.extend(b"PK\x06\x07");
        bytes.extend([0; 12]);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(b"PK\x05\x06");
        bytes.extend([0; 4]);
        bytes.extend([255; 12]);
        bytes.extend([0; 2]);
        let error = run(Arc::new(AtomicBool::new(false)), 30, || {
            zip::ZipArchive::new(Cursor::new(bytes))?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.is(code::ARCHIVE_LIMIT), "{error:?}");
    }
    #[test]
    fn compressed_tar_long_metadata_is_rejected_without_reading_its_body() {
        let mut header = tar::Header::new_gnu();
        header.set_path("PaxHeader").unwrap();
        header.set_entry_type(tar::EntryType::XHeader);
        header.set_size((archive_budget::HEADER + 1) as u64);
        header.set_cksum();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(header.as_bytes()).unwrap();
        let bytes = gz.finish().unwrap();
        let result = run(Arc::new(AtomicBool::new(false)), 30, || {
            let reader = flate2::read::GzDecoder::new(archive_budget::Reader(Cursor::new(bytes)));
            let mut archive = tar::Archive::new(reader);
            archive.entries()?.next().unwrap()?;
            Ok(())
        });
        assert!(result.unwrap_err().is(code::ARCHIVE_LIMIT));
    }
    #[test]
    fn xz_dictionary_limit_has_a_structured_error() {
        let mut bytes = b"\xfd7zXZ\0".to_vec();
        bytes.extend([0, 1]);
        bytes.extend(crc(&[0, 1]).to_le_bytes());
        let block = [2, 0, 0x21, 1, 40, 0, 0, 0];
        bytes.extend(block);
        bytes.extend(crc(&block).to_le_bytes());
        let result = run(Arc::new(AtomicBool::new(false)), 30, || {
            let stream =
                xz2::stream::Stream::new_stream_decoder(archive_budget::DICTIONARY as u64, 0)
                    .unwrap();
            let mut reader = XzReader(xz2::read::XzDecoder::new_stream(Cursor::new(bytes), stream));
            reader.read_exact(&mut [0])?;
            Ok(())
        });
        assert!(result.unwrap_err().is(code::ARCHIVE_LIMIT));
    }
    #[test]
    fn cyclic_coder_graph_and_excessive_work_are_refused() {
        let header = [
            0x17, 6, 0, 1, 9, 1, 0, 7, 0x0b, 1, 0, 2, 1, 0, 1, 0, 0, 0, 0x0c, 1, 1, 0, 0,
        ];
        assert!(seven_result(seven(&header, header.len() as u64))
            .unwrap_err()
            .is(code::ARCHIVE_LIMIT));
        let _scope =
            archive_budget::Scope::new(Arc::new(AtomicBool::new(false)), Duration::from_secs(30));
        assert!(archive_budget::work(archive_budget::WORK + 1).is_err());
        assert!(failure().unwrap().is(code::ARCHIVE_LIMIT));
    }
    #[test]
    fn legacy_zip_lzma_input_is_consumed_once() {
        let original = vec![b'a'; 40_000];
        let options = xz2::stream::LzmaOptions::new_preset(1).unwrap();
        let stream = xz2::stream::Stream::new_lzma_encoder(&options).unwrap();
        let mut encoder = xz2::write::XzEncoder::new_stream(Vec::new(), stream);
        encoder.write_all(&original).unwrap();
        let encoded = encoder.finish().unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "legacy",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&encoded).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
        bytes[8..10].copy_from_slice(&14u16.to_le_bytes());
        bytes[central + 10..central + 12].copy_from_slice(&14u16.to_le_bytes());
        bytes[14..18].copy_from_slice(&crc(&original).to_le_bytes());
        bytes[central + 16..central + 20].copy_from_slice(&crc(&original).to_le_bytes());
        bytes[22..26].copy_from_slice(&(original.len() as u32).to_le_bytes());
        bytes[central + 24..central + 28].copy_from_slice(&(original.len() as u32).to_le_bytes());
        run(Arc::new(AtomicBool::new(false)), 30, || {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
            let mut file = archive.by_index(0)?;
            let mut decoded = Vec::new();
            file.read_to_end(&mut decoded)?;
            assert_eq!(decoded, original);
            Ok(())
        })
        .unwrap();
    }
}
