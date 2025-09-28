use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType, typed_object_type,
};
mod alloc;
#[cfg(feature = "bytes")]
mod bytes;
mod map;
mod num;
mod option;
#[cfg(feature = "uuid")]
mod uuid_impl;
mod time;
pub use time::*;
impl TuxIOType for bool {
    fn const_size(&self) -> Option<usize> {
        Some(1)
    }
    fn size(&self) -> usize {
        1
    }
}
typed_object_type!(
    bool => 10
);
impl WritableObjectType for bool {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[*self as u8])?;
        Ok(())
    }
}
impl ReadableObjectType for bool {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(1)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 1];
        reader.read_exact(&mut buffer)?;
        match buffer[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(EncodingError::UnexpectedEof),
        }
    }
}
impl<const N: usize> TuxIOType for [u8; N] {
    fn const_size(&self) -> Option<usize> {
        Some(N)
    }
    fn size(&self) -> usize {
        N
    }
}
impl<const N: usize> TypedObjectType for [u8; N] {
    fn type_key() -> u8 {
        11
    }
}
impl<const N: usize> WritableObjectType for [u8; N] {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        is_size_allowed(N)?;
        (N as u16).write_to_writer(writer)?;
        writer.write_all(self)?;
        Ok(())
    }
}
impl<const N: usize> ReadableObjectType for [u8; N] {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(N + 2)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        if length != N {
            return Err(EncodingError::UnexpectedEof);
        }
        let mut buffer = [0u8; N];
        reader.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}
#[inline(always)]
pub(crate) fn is_size_allowed(size: usize) -> Result<(), EncodingError> {
    if size > u16::MAX as usize {
        return Err(EncodingError::TypeTooLarge(size));
    }
    Ok(())
}
