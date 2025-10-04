use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType,
};
macro_rules! number {
    (
        $($type:ty => {
            size: $size:literal,
            key: $key:literal
        }),*
    ) => {
        $(
        impl TuxIOType for $type {
            fn const_size(&self) -> Option<usize> {
                Some($size)
            }
            fn size(&self) -> usize {
                $size
            }
        }
        impl TypedObjectType for $type {
            fn type_key() -> u8 {
                $key
            }
        }
        impl ConstTypedObjectType for $type {
            const TYPE_KEY: u8 = $key;
        }
        impl WritableObjectType for $type {
            fn write_to_writer<W: std::io::Write>(
                &self,
                writer: &mut W,
            ) -> Result<(), EncodingError> {
                writer.write_all(&self.to_le_bytes())?;
                Ok(())
            }
        }
        impl ReadableObjectType for $type {
            fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
                Ok($size)
            }
            fn read_from_reader<R: std::io::Read>(
                reader: &mut R,
            ) -> Result<Self, EncodingError>
            where
                Self: Sized,
            {
                let mut buffer = [0u8; $size];
                reader.read_exact(&mut buffer)?;
                Ok(Self::from_le_bytes(buffer))
            }
            fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
            where
                Self: Sized,
            {
                let bytes: &[u8; $size] = bytes[..$size]
                    .try_into()
                    .map_err(|_| EncodingError::UnexpectedEof)?;
                Ok(Self::from_le_bytes(*bytes))
            }
        }
        #[cfg(feature = "tokio")]
        impl crate::tokio_io::AsyncWritableObjectType for $type {
            fn write_to_async_writer<W>(
                &self,
                writer: &mut W,
            ) -> impl Future<Output = Result<(), crate::EncodingError>> + Send
            where
                Self: Sync,
                W: tokio::io::AsyncWrite + Unpin + Send {
                use tokio::io::AsyncWriteExt;
                async move {
                    writer
                        .write_all(&self.to_le_bytes())
                        .await
                        .map_err(crate::EncodingError::IOError)?;
                    Ok(())
                }
            }
        }
        #[cfg(feature = "tokio")]
            impl crate::tokio_io::AsyncReadableObjectType for $type{
        fn read_from_async_reader<R>(
            reader: &mut R,
        ) -> impl Future<Output = Result<Self, crate::EncodingError>> + Send
        where
            Self: Sync + Sized,
            R: tokio::io::AsyncRead + Unpin + Send {
            use tokio::io::AsyncReadExt;
            async move {
                let mut buf = [0u8;  $size];
                reader
                    .read_exact(&mut buf)
                    .await
                    .map_err(crate::EncodingError::IOError)?;
                Ok(Self::from_le_bytes(buf))
            }
        }
            }



        )*

    };
}

number!(
    u8 => {
        size: 1,
        key: 0
    },
    u16 => {
        size: 2,
        key: 1
    },
    u32 => {
        size: 4,
        key: 2
    },
    u64 => {
        size: 8,
        key: 3
    },
    i8 => {
        size: 1,
        key: 4
    },
    i16 => {
        size: 2,
        key: 5
    },
    i32 => {
        size: 4,
        key: 6
    },
    i64 => {
        size: 8,
        key: 7
    },
    f32 => {
        size: 4,
        key: 8
    },
    f64 => {
        size: 8,
        key: 9
    }
);
