use std::{
    fmt::Display,
    io::{Read, Seek},
};

use http::HeaderName;

use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TagKeyType, Tags, TuxIOType,
    TypedObjectType, WritableObjectType, types::length_is_allowed,
};
pub type MetadataMap = Tags<MetaKey>;

/// Lookups by [HeaderName], which is how callers almost always hold a metadata key.
///
/// These exist instead of a `Borrow<HeaderName> for MetaKey` impl: with that impl in place, the
/// common `metadata.get(&SOME_HEADER.into())` becomes ambiguous, because the borrowed key type
/// could be inferred as either `MetaKey` or `HeaderName`.
impl MetadataMap {
    /// Looks up a value by header name, without allocating a [MetaKey].
    pub fn get_header(&self, name: &HeaderName) -> Option<&crate::ValueType> {
        // `MetaKey` is a newtype over `HeaderName`, so a reference to one is a reference to the
        // other as far as hashing and equality are concerned.
        self.0.get(MetaKey::wrap_ref(name))
    }
    /// Mutable counterpart of [MetadataMap::get_header].
    pub fn get_header_mut(&mut self, name: &HeaderName) -> Option<&mut crate::ValueType> {
        self.0.get_mut(MetaKey::wrap_ref(name))
    }
    pub fn contains_header(&self, name: &HeaderName) -> bool {
        self.0.contains_key(MetaKey::wrap_ref(name))
    }
    pub fn remove_header(&mut self, name: &HeaderName) -> Option<crate::ValueType> {
        self.0.remove(MetaKey::wrap_ref(name))
    }
    /// Inserts a value under a header name.
    pub fn insert_header(
        &mut self,
        name: HeaderName,
        value: crate::ValueType,
    ) -> Option<crate::ValueType> {
        self.0.insert(MetaKey(name), value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct MetaKey(HeaderName);

impl MetaKey {
    /// Views a [HeaderName] reference as a [MetaKey] reference.
    ///
    /// Sound because `MetaKey` is `#[repr(transparent)]` over `HeaderName` and its derived `Hash`
    /// and `Eq` delegate straight to the wrapped value, so the two hash and compare identically.
    fn wrap_ref(name: &HeaderName) -> &MetaKey {
        // SAFETY: `MetaKey` is a `repr(transparent)` newtype around `HeaderName`, so the two have
        // the same layout and a shared reference can be cast between them.
        unsafe { &*(name as *const HeaderName as *const MetaKey) }
    }
}

impl MetaKey {
    /// Borrows the underlying [HeaderName].
    pub fn as_header_name(&self) -> &HeaderName {
        &self.0
    }
    /// Consumes the key, returning the underlying [HeaderName].
    pub fn into_header_name(self) -> HeaderName {
        self.0
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    /// Parses a metadata key from a header-name string.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncodingError> {
        HeaderName::from_bytes(bytes)
            .map(MetaKey)
            .map_err(EncodingError::other)
    }
}
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
impl From<&HeaderName> for MetaKey {
    fn from(value: &HeaderName) -> Self {
        MetaKey(value.clone())
    }
}
impl From<MetaKey> for HeaderName {
    fn from(value: MetaKey) -> Self {
        value.0
    }
}
impl AsRef<HeaderName> for MetaKey {
    fn as_ref(&self) -> &HeaderName {
        &self.0
    }
}
impl AsRef<str> for MetaKey {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
impl std::ops::Deref for MetaKey {
    type Target = HeaderName;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl TryFrom<&str> for MetaKey {
    type Error = EncodingError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_bytes(value.as_bytes())
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
        length_is_allowed("MetaKey", bytes.len())?;
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
