use std::io::Read;

use crate::{EncodingError, ReadableObjectType, TuxIOType, WritableObjectType};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub struct ZStdCompressionType(pub i32);
impl TuxIOType for ZStdCompressionType {
    fn const_size(&self) -> Option<usize> {
        Some(5)
    }
    fn size(&self) -> usize {
        5
    }
}
impl WritableObjectType for ZStdCompressionType {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[1])?;
        writer.write_all(&self.0.to_le_bytes())?;
        Ok(())
    }
}
impl ReadableObjectType for ZStdCompressionType {
    fn read_size<R: Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(5)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 5];
        reader.read_exact(&mut buffer)?;
        if buffer[0] != 1 {
            return Err(EncodingError::InvalidCompressionType(buffer[0]));
        }
        let level = i32::from_le_bytes(
            buffer[1..5]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        Ok(ZStdCompressionType(level))
    }
    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if bytes.len() < 5 {
            return Err(EncodingError::UnexpectedEof);
        }
        if bytes[0] != 1 {
            return Err(EncodingError::InvalidCompressionType(bytes[0]));
        }
        let level = i32::from_le_bytes(
            bytes[1..5]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        Ok(ZStdCompressionType(level))
    }
}#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub struct GzipCompressionType(pub u32);
impl TuxIOType for GzipCompressionType {
    fn const_size(&self) -> Option<usize> {
        Some(5)
    }
    fn size(&self) -> usize {
        5
    }
}
impl WritableObjectType for GzipCompressionType {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[2])?;
        writer.write_all(&self.0.to_le_bytes())?;
        Ok(())
    }
}
impl ReadableObjectType for GzipCompressionType {
    fn read_size<R: Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(5)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 5];
        reader.read_exact(&mut buffer)?;
        if buffer[0] != 2 {
            return Err(EncodingError::InvalidCompressionType(buffer[0]));
        }
        let level = u32::from_le_bytes(
            buffer[1..5]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        Ok(GzipCompressionType(level))
    }
    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if bytes.len() < 5 {
            return Err(EncodingError::UnexpectedEof);
        }
        if bytes[0] != 2 {
            return Err(EncodingError::InvalidCompressionType(bytes[0]));
        }
        let level = u32::from_le_bytes(
            bytes[1..5]
                .try_into()
                .map_err(|_| EncodingError::UnexpectedEof)?,
        );
        Ok(GzipCompressionType(level))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoCompression;
impl TuxIOType for NoCompression {
    fn const_size(&self) -> Option<usize> {
        Some(5)
    }
    fn size(&self) -> usize {
        5
    }
}
impl WritableObjectType for NoCompression {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[0, 0, 0, 0, 0])?;
        Ok(())
    }
}
impl ReadableObjectType for NoCompression {
    fn read_size<R: Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(5)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 5];
        reader.read_exact(&mut buffer)?;
        if buffer[0] != 0 {
            return Err(EncodingError::InvalidCompressionType(buffer[0]));
        }
        Ok(NoCompression)
    }
    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        if bytes.len() < 5 {
            return Err(EncodingError::UnexpectedEof);
        }
        if bytes[0] != 0 {
            return Err(EncodingError::InvalidCompressionType(bytes[0]));
        }
        Ok(NoCompression)
    }
}
