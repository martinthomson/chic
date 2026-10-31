//! Chained Hash Integrity Coding (CHIC).
//!
//! An implementation of the `chi` content-coding from draft-thomson-httpbis-chic.
//! Content is split into chiclets, each of which carries the hash of the chiclet
//! that follows it, so that content can be validated progressively.
//!
//! The crate is split into an IO-free core ([`Layout`], [`ChainBuilder`],
//! [`LayoutHasher`], [`Emitter`], [`Decoder`]) and synchronous adapters in [`sync`].
//! Asynchronous adapters can be layered over the same core.

mod chain;
mod decode;
mod emit;
mod error;
mod layout;
pub mod sync;
pub mod varint;

pub use chain::{Chain, ChainBuilder, LayoutHasher};
pub use decode::Decoder;
pub use emit::{Emitter, Header, Piece};
pub use error::{Error, Result};
pub use layout::{Chiclet, Layout};

/// The length of a hash, in bytes.
pub const HASH_LEN: usize = 32;

/// A SHA-256 hash value.
pub type Digest = [u8; HASH_LEN];

/// The largest content length of a non-final chiclet.
/// A chiclet, including its length (8 bytes) and hash (32 bytes),
/// cannot be 2^61 bytes or longer.
pub const MAX_CONTENT: u64 = (1 << 61) - 1 - 8 - 32;

/// The largest content length of a final chiclet.
pub const MAX_FINAL_CONTENT: u64 = (1 << 61) - 2;
