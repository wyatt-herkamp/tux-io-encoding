use std::io::Read;
mod types;
pub use types::*;
use crate::{ EncodingError, ReadableObjectType, TuxIOType, WritableObjectType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionTypes {
    None(NoCompression),
    ZSTD(ZStdCompressionType),
    Gzip(GzipCompressionType),
}
impl Default for CompressionTypes {
    fn default() -> Self {
        CompressionTypes::None(NoCompression)
    }
}
impl TryFrom<[u8;5]> for CompressionTypes {
    type Error = EncodingError;
    fn try_from(value: [u8; 5]) -> Result<Self, Self::Error> {
        match value[0] {
            0 => Ok(CompressionTypes::None(NoCompression)),
            1 => {
                Ok(CompressionTypes::ZSTD(ZStdCompressionType::read_from_bytes(&value)?))
            }
            2 => {
                Ok(CompressionTypes::Gzip(GzipCompressionType::read_from_bytes(&value)?))
            }
            other => Err(EncodingError::InvalidCompressionType(other)),
        }
    }
}

impl TryFrom<&[u8]> for CompressionTypes {
    type Error = EncodingError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 5 {
            return Err(EncodingError::UnexpectedEof);
        }
        let array: [u8; 5] = value[0..5]
            .try_into()
            .map_err(|_| EncodingError::UnexpectedEof)?;
        Self::try_from(array)
    }
}

impl TuxIOType for CompressionTypes {
    fn const_size(&self) -> Option<usize> {
        Some(5)
    }
    fn size(&self) -> usize {
        5
    }
}
impl WritableObjectType for CompressionTypes {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        match self {
            CompressionTypes::None(c) => c.write_to_writer(writer),
            CompressionTypes::ZSTD(c) => c.write_to_writer(writer),
            CompressionTypes::Gzip(c) => c.write_to_writer(writer),
        }
    }
}
impl ReadableObjectType for CompressionTypes {
    fn read_size<R: Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(1)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 5];
        reader.read_exact(&mut buffer)?;
        Self::try_from(buffer)
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if bytes.len() <= 5 {
            return Err(EncodingError::UnexpectedEof);
        }
        let buffer: [u8; 5] = bytes[0..5]
            .try_into()
            .map_err(|_| EncodingError::UnexpectedEof)?;
        Self::try_from(buffer)
    }
}

#[cfg(feature = "get-size2")]
mod get_size2_impl {
    use super::*;
    use get_size2::GetSize;
    impl GetSize for CompressionTypes {
        // No heap allocation
    }
}
