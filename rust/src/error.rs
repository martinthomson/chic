use std::io;

/// Errors from constructing or validating CHIC content.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A zero length was provided for a non-final chiclet.
    #[error("zero length for a non-final chiclet")]
    ZeroLength,
    /// A chiclet exceeded the applicable length limit.
    #[error("chiclet too long")]
    ChicletTooLong,
    /// Content ended before a chiclet was complete.
    #[error("content truncated")]
    Truncated,
    /// The hash of a chiclet did not match the expected value.
    #[error("hash mismatch in chiclet {chiclet}")]
    HashMismatch {
        /// The zero-based index of the chiclet that failed validation.
        chiclet: u64,
    },
    /// A length could not be converted between integer types.
    #[error("integer conversion failed: {0}")]
    Conversion(#[from] std::num::TryFromIntError),
    /// The decoder was used after it reported an error.
    #[error("decoder used after failure")]
    Failed,
    /// An IO error.
    #[error("IO error: {0}")]
    Io(#[source] io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

// Not `#[from]`: errors that were converted to `io::Error`,
// such as by `DecodeReader`, are recovered rather than wrapped.
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        type DynError = dyn std::error::Error + Send + Sync;
        if e.get_ref().is_some_and(DynError::is::<Self>) {
            if let Some(Ok(inner)) = e.into_inner().map(DynError::downcast::<Self>) {
                return *inner;
            }
            unreachable!("checked above");
        }
        Self::Io(e)
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => e,
            Error::Truncated => io::Error::new(io::ErrorKind::UnexpectedEof, e),
            Error::ZeroLength => io::Error::new(io::ErrorKind::InvalidInput, e),
            e => io::Error::new(io::ErrorKind::InvalidData, e),
        }
    }
}
