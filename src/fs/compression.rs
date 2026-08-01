use std::io::Write;

use http::HeaderName;

use crate::{
    CompressionTypes, MetaKey,
    fs::{ObjectFileError, ObjectFileResult, ObjectWriter},
};

/// Metadata key holding the content length before compression.
///
/// [crate::ObjectHeader::content_length] is the number of bytes actually stored, so for a
/// compressed object this is the only record of how large the content really is.
pub const UNCOMPRESSED_LENGTH: HeaderName = HeaderName::from_static("x-tuxio-uncompressed-length");

/// The metadata key form of [UNCOMPRESSED_LENGTH].
pub fn uncompressed_length_key() -> MetaKey {
    MetaKey::from(UNCOMPRESSED_LENGTH)
}

/// True when this build can read and write the given compression type.
pub fn is_supported(compression: CompressionTypes) -> bool {
    match compression {
        CompressionTypes::None(_) => true,
        CompressionTypes::ZSTD(_) => cfg!(feature = "zstd"),
        CompressionTypes::Gzip(_) => cfg!(feature = "gzip"),
    }
}

/// Errors unless this build can handle `compression`.
pub fn ensure_supported(compression: CompressionTypes) -> ObjectFileResult<()> {
    if is_supported(compression) {
        Ok(())
    } else {
        Err(ObjectFileError::UnsupportedCompression(compression))
    }
}

/// Compresses content on its way into an [ObjectWriter].
///
/// Obtained from [ObjectWriter::content_encoder]. For an uncompressed object it is a pass-through,
/// so a caller can use the same code path either way. [ContentEncoder::finish] must be called to
/// flush the codec — dropping it loses the tail of the content.
pub enum ContentEncoder<'writer> {
    Stored {
        writer: &'writer mut ObjectWriter,
        uncompressed_length: u64,
    },
    #[cfg(feature = "zstd")]
    Zstd {
        encoder: Box<zstd::stream::write::Encoder<'static, &'writer mut ObjectWriter>>,
        uncompressed_length: u64,
    },
    #[cfg(feature = "gzip")]
    Gzip {
        encoder: Box<flate2::write::GzEncoder<&'writer mut ObjectWriter>>,
        uncompressed_length: u64,
    },
}

impl<'writer> ContentEncoder<'writer> {
    pub(crate) fn new(
        writer: &'writer mut ObjectWriter,
        compression: CompressionTypes,
    ) -> ObjectFileResult<Self> {
        ensure_supported(compression)?;
        match compression {
            CompressionTypes::None(_) => Ok(ContentEncoder::Stored {
                writer,
                uncompressed_length: 0,
            }),
            #[cfg(feature = "zstd")]
            CompressionTypes::ZSTD(level) => Ok(ContentEncoder::Zstd {
                encoder: Box::new(
                    zstd::stream::write::Encoder::new(writer, level.0)
                        .map_err(ObjectFileError::IO)?,
                ),
                uncompressed_length: 0,
            }),
            #[cfg(feature = "gzip")]
            CompressionTypes::Gzip(level) => Ok(ContentEncoder::Gzip {
                encoder: Box::new(flate2::write::GzEncoder::new(
                    writer,
                    flate2::Compression::new(level.0),
                )),
                uncompressed_length: 0,
            }),
            #[allow(unreachable_patterns)]
            other => Err(ObjectFileError::UnsupportedCompression(other)),
        }
    }

    /// Content bytes fed in so far, before compression.
    pub fn uncompressed_length(&self) -> u64 {
        match self {
            ContentEncoder::Stored {
                uncompressed_length,
                ..
            } => *uncompressed_length,
            #[cfg(feature = "zstd")]
            ContentEncoder::Zstd {
                uncompressed_length,
                ..
            } => *uncompressed_length,
            #[cfg(feature = "gzip")]
            ContentEncoder::Gzip {
                uncompressed_length,
                ..
            } => *uncompressed_length,
        }
    }

    fn record(&mut self, written: usize) {
        let written = written as u64;
        match self {
            ContentEncoder::Stored {
                uncompressed_length,
                ..
            } => *uncompressed_length += written,
            #[cfg(feature = "zstd")]
            ContentEncoder::Zstd {
                uncompressed_length,
                ..
            } => *uncompressed_length += written,
            #[cfg(feature = "gzip")]
            ContentEncoder::Gzip {
                uncompressed_length,
                ..
            } => *uncompressed_length += written,
        }
    }

    /// Flushes the codec and records the uncompressed length in the object's metadata.
    ///
    /// Returns the uncompressed content length.
    pub fn finish(self) -> ObjectFileResult<u64> {
        let uncompressed_length = self.uncompressed_length();
        let writer = match self {
            ContentEncoder::Stored { writer, .. } => writer,
            #[cfg(feature = "zstd")]
            ContentEncoder::Zstd { encoder, .. } => {
                encoder.finish().map_err(ObjectFileError::IO)?
            }
            #[cfg(feature = "gzip")]
            ContentEncoder::Gzip { encoder, .. } => {
                encoder.finish().map_err(ObjectFileError::IO)?
            }
        };
        writer.flush().map_err(ObjectFileError::IO)?;
        // Only worth recording when the stored length differs from the real one.
        if writer.content_length() != uncompressed_length {
            writer
                .metadata_mut()
                .insert(uncompressed_length_key(), uncompressed_length.into());
        }
        Ok(uncompressed_length)
    }
}

impl Write for ContentEncoder<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = match self {
            ContentEncoder::Stored { writer, .. } => writer.write(buf)?,
            #[cfg(feature = "zstd")]
            ContentEncoder::Zstd { encoder, .. } => encoder.write(buf)?,
            #[cfg(feature = "gzip")]
            ContentEncoder::Gzip { encoder, .. } => encoder.write(buf)?,
        };
        self.record(written);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            ContentEncoder::Stored { writer, .. } => writer.flush(),
            #[cfg(feature = "zstd")]
            ContentEncoder::Zstd { encoder, .. } => encoder.flush(),
            #[cfg(feature = "gzip")]
            ContentEncoder::Gzip { encoder, .. } => encoder.flush(),
        }
    }
}
