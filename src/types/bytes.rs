use std::io::Read;

use bytes::{Bytes, BytesMut};

use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType, typed_object_type, types::is_size_allowed,
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
        is_size_allowed(self.len())?;
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
        let mut writer = BytesMut::with_capacity(length);
        reader.read_exact(&mut writer)?;
        Ok(writer.freeze())
    }
}
