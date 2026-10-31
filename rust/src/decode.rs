use sha2::{Digest as _, Sha256};

use crate::{Digest, Error, HASH_LEN, MAX_CONTENT, MAX_FINAL_CONTENT, Result, varint};

/// The largest chiclet length value that every implementation is required to support.
const DEFAULT_MAX_LENGTH: u64 = (1 << 14) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Reading a length field, with the bytes read so far.
    Header { bytes: [u8; 8], len: usize },
    /// Reading the content and hash of a non-final chiclet.
    Body { len: usize },
    /// Reading the content of the final chiclet, to the end of input.
    Final,
    /// Validation is complete.
    Done,
    /// An error occurred.
    Failed,
}

impl State {
    /// The state at the start of a chiclet.
    const START: Self = Self::Header {
        bytes: [0; 8],
        len: 0,
    };
}

/// Progressively decodes and validates CHIC content, without performing any IO.
///
/// Content is only released once the chiclet containing it has been validated.
#[derive(Debug)]
pub struct Decoder {
    expected: Digest,
    max_length: u64,
    state: State,
    hasher: Sha256,
    buf: Vec<u8>,
    index: u64,
}

impl Decoder {
    /// Create a decoder that validates against the hash of the first chiclet.
    /// This decoder rejects chiclets with a length value greater than 16383,
    /// the largest that every implementation is required to support
    /// (a total chiclet size of 16417 bytes); see [`Decoder::with_max_length`].
    #[must_use]
    pub fn new(expected: Digest) -> Self {
        Self {
            expected,
            max_length: DEFAULT_MAX_LENGTH,
            state: State::START,
            hasher: Sha256::new(),
            buf: Vec::new(),
            index: 0,
        }
    }

    /// Set the maximum length value of chiclets; that is, the number of content bytes.
    ///
    /// This does not include the length field or hash,
    /// so the largest chiclet accepted is up to 40 bytes longer than `max`.
    /// The limit also applies to the content of the final chiclet,
    /// even though its length field is zero.
    /// The limit cannot be raised beyond what can be held in memory.
    #[must_use]
    pub fn with_max_length(mut self, max: u64) -> Self {
        let memory_limit = u64::try_from(usize::MAX - HASH_LEN).unwrap_or(u64::MAX);
        self.max_length = max.min(memory_limit);
        self
    }

    fn fail<T>(&mut self, e: Error) -> Result<T> {
        self.state = State::Failed;
        self.buf = Vec::new();
        Err(e)
    }

    fn check(&mut self) -> Result<()> {
        let h = std::mem::take(&mut self.hasher).finalize();
        if h[..] == self.expected[..] {
            Ok(())
        } else {
            let chiclet = self.index;
            self.fail(Error::HashMismatch { chiclet })
        }
    }

    /// Process `input`, appending any validated content to `out`.
    /// All of `input` is consumed.
    ///
    /// # Errors
    /// If validation fails, or the decoder previously failed.
    pub fn feed(&mut self, mut input: &[u8], out: &mut Vec<u8>) -> Result<()> {
        while !input.is_empty() {
            match self.state {
                State::Header { mut bytes, len } => {
                    bytes[len] = input[0];
                    self.hasher.update(&input[..1]);
                    input = &input[1..];
                    let header_len = len + 1;
                    if header_len < varint::len_from_first(bytes[0]) {
                        self.state = State::Header {
                            bytes,
                            len: header_len,
                        };
                        continue;
                    }
                    let (len, _) = varint::decode(&bytes[..header_len]).expect("complete varint");
                    if len == 0 {
                        self.state = State::Final;
                    } else if len > self.max_length || len > MAX_CONTENT {
                        return self.fail(Error::ChicletTooLong);
                    } else {
                        // The limit ensures that this conversion succeeds.
                        let len = usize::try_from(len).expect("length fits in memory");
                        self.buf.reserve_exact(len + HASH_LEN);
                        self.state = State::Body { len };
                    }
                }
                State::Body { len } => {
                    let n = (len + HASH_LEN - self.buf.len()).min(input.len());
                    self.hasher.update(&input[..n]);
                    self.buf.extend_from_slice(&input[..n]);
                    input = &input[n..];
                    if self.buf.len() == len + HASH_LEN {
                        self.check()?;
                        out.extend_from_slice(&self.buf[..len]);
                        self.expected.copy_from_slice(&self.buf[len..]);
                        self.buf.clear();
                        self.index += 1;
                        self.state = State::START;
                    }
                }
                State::Final => {
                    let limit = self.max_length.min(MAX_FINAL_CONTENT);
                    let len = self.buf.len().saturating_add(input.len());
                    if u64::try_from(len).map_or(true, |len| len > limit) {
                        return self.fail(Error::ChicletTooLong);
                    }
                    self.hasher.update(input);
                    self.buf.extend_from_slice(input);
                    input = &[];
                }
                State::Done | State::Failed => return self.fail(Error::Failed),
            }
        }
        Ok(())
    }

    /// Signal the end of input, appending the content of the final chiclet to `out`.
    ///
    /// # Errors
    /// If validation fails, input ended before the final chiclet,
    /// or the decoder previously failed.
    pub fn finish(&mut self, out: &mut Vec<u8>) -> Result<()> {
        match self.state {
            State::Final => {
                self.check()?;
                out.append(&mut self.buf);
                self.state = State::Done;
                Ok(())
            }
            State::Header { .. } | State::Body { .. } => self.fail(Error::Truncated),
            State::Done | State::Failed => self.fail(Error::Failed),
        }
    }

    /// The number of chiclets that have been validated.
    #[must_use]
    pub fn chiclets(&self) -> u64 {
        self.index + u64::from(self.state == State::Done)
    }
}
