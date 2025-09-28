use std::io::Read;

use crate::{EncodingError, ReadableObjectType, TuxIOType, WritableObjectType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionTypes {
    None = 0,
}
impl TryFrom<u8> for CompressionTypes {
    type Error = EncodingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CompressionTypes::None),
            _ => Err(EncodingError::InvalidCompressionType(value)),
        }
    }
}
impl TuxIOType for CompressionTypes {
    fn const_size(&self) -> Option<usize> {
        Some(1)
    }
    fn size(&self) -> usize {
        1
    }
}
impl WritableObjectType for CompressionTypes {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[*self as u8])?;
        Ok(())
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
        let mut buffer = [0u8; 1];
        reader.read_exact(&mut buffer)?;
        CompressionTypes::try_from(buffer[0])
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if bytes.is_empty() {
            return Err(EncodingError::UnexpectedEof);
        }
        CompressionTypes::try_from(bytes[0])
    }
}

#[cfg(feature="get-size2")]
mod get_size2_impl{
    use super::*;
    use get_size2::GetSize;
    impl GetSize for CompressionTypes{
        // No heap allocation

    }
}
