use std::{
    borrow::Borrow,
    collections::HashMap,
    hash::Hash,
    io::{Read, Seek, Write},
};
mod meta_key;
use crate::{
    EncodingError, ReadableObjectType, TuxIOType, ValueType, WritableObjectType,
    types::count_is_allowed,
};
pub use meta_key::*;
pub trait TagKeyType:
    Hash
    + PartialEq
    + Eq
    + Clone
    + std::fmt::Debug
    + WritableObjectType
    + ReadableObjectType
    + TuxIOType
{
}
impl TagKeyType for String {}
#[derive(Debug, Clone, PartialEq)]
pub struct Tags<Key: TagKeyType = String>(pub HashMap<Key, ValueType>);
impl<Key: TagKeyType> Default for Tags<Key> {
    fn default() -> Self {
        Self::new()
    }
}
impl<Key: TagKeyType> Tags<Key> {
    pub fn new() -> Self {
        Tags(HashMap::new())
    }
    pub fn insert(&mut self, key: Key, value: ValueType) -> Option<ValueType> {
        self.0.insert(key, value)
    }
    pub fn get<Q>(&self, key: &Q) -> Option<&ValueType>
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.0.get(key)
    }
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut ValueType>
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.0.get_mut(key)
    }
    pub fn remove(&mut self, key: &Key) -> Option<ValueType> {
        self.0.remove(key)
    }
    pub fn number_of_tags(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Tags(HashMap::with_capacity(capacity))
    }
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.0.contains_key(key)
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, Key, ValueType> {
        self.0.iter()
    }
    pub fn keys(&self) -> std::collections::hash_map::Keys<'_, Key, ValueType> {
        self.0.keys()
    }
    pub fn values(&self) -> std::collections::hash_map::Values<'_, Key, ValueType> {
        self.0.values()
    }
    /// Inserts a value only when the key is absent, returning whether it was inserted.
    pub fn insert_if_absent(&mut self, key: Key, value: ValueType) -> bool {
        match self.0.entry(key) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(value);
                true
            }
        }
    }
    pub fn find_from_reader<R: Read + Seek>(
        reader: &mut R,
        key: &Key,
    ) -> Result<Option<ValueType>, EncodingError> {
        let tags_count = u16::read_from_reader(reader)? as usize;
        for _ in 0..tags_count {
            let tag_key = Key::read_from_reader(reader)?;
            if &tag_key == key {
                return Ok(Some(ValueType::read_from_reader(reader)?));
            } else {
                // Skip the value if the key does not match
                ValueType::skip(reader)?;
            }
        }
        Ok(None)
    }
    pub fn read_tag_count<R: Read + Seek>(reader: &mut R) -> Result<u16, EncodingError> {
        u16::read_from_reader(reader)
    }
}

impl<Key: TagKeyType> FromIterator<(Key, ValueType)> for Tags<Key> {
    fn from_iter<I: IntoIterator<Item = (Key, ValueType)>>(iter: I) -> Self {
        Tags(iter.into_iter().collect())
    }
}
impl<Key: TagKeyType> Extend<(Key, ValueType)> for Tags<Key> {
    fn extend<I: IntoIterator<Item = (Key, ValueType)>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}
impl<Key: TagKeyType> IntoIterator for Tags<Key> {
    type Item = (Key, ValueType);
    type IntoIter = std::collections::hash_map::IntoIter<Key, ValueType>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<'tags, Key: TagKeyType> IntoIterator for &'tags Tags<Key> {
    type Item = (&'tags Key, &'tags ValueType);
    type IntoIter = std::collections::hash_map::Iter<'tags, Key, ValueType>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<Tag: TagKeyType> TuxIOType for Tags<Tag> {
    /// The `u16` pair count, then every key and value.
    ///
    /// `ValueType::size()` covers the type-key byte in front of each value, so there is nothing to add
    /// per pair here. This used to add `1` itself, compensating for that byte being missing from
    /// `ValueType::size()` — the compensation was right and the type was wrong.
    fn size(&self) -> usize {
        let size_of_contents: usize = self.0.iter().map(|(k, v)| k.size() + v.size()).sum();
        size_of_contents + 2
    }
}
impl<Tag: TagKeyType> WritableObjectType for Tags<Tag> {
    fn write_to_writer<W: Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        count_is_allowed("Tags", self.0.len())?;
        (self.0.len() as u16).write_to_writer(writer)?;
        for (key, value) in &self.0 {
            // Write the key length and key
            key.write_to_writer(writer)?;
            // Write the value
            value.write_to_writer(writer)?;
        }
        Ok(())
    }
}
impl<Tag: TagKeyType> ReadableObjectType for Tags<Tag> {
    /// Measures the encoded size of the tag block starting at the reader's current position.
    ///
    /// Offsets are tracked relative to where the block starts, so this works for a block sitting
    /// at any offset in a file — not just at the beginning of the reader. The cursor is left at
    /// the end of the block.
    fn read_size<R: Read + Seek>(reader: &mut R) -> Result<usize, EncodingError> {
        let block_start = reader.stream_position()?;
        let tags_count = u16::read_from_reader(reader)? as usize;
        let mut total_size = 2_usize;
        for _ in 0..tags_count {
            let key_size = Tag::read_size(reader)?;
            total_size += key_size;
            reader.seek(std::io::SeekFrom::Start(block_start + total_size as u64))?;
            let value_size = ValueType::read_size(reader)?;
            total_size += value_size;
            reader.seek(std::io::SeekFrom::Start(block_start + total_size as u64))?;
        }
        Ok(total_size)
    }
    fn read_from_reader<R: Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let tag_count = u16::read_from_reader(reader)? as usize;
        let mut tags = HashMap::with_capacity(tag_count);

        for _ in 0..tag_count {
            let key = Tag::read_from_reader(reader)?;
            // Read the value
            let value = ValueType::read_from_reader(reader)?;
            tags.insert(key, value);
        }
        Ok(Tags(tags))
    }
}
#[cfg(feature = "tokio")]
mod tokio_async {
    //! Async support for a tag or metadata block.
    //!
    //! The count, then each key and value in turn. Bounded by the format — a metadata block has to fit
    //! in the first 64 KiB of the file — so reading it whole is fine, and the per-entry awaits here just
    //! avoid needing a separate buffering step in the caller.

    use tokio::io::{AsyncRead, AsyncReadExt};

    use crate::{
        EncodingError, TagKeyType, Tags, ValueType,
        tokio_io::{AsyncReadableObjectType, AsyncWritableObjectType},
    };

    impl<Key: TagKeyType + Sync> AsyncWritableObjectType for Tags<Key> {}

    // `Send` on the key as well as `Sync`: a key is held across the await that reads its value, so the
    // returned future is only `Send` — which the trait requires — if the key is too.
    impl<Key> AsyncReadableObjectType for Tags<Key>
    where
        Key: TagKeyType + Send + Sync + AsyncReadableObjectType,
    {
        async fn read_from_async_reader<R>(reader: &mut R) -> Result<Self, EncodingError>
        where
            Self: Sync + Sized,
            R: AsyncRead + Unpin + Send,
        {
            let count = reader.read_u16_le().await.map_err(EncodingError::IOError)? as usize;
            let mut tags = std::collections::HashMap::with_capacity(count);
            for _ in 0..count {
                let key = Key::read_from_async_reader(reader).await?;
                let value = ValueType::read_from_async_reader(reader).await?;
                tags.insert(key, value);
            }
            Ok(Tags(tags))
        }
    }
}

#[cfg(feature = "get-size2")]
mod get_size2 {
    use get_size2::GetSize;

    use crate::{TagKeyType, Tags};

    impl<T: TagKeyType + GetSize> GetSize for Tags<T> {
        fn get_heap_size(&self) -> usize {
            self.0.get_heap_size()
        }
        fn get_heap_size_with_tracker<U: get_size2::GetSizeTracker>(
            &self,
            tracker: U,
        ) -> (usize, U) {
            self.0.get_heap_size_with_tracker(tracker)
        }
    }
}
#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    #[test]
    pub fn test_tags_serialization() {
        let mut tags = Tags::<String>(HashMap::new());
        tags.0
            .insert("tag1".into(), ValueType::String("value1".into()));
        tags.0
            .insert("tag2".into(), ValueType::String("value2".into()));
        let computed_size = tags.size();
        let mut buffer = Vec::with_capacity(computed_size);
        tags.write_to_writer(&mut buffer).unwrap();
        println!("Serialized Tags: {:?}", buffer);
        let read_size =
            Tags::<String>::read_size(&mut Cursor::new(&mut buffer.as_slice())).unwrap();

        assert_eq!(computed_size, read_size);
        let deserialized_tags: Tags = Tags::read_from_reader(&mut buffer.as_slice()).unwrap();
        assert_eq!(tags, deserialized_tags);
    }
}
