//! Async counterparts to the encoding traits, for `tokio`.
//!
//! Currently the async writing is unstable and may change.
//!
//! # Which types have these
//!
//! Everything that can appear in a metadata or tag block: the numeric types, [crate::ObjectHeader],
//! [String], [crate::ValueType], [crate::Tags] (so [crate::MetadataMap] too), the raw date and time
//! types, and — under their features — `Uuid` and `Bytes`.
//!
//! # Why reading is not incremental
//!
//! [AsyncReadableObjectType::read_from_async_reader] for a composite type reads its bytes into a buffer
//! and then parses them with the synchronous codec, rather than awaiting field by field. That is
//! deliberate: a header is 32 bytes and a metadata block is bounded to 64 KiB by the format, so one
//! `read_exact` is both simpler and fewer syscalls than a dozen awaits. Only the *content* section is
//! large, and that streams through [tokio::io] without going through these traits at all.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{EncodingError, ReadableObjectType, WritableObjectType};

/// An asynchronous version of [WritableObjectType].
pub trait AsyncWritableObjectType: WritableObjectType {
    /// Asynchronously writes the object to a writer.
    ///
    /// The default implementation encodes to a buffer and writes that, which is the right shape for
    /// everything here — see the note on incremental reading in the module documentation.
    fn write_to_async_writer<W>(
        &self,
        writer: &mut W,
    ) -> impl Future<Output = Result<(), EncodingError>> + Send
    where
        Self: Sync,
        W: AsyncWrite + Unpin + Send,
    {
        async move {
            let result = self.write_to_bytes()?;
            writer
                .write_all(&result)
                .await
                .map_err(EncodingError::IOError)?;
            Ok(())
        }
    }
}

/// An asynchronous version of [ReadableObjectType].
pub trait AsyncReadableObjectType: ReadableObjectType {
    /// Asynchronously reads the object from a reader.
    fn read_from_async_reader<R>(
        reader: &mut R,
    ) -> impl Future<Output = Result<Self, EncodingError>> + Send
    where
        Self: Sync + Sized,
        R: AsyncRead + Unpin + Send;
}

/// Reads exactly `length` bytes, for a type whose length prefix has already been read.
pub(crate) async fn read_exact_vec<R>(
    reader: &mut R,
    length: usize,
) -> Result<Vec<u8>, EncodingError>
where
    R: AsyncRead + Unpin + Send,
{
    let mut buffer = vec![0u8; length];
    reader
        .read_exact(&mut buffer)
        .await
        .map_err(EncodingError::IOError)?;
    Ok(buffer)
}

/// Reads a `u16` length prefix and then that many bytes.
///
/// The shape every variable-width type in this format shares.
pub(crate) async fn read_length_prefixed<R>(reader: &mut R) -> Result<Vec<u8>, EncodingError>
where
    R: AsyncRead + Unpin + Send,
{
    let length = reader.read_u16_le().await.map_err(EncodingError::IOError)? as usize;
    read_exact_vec(reader, length).await
}

#[cfg(test)]
mod tests {
    //! The async and synchronous codecs must be interchangeable.
    //!
    //! Two implementations of one wire format is the setup that drifts. These assert the property that
    //! matters: whatever the sync writer produces, the async reader reads back — and the other way round.
    //! Before this, the async traits covered only the numeric types, the header and `Uuid`, so a metadata
    //! block could not be read asynchronously at all.

    use std::io::Cursor;

    use super::*;
    use crate::*;

    /// Sync-written bytes read back by the async reader, and vice versa.
    async fn interchangeable<T>(value: T)
    where
        T: AsyncReadableObjectType
            + AsyncWritableObjectType
            + PartialEq
            + std::fmt::Debug
            + Sync
            + Send,
    {
        let sync_encoded = value.write_to_bytes().unwrap();

        let decoded_async = T::read_from_async_reader(&mut Cursor::new(sync_encoded.clone()))
            .await
            .unwrap_or_else(|error| panic!("{value:?}: async read of sync bytes failed: {error}"));
        assert_eq!(
            decoded_async, value,
            "async reader disagreed with sync writer"
        );

        let mut async_encoded = Vec::new();
        value
            .write_to_async_writer(&mut async_encoded)
            .await
            .unwrap();
        assert_eq!(
            async_encoded, sync_encoded,
            "{value:?}: the two writers produced different bytes"
        );

        let decoded_sync = T::read_from_reader(&mut Cursor::new(&async_encoded)).unwrap();
        assert_eq!(
            decoded_sync, value,
            "sync reader disagreed with async writer"
        );
    }

    #[tokio::test]
    async fn numbers_are_interchangeable() {
        interchangeable(7u8).await;
        interchangeable(1234u16).await;
        interchangeable(u32::MAX).await;
        interchangeable(u64::MAX).await;
        interchangeable(-7i8).await;
        interchangeable(i64::MIN).await;
        interchangeable(0.5f32).await;
        interchangeable(0.5f64).await;
    }

    #[tokio::test]
    async fn strings_and_byte_blocks_are_interchangeable() {
        interchangeable(String::new()).await;
        interchangeable("hello tuxio".to_owned()).await;
        // Multi-byte, because the length prefix counts bytes rather than characters.
        interchangeable("\u{5199}\u{771f}".to_owned()).await;
        interchangeable(vec![1u8, 2, 3]).await;
        interchangeable(Vec::<u8>::new()).await;
    }

    #[tokio::test]
    async fn the_header_is_interchangeable() {
        interchangeable(ObjectHeader {
            tags_start: 288,
            content_start: 544,
            content_length: 1234,
            ..Default::default()
        })
        .await;
    }

    #[tokio::test]
    async fn time_types_are_interchangeable() {
        let date = RawDate {
            year: 2026,
            month: 7,
            day: 30,
        };
        let time = RawTime {
            seconds_from_midnight: 43_200,
            nanoseconds: 1,
        };
        let timezone = RawTimeZone { offset: -18_000 };
        interchangeable(date).await;
        interchangeable(time).await;
        interchangeable(timezone).await;
        interchangeable(RawDateTime {
            date,
            time,
            timezone,
        })
        .await;
    }

    /// Every [ValueType] variant, since the async reader dispatches on the type key by hand and a
    /// missing arm would only show up for that one variant.
    #[tokio::test]
    async fn every_value_variant_is_interchangeable() {
        interchangeable(ValueType::Bool(true)).await;
        interchangeable(ValueType::Bool(false)).await;
        interchangeable(ValueType::U8(1)).await;
        interchangeable(ValueType::U16(2)).await;
        interchangeable(ValueType::U32(3)).await;
        interchangeable(ValueType::U64(4)).await;
        interchangeable(ValueType::I8(-1)).await;
        interchangeable(ValueType::I16(-2)).await;
        interchangeable(ValueType::I32(-3)).await;
        interchangeable(ValueType::I64(-4)).await;
        interchangeable(ValueType::F32(0.5)).await;
        interchangeable(ValueType::F64(0.25)).await;
        interchangeable(ValueType::String("value".to_owned())).await;
        interchangeable(ValueType::Bytes(vec![1, 2, 3])).await;
        interchangeable(ValueType::Date(RawDate {
            year: 2026,
            month: 7,
            day: 30,
        }))
        .await;
        interchangeable(ValueType::Time(RawTime {
            seconds_from_midnight: 1,
            nanoseconds: 2,
        }))
        .await;
        interchangeable(ValueType::RawDateTime(RawDateTime {
            date: RawDate {
                year: 2026,
                month: 7,
                day: 30,
            },
            time: RawTime {
                seconds_from_midnight: 1,
                nanoseconds: 2,
            },
            timezone: RawTimeZone { offset: 0 },
        }))
        .await;
        #[cfg(feature = "uuid")]
        interchangeable(ValueType::Uuid(uuid::Uuid::from_u128(42))).await;
    }

    /// A tag block, which is what makes a metadata section readable asynchronously.
    #[tokio::test]
    async fn a_tag_block_is_interchangeable() {
        interchangeable(Tags::<String>::new()).await;

        let mut tags = Tags::<String>::new();
        tags.insert("one".to_owned(), ValueType::String("value".to_owned()));
        tags.insert("two".to_owned(), ValueType::U32(7));
        tags.insert("three".to_owned(), ValueType::Bool(true));
        interchangeable(tags).await;
    }

    /// An unknown type key is reported as such rather than as a short read.
    #[tokio::test]
    async fn an_unknown_type_key_is_rejected() {
        let error = ValueType::read_from_async_reader(&mut Cursor::new(vec![250u8, 0, 0]))
            .await
            .expect_err("250 is not a registered type key");
        assert!(
            matches!(error, EncodingError::UnknownTypeKey(250)),
            "got {error:?}"
        );
    }

    #[cfg(feature = "bytes")]
    #[tokio::test]
    async fn bytes_are_interchangeable() {
        interchangeable(bytes::Bytes::from_static(b"hello")).await;
        interchangeable(bytes::Bytes::new()).await;
    }

    #[cfg(feature = "uuid")]
    #[tokio::test]
    async fn uuids_are_interchangeable() {
        interchangeable(uuid::Uuid::nil()).await;
        interchangeable(uuid::Uuid::from_u128(u128::MAX)).await;
    }
}
