use std::{
    fmt::Display,
    io::{Read, Seek},
};

use http::HeaderName;

use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TagKeyType, Tags, TuxIOType,
    TypedObjectType, WritableObjectType, types::is_size_allowed,
};
pub type MetadataMap = Tags<MetaKey>;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetaKey(HeaderName);

impl Display for MetaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}
impl TagKeyType for MetaKey {}
impl From<HeaderName> for MetaKey {
    fn from(value: HeaderName) -> Self {
        MetaKey(value)
    }
}
impl TuxIOType for MetaKey {
    fn size(&self) -> usize {
        self.0.as_str().len() + 2
    }
}
impl ConstTypedObjectType for MetaKey {
    const TYPE_KEY: u8 = 12;
}
impl TypedObjectType for MetaKey {
    fn type_key() -> u8 {
        Self::TYPE_KEY
    }
}
impl WritableObjectType for MetaKey {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        let bytes = self.0.as_str().as_bytes();
        is_size_allowed(bytes.len())?;
        (bytes.len() as u16).write_to_writer(writer)?;
        writer.write_all(bytes)?;
        Ok(())
    }
}
impl ReadableObjectType for MetaKey {
    fn read_size<R: Read + Seek>(reader: &mut R) -> Result<usize, EncodingError> {
        Vec::<u8>::read_size(reader)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let content = Vec::<u8>::read_from_reader(reader)?;
        HeaderName::from_bytes(&content)
            .map(MetaKey::from)
            .map_err(EncodingError::other)
    }
    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let content = Vec::<u8>::read_from_bytes(bytes)?;
        HeaderName::from_bytes(&content)
            .map(MetaKey::from)
            .map_err(EncodingError::other)
    }
}

#[cfg(feature = "get-size2")]
mod get_size2_impl {
    use super::*;
    use get_size2::GetSize;

    impl GetSize for MetaKey {
        // TODO: Technically the heap size is not included because HeaderName doesn't expose a way to check if the internal storage is heap allocated or not.
    }
}
