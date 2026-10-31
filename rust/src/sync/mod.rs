//! Synchronous adapters for encoding and decoding.
//!
//! Three construction modes are provided:
//!
//! - [`memory`]: content held in a single slice.
//! - [`chunked`]: content provided in a series of writes, each of which ends a chiclet.
//! - [`file`](mod@file): content read from a seekable source.
//!
//! All modes hash content in a single forward pass,
//! then write output in a second forward pass.

pub mod chunked;
pub mod file;
pub mod memory;
mod reader;

pub use chunked::{ChunkedEncoder, FileSpool, Spool, VecSpool};
pub use file::{digest_file, encode_file};
pub use memory::{decode_slice, digest_slice, encode_slice, encode_slice_to};
pub use reader::DecodeReader;

use std::io::Write;

use crate::{Chain, Emitter, Layout, Piece, Result};

/// Write the encoded form of content, using `content` to copy each range of content.
fn emit<W: Write + ?Sized>(
    layout: &Layout,
    chain: &Chain,
    w: &mut W,
    mut content: impl FnMut(&mut W, u64, u64) -> Result<()>,
) -> Result<()> {
    for piece in Emitter::new(layout, chain) {
        match piece {
            Piece::Header(h) => w.write_all(h.as_bytes())?,
            Piece::Content { offset, len } => content(w, offset, len)?,
            Piece::Hash(h) => w.write_all(h)?,
        }
    }
    Ok(())
}
