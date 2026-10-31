use sha2::{Digest as _, Sha256};

use crate::{Digest, Error, Layout, Result, varint};

/// Builds the hash chain for a sequence of chiclets, processing content forwards.
///
/// The hash of each chiclet covers its length, content, and the hash of the next chiclet.
/// Content is hashed in order as it is provided; the partially-complete hash state
/// for each chiclet is retained so that the hash of the next chiclet can be added
/// once it is known.  [`ChainBuilder::finish`] then completes the hashes in reverse,
/// which only processes a small amount of data for each chiclet.
#[derive(Debug, Default, Clone)]
pub struct ChainBuilder {
    done: Vec<Sha256>,
    current: Option<Sha256>,
    final_begun: bool,
}

impl ChainBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn begin(&mut self, length_field: u64) {
        assert!(self.current.is_none(), "chiclet already open");
        assert!(!self.final_begun, "final chiclet already started");
        let (b, n) = varint::encode(length_field);
        let mut h = Sha256::new();
        h.update(&b[..n]);
        self.current = Some(h);
    }

    /// Start a non-final chiclet with `len` bytes of content.
    ///
    /// # Panics
    /// If `len` is zero, if a chiclet is already open, or after the final chiclet is started.
    pub fn begin_chiclet(&mut self, len: u64) {
        assert_ne!(len, 0, "non-final chiclets cannot be empty");
        self.begin(len);
    }

    /// Start the final chiclet.
    ///
    /// # Panics
    /// If a chiclet is already open, or after the final chiclet is started.
    pub fn begin_final(&mut self) {
        self.begin(0);
        self.final_begun = true;
    }

    /// Add content to the open chiclet.
    ///
    /// # Panics
    /// If no chiclet is open.
    pub fn update(&mut self, data: &[u8]) {
        self.current.as_mut().expect("no chiclet open").update(data);
    }

    /// End the open chiclet.
    ///
    /// # Panics
    /// If no chiclet is open.
    pub fn end(&mut self) {
        let h = self.current.take().expect("no chiclet open");
        self.done.push(h);
    }

    /// Discard the most recently completed non-final chiclet.
    pub(crate) fn pop(&mut self) {
        assert!(self.current.is_none() && !self.final_begun);
        self.done.pop().expect("no chiclet to discard");
    }

    /// The number of completed chiclets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.done.len()
    }

    /// Whether no chiclets have been completed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    /// Complete the hash chain.
    ///
    /// # Panics
    /// If a chiclet is open, or the final chiclet has not been completed.
    #[must_use]
    pub fn finish(self) -> Chain {
        assert!(self.current.is_none(), "chiclet still open");
        assert!(self.final_begun, "final chiclet missing");
        let mut hashes = Vec::with_capacity(self.done.len());
        let mut next: Option<Digest> = None;
        for mut h in self.done.into_iter().rev() {
            if let Some(n) = next {
                h.update(n);
            }
            let d: Digest = h.finalize().into();
            hashes.push(d);
            next = Some(d);
        }
        hashes.reverse();
        Chain { hashes }
    }
}

/// The completed hashes for all chiclets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    hashes: Vec<Digest>,
}

impl Chain {
    /// The hash of the first chiclet, which is carried outside of the content.
    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.hashes[0]
    }

    /// The hash of chiclet `i`.
    #[must_use]
    pub fn hash(&self, i: usize) -> Option<&Digest> {
        self.hashes.get(i)
    }

    /// The number of chiclets.
    #[must_use]
    #[allow(clippy::len_without_is_empty, reason = "a chain is never empty")]
    pub fn len(&self) -> usize {
        self.hashes.len()
    }
}

/// Hashes content according to a [`Layout`].
///
/// Content can be provided in pieces of any size;
/// chiclet boundaries are applied according to the layout.
#[derive(Debug)]
pub struct LayoutHasher<'a> {
    layout: &'a Layout,
    builder: ChainBuilder,
    next: usize,
    remaining: u64,
}

impl<'a> LayoutHasher<'a> {
    #[must_use]
    pub fn new(layout: &'a Layout) -> Self {
        Self {
            layout,
            builder: ChainBuilder::new(),
            next: 0,
            remaining: 0,
        }
    }

    fn start_next(&mut self) {
        if self.next > 0 {
            self.builder.end();
        }
        let lengths = self.layout.lengths();
        assert!(
            self.next < lengths.len(),
            "more content than the layout allows"
        );
        let len = lengths[self.next];
        if self.next + 1 == lengths.len() {
            self.builder.begin_final();
        } else {
            self.builder.begin_chiclet(len);
        }
        self.remaining = len;
        self.next += 1;
    }

    /// Add content.
    ///
    /// # Panics
    /// If more content is provided than the layout describes.
    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            if self.remaining == 0 {
                self.start_next();
            }
            let n = usize::try_from(self.remaining).map_or(data.len(), |r| r.min(data.len()));
            self.builder.update(&data[..n]);
            self.remaining -= u64::try_from(n).expect("bounded by a u64");
            data = &data[n..];
        }
    }

    /// Complete the hash chain.
    ///
    /// # Errors
    /// [`Error::Truncated`] if less content was provided than the layout describes.
    pub fn finish(mut self) -> Result<Chain> {
        if self.remaining > 0 {
            return Err(Error::Truncated);
        }
        // Start any remaining chiclets; only an empty final chiclet can be pending.
        while self.next < self.layout.len() {
            self.start_next();
            if self.remaining > 0 {
                return Err(Error::Truncated);
            }
        }
        self.builder.end();
        Ok(self.builder.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256(parts: &[&[u8]]) -> Digest {
        let mut h = Sha256::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().into()
    }

    #[test]
    fn two_chiclets() {
        let layout = Layout::new(11, [5]).unwrap();
        let mut h = LayoutHasher::new(&layout);
        for b in b"hello world" {
            h.update(std::slice::from_ref(b));
        }
        let chain = h.finish().unwrap();
        let h1 = sha256(&[b"\x00 world"]);
        let h0 = sha256(&[b"\x05hello", &h1]);
        assert_eq!(chain.hash(1), Some(&h1));
        assert_eq!(chain.digest(), &h0);
    }

    #[test]
    fn short() {
        let layout = Layout::new(11, [5]).unwrap();
        let mut h = LayoutHasher::new(&layout);
        h.update(b"hello");
        assert!(matches!(h.finish(), Err(Error::Truncated)));
    }

    #[test]
    #[should_panic(expected = "more content")]
    fn long() {
        let layout = Layout::new(3, []).unwrap();
        let mut h = LayoutHasher::new(&layout);
        h.update(b"abcd");
    }
}
