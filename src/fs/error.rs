use std::path::PathBuf;

use crate::{CompressionTypes, EncodingError};

pub type ObjectFileResult<T> = Result<T, ObjectFileError>;

#[derive(Debug, thiserror::Error)]
pub enum ObjectFileError {
    #[error(transparent)]
    Encoding(#[from] EncodingError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// The metadata and tag sections no longer fit in the space reserved for them, and the caller
    /// asked for an in-place update.
    #[error(
        "the metadata and tag sections need {required} bytes but only {available} are reserved in the file"
    )]
    ReservedSpaceExceeded { required: usize, available: usize },
    /// `tags_start` is a `u16` and `content_start` a `u32`, so the sections in front of the content
    /// are bounded by the format itself.
    #[error("the metadata section must start within the first {limit} bytes, {required} required")]
    SectionOffsetTooLarge { required: usize, limit: usize },
    #[error(
        "requested range {offset}..{end} lies outside the object content of {content_length} bytes"
    )]
    RangeOutOfBounds {
        offset: u64,
        end: u64,
        content_length: u64,
    },
    #[error("the object at {0} was opened read-only")]
    ReadOnly(PathBuf),
    /// The object is compressed with a codec this build was not compiled with.
    #[error("compression type {0:?} is not supported by this build of tux-io-encoding")]
    UnsupportedCompression(CompressionTypes),
    /// Ranged reads seek into the stored bytes, which is meaningless once a codec is in the way.
    #[error("ranged reads are not supported on compressed objects")]
    RangedReadOnCompressed,
}

impl ObjectFileError {
    /// True when the error was an [std::io::ErrorKind::NotFound].
    pub fn is_not_found(&self) -> bool {
        match self {
            ObjectFileError::IO(err) => err.kind() == std::io::ErrorKind::NotFound,
            ObjectFileError::Encoding(EncodingError::IOError(err)) => {
                err.kind() == std::io::ErrorKind::NotFound
            }
            _ => false,
        }
    }
}
