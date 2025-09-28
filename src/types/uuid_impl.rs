use std::io::{Read, SeekFrom};

use uuid::Uuid;

use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType, typed_object_type,
};

impl TuxIOType for Uuid {
    fn const_size(&self) -> Option<usize> {
        Some(16)
    }
    fn size(&self) -> usize {
        16
    }
}
typed_object_type!(
    Uuid => 17
);
impl ReadableObjectType for Uuid {
    fn read_size<R: std::io::Read + std::io::Seek>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(16)
    }

    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf)?;
        Ok(Uuid::from_bytes(buf))
    }
    fn skip<R: Read + std::io::Seek>(reader: &mut R) -> Result<(), EncodingError>
    where
        Self: Sized,
    {
        reader.seek(SeekFrom::Current(16))?;
        Ok(())
    }
}
impl WritableObjectType for Uuid {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}
