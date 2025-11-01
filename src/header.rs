use std::io::SeekFrom;

use crate::{
    EncodingError, FileSections, ReadableObjectType, TuxIOType, WritableObjectType,
    compression_types::{CompressionTypes, NoCompression},
};
pub const MAGIC_VALUE: [u8; 3] = [0x54, 0x55, 0x58]; // "TUX"
const CURRENT_VERSION: u8 = 0;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHeader {
    /// Version of TuxIO Object
    /// Current version is 0
    pub version: u8,
    /// Compression type of the object
    ///
    /// Compression only applies to the content of the object not metadata or tags
    pub compression_type: CompressionTypes,
    /// The byte offset that the object tags start at
    pub tags_start: u16,
    /// The byte offset that the object content starts at
    pub content_start: u32,
    /// The length of the content in bytes
    pub content_length: u64,
    /// Bit flags for the object
    ///
    /// Currently unused
    pub bit_flags: u8,
}
impl ObjectHeader {
    /// Returns the amount of space reserved for tags in the file.
    ///
    /// ### Note
    /// This could contain blank space for padding to prevent resizing the file.
    pub fn tags_space(&self) -> usize {
        let tags_start = self.tags_start as usize;
        let content_start = self.content_start as usize;
        content_start - tags_start
    }
    /// Returns the amount of space used for metadata and tags in the file.
    pub fn meta_and_tag_space(&self) -> usize {
        let meta_start = 32; // The size of the ObjectHeader
        let content_start = self.content_start as usize;
        content_start - meta_start
    }
    /// Creates a [SeekFrom] for the given section.
    pub fn seek(&self, section: FileSections) -> SeekFrom {
        match section {
            FileSections::Header => SeekFrom::Start(0),
            FileSections::Metadata => SeekFrom::Start(32),
            FileSections::Tags => SeekFrom::Start(self.tags_start as u64),
            FileSections::Content => SeekFrom::Start(self.content_start as u64),
        }
    }
}
impl Default for ObjectHeader {
    fn default() -> Self {
        ObjectHeader {
            version: CURRENT_VERSION,
            compression_type: CompressionTypes::None(NoCompression),
            tags_start: 0,
            content_start: 0,
            content_length: 0,
            bit_flags: 0,
        }
    }
}
impl ObjectHeader {
    /// Ensure's that a header starts with [MAGIC_VALUE]
    ///
    /// Then returns the version byte.
    pub fn header_entry(entry: &[u8]) -> Result<u8, EncodingError> {
        if entry[0..3] != MAGIC_VALUE {
            return Err(EncodingError::InvalidMagic);
        }
        Ok(entry[3])
    }
}
impl TuxIOType for ObjectHeader {
    fn const_size(&self) -> Option<usize> {
        Some(32)
    }
    fn size(&self) -> usize {
        32
    }
}

impl ReadableObjectType for ObjectHeader {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(32)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut content = [0u8; 32];
        reader.read_exact(&mut content)?;
        Self::read_from_bytes(&content)
    }
    fn skip<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<(), EncodingError>
    where
        Self: Sized,
    {
        reader.seek(SeekFrom::Start(32))?;
        Ok(())
    }
    fn read_from_bytes(content: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if content.len() < 32 {
            return Err(EncodingError::UnexpectedEof);
        }
        let version = Self::header_entry(&content[0..4])?;
        if version != CURRENT_VERSION {
            return Err(EncodingError::InvalidMagic);
        }
        let compression_type_bytes = &content[4..9];
        let compression_type = CompressionTypes::try_from(compression_type_bytes)?;
        let tags_start = u16::from_le_bytes(
            content[9..11]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        let content_start = u32::from_le_bytes(
            content[11..15]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        let content_length = u64::from_le_bytes(
            content[15..23]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        let bit_flags = content[23];
        Ok(ObjectHeader {
            version,
            compression_type,
            tags_start,
            content_start,
            content_length,
            bit_flags,
        })
    }
}

impl WritableObjectType for ObjectHeader {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&MAGIC_VALUE)?;
        writer.write_all(&[self.version])?;
        self.compression_type.write_to_writer(writer)?;
        self.tags_start.write_to_writer(writer)?;
        self.content_start.write_to_writer(writer)?;
        self.content_length.write_to_writer(writer)?;
        writer.write_all(&[self.bit_flags])?;
        if self.const_size().is_some() {
            let padding = 32 - (MAGIC_VALUE.len() + 1 + 5 + 2 + 4 + 8 + 1);
            if padding > 0 {
                writer.write_all(&vec![0; padding])?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "tokio")]
mod tokio_async {
    //! Async implementation for tokio
    //!
    //! Reading is done by reading all 32 bytes into a buffer and then parsing it.
    use tokio::io::{AsyncRead, AsyncReadExt};

    use super::ObjectHeader;
    use crate::{ReadableObjectType, tokio_io::*};

    impl AsyncWritableObjectType for ObjectHeader {}
    impl AsyncReadableObjectType for ObjectHeader {
        fn read_from_async_reader<R>(
            reader: &mut R,
        ) -> impl Future<Output = Result<Self, crate::EncodingError>> + Send
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            async move {
                let mut buf = [0u8; 32];
                reader
                    .read_exact(&mut buf)
                    .await
                    .map_err(crate::EncodingError::IOError)?;
                <ObjectHeader as ReadableObjectType>::read_from_bytes(&buf)
            }
        }
    }
}
#[cfg(feature = "get-size2")]
mod get_size2_impl {
    use super::*;
    use get_size2::GetSize;
    impl GetSize for ObjectHeader {}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_object_header_read_write() {
        let header = ObjectHeader {
            version: CURRENT_VERSION,
            compression_type: CompressionTypes::None(NoCompression),
            tags_start: 10,
            content_start: 20,
            content_length: 100,
            bit_flags: 0,
        };
        let mut buffer = Vec::new();
        header.write_to_writer(&mut buffer).unwrap();
        assert_eq!(buffer.len(), 32);
        println!("Buffer: {:?}", buffer);
        let read_header = ObjectHeader::read_from_reader(&mut buffer.as_slice()).unwrap();
        assert_eq!(header, read_header);
    }

    #[test]
    fn test_size_math() {
        let header = ObjectHeader {
            version: CURRENT_VERSION,
            compression_type: CompressionTypes::default(),
            tags_start: 64,
            content_start: 256,
            content_length: 256,
            bit_flags: 0,
        };
        assert_eq!(header.tags_space(), 192); // 256 - 64
        assert_eq!(header.meta_and_tag_space(), 224); // 256 - 32
    }
}
