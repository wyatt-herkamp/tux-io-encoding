use std::{fs::File, io::Read};

/// A reader bounded to the content section of an object file.
///
/// Reads stop at the end of the content even though the underlying file handle could keep going,
/// so a caller cannot accidentally read padding or a neighbouring section.
#[derive(Debug)]
pub struct ContentReader<'object> {
    file: &'object mut File,
    remaining: u64,
}

impl<'object> ContentReader<'object> {
    pub(crate) fn new(file: &'object mut File, remaining: u64) -> Self {
        Self { file, remaining }
    }

    /// Bytes still available from this reader.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// True once the whole range has been consumed.
    pub fn is_empty(&self) -> bool {
        self.remaining == 0
    }
}

impl Read for ContentReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let limit = self.remaining.min(buf.len() as u64) as usize;
        let read = self.file.read(&mut buf[..limit])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

/// Streams the content section of an object through a decompressor.
///
/// Only produced by [crate::fs::TuxObject::decompressed_content_reader]; for uncompressed objects
/// it is a thin pass-through so callers can use one type either way.
pub enum DecodedContentReader<'object> {
    Stored(ContentReader<'object>),
    #[cfg(feature = "zstd")]
    Zstd(Box<zstd::stream::read::Decoder<'static, std::io::BufReader<ContentReader<'object>>>>),
    #[cfg(feature = "gzip")]
    Gzip(Box<flate2::read::GzDecoder<ContentReader<'object>>>),
}

impl Read for DecodedContentReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            DecodedContentReader::Stored(reader) => reader.read(buf),
            #[cfg(feature = "zstd")]
            DecodedContentReader::Zstd(reader) => reader.read(buf),
            #[cfg(feature = "gzip")]
            DecodedContentReader::Gzip(reader) => reader.read(buf),
        }
    }
}
