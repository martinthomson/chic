//! Encoding content from a seekable source, such as a file.

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::{Chain, Digest, Error, Layout, LayoutHasher, Result};

const BUFFER_SIZE: usize = 1 << 16;

/// Pass 1: determine the length of content from the current position to the end,
/// then read it all, building the hash chain.
/// Returns the starting position, the layout, and the chain.
fn hash<R: Read + Seek + ?Sized>(
    r: &mut R,
    lengths: impl IntoIterator<Item = u64>,
) -> Result<(u64, Layout, Chain)> {
    let start = r.stream_position()?;
    let end = r.seek(SeekFrom::End(0))?;
    r.seek(SeekFrom::Start(start))?;
    let layout = Layout::new(end.saturating_sub(start), lengths)?;

    let mut hasher = LayoutHasher::new(&layout);
    let mut buf = vec![0; BUFFER_SIZE];
    let mut remaining = layout.content_len();
    while remaining > 0 {
        let want = usize::try_from(remaining).map_or(buf.len(), |r| r.min(buf.len()));
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            return Err(Error::Truncated);
        }
        hasher.update(&buf[..n]);
        remaining -= u64::try_from(n).expect("bounded by a u64");
    }
    let chain = hasher.finish()?;
    Ok((start, layout, chain))
}

/// Encode the content of `r`, from its current position to the end, to `w`.
/// Chiclet lengths are drawn from `lengths` (see [`Layout::new`]).
/// Returns the hash of the first chiclet.
///
/// Content is read twice, both times in order: once to hash it, then again to write it.
/// The content must not change between these two passes; the output is invalid if it does.
///
/// # Errors
/// If the lengths are invalid, or if reading or writing fails.
pub fn encode_file<R, W>(
    r: &mut R,
    lengths: impl IntoIterator<Item = u64>,
    w: &mut W,
) -> Result<Digest>
where
    R: Read + Seek + ?Sized,
    W: Write + ?Sized,
{
    let (start, layout, chain) = hash(r, lengths)?;
    r.seek(SeekFrom::Start(start))?;
    super::emit(&layout, &chain, w, |w, _offset, len| {
        // Content pieces are contiguous, so this only needs to read in order.
        let copied = io::copy(&mut Read::take(&mut *r, len), w)?;
        if copied == len {
            Ok(())
        } else {
            Err(Error::Truncated)
        }
    })?;
    Ok(*chain.digest())
}

/// Compute the hash of the first chiclet for the content of `r`,
/// from its current position to the end, without encoding it.
///
/// # Errors
/// If the lengths are invalid, or if reading fails.
pub fn digest_file<R: Read + Seek + ?Sized>(
    r: &mut R,
    lengths: impl IntoIterator<Item = u64>,
) -> Result<Digest> {
    Ok(*hash(r, lengths)?.2.digest())
}
