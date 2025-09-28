use std::{
    borrow::Borrow,
    collections::HashMap,
    hash::Hash,
    io::{Read, Seek, Write},
};
mod meta_key;
use crate::{
    EncodingError, ReadableObjectType, TuxIOType, ValueType, WritableObjectType,
    types::is_size_allowed,
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

impl<Tag: TagKeyType> TuxIOType for Tags<Tag> {
    fn size(&self) -> usize {
        // Calculate the size as the sum of the sizes of all tags
        let size_of_contents: usize = self.0.iter().map(|(k, v)| k.size() + 1 + v.size()).sum();
        size_of_contents + 2 // Add 2 bytes for the tag count
    }
}
impl<Tag: TagKeyType> WritableObjectType for Tags<Tag> {
    fn write_to_writer<W: Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        is_size_allowed(self.0.len())?;
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
    fn read_size<R: Read + Seek>(reader: &mut R) -> Result<usize, EncodingError> {
        let tags_count = u16::read_from_reader(reader)? as usize;
        let mut total_size = 2_usize;
        for _ in 0..tags_count {
            let key_size = Tag::read_size(reader)?;
            total_size += key_size;
            reader.seek(std::io::SeekFrom::Start(total_size as u64))?;
            let value_size = ValueType::read_size(reader)?;
            total_size += value_size;
            reader.seek(std::io::SeekFrom::Start(total_size as u64))?;
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
#[cfg(feature="get-size2")]
mod get_size2{
    use get_size2::GetSize;

    use crate::{TagKeyType, Tags};

    impl<T: TagKeyType + GetSize> GetSize for Tags<T> {
        fn get_heap_size(&self) -> usize {
            self.0.get_heap_size()
        }
        fn get_heap_size_with_tracker<U: get_size2::GetSizeTracker>(&self, tracker: U) -> (usize, U) {
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
