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
