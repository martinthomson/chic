//! Encoding and decoding content held in memory.

use std::io::Write;

use crate::{Chain, Decoder, Digest, Layout, LayoutHasher, Result};

fn hash(input: &[u8], lengths: impl IntoIterator<Item = u64>) -> Result<(Layout, Chain)> {
    let layout = Layout::new(u64::try_from(input.len())?, lengths)?;
    let mut hasher = LayoutHasher::new(&layout);
    hasher.update(input);
    let chain = hasher.finish()?;
    Ok((layout, chain))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "ranges are within the input slice"
)]
fn range(input: &[u8], offset: u64, len: u64) -> &[u8] {
    &input[offset as usize..(offset + len) as usize]
}

/// Encode `input`, with chiclet lengths drawn from `lengths` (see [`Layout::new`]).
/// Returns the encoded content and the hash of the first chiclet.
///
/// # Errors
/// If the lengths are invalid.
pub fn encode_slice(
    input: &[u8],
    lengths: impl IntoIterator<Item = u64>,
) -> Result<(Vec<u8>, Digest)> {
    let (layout, chain) = hash(input, lengths)?;
    let mut out = Vec::with_capacity(usize::try_from(layout.encoded_len()).unwrap_or(0));
    super::emit(&layout, &chain, &mut out, |w, offset, len| {
        w.extend_from_slice(range(input, offset, len));
        Ok(())
    })?;
    Ok((out, *chain.digest()))
}

/// Encode `input` to `w`, with chiclet lengths drawn from `lengths` (see [`Layout::new`]).
/// Returns the hash of the first chiclet.
///
/// # Errors
/// If the lengths are invalid or writing fails.
pub fn encode_slice_to<W: Write + ?Sized>(
    input: &[u8],
    lengths: impl IntoIterator<Item = u64>,
    w: &mut W,
) -> Result<Digest> {
    let (layout, chain) = hash(input, lengths)?;
    super::emit(&layout, &chain, w, |w, offset, len| {
        Ok(w.write_all(range(input, offset, len))?)
    })?;
    Ok(*chain.digest())
}

/// Compute the hash of the first chiclet for `input`, without encoding it.
///
/// # Errors
/// If the lengths are invalid.
pub fn digest_slice(input: &[u8], lengths: impl IntoIterator<Item = u64>) -> Result<Digest> {
    Ok(*hash(input, lengths)?.1.digest())
}

/// Decode and validate `input` against `expected`,
/// rejecting chiclets with more than `max_length` bytes of content
/// (see [`Decoder::with_max_length`]).
///
/// # Errors
/// If validation fails.
pub fn decode_slice(input: &[u8], expected: Digest, max_length: u64) -> Result<Vec<u8>> {
    let mut decoder = Decoder::new(expected).with_max_length(max_length);
    let mut out = Vec::with_capacity(input.len());
    decoder.feed(input, &mut out)?;
    decoder.finish(&mut out)?;
    Ok(out)
}
