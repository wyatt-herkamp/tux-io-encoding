use std::{collections::HashMap, hash::Hash, io::SeekFrom};

use crate::{EncodingError, ReadableObjectType, TuxIOType, TypedObjectType, WritableObjectType};

impl<K, V> TuxIOType for HashMap<K, V>
where
    K: TuxIOType + TypedObjectType + std::hash::Hash + Eq,
    V: TuxIOType + TypedObjectType,
{
    fn size(&self) -> usize {
        // 2 for length, 1 for key type, 1 for value type
        let mut base_size = 2 + 1 + 1;
        for (k, v) in self {
            base_size += k.size() + v.size();
        }
        base_size
    }
}

impl<K, V> ReadableObjectType for HashMap<K, V>
where
    K: TuxIOType + TypedObjectType + std::hash::Hash + Eq + ReadableObjectType,
    V: TuxIOType + TypedObjectType + ReadableObjectType,
{
    fn read_size<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<usize, EncodingError> {
        let mut base_size = 2 + 1 + 1;
        let length = u16::read_from_reader(reader)? as usize;
        reader.seek(SeekFrom::Current(2))?;
        for _ in 0..length {
            base_size += K::read_size(reader)? + V::read_size(reader)?;
        }
        Ok(base_size)
    }

    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let read_length = u16::read_from_reader(reader)?;
        let key_type = u8::read_from_reader(reader)?;
        let value_type = u8::read_from_reader(reader)?;
        if key_type != K::type_key() {
            return Err(EncodingError::MismatchedObjectType(K::type_key(), key_type));
        }
        if value_type != V::type_key() {
            return Err(EncodingError::MismatchedObjectType(
                V::type_key(),
                value_type,
            ));
        }

        let mut map = HashMap::with_capacity(read_length as usize);
        for _ in 0..read_length {
            let key = K::read_from_reader(reader)?;
            let value = V::read_from_reader(reader)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<K, V> WritableObjectType for HashMap<K, V>
where
    K: TuxIOType + TypedObjectType + Hash + Eq + WritableObjectType,
    V: TuxIOType + TypedObjectType + WritableObjectType,
{
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        let length = self.len() as u16;
        length.write_to_writer(writer)?;
        K::type_key().write_to_writer(writer)?;
        V::type_key().write_to_writer(writer)?;
        for (k, v) in self {
            k.write_to_writer(writer)?;
            v.write_to_writer(writer)?;
        }
        Ok(())
    }
}
