use std::collections::HashSet;
use std::hash::Hash;
use std::io::{Read, Seek};

use crate::types::is_size_allowed;
use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TypedObjectType, WritableObjectType,
    typed_object_type,
};
use crate::{ReadWithSize, TuxIOType};
impl<T: TuxIOType> TuxIOType for Vec<T> {
    fn size(&self) -> usize {
        self.len() + 2
    }
}
impl<T: TuxIOType> TuxIOType for HashSet<T> {
    fn size(&self) -> usize {
        self.len() + 2
    }
}
typed_object_type!(
    Vec<u8> => 11
);
impl<T: TuxIOType + WritableObjectType> WritableObjectType for Vec<T> {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        is_size_allowed(self.len())?;
        (self.len() as u16).write_to_writer(writer)?;
        for item in self {
            item.write_to_writer(writer)?;
        }
        Ok(())
    }
}
impl<T: TuxIOType + ReadableObjectType> ReadableObjectType for Vec<T> {
    fn read_size<R: Read>(reader: &mut R) -> Result<usize, EncodingError> {
        let length = u16::read_from_reader(reader)? as usize;
        Ok(length + 2)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        let mut buffer = Vec::with_capacity(length);
        for _ in 0..length {
            buffer.push(T::read_from_reader(reader)?);
        }
        Ok(buffer)
    }
}

impl<T: TuxIOType + WritableObjectType> WritableObjectType for HashSet<T> {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        is_size_allowed(self.len())?;
        (self.len() as u16).write_to_writer(writer)?;
        for item in self {
            item.write_to_writer(writer)?;
        }
        Ok(())
    }
}
impl<T: TuxIOType + ReadableObjectType + Eq + Hash> ReadableObjectType for HashSet<T> {
    fn read_size<R: Read>(reader: &mut R) -> Result<usize, EncodingError> {
        let length = u16::read_from_reader(reader)? as usize;
        Ok(length + 2)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        let mut items = HashSet::with_capacity(length);
        for _ in 0..length {
            items.insert(T::read_from_reader(reader)?);
        }
        Ok(items)
    }
}

impl TuxIOType for String {
    fn size(&self) -> usize {
        self.len() + 2
    }
}

typed_object_type!(
    String => 12
);
impl WritableObjectType for String {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        let bytes = self.as_bytes();
        is_size_allowed(bytes.len())?;
        (bytes.len() as u16).write_to_writer(writer)?;
        writer.write_all(bytes)?;
        Ok(())
    }
}
impl ReadableObjectType for String {
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        let mut buffer = vec![0u8; length];
        reader.read_exact(&mut buffer)?;
        String::from_utf8(buffer).map_err(|_| EncodingError::UnexpectedEof)
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_bytes(&bytes[0..2])? as usize;
        if bytes.len() < length + 2 {
            return Err(EncodingError::UnexpectedEof);
        }
        String::from_utf8(bytes[2..length + 2].to_vec()).map_err(|_| EncodingError::UnexpectedEof)
    }

    fn read_size<R: Read>(reader: &mut R) -> Result<usize, EncodingError> {
        let length = u16::read_from_reader(reader)? as usize;
        Ok(length + 2)
    }
    fn skip<R: Read + Seek>(reader: &mut R) -> Result<(), EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        reader.seek(std::io::SeekFrom::Current(length as i64))?;
        Ok(())
    }
}

impl ReadWithSize for Vec<u8> {
    type Size = u16;

    fn read_with_size<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        size: Self::Size,
    ) -> Result<Self, EncodingError> {
        let mut buffer = vec![0u8; size as usize];
        reader.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}
impl ReadWithSize for String {
    type Size = u16;

    fn read_with_size<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        size: Self::Size,
    ) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = vec![0u8; size as usize];
        reader.read_exact(&mut buffer)?;
        String::from_utf8(buffer).map_err(|_| EncodingError::UnexpectedEof)
    }
}
