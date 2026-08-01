use std::collections::HashSet;
use std::hash::Hash;
use std::io::{Read, Seek};

use crate::types::{count_is_allowed, length_is_allowed};
use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TypedObjectType, WritableObjectType,
    typed_object_type,
};
use crate::{ReadWithSize, TuxIOType};
impl<T: TuxIOType> TuxIOType for Vec<T> {
    /// The length prefix plus the encoded size of every element.
    ///
    /// For `Vec<u8>` this is `len + 2`, because a `u8` encodes as one byte.
    fn size(&self) -> usize {
        self.iter().map(TuxIOType::size).sum::<usize>() + 2
    }
}
impl<T: TuxIOType> TuxIOType for HashSet<T> {
    /// The length prefix plus the encoded size of every element.
    fn size(&self) -> usize {
        self.iter().map(TuxIOType::size).sum::<usize>() + 2
    }
}
typed_object_type!(
    Vec<u8> => 11
);
impl<T: TuxIOType + WritableObjectType> WritableObjectType for Vec<T> {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        count_is_allowed("Vec", self.len())?;
        (self.len() as u16).write_to_writer(writer)?;
        for item in self {
            item.write_to_writer(writer)?;
        }
        Ok(())
    }
}
impl<T: TuxIOType + ReadableObjectType> ReadableObjectType for Vec<T> {
    /// Reads the length prefix and returns `len + 2`.
    ///
    /// ### Note
    /// This is only accurate when `T` encodes to exactly one byte per element, which is the case
    /// for the registered `Vec<u8>` byte-block type (type key 11) — the only `Vec` the value
    /// encoding ever reads back. Element-wise vectors of larger types must be sized by decoding
    /// them, as a per-element scan here would put a seek loop on the metadata read path.
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
        count_is_allowed("HashSet", self.len())?;
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
        length_is_allowed("String", bytes.len())?;
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
        Ok(String::from_utf8(buffer)?)
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_bytes(&bytes[0..2])? as usize;
        if bytes.len() < length + 2 {
            return Err(EncodingError::UnexpectedEof);
        }
        Ok(String::from_utf8(bytes[2..length + 2].to_vec())?)
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

#[cfg(feature = "tokio")]
mod tokio_async {
    //! Async support for the length-prefixed types.
    //!
    //! These had none, so the async traits were unusable for the two types every metadata and tag block
    //! is made of — only the numeric types, the header and `Uuid` had them.

    use tokio::io::AsyncRead;

    use crate::{
        EncodingError,
        tokio_io::{AsyncReadableObjectType, AsyncWritableObjectType, read_length_prefixed},
    };

    impl AsyncWritableObjectType for String {}
    impl AsyncReadableObjectType for String {
        async fn read_from_async_reader<R>(reader: &mut R) -> Result<Self, EncodingError>
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            Ok(String::from_utf8(read_length_prefixed(reader).await?)?)
        }
    }

    impl AsyncWritableObjectType for Vec<u8> {}
    impl AsyncReadableObjectType for Vec<u8> {
        async fn read_from_async_reader<R>(reader: &mut R) -> Result<Self, EncodingError>
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            read_length_prefixed(reader).await
        }
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
        Ok(String::from_utf8(buffer)?)
    }
}
