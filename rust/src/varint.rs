//! QUIC variable-length integers (RFC 9000, Section 16).

/// The largest value that can be encoded.
pub const MAX: u64 = (1 << 62) - 1;

/// The number of bytes in the minimal encoding of `v`.
#[must_use]
pub const fn encoded_len(v: u64) -> usize {
    if v < 1 << 6 {
        1
    } else if v < 1 << 14 {
        2
    } else if v < 1 << 30 {
        4
    } else {
        8
    }
}

/// The total length of an encoded value, determined from its first byte.
#[must_use]
pub const fn len_from_first(b: u8) -> usize {
    1 << (b >> 6)
}

/// Encode `v` minimally.  Returns a buffer and the number of bytes used.
///
/// # Panics
/// If `v` exceeds [`MAX`].
#[must_use]
pub fn encode(v: u64) -> ([u8; 8], usize) {
    assert!(v <= MAX, "varint value out of range");
    let n = encoded_len(v);
    let mut out = [0; 8];
    out[..n].copy_from_slice(&v.to_be_bytes()[8 - n..]);
    out[0] |= match n {
        1 => 0x00,
        2 => 0x40,
        4 => 0x80,
        _ => 0xc0,
    };
    (out, n)
}

/// Decode a value from the start of `buf`.
/// Returns the value and the number of bytes read,
/// or `None` if `buf` does not hold a complete value.
#[must_use]
pub fn decode(buf: &[u8]) -> Option<(u64, usize)> {
    let first = *buf.first()?;
    let n = len_from_first(first);
    let rest = buf.get(1..n)?;
    let v = rest
        .iter()
        .fold(u64::from(first & 0x3f), |v, &b| (v << 8) | u64::from(b));
    Some((v, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc9000_examples() {
        for (v, enc) in [
            (
                151_288_809_941_952_652,
                &[0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c][..],
            ),
            (494_878_333, &[0x9d, 0x7f, 0x3e, 0x7d]),
            (15_293, &[0x7b, 0xbd]),
            (37, &[0x25]),
        ] {
            let (b, n) = encode(v);
            assert_eq!(&b[..n], enc);
            assert_eq!(decode(enc), Some((v, enc.len())));
        }
        // Non-minimal encoding is accepted.
        assert_eq!(decode(&[0x40, 0x25]), Some((37, 2)));
    }

    #[test]
    fn boundaries() {
        for (v, n) in [
            (0, 1),
            (63, 1),
            (64, 2),
            (16383, 2),
            (16384, 4),
            ((1 << 30) - 1, 4),
            (1 << 30, 8),
            (MAX, 8),
        ] {
            let (b, len) = encode(v);
            assert_eq!(len, n);
            assert_eq!(decode(&b[..len]), Some((v, n)));
            assert_eq!(decode(&b[..len - 1]), None);
        }
        assert_eq!(decode(&[]), None);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn too_large() {
        let _ = encode(MAX + 1);
    }
}
