use std::{
    fs::OpenOptions,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    iter::repeat,
    path::PathBuf,
};

use chic::{
    Decoder, Digest, Error, Layout,
    sync::{
        ChunkedEncoder, DecodeReader, FileSpool, VecSpool, decode_slice, digest_file, digest_slice,
        encode_file, encode_slice, encode_slice_to,
    },
};
use sha2::{Digest as _, Sha256};

/// The chiclet content length that every implementation is required to support.
const DEFAULT_CHICLET_SIZE: u64 = (1 << 14) - 1;

fn sha256(parts: &[&[u8]]) -> Digest {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// Deterministic pseudo-random bytes.
fn content(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x.to_le_bytes()[0]
        })
        .collect()
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("chic-test-{}-{name}", std::process::id()))
}

#[test]
fn empty() {
    let (enc, digest) = encode_slice(b"", repeat(DEFAULT_CHICLET_SIZE)).unwrap();
    assert_eq!(enc, [0]);
    assert_eq!(digest, sha256(&[&[0]]));
    assert_eq!(
        digest[..4],
        [0x6e, 0x34, 0x0b, 0x9c],
        "SHA-256(0x00) = 6e340b9c..."
    );
    assert_eq!(decode_slice(&enc, digest, 0).unwrap(), b"");
}

#[test]
fn two_chiclets() {
    let (enc, digest) = encode_slice(b"hello world", [5]).unwrap();
    let h1 = sha256(&[b"\x00 world"]);
    let h0 = sha256(&[b"\x05hello", &h1]);
    let mut expected = b"\x05hello".to_vec();
    expected.extend_from_slice(&h1);
    expected.extend_from_slice(b"\x00 world");
    assert_eq!(enc, expected);
    assert_eq!(digest, h0);
    assert_eq!(decode_slice(&enc, digest, 16).unwrap(), b"hello world");
}

#[test]
fn two_byte_length() {
    let data = content(200, 1);
    let (enc, digest) = encode_slice(&data, repeat(100)).unwrap();
    let h1 = sha256(&[&[0], &data[100..]]);
    let h0 = sha256(&[&[0x40, 100], &data[..100], &h1]);
    assert_eq!(digest, h0);
    assert_eq!(&enc[..2], &[0x40, 100]);
    assert_eq!(enc.len(), 2 + 100 + 32 + 1 + 100);
}

/// Encode with every mode, check that they agree, and return the result.
fn encode_all_modes(data: &[u8], lengths: &[u64]) -> (Vec<u8>, Digest) {
    let layout = Layout::new(u64::try_from(data.len()).unwrap(), lengths.iter().copied()).unwrap();
    let (enc, digest) = encode_slice(data, lengths.iter().copied()).unwrap();
    assert_eq!(u64::try_from(enc.len()).unwrap(), layout.encoded_len());
    assert_eq!(digest_slice(data, lengths.iter().copied()).unwrap(), digest);

    let mut out = Vec::new();
    let d = encode_slice_to(data, lengths.iter().copied(), &mut out).unwrap();
    assert_eq!((&out, d), (&enc, digest), "encode_slice_to");

    // File mode over a cursor, starting at a non-zero position.
    let mut src = b"junk".to_vec();
    src.extend_from_slice(data);
    let mut cursor = Cursor::new(src);
    cursor.seek(SeekFrom::Start(4)).unwrap();
    let mut out = Vec::new();
    let d = encode_file(&mut cursor, lengths.iter().copied(), &mut out).unwrap();
    assert_eq!((&out, d), (&enc, digest), "encode_file");
    cursor.seek(SeekFrom::Start(4)).unwrap();
    assert_eq!(
        digest_file(&mut cursor, lengths.iter().copied()).unwrap(),
        digest
    );

    // Chunked mode, with chunks that match the layout.
    let mut chunked = ChunkedEncoder::<VecSpool>::default();
    for c in layout.iter() {
        chunked
            .write_chunk(
                &data[usize::try_from(c.offset).unwrap()
                    ..usize::try_from(c.offset + c.len).unwrap()],
            )
            .unwrap();
        chunked.write_chunk(b"").unwrap(); // ignored
    }
    let mut out = Vec::new();
    let d = chunked.finish(&mut out).unwrap();
    assert_eq!((&out, d), (&enc, digest), "chunked");

    (enc, digest)
}

#[test]
fn modes_agree() {
    let n = 64u64;
    for len in [0, 1, 63, 64, 65, 128, 129, 1000] {
        let data = content(len, u64::try_from(len).unwrap());
        let (enc, digest) = encode_all_modes(&data, &vec![n; 100]);
        assert_eq!(decode_slice(&enc, digest, n).unwrap(), data);
    }
    let data = content(1000, 7);
    for lengths in [
        vec![],
        vec![1],
        vec![999],
        vec![1000],
        vec![3, 500, 1, 70000],
    ] {
        let (enc, digest) = encode_all_modes(&data, &lengths);
        assert_eq!(decode_slice(&enc, digest, 1000).unwrap(), data);
    }
}

#[test]
fn random_layouts() {
    for seed in 0..50u64 {
        let r = content(64, seed + 1000);
        let len = usize::from(r[0]) * 40 + usize::from(r[1]);
        let data = content(len, seed);
        // Lengths up to 256, never zero; the final chiclet can be longer.
        let lengths: Vec<u64> = r[2..].iter().map(|&b| u64::from(b) + 1).collect();
        let (enc, digest) = encode_all_modes(&data, &lengths);
        assert_eq!(
            decode_slice(&enc, digest, u64::try_from(len).unwrap()).unwrap(),
            data
        );
    }
}

#[test]
fn file_and_file_spool() {
    let data = content(100_000, 3);
    let path = temp_path("input");
    std::fs::write(&path, &data).unwrap();
    let mut f = std::fs::File::open(&path).unwrap();
    let mut out = Vec::new();
    let digest = encode_file(&mut f, repeat(DEFAULT_CHICLET_SIZE), &mut out).unwrap();
    std::fs::remove_file(&path).unwrap();
    let (enc, d) = encode_slice(&data, repeat(DEFAULT_CHICLET_SIZE)).unwrap();
    assert_eq!((out, digest), (enc, d));

    let spool_path = temp_path("spool");
    let spool_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&spool_path)
        .unwrap();
    let mut chunked = ChunkedEncoder::new(FileSpool::new(spool_file).unwrap())
        .with_max_length(DEFAULT_CHICLET_SIZE);
    // Large writes are split at the maximum.
    chunked.write_chunk(&data[..50_000]).unwrap();
    chunked.write_chunk(&data[50_000..]).unwrap();
    let mut out = Vec::new();
    let digest = chunked.finish(&mut out).unwrap();
    std::fs::remove_file(&spool_path).unwrap();
    assert_eq!(
        decode_slice(&out, digest, DEFAULT_CHICLET_SIZE).unwrap(),
        data
    );
    let lengths = [16383, 16383, 16383, 851, 16383, 16383, 16383];
    let (enc, d) = encode_slice(&data, lengths).unwrap();
    assert_eq!((out, digest), (enc, d));
}

#[test]
fn chunked_empty_final() {
    let mut chunked = ChunkedEncoder::<VecSpool>::default();
    chunked.write_chunk(b"abc").unwrap();
    chunked.write_chunk(b"defg").unwrap();
    let mut out = Vec::new();
    let digest = chunked.finish_with_empty_final(&mut out).unwrap();
    assert_eq!(out.len(), 2 * 33 + 3 + 4 + 1);
    assert_eq!(*out.last().unwrap(), 0);
    assert_eq!(decode_slice(&out, digest, 10).unwrap(), b"abcdefg");

    let mut out = Vec::new();
    let digest = ChunkedEncoder::<VecSpool>::default()
        .finish_with_empty_final(&mut out)
        .unwrap();
    assert_eq!((out, digest), encode_slice(b"", []).unwrap());
}

#[test]
fn tamper() {
    let data = content(50, 9);
    let layout = Layout::new(50, repeat(7)).unwrap();
    let (enc, digest) = encode_slice(&data, repeat(7)).unwrap();
    // The range of each chiclet in the encoding, with its content offset.
    let mut ranges = Vec::new();
    let mut start = 0;
    for c in layout.iter() {
        let len = usize::try_from(if c.is_final {
            1 + c.len
        } else {
            1 + c.len + 32
        })
        .unwrap();
        ranges.push((start..start + len, usize::try_from(c.offset).unwrap()));
        start += len;
    }
    assert_eq!(start, enc.len());

    for i in 0..enc.len() {
        for bit in [0x01, 0x80] {
            let mut bad = enc.clone();
            bad[i] ^= bit;
            let mut decoder = Decoder::new(digest).with_max_length(100);
            let mut out = Vec::new();
            let res = decoder
                .feed(&bad, &mut out)
                .and_then(|()| decoder.finish(&mut out));
            assert!(res.is_err(), "flipping byte {i} not detected");
            // Nothing is released from the damaged chiclet onwards.
            let (_, released) = ranges.iter().find(|(r, _)| r.contains(&i)).unwrap();
            assert_eq!(out, &data[..*released], "byte {i}");
        }
    }
}

#[test]
fn truncation() {
    let data = content(50, 10);
    let (enc, digest) = encode_slice(&data, repeat(7)).unwrap();
    for cut in 0..enc.len() {
        let mut decoder = Decoder::new(digest);
        let mut out = Vec::new();
        let res = decoder
            .feed(&enc[..cut], &mut out)
            .and_then(|()| decoder.finish(&mut out));
        assert!(
            matches!(res, Err(Error::Truncated | Error::HashMismatch { .. })),
            "cut at {cut}: {res:?}"
        );
        assert!(data.starts_with(&out));
    }
}

#[test]
fn wrong_digest() {
    let (enc, mut digest) = encode_slice(b"hello world", [5]).unwrap();
    digest[0] ^= 1;
    assert!(matches!(
        decode_slice(&enc, digest, 16),
        Err(Error::HashMismatch { chiclet: 0 })
    ));
}

#[test]
fn decoder_limits() {
    let data = content(100, 11);
    let (enc, digest) = encode_slice(&data, [60]).unwrap();
    assert!(matches!(
        decode_slice(&enc, digest, 59),
        Err(Error::ChicletTooLong)
    ));
    // The final chiclet is 40 bytes.
    let (enc, digest) = encode_slice(&data, [30, 30]).unwrap();
    assert!(matches!(
        decode_slice(&enc, digest, 39),
        Err(Error::ChicletTooLong)
    ));
    assert_eq!(decode_slice(&enc, digest, 40).unwrap(), data);
}

#[test]
fn decoder_failed_state() {
    let (enc, digest) = encode_slice(b"hello world", [5]).unwrap();
    let mut decoder = Decoder::new(digest);
    let mut out = Vec::new();
    decoder.feed(&enc, &mut out).unwrap();
    decoder.finish(&mut out).unwrap();
    assert_eq!(decoder.chiclets(), 2);
    assert!(matches!(decoder.feed(b"x", &mut out), Err(Error::Failed)));
}

#[test]
fn non_minimal_length() {
    // A two-byte encoding of 5 is accepted, provided the hash covers it.
    let h1 = sha256(&[b"\x00 world"]);
    let h0 = sha256(&[b"\x40\x05hello", &h1]);
    let mut enc = b"\x40\x05hello".to_vec();
    enc.extend_from_slice(&h1);
    enc.extend_from_slice(b"\x00 world");
    assert_eq!(decode_slice(&enc, h0, 16).unwrap(), b"hello world");
}

#[test]
fn decode_reader() {
    /// A reader that returns at most 7 bytes at a time.
    struct Dribble<'a>(&'a [u8]);
    impl Read for Dribble<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.0.len()).min(7);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let data = content(100_000, 12);
    let (enc, digest) = encode_slice(&data, repeat(1000)).unwrap();

    let mut out = Vec::new();
    DecodeReader::new(Dribble(&enc), digest)
        .read_to_end(&mut out)
        .unwrap();
    assert_eq!(out, data);

    let mut bad = enc.clone();
    bad[50_000] ^= 1;
    let mut out = Vec::new();
    let err = DecodeReader::new(&bad[..], digest)
        .read_to_end(&mut out)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(out.len() < 50_000);
}

#[test]
fn layout_errors() {
    assert!(matches!(
        encode_slice(b"abc", [1, 0]),
        Err(Error::ZeroLength)
    ));
    let mut sink = std::io::sink();
    assert!(matches!(
        encode_file(&mut Cursor::new(b"abc"), repeat(0), &mut sink),
        Err(Error::ZeroLength)
    ));
    sink.flush().unwrap();
}

#[test]
fn io_error_roundtrip() {
    let e: std::io::Error = Error::HashMismatch { chiclet: 3 }.into();
    assert!(matches!(Error::from(e), Error::HashMismatch { chiclet: 3 }));
}
