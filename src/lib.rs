/*!
A binary format for objects stored on disk: content, typed metadata and tags in one file.

An object is four sections in a single file, described by a fixed 32-byte header:

```text
0                 32          tags_start      content_start
┌─────────────────┬───────────┬──────────────┬─────────────────────┐
│ ObjectHeader    │ Metadata  │ Tags         │ Content             │
│ (32 bytes)      │ + padding │ + padding    │ (content_length)    │
└─────────────────┴───────────┴──────────────┴─────────────────────┘
```

**Metadata** is system-controlled and read whenever the object is opened — a content type, an ETag, a
last-modified time. **Tags** are user-controlled and read on demand. Both are maps of string keys to
[ValueType] values, encoded identically; the difference is who writes them and when they are read.
The padding after each is what makes an edit to either cheap: as long as the new section fits, the
content never moves.

# Two layers

- [fs] — whole objects on the filesystem: [fs::TuxObject], [fs::ObjectWriter], ranged reads, in-place
  metadata edits. Start here.
- this module — the encoding of the individual pieces, for anyone embedding the format elsewhere:
  [ObjectHeader], [Tags], [MetadataMap], [ValueType], and the traits below.

```no_run
use std::io::Write;
use tux_io_encoding::MetadataMap;
use tux_io_encoding::fs::{CreateOptions, TuxObject};

let mut metadata = MetadataMap::new();
metadata.insert(http::header::CONTENT_TYPE.into(), "text/plain".to_owned().into());

let mut writer = TuxObject::create("object.tuxio", CreateOptions::new().with_metadata(metadata))?;
writer.write_all(b"Hello, world!")?;
let mut object = writer.finish()?;

assert_eq!(object.read_content_to_vec()?, b"Hello, world!");
# Ok::<(), Box<dyn std::error::Error>>(())
```

# The traits

| Trait | What it adds |
| ----- | ------------ |
| [TuxIOType] | how large the encoding is — [TuxIOType::size] |
| [WritableObjectType] | encoding a value |
| [ReadableObjectType] | decoding one, and [ReadableObjectType::read_size] to measure without decoding |
| [TypedObjectType] | a type key, so the value can go in a map where the type is not known statically |
| [ConstTypedObjectType] | the same key as a constant, so it can be matched on |
| [ReadWithSize] | decoding when the length is already known from elsewhere |

Implement [TuxIOType] plus the two directions for a new type; add [TypedObjectType] only if it needs
to be storable as a [ValueType]. **The three must agree on one number** — see the invariant documented
on [TuxIOType].

# Type keys

Every [ValueType] is written as a one-byte type key followed by the value. The keys are part of the
on-disk format and must never be reused:

| Key | Type | Rust | Encoded size (excluding the key) |
| --- | ---- | ---- | -------------------------------- |
| 0 | byte | [u8] | 1 |
| 1 | u16 | [u16] | 2 |
| 2 | u32 | [u32] | 4 |
| 3 | u64 | [u64] | 8 |
| 4 | i8 | [i8] | 1 |
| 5 | i16 | [i16] | 2 |
| 6 | i32 | [i32] | 4 |
| 7 | i64 | [i64] | 8 |
| 8 | f32 | [f32] | 4 |
| 9 | f64 | [f64] | 8 |
| 10 | boolean | [bool] | 1 |
| 11 | byte array | [Vec]`<u8>`, `[u8; N]`, `bytes::Bytes` | 2 + len |
| 12 | string | [String], [MetaKey] | 2 + len |
| 13 | date | [RawDate] | 4 |
| 14 | time | [RawTime] | 8 |
| 15 | timezone | [RawTimeZone] | 4 |
| 16 | datetime | [RawDateTime] | 16 |
| 17 | uuid | `uuid::Uuid` | 16 |

Three Rust types share key 11 on purpose: they are one wire format, so a value written as any of them
reads back as any other.

# Limits

From the format, not the implementation:

- a string or byte array is length-prefixed with a `u16`, so **65535 bytes** each
- a map's entry count is a `u16`, so **65535 entries**
- `tags_start` is a `u16`, so the metadata section must fit in the **first 64 KiB** of the file
- `content_start` is a `u32`, so everything before the content must fit in **4 GiB**

Numbers are little-endian throughout.

# Features

Nothing is on by default. `tokio` adds async readers and writers; `chrono` converts the raw date and
time types; `uuid` and `bytes` add those as value types; `zstd` and `gzip` supply content codecs (the
header can *name* a codec regardless — these are what make it readable); `get-size2` reports heap
usage.
*/

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
    /// A string field held bytes that are not valid UTF-8.
    ///
    /// Distinct from [EncodingError::UnexpectedEof], which all three string decoders used to report
    /// for this: a truncated file and a corrupt one call for different responses, and the error is the
    /// only thing that tells them apart.
    #[error("a string field is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    /// A fixed-width field held a byte outside the set its type defines.
    #[error("{type_name} cannot hold the value {byte:#04x}")]
    InvalidValue { type_name: &'static str, byte: u8 },
    /// A value's encoded length exceeds the `u16` its length prefix is stored in.
    #[error("a {type_name} of {size} bytes exceeds the {limit}-byte limit for its length prefix")]
    TypeTooLarge {
        type_name: &'static str,
        size: usize,
        limit: usize,
    },
    /// A collection holds more entries than its `u16` count can address.
    #[error("a {type_name} of {count} entries exceeds the limit of {limit}")]
    TooManyEntries {
        type_name: &'static str,
        count: usize,
        limit: usize,
    },
    /// A length prefix named a width the target type cannot hold.
    #[error("{type_name} expected {expected} bytes but the length prefix says {found}")]
    LengthMismatch {
        type_name: &'static str,
        expected: usize,
        found: usize,
    },
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
/// A type with a defined encoding in this format.
///
/// The base trait: it says how large the encoding is, which is what the file layer reserves space
/// from. [ReadableObjectType] and [WritableObjectType] add the two directions.
///
/// # The size invariant
///
/// For any value, these must all be the same number:
///
/// - [TuxIOType::size]
/// - the length of [WritableObjectType::write_to_bytes]
/// - [ReadableObjectType::read_size] over that output
/// - [TuxIOType::const_size], when it returns [Some]
///
/// "The encoding" means everything the value puts on the wire, including its own length prefix and,
/// for a [ValueType], its type-key byte. Under-reporting overruns the section the value was promised;
/// over-reporting leaves a reader positioned past whatever follows it. The `invariants` test module
/// checks this for every registered type.
pub trait TuxIOType {
    /// The encoded size, when it is the same for every value of this type.
    ///
    /// [None] means the size depends on the value — a string, a byte block, a collection.
    fn const_size(&self) -> Option<usize> {
        None
    }
    /// The encoded size of this value. See the invariant on [TuxIOType].
    fn size(&self) -> usize;
}
pub trait ReadableObjectType: TuxIOType {
    /// Measures the encoded size of the next value without decoding it.
    ///
    /// This exists so a section can be skipped, or a block measured, without allocating what is in it —
    /// which is how the file layer finds where the metadata ends without parsing every value.
    ///
    /// **The cursor is left at the end of the measured value**, not restored to where it started. Seek
    /// back yourself if you need to read what you just measured. (The documentation here used to claim
    /// the opposite; nothing implemented that, and callers depend on the advance.)
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

#[cfg(test)]
mod invariants {
    //! The one rule that binds [TuxIOType], [WritableObjectType] and [ReadableObjectType] together.
    //!
    //! For any value, these three have to agree on a single number:
    //!
    //! - [TuxIOType::size] — what the value *will* occupy
    //! - the length of [WritableObjectType::write_to_bytes] — what it *does* occupy
    //! - [ReadableObjectType::read_size] — what a reader measures it as, without decoding it
    //!
    //! Nothing enforced that before, and two types disagreed. It matters because `size()` is what the
    //! file layer reserves space from: a type reporting less than it writes overruns the section it was
    //! promised, and one reporting less than `read_size` measures leaves a reader mis-positioned for
    //! whatever follows.

    use std::io::Cursor;

    use crate::*;

    /// Asserts the three-way agreement for one value.
    #[track_caller]
    fn sizes_agree<T>(value: T)
    where
        T: TuxIOType + WritableObjectType + ReadableObjectType + std::fmt::Debug,
    {
        let encoded = value
            .write_to_bytes()
            .unwrap_or_else(|error| panic!("{value:?} failed to encode: {error}"));
        assert_eq!(
            encoded.len(),
            value.size(),
            "{value:?}: size() disagrees with the bytes written"
        );

        let measured = T::read_size(&mut Cursor::new(&encoded))
            .unwrap_or_else(|error| panic!("{value:?} failed to measure: {error}"));
        assert_eq!(
            measured,
            value.size(),
            "{value:?}: read_size() disagrees with size()"
        );

        // And whatever `const_size` claims, when it claims anything, must be that same number.
        if let Some(const_size) = value.const_size() {
            assert_eq!(
                const_size,
                value.size(),
                "{value:?}: const_size() disagrees with size()"
            );
        }
    }

    #[test]
    fn numbers_agree() {
        sizes_agree(7u8);
        sizes_agree(7u16);
        sizes_agree(7u32);
        sizes_agree(7u64);
        sizes_agree(-7i8);
        sizes_agree(-7i16);
        sizes_agree(-7i32);
        sizes_agree(-7i64);
        sizes_agree(0.5f32);
        sizes_agree(0.5f64);
        sizes_agree(true);
        sizes_agree(false);
    }

    #[test]
    fn strings_and_byte_blocks_agree() {
        sizes_agree(String::new());
        sizes_agree("hello tuxio".to_owned());
        // Multi-byte, because the length prefix counts bytes and not characters.
        sizes_agree("\u{5199}\u{771f}".to_owned());
        sizes_agree(vec![1u8, 2, 3]);
        sizes_agree(Vec::<u8>::new());
    }

    /// A fixed-size byte array is a byte block like any other, so it carries the same length prefix.
    ///
    /// `[u8; N]::size()` returned `N`, while it wrote `N + 2` and measured as `N + 2` — so reserving
    /// space from `size()` came up two bytes short for every one of them.
    #[test]
    fn fixed_size_byte_arrays_agree() {
        sizes_agree([0u8; 0]);
        sizes_agree([1u8, 2, 3, 4]);
        sizes_agree([7u8; 32]);
    }

    #[test]
    fn time_types_agree() {
        sizes_agree(RawDate {
            year: 2026,
            month: 7,
            day: 30,
        });
        sizes_agree(RawTime {
            seconds_from_midnight: 43_200,
            nanoseconds: 1,
        });
        sizes_agree(RawTimeZone { offset: -18_000 });
        sizes_agree(RawDateTime {
            date: RawDate {
                year: 2026,
                month: 7,
                day: 30,
            },
            time: RawTime {
                seconds_from_midnight: 43_200,
                nanoseconds: 1,
            },
            timezone: RawTimeZone { offset: 0 },
        });
    }

    /// A [ValueType] writes a type-key byte in front of its value, so its size has to include it.
    ///
    /// It did not, and [Tags] quietly added `+ 1` per pair to compensate — which meant the
    /// compensation was correct and the type was wrong, and any other caller of `ValueType::size()`
    /// was short by one byte per value.
    #[test]
    fn values_include_their_type_key() {
        sizes_agree(ValueType::Bool(true));
        sizes_agree(ValueType::U8(1));
        sizes_agree(ValueType::U64(u64::MAX));
        sizes_agree(ValueType::I32(-1));
        sizes_agree(ValueType::F64(0.5));
        sizes_agree(ValueType::String("hello".to_owned()));
        sizes_agree(ValueType::Bytes(vec![1, 2, 3]));
        sizes_agree(ValueType::Date(RawDate {
            year: 2026,
            month: 7,
            day: 30,
        }));

        // Concretely: one type-key byte, one length prefix of two, five bytes of content.
        assert_eq!(ValueType::String("hello".to_owned()).size(), 1 + 2 + 5);
    }

    #[test]
    fn tags_agree() {
        let mut tags = Tags::<String>::new();
        sizes_agree(tags.clone());

        tags.insert("one".to_owned(), ValueType::String("value".to_owned()));
        tags.insert("two".to_owned(), ValueType::U32(7));
        sizes_agree(tags);
    }

    #[test]
    fn a_header_agrees() {
        sizes_agree(ObjectHeader::default());
    }

    #[cfg(feature = "bytes")]
    #[test]
    fn bytes_agree() {
        sizes_agree(bytes::Bytes::from_static(b"hello tuxio"));
        sizes_agree(bytes::Bytes::new());
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuids_agree() {
        sizes_agree(uuid::Uuid::nil());
        sizes_agree(uuid::Uuid::from_u128(u128::MAX));
    }
}
