use crate::{Error, HASH_LEN, MAX_CONTENT, MAX_FINAL_CONTENT, Result, varint};

/// The arrangement of content into chiclets.
///
/// A layout always holds at least one chiclet.
/// The last chiclet is the final chiclet; all others have non-zero length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    lengths: Vec<u64>,
}

/// A single chiclet within a [`Layout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chiclet {
    /// The offset of the chiclet content within the unencoded content.
    pub offset: u64,
    /// The length of the chiclet content.
    pub len: u64,
    /// Whether this is the final chiclet.
    pub is_final: bool,
}

impl Layout {
    /// Arrange `total` bytes of content into chiclets.
    ///
    /// `lengths` provides the content length of each chiclet in turn.
    /// Lengths are drawn until a length reaches or exceeds the content that remains,
    /// at which point the remainder becomes the final chiclet.
    /// If `lengths` ends early, all remaining content goes in the final chiclet.
    /// `std::iter::repeat(n)` therefore produces chiclets of `n` bytes,
    /// with a final chiclet of between 1 and `n` bytes (or 0 bytes, if `total` is 0).
    ///
    /// # Errors
    /// [`Error::ZeroLength`] if a zero length is drawn;
    /// [`Error::ChicletTooLong`] if any chiclet exceeds the absolute maximum length.
    pub fn new<I: IntoIterator<Item = u64>>(total: u64, lengths: I) -> Result<Self> {
        let mut remaining = total;
        let mut out = Vec::new();
        for len in lengths {
            if len == 0 {
                return Err(Error::ZeroLength);
            }
            if len >= remaining {
                break;
            }
            if len > MAX_CONTENT {
                return Err(Error::ChicletTooLong);
            }
            out.push(len);
            remaining -= len;
        }
        if remaining > MAX_FINAL_CONTENT {
            return Err(Error::ChicletTooLong);
        }
        out.push(remaining);
        Ok(Self { lengths: out })
    }

    /// Build a layout from non-final chiclet lengths and a final chiclet length.
    /// Callers are responsible for checking lengths.
    pub(crate) fn from_parts(mut lengths: Vec<u64>, final_len: u64) -> Self {
        debug_assert!(lengths.iter().all(|&l| l > 0 && l <= MAX_CONTENT));
        debug_assert!(final_len <= MAX_FINAL_CONTENT);
        lengths.push(final_len);
        Self { lengths }
    }

    /// The content lengths of all chiclets, with the final chiclet last.
    #[must_use]
    pub fn lengths(&self) -> &[u64] {
        &self.lengths
    }

    /// The number of chiclets.
    #[must_use]
    #[allow(clippy::len_without_is_empty, reason = "a layout is never empty")]
    pub fn len(&self) -> usize {
        self.lengths.len()
    }

    /// The total length of content.
    #[must_use]
    pub fn content_len(&self) -> u64 {
        self.lengths.iter().sum()
    }

    /// The total length of the encoded form of the content.
    ///
    /// # Panics
    /// Never; per-chiclet overhead always fits in a `u64`.
    #[must_use]
    pub fn encoded_len(&self) -> u64 {
        self.iter()
            .map(|c| {
                if c.is_final {
                    1 + c.len
                } else {
                    let overhead = varint::encoded_len(c.len) + HASH_LEN;
                    u64::try_from(overhead).expect("small value") + c.len
                }
            })
            .sum()
    }

    /// Iterate over the chiclets in this layout.
    pub fn iter(&self) -> impl Iterator<Item = Chiclet> + '_ {
        let last = self.lengths.len() - 1;
        self.lengths
            .iter()
            .enumerate()
            .scan(0, move |offset, (i, &len)| {
                let c = Chiclet {
                    offset: *offset,
                    len,
                    is_final: i == last,
                };
                *offset += len;
                Some(c)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::repeat;

    #[test]
    fn empty() {
        let l = Layout::new(0, repeat(10)).unwrap();
        assert_eq!(l.lengths(), &[0]);
        assert_eq!(l.encoded_len(), 1);
        let l = Layout::new(0, []).unwrap();
        assert_eq!(l.lengths(), &[0]);
    }

    #[test]
    fn fixed() {
        let l = Layout::new(1, repeat(10)).unwrap();
        assert_eq!(l.lengths(), &[1]);
        let l = Layout::new(10, repeat(10)).unwrap();
        assert_eq!(l.lengths(), &[10]);
        let l = Layout::new(11, repeat(10)).unwrap();
        assert_eq!(l.lengths(), &[10, 1]);
        let l = Layout::new(30, repeat(10)).unwrap();
        assert_eq!(l.lengths(), &[10, 10, 10]);
        assert_eq!(l.content_len(), 30);
        assert_eq!(l.encoded_len(), 2 * (1 + 10 + 32) + 1 + 10);
    }

    #[test]
    fn explicit() {
        let l = Layout::new(10, vec![3, 4]).unwrap();
        assert_eq!(l.lengths(), &[3, 4, 3]);
        let l = Layout::new(10, vec![3, 7, 1]).unwrap();
        assert_eq!(l.lengths(), &[3, 7]);
        let l = Layout::new(10, vec![3, 100]).unwrap();
        assert_eq!(l.lengths(), &[3, 7]);
        let offsets: Vec<_> = l.iter().map(|c| (c.offset, c.is_final)).collect();
        assert_eq!(offsets, [(0, false), (3, true)]);
    }

    #[test]
    fn zero_length() {
        assert!(matches!(Layout::new(10, repeat(0)), Err(Error::ZeroLength)));
        assert!(matches!(
            Layout::new(10, vec![3, 0]),
            Err(Error::ZeroLength)
        ));
    }

    #[test]
    fn too_long() {
        assert!(matches!(
            Layout::new(MAX_FINAL_CONTENT + 1, []),
            Err(Error::ChicletTooLong)
        ));
        assert!(matches!(
            Layout::new(u64::MAX, [MAX_CONTENT + 1]),
            Err(Error::ChicletTooLong)
        ));
        let l = Layout::new(MAX_CONTENT + 1, [MAX_CONTENT]).unwrap();
        assert_eq!(l.lengths(), &[MAX_CONTENT, 1]);
        // The largest non-final chiclet is exactly 2^61 - 1 bytes.
        assert_eq!(l.encoded_len() - 2, (1 << 61) - 1);
    }
}
