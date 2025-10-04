use std::io::SeekFrom;

use crate::{
    compression_types::{CompressionTypes, NoCompression}, EncodingError, FileSections, ReadableObjectType, TuxIOType, WritableObjectType
};
pub const MAGIC_VALUE: [u8; 3] = [0x54, 0x55, 0x58]; // "TUX"
const CURRENT_VERSION: u8 = 0;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHeader {
    pub version: u8,
    pub compression_type: CompressionTypes,
    pub tags_start: u16,
    pub content_start: u32,
    pub content_length: u64,
    pub bit_flags: u8,
}
impl ObjectHeader {
    /// Returns true if the object has tags.
    #[inline(always)]
    pub fn has_tags(&self) -> bool {
        self.tags_start != 0
    }
    /// Returns the amount of space reserved for tags in the file.
    ///
    /// ### Note
    /// This could contain blank space for padding to prevent resizing the file.
    pub fn tags_space(&self) -> usize {
        let tags_start = self.tags_start as usize;
        let content_start = self.content_start as usize;
        content_start - tags_start
    }
    pub fn meta_and_tag_space(&self) -> usize {
        let meta_start = 32; // The size of the ObjectHeader
        let content_start = self.content_start as usize;
        content_start - meta_start
    }
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
            compression_type: CompressionTypes::None(NoCompression::default()),
            tags_start: 0,
            content_start: 0,
            content_length: 0,
            bit_flags: 0,
        }
    }
}
impl ObjectHeader {
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
