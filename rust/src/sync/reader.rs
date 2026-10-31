use std::io::{self, Read};

use crate::{Decoder, Digest};

const BUFFER_SIZE: usize = 1 << 14;

/// Reads and validates CHIC content from an underlying reader.
///
/// Only validated content is returned.
/// The content of each chiclet becomes available once the entire chiclet has been read;
/// the final chiclet is only available once the underlying reader reaches its end.
#[derive(Debug)]
pub struct DecodeReader<R> {
    inner: R,
    decoder: Decoder,
    out: Vec<u8>,
    pos: usize,
    eof: bool,
    buf: Box<[u8]>,
}

impl<R: Read> DecodeReader<R> {
    /// Read from `inner`, validating against the hash of the first chiclet.
    pub fn new(inner: R, expected: Digest) -> Self {
        Self::with_decoder(inner, Decoder::new(expected))
    }

    /// Read from `inner`, using a configured decoder.
    pub fn with_decoder(inner: R, decoder: Decoder) -> Self {
        Self {
            inner,
            decoder,
            out: Vec::new(),
            pos: 0,
            eof: false,
            buf: vec![0; BUFFER_SIZE].into_boxed_slice(),
        }
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if dst.is_empty() {
            return Ok(0);
        }
        loop {
            if self.pos < self.out.len() {
                let n = dst.len().min(self.out.len() - self.pos);
                dst[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if self.eof {
                return Ok(0);
            }
            self.out.clear();
            self.pos = 0;
            let n = match self.inner.read(&mut self.buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if n == 0 {
                self.eof = true;
                self.decoder.finish(&mut self.out)?;
            } else {
                self.decoder.feed(&self.buf[..n], &mut self.out)?;
            }
        }
    }
}
