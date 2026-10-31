//! Encoding content that is provided in chunks, where each chunk ends a chiclet.

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::{ChainBuilder, Digest, Error, Layout, MAX_CONTENT, MAX_FINAL_CONTENT, Result};

/// Storage for content while it is being encoded.
pub trait Spool {
    /// Append data to the end of stored content.
    ///
    /// # Errors
    /// If storage fails.
    fn append(&mut self, data: &[u8]) -> Result<()>;

    /// Copy `len` bytes of stored content, starting at `offset`, to `w`.
    ///
    /// # Errors
    /// [`Error::Truncated`] if the range is not stored, or an error if reading or writing fails.
    fn copy_range_to(&mut self, offset: u64, len: u64, w: &mut dyn Write) -> Result<()>;
}

/// Content held in memory.
#[derive(Debug, Default, Clone)]
pub struct VecSpool(pub Vec<u8>);

impl Spool for VecSpool {
    fn append(&mut self, data: &[u8]) -> Result<()> {
        self.0.extend_from_slice(data);
        Ok(())
    }

    fn copy_range_to(&mut self, offset: u64, len: u64, w: &mut dyn Write) -> Result<()> {
        let range = self
            .0
            .get(usize::try_from(offset)?..usize::try_from(offset + len)?)
            .ok_or(Error::Truncated)?;
        Ok(w.write_all(range)?)
    }
}

/// Content held in a file, or anything that can be read, written, and seeked.
/// Content is stored starting from the position of the file when the spool is created.
#[derive(Debug)]
pub struct FileSpool<F> {
    f: F,
    base: u64,
    len: u64,
}

impl<F: Read + Write + Seek> FileSpool<F> {
    /// # Errors
    /// If the current position cannot be determined.
    pub fn new(mut f: F) -> io::Result<Self> {
        let base = f.stream_position()?;
        Ok(Self { f, base, len: 0 })
    }

    /// Recover the underlying file.
    pub fn into_inner(self) -> F {
        self.f
    }
}

impl<F: Read + Write + Seek> Spool for FileSpool<F> {
    fn append(&mut self, data: &[u8]) -> Result<()> {
        let n = u64::try_from(data.len())?;
        self.f.seek(SeekFrom::Start(self.base + self.len))?;
        self.f.write_all(data)?;
        self.len += n;
        Ok(())
    }

    fn copy_range_to(&mut self, offset: u64, len: u64, w: &mut dyn Write) -> Result<()> {
        if offset + len > self.len {
            return Err(Error::Truncated);
        }
        self.f.seek(SeekFrom::Start(self.base + offset))?;
        let copied = io::copy(&mut Read::take(&mut self.f, len), w)?;
        if copied == len {
            Ok(())
        } else {
            Err(Error::Truncated)
        }
    }
}

/// Feeds written bytes to a [`ChainBuilder`].
struct HashWriter<'a>(&'a mut ChainBuilder);

impl Write for HashWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Encodes content that is provided in chunks.
///
/// Each call to [`ChunkedEncoder::write_chunk`] ends a chiclet,
/// so that chiclet boundaries follow the structure of the content.
/// Content is hashed as it is written and retained in a [`Spool`] until output is produced.
///
/// This does not implement [`std::io::Write`], because that trait permits
/// partial writes and buffering, which would not preserve chunk boundaries.
#[derive(Debug, Default)]
pub struct ChunkedEncoder<S = VecSpool> {
    spool: S,
    builder: ChainBuilder,
    lengths: Vec<u64>,
    max_length: Option<u64>,
}

impl<S: Spool> ChunkedEncoder<S> {
    pub fn new(spool: S) -> Self {
        Self {
            spool,
            builder: ChainBuilder::new(),
            lengths: Vec::new(),
            max_length: None,
        }
    }

    /// Split chunks that are longer than `max` bytes into multiple chiclets,
    /// so that no chiclet has a length value greater than `max`.
    /// This does not include the length field or hash.
    ///
    /// # Panics
    /// If `max` is zero.
    #[must_use]
    pub fn with_max_length(mut self, max: u64) -> Self {
        assert_ne!(max, 0, "chiclets cannot be empty");
        self.max_length = Some(max);
        self
    }

    /// Add a chunk of content, which ends a chiclet.  Empty chunks are ignored.
    ///
    /// # Errors
    /// If the chunk is too long or storing it fails.
    pub fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let max = self.max_length.unwrap_or(MAX_CONTENT).min(MAX_CONTENT);
        let size = usize::try_from(max).unwrap_or(usize::MAX);
        for piece in data.chunks(size) {
            let len = u64::try_from(piece.len())?;
            self.spool.append(piece)?;
            self.builder.begin_chiclet(len);
            self.builder.update(piece);
            self.builder.end();
            self.lengths.push(len);
        }
        Ok(())
    }

    /// Finish encoding, writing the encoded content to `w`.
    /// The last chunk becomes the final chiclet.
    /// Returns the hash of the first chiclet.
    ///
    /// The last chunk is read back from the spool to hash it again as the final chiclet.
    ///
    /// # Errors
    /// If reading from the spool or writing fails.
    pub fn finish<W: Write + ?Sized>(mut self, w: &mut W) -> Result<Digest> {
        let final_len = if let Some(len) = self.lengths.pop() {
            if len > MAX_FINAL_CONTENT {
                return Err(Error::ChicletTooLong);
            }
            let offset = self.lengths.iter().sum();
            self.builder.pop();
            self.builder.begin_final();
            self.spool
                .copy_range_to(offset, len, &mut HashWriter(&mut self.builder))?;
            len
        } else {
            self.builder.begin_final();
            0
        };
        self.builder.end();
        self.emit(final_len, w)
    }

    /// Finish encoding, writing the encoded content to `w`.
    /// All chunks become non-final chiclets, followed by an empty final chiclet.
    /// Returns the hash of the first chiclet.
    ///
    /// # Errors
    /// If reading from the spool or writing fails.
    pub fn finish_with_empty_final<W: Write + ?Sized>(mut self, w: &mut W) -> Result<Digest> {
        self.builder.begin_final();
        self.builder.end();
        self.emit(0, w)
    }

    fn emit<W: Write + ?Sized>(self, final_len: u64, w: &mut W) -> Result<Digest> {
        let Self {
            mut spool,
            builder,
            lengths,
            ..
        } = self;
        let layout = Layout::from_parts(lengths, final_len);
        let chain = builder.finish();
        let mut w = w;
        super::emit(&layout, &chain, &mut w, |w, offset, len| {
            spool.copy_range_to(offset, len, w)
        })?;
        Ok(*chain.digest())
    }
}
