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
