pub mod compression_types;
pub mod fs;
mod header;
mod tags;
#[cfg(feature = "tokio")]
pub mod tokio_io;
mod types;
mod value;
use std::io::{Read, Seek, SeekFrom};

pub use compression_types::CompressionTypes;
pub use header::*;
pub use tags::*;
pub use types::{RawDate, RawDateTime, RawTime, RawTimeZone};

pub use value::*;

#[cfg(feature = "chrono")]
pub use types::chrono_impl::ChronoError;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSections {
    /// Object Header Start
    Header,
    /// Object Metadata Start
    ///
    /// Always 32 bytes
    Metadata,
    /// Object Tags Start
    Tags,
    /// Object Content Start
    Content,
}

pub trait SeekWithHeader {
    fn seek_to_section_with_header(
        &mut self,
        section: FileSections,
        header: &ObjectHeader,
    ) -> Result<(), EncodingError>;
    /// Reads the header and seeks to the specified section.
    fn seek_to_section(&mut self, section: FileSections) -> Result<(), EncodingError>;
}
impl<T: Seek + Read> SeekWithHeader for T {
    fn seek_to_section_with_header(
        &mut self,
        section: FileSections,
        header: &ObjectHeader,
    ) -> Result<(), EncodingError> {
        let seek_from = header.seek(section);
        self.seek(seek_from).map_err(EncodingError::IOError)?;
        Ok(())
    }
    fn seek_to_section(&mut self, section: FileSections) -> Result<(), EncodingError> {
        match section {
            FileSections::Header => {
                self.seek(SeekFrom::Start(0))?;
                Ok(())
            }
            FileSections::Metadata => {
                self.seek(SeekFrom::Start(32))?;
                Ok(())
            }
            other => {
                self.seek(SeekFrom::Start(0))?;
                let header = ObjectHeader::read_from_reader(self)?;
                self.seek_to_section_with_header(other, &header)
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncodingError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error("Invalid object header magic value")]
    InvalidMagic,
    #[error("Invalid object header version {0}")]
    UnsupportedVersion(u8),
    #[error("Invalid object header compression type {0}")]
    InvalidCompressionType(u8),
    #[error("Unexpected End Of Buffer")]
    UnexpectedEof,
    #[error("Unknown type key {0}")]
    UnknownTypeKey(u8),
    #[error("Type to Large {0} bytes limit to u16::MAX")]
    TypeTooLarge(usize),
    #[error("{0}")]
    OtherDecodingError(Box<dyn std::error::Error + Send + Sync>),

    #[error("Mismatched Object Type expected {0}, found {1}")]
    MismatchedObjectType(u8, u8),
}
impl EncodingError {
    pub fn other<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        EncodingError::OtherDecodingError(Box::new(err))
    }
}
/// A Type Supported by TuxIOEncoding
pub trait TuxIOType {
    /// If the type is fixed size, return the size.
    fn const_size(&self) -> Option<usize> {
        None
    }
    /// Calculates the size of the object.
    fn size(&self) -> usize;
}
pub trait ReadableObjectType: TuxIOType {
    /// Reads the size of the objects from the reader.
    ///
    /// This might involves reading from the reader to determine the size.
    /// But the reader cursor will be returned to the original position.
    fn read_size<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<usize, EncodingError>;
    /// Reads the object from bytes.
    #[inline(always)]
    fn read_from_bytes(bytes: &[u8]) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut reader = std::io::Cursor::new(bytes);
        Self::read_from_reader(&mut reader)
    }

    /// Reads the object from the reader.
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized;
    /// Skips the object type
    ///
    /// Default implementation reads the object and discards it.
    #[inline(always)]
    fn skip<R: Read + Seek>(reader: &mut R) -> Result<(), EncodingError>
    where
        Self: Sized,
    {
        Self::read_from_reader(reader).map(|_| ())
    }
}
pub trait ReadWithSize: ReadableObjectType {
    type Size;

    fn read_with_size<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        size: Self::Size,
    ) -> Result<Self, EncodingError>
    where
        Self: Sized;
}
pub trait WritableObjectType: TuxIOType {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError>;

    fn write_to_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        let mut buffer = Vec::with_capacity(self.size());
        self.write_to_writer(&mut buffer)?;
        Ok(buffer)
    }
}
/// A typed Object is an object that has a specific type key. Used for Map Types
pub trait TypedObjectType: TuxIOType + ReadableObjectType + WritableObjectType {
    fn type_key() -> u8;

    fn write_with_type<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[Self::type_key()])?;
        self.write_to_writer(writer)
    }
}
pub trait ConstTypedObjectType: TuxIOType {
    const TYPE_KEY: u8;
}
macro_rules! typed_object_type {
    ($type:ty => $type_key:literal) => {
        impl TypedObjectType for $type {
            fn type_key() -> u8 {
                $type_key
            }
        }
        impl ConstTypedObjectType for $type {
            const TYPE_KEY: u8 = $type_key;
        }
    };
    () => {};
}
pub(crate) use typed_object_type;
