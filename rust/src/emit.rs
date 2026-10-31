use crate::{Chain, Digest, Layout, varint};

/// The encoded length field of a chiclet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    bytes: [u8; 8],
    len: u8,
}

impl Header {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// A piece of encoded output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece<'a> {
    /// The length field of a chiclet.
    Header(Header),
    /// A range of the unencoded content; this is never empty.
    Content { offset: u64, len: u64 },
    /// The hash at the end of a non-final chiclet.
    Hash(&'a Digest),
}

/// The next part of a chiclet to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Header,
    Content,
    Hash,
}

/// Describes the encoded form of content, without performing any IO.
///
/// This produces a sequence of [`Piece`]s in output order.
/// Content pieces refer to the unencoded content, in order and without gaps,
/// so that a caller can copy content from a sequential source.
#[derive(Debug)]
pub struct Emitter<'a> {
    layout: &'a Layout,
    chain: &'a Chain,
    index: usize,
    offset: u64,
    stage: Stage,
}

impl<'a> Emitter<'a> {
    /// # Panics
    /// If the chain and layout do not have the same number of chiclets.
    #[must_use]
    pub fn new(layout: &'a Layout, chain: &'a Chain) -> Self {
        assert_eq!(layout.len(), chain.len(), "layout and chain mismatch");
        Self {
            layout,
            chain,
            index: 0,
            offset: 0,
            stage: Stage::Header,
        }
    }
}

impl<'a> Iterator for Emitter<'a> {
    type Item = Piece<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let len = *self.layout.lengths().get(self.index)?;
            let is_final = self.index + 1 == self.layout.len();
            match self.stage {
                Stage::Header => {
                    self.stage = Stage::Content;
                    let (bytes, n) = varint::encode(if is_final { 0 } else { len });
                    return Some(Piece::Header(Header {
                        bytes,
                        len: u8::try_from(n).expect("varints are at most 8 bytes"),
                    }));
                }
                Stage::Content => {
                    self.stage = Stage::Hash;
                    let offset = self.offset;
                    self.offset += len;
                    if len > 0 {
                        return Some(Piece::Content { offset, len });
                    }
                }
                Stage::Hash => {
                    self.stage = Stage::Header;
                    self.index += 1;
                    if !is_final {
                        return self.chain.hash(self.index).map(Piece::Hash);
                    }
                }
            }
        }
    }
}
