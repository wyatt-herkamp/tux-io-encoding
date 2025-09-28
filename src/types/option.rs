use std::io::Seek;

use crate::{EncodingError, ReadableObjectType, TuxIOType, WritableObjectType};

impl<T: TuxIOType> TuxIOType for Option<T> {
    fn const_size(&self) -> Option<usize> {
        None
    }
    fn size(&self) -> usize {
        match self {
            Some(inner) => 1 + inner.size(),
            None => 1,
        }
    }
}

impl<T: TuxIOType + ReadableObjectType> ReadableObjectType for Option<T> {
    fn read_size<R: std::io::Read + Seek>(reader: &mut R) -> Result<usize, EncodingError> {
        Ok(1 + T::read_size(reader)?)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let has_value = u8::read_from_reader(reader)? != 0;
        if has_value {
            Ok(Some(T::read_from_reader(reader)?))
        } else {
            Ok(None)
        }
    }
}
impl<T: TuxIOType + WritableObjectType> WritableObjectType for Option<T> {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        match self {
            Some(inner) => {
                writer.write_all(&[1])?;
                inner.write_to_writer(writer)?;
            }
            None => {
                writer.write_all(&[0])?;
            }
        }
        Ok(())
    }
}
