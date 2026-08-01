use tux_io_encoding_macros::ValueEnum;

use crate::{
    ConstTypedObjectType, EncodingError, RawDate, RawDateTime, RawTime, ReadableObjectType,
    TuxIOType, WritableObjectType,
};
#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum ValueType {
    String(String),
    Bytes(Vec<u8>),
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Date(RawDate),
    Time(RawTime),
    RawDateTime(RawDateTime),
    #[cfg(feature = "uuid")]
    Uuid(uuid::Uuid),
}
impl ValueType {
    /// Returns the string representation of the value if it is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ValueType::String(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(feature = "get-size2")]
mod get_size2_impl {
    use super::*;
    use get_size2::GetSize;
    impl GetSize for ValueType {
        fn get_heap_size(&self) -> usize {
            match self {
                ValueType::String(s) => s.get_heap_size(),
                ValueType::Bytes(b) => b.get_heap_size(),
                _ => 0,
            }
        }
        fn get_heap_size_with_tracker<T: get_size2::GetSizeTracker>(
            &self,
            tracker: T,
        ) -> (usize, T) {
            match self {
                ValueType::String(s) => s.get_heap_size_with_tracker(tracker),
                ValueType::Bytes(b) => b.get_heap_size_with_tracker(tracker),
                _ => (0, tracker),
            }
        }
    }
}

#[cfg(feature = "tokio")]
mod tokio_async {
    //! Async support for [ValueType].
    //!
    //! A value is a type key then the value itself, so this reads the key, then dispatches to the inner
    //! type's async reader. `ValueType` had no async impl at all, which meant nothing that *contains*
    //! values — metadata, tags — could have one either.

    use tokio::io::{AsyncRead, AsyncReadExt};

    use super::*;
    use crate::tokio_io::{AsyncReadableObjectType, AsyncWritableObjectType};

    impl AsyncWritableObjectType for ValueType {}
    impl AsyncReadableObjectType for ValueType {
        async fn read_from_async_reader<R>(reader: &mut R) -> Result<Self, EncodingError>
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            let type_key = reader.read_u8().await.map_err(EncodingError::IOError)?;
            match type_key {
                <String as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::String(
                    String::read_from_async_reader(reader).await?,
                )),
                <Vec<u8> as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::Bytes(
                    <Vec<u8>>::read_from_async_reader(reader).await?,
                )),
                <bool as ConstTypedObjectType>::TYPE_KEY => {
                    let byte = reader.read_u8().await.map_err(EncodingError::IOError)?;
                    match byte {
                        0 => Ok(ValueType::Bool(false)),
                        1 => Ok(ValueType::Bool(true)),
                        byte => Err(EncodingError::InvalidValue {
                            type_name: "bool",
                            byte,
                        }),
                    }
                }
                <u8 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::U8(u8::read_from_async_reader(reader).await?))
                }
                <u16 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::U16(u16::read_from_async_reader(reader).await?))
                }
                <u32 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::U32(u32::read_from_async_reader(reader).await?))
                }
                <u64 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::U64(u64::read_from_async_reader(reader).await?))
                }
                <i8 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::I8(i8::read_from_async_reader(reader).await?))
                }
                <i16 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::I16(i16::read_from_async_reader(reader).await?))
                }
                <i32 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::I32(i32::read_from_async_reader(reader).await?))
                }
                <i64 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::I64(i64::read_from_async_reader(reader).await?))
                }
                <f32 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::F32(f32::read_from_async_reader(reader).await?))
                }
                <f64 as ConstTypedObjectType>::TYPE_KEY => {
                    Ok(ValueType::F64(f64::read_from_async_reader(reader).await?))
                }
                <RawDate as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::Date(
                    RawDate::read_from_async_reader(reader).await?,
                )),
                <RawTime as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::Time(
                    RawTime::read_from_async_reader(reader).await?,
                )),
                <RawDateTime as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::RawDateTime(
                    RawDateTime::read_from_async_reader(reader).await?,
                )),
                #[cfg(feature = "uuid")]
                <uuid::Uuid as ConstTypedObjectType>::TYPE_KEY => Ok(ValueType::Uuid(
                    uuid::Uuid::read_from_async_reader(reader).await?,
                )),
                other => Err(EncodingError::UnknownTypeKey(other)),
            }
        }
    }
}
