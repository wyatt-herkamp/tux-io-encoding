use std::io::Read;

use bytes::{Bytes, BytesMut};

use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType, typed_object_type, types::length_is_allowed,
};

impl TuxIOType for Bytes {
    fn size(&self) -> usize {
        self.len() + 2
    }
}
typed_object_type!(
    Bytes => 11
);
impl WritableObjectType for Bytes {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        length_is_allowed("Bytes", self.len())?;
        (self.len() as u16).write_to_writer(writer)?;
        writer.write_all(self)?;
        Ok(())
    }
}
impl ReadableObjectType for Bytes {
    fn read_size<R: Read>(reader: &mut R) -> Result<usize, EncodingError> {
        let length = u16::read_from_reader(reader)? as usize;
        Ok(length + 2)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        // `BytesMut::with_capacity` reserves capacity but leaves the length at zero, so deref'ing
        // it hands `read_exact` an empty slice and nothing is actually read.
        let mut writer = BytesMut::zeroed(length);
        reader.read_exact(&mut writer)?;
        Ok(writer.freeze())
    }
}

#[cfg(feature = "tokio")]
mod tokio_async {
    //! Async support for `Bytes`, matching the other byte-block types.

    use bytes::Bytes;
    use tokio::io::AsyncRead;

    use crate::{
        EncodingError,
        tokio_io::{AsyncReadableObjectType, AsyncWritableObjectType, read_length_prefixed},
    };

    impl AsyncWritableObjectType for Bytes {}
    impl AsyncReadableObjectType for Bytes {
        async fn read_from_async_reader<R>(reader: &mut R) -> Result<Self, EncodingError>
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            Ok(Bytes::from(read_length_prefixed(reader).await?))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bytes::Bytes;

    use crate::{ReadableObjectType, TuxIOType, WritableObjectType};

    #[test]
    fn round_trip() {
        let value = Bytes::from_static(b"hello tuxio");
        let encoded = value.write_to_bytes().unwrap();
        assert_eq!(encoded.len(), value.size());

        let decoded = Bytes::read_from_reader(&mut Cursor::new(&encoded)).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn round_trip_empty() {
        let value = Bytes::new();
        let encoded = value.write_to_bytes().unwrap();
        let decoded = Bytes::read_from_reader(&mut Cursor::new(&encoded)).unwrap();
        assert_eq!(decoded, value);
    }
}
