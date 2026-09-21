//! Externally encoded method-14 conformance fixtures, independent of our decoder.
use super::budget;
use std::{
    io::{Cursor, Read},
    sync::{atomic::AtomicBool, Arc},
};

const EOS: &[u8] = include_bytes!("fixtures/zip-lzma/python-eos.zip");
const NO_EOS: &[u8] = include_bytes!("fixtures/zip-lzma/liblzma-no-eos.zip");
const EMPTY: &[u8] = include_bytes!("fixtures/zip-lzma/liblzma-no-eos-empty.zip");

fn read(bytes: &[u8], name: &str) -> crate::errors::AppResult<Vec<u8>> {
    budget::run(Arc::new(AtomicBool::new(false)), 30, || {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut file = archive.by_name(name)?;
        let mut output = Vec::new();
        file.read_to_end(&mut output)?;
        Ok(output)
    })
}
fn central(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .position(|part| part == b"PK\x01\x02")
        .unwrap()
}
fn data_start(bytes: &[u8]) -> usize {
    30 + usize::from(u16::from_le_bytes([bytes[26], bytes[27]]))
        + usize::from(u16::from_le_bytes([bytes[28], bytes[29]]))
}
fn set_size(bytes: &mut [u8], size: u32) {
    let at = central(bytes);
    bytes[22..26].copy_from_slice(&size.to_le_bytes());
    bytes[at + 24..at + 28].copy_from_slice(&size.to_le_bytes());
}

#[test]
fn external_eos_and_no_eos_files_decode_repeatedly_in_small_reads() {
    let expected: Vec<u8> = (0..256)
        .map(|value| value as u8)
        .cycle()
        .take(40960)
        .collect();
    for (bytes, content, name) in [
        (EOS, expected.as_slice(), "payload.bin"),
        (EOS, b"".as_slice(), "empty.bin"),
        (
            NO_EOS,
            b"ZIP LZMA without an end marker.\n".as_slice(),
            "payload.bin",
        ),
        (EMPTY, b"".as_slice(), "payload.bin"),
    ] {
        budget::run(Arc::new(AtomicBool::new(false)), 30, || {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
            for _ in 0..2 {
                let mut file = archive.by_name(name)?;
                let mut output = Vec::new();
                let mut chunk = [0; 7];
                loop {
                    let count = file.read(&mut chunk)?;
                    if count == 0 {
                        break;
                    }
                    output.extend_from_slice(&chunk[..count]);
                }
                assert_eq!(output, content);
            }
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn flags_require_a_real_eos_marker_even_for_empty_files() {
    for fixture in [NO_EOS, EMPTY] {
        let mut bytes = fixture.to_vec();
        let at = central(&bytes);
        bytes[6] |= 2;
        bytes[at + 8] |= 2;
        assert!(read(&bytes, "payload.bin").is_err());
    }
}

#[test]
fn malformed_properties_sizes_crc_and_lengths_are_rejected() {
    let start = data_start(NO_EOS);
    for length in [0u16, 4, 6, u16::MAX] {
        let mut bytes = NO_EOS.to_vec();
        bytes[start + 2..start + 4].copy_from_slice(&length.to_le_bytes());
        assert!(read(&bytes, "payload.bin").is_err());
    }
    let mut bytes = NO_EOS.to_vec();
    bytes[start + 4] = 225;
    assert!(read(&bytes, "payload.bin").is_err());
    for source in [EOS, NO_EOS, EMPTY] {
        for size in [0, 1, 100_000] {
            if size == 0 && source == EMPTY {
                continue;
            }
            let mut bytes = source.to_vec();
            set_size(&mut bytes, size);
            assert!(read(&bytes, "payload.bin").is_err());
        }
        let mut bytes = source.to_vec();
        let at = central(&bytes);
        bytes[at + 16] ^= 1;
        assert!(read(&bytes, "payload.bin").is_err());
    }
}

#[test]
fn dictionary_and_cancellation_limits_precede_decode() {
    let mut bytes = NO_EOS.to_vec();
    let start = data_start(&bytes);
    bytes[start + 5..start + 9].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read(&bytes, "payload.bin")
        .unwrap_err()
        .is(crate::errors::code::ARCHIVE_LIMIT));
    let error = budget::run(Arc::new(AtomicBool::new(true)), 30, || {
        let mut archive = zip::ZipArchive::new(Cursor::new(EOS))?;
        archive.by_index(0)?.read_to_end(&mut Vec::new())?;
        Ok(())
    })
    .unwrap_err();
    assert!(error.is(crate::errors::code::CANCELLED));
}

#[test]
fn truncated_compressed_streams_never_report_success() {
    for source in [EOS, NO_EOS, EMPTY] {
        let at = central(source);
        let original_size = u32::from_le_bytes(source[at + 20..at + 24].try_into().unwrap());
        for size in [0, 3, 8, original_size - 1] {
            let mut bytes = source.to_vec();
            bytes[18..22].copy_from_slice(&size.to_le_bytes());
            bytes[at + 20..at + 24].copy_from_slice(&size.to_le_bytes());
            assert!(
                read(&bytes, "payload.bin").is_err(),
                "accepted {size} of {original_size}"
            );
        }
    }
}

#[test]
fn non_seekable_reader_uses_local_header_flags_and_size() {
    for source in [EOS, NO_EOS, EMPTY] {
        let expected = read(source, "payload.bin").unwrap();
        budget::run(Arc::new(AtomicBool::new(false)), 30, || {
            let mut input = Cursor::new(source);
            let mut file = zip::read::read_zipfile_from_stream(&mut input)?.unwrap();
            let mut output = Vec::new();
            file.read_to_end(&mut output)?;
            assert_eq!(output, expected);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn sdk_version_is_metadata_but_compressed_trailing_bytes_are_rejected() {
    let mut bytes = NO_EOS.to_vec();
    let start = data_start(&bytes);
    bytes[start..start + 2].copy_from_slice(&[99, 42]);
    assert_eq!(
        read(&bytes, "payload.bin").unwrap(),
        b"ZIP LZMA without an end marker.\n"
    );
    let at = central(&bytes);
    let size = u32::from_le_bytes(bytes[at + 20..at + 24].try_into().unwrap()) + 1;
    bytes[18..22].copy_from_slice(&size.to_le_bytes());
    bytes[at + 20..at + 24].copy_from_slice(&size.to_le_bytes());
    assert!(read(&bytes, "payload.bin").is_err());
}

#[test]
fn buffered_expansion_remains_bounded() {
    assert!(read(EOS, "expansion.bin")
        .unwrap_err()
        .is(crate::errors::code::ARCHIVE_LIMIT));
}
