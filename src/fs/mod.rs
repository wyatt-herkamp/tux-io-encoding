//! Reading and writing complete TuxIO objects on the filesystem.
//!
//! The rest of the crate encodes the individual pieces of an object — the [crate::ObjectHeader],
//! the metadata and tag maps, the value types. This module puts them together into a single file:
//!
//! ```text
//! 0                 32          tags_start      content_start
//! ┌─────────────────┬───────────┬──────────────┬─────────────────────┐
//! │ ObjectHeader    │ Metadata  │ Tags         │ Content             │
//! │ (32 bytes)      │ + padding │ + padding    │ (content_length)    │
//! └─────────────────┴───────────┴──────────────┴─────────────────────┘
//! ```
//!
//! Writing goes through [ObjectWriter], which streams the content into space reserved by a
//! [LayoutOptions] and writes the prefix last, once the content length is known. The file is only
//! renamed into place on [ObjectWriter::finish], so readers never see a partial object.
//!
//! ```no_run
//! use std::io::Write;
//! use tux_io_encoding::MetadataMap;
//! use tux_io_encoding::fs::{CreateOptions, TuxObject};
//!
//! let mut metadata = MetadataMap::new();
//! metadata.insert(http::header::CONTENT_TYPE.into(), "text/plain".to_owned().into());
//!
//! let mut writer =
//!     TuxObject::create("object.tuxio", CreateOptions::new().with_metadata(metadata))?;
//! writer.write_all(b"Hello, world!")?;
//! let mut object = writer.finish()?;
//!
//! assert_eq!(object.read_content_to_vec()?, b"Hello, world!");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The padding after each section is what keeps metadata edits cheap: as long as the new sections
//! fit in the reserved prefix the content never moves. [TuxObject::set_metadata] and friends are
//! atomic (rewrite plus rename); [TuxObject::set_sections_in_place] trades that safety for speed.

#[cfg(feature = "tokio")]
mod async_io;
mod compression;
mod error;
mod layout;
mod object;
mod reader;
mod writer;

#[cfg(feature = "tokio")]
pub use async_io::*;
pub use compression::*;
pub use error::*;
pub use layout::*;
pub use object::*;
pub use reader::*;
pub use writer::*;

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use http::header::{CONTENT_TYPE, ETAG, LAST_MODIFIED};

    use super::*;
    use crate::{
        MetadataMap, RawDate, RawDateTime, RawTime, RawTimeZone, Tags, TuxIOType, ValueType,
    };

    /// A fixed timestamp, so the tests do not need the `chrono` feature to build one.
    fn timestamp() -> RawDateTime {
        RawDateTime {
            date: RawDate {
                year: 2026,
                month: 7,
                day: 30,
            },
            time: RawTime {
                seconds_from_midnight: 12 * 60 * 60,
                nanoseconds: 0,
            },
            timezone: RawTimeZone { offset: 0 },
        }
    }

    /// A scratch directory that removes itself, so these tests need no extra dependency.
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tux-io-encoding-fs-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
        fn join(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_metadata() -> MetadataMap {
        let mut metadata = MetadataMap::new();
        metadata.insert(CONTENT_TYPE.into(), "text/plain".to_owned().into());
        metadata.insert(LAST_MODIFIED.into(), timestamp().into());
        metadata
    }

    fn sample_tags() -> Tags {
        let mut tags = Tags::new();
        tags.insert("tag".to_owned(), "value".to_owned().into());
        tags
    }

    #[test]
    fn create_and_read_back() {
        let dir = TempDir::new("round-trip");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(
            &path,
            CreateOptions::new()
                .with_metadata(sample_metadata())
                .with_tags(sample_tags()),
        )
        .unwrap();
        writer.write_all(b"Hello, world!").unwrap();
        assert_eq!(writer.content_length(), 13);
        let object = writer.finish().unwrap();
        assert_eq!(object.content_length(), 13);
        drop(object);

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(object.content_length(), 13);
        assert_eq!(
            object
                .metadata()
                .get_header(&CONTENT_TYPE)
                .and_then(ValueType::as_str),
            Some("text/plain")
        );
        assert_eq!(object.tag_count().unwrap(), 1);
        assert_eq!(
            object.read_tags().unwrap().get("tag"),
            Some(&"value".to_owned().into())
        );
        assert_eq!(object.read_content_to_vec().unwrap(), b"Hello, world!");
    }

    #[test]
    fn writer_publishes_atomically() {
        let dir = TempDir::new("atomic");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        writer.write_all(b"partial").unwrap();
        // Nothing at the destination until `finish` renames the temp file over it.
        assert!(!path.exists());
        writer.finish().unwrap();
        assert!(path.exists());
    }

    #[test]
    fn dropping_a_writer_removes_the_temp_file() {
        let dir = TempDir::new("abandoned");
        let path = dir.join("object.tuxio");

        {
            let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
            writer.write_all(b"abandoned").unwrap();
        }

        assert!(!path.exists());
        let leftovers: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn aborting_a_writer_removes_the_temp_file() {
        let dir = TempDir::new("aborted");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        writer.write_all(b"aborted").unwrap();
        writer.abort().unwrap();

        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    }

    #[test]
    fn ranged_reads_return_slices_of_the_content() {
        let dir = TempDir::new("ranged");
        let path = dir.join("object.tuxio");

        let content = b"0123456789";
        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        writer.write_all(content).unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open(&path).unwrap();

        let mut buffer = Vec::new();
        object
            .content_range_reader(3, Some(4))
            .unwrap()
            .read_to_end(&mut buffer)
            .unwrap();
        assert_eq!(buffer, b"3456");

        // `None` reads to the end.
        buffer.clear();
        object
            .content_range_reader(7, None)
            .unwrap()
            .read_to_end(&mut buffer)
            .unwrap();
        assert_eq!(buffer, b"789");

        // A reader must not run past the content into padding or a neighbouring section.
        buffer.clear();
        object
            .content_range_reader(0, Some(10))
            .unwrap()
            .read_to_end(&mut buffer)
            .unwrap();
        assert_eq!(buffer, content);
    }

    #[test]
    fn ranged_reads_reject_out_of_bounds() {
        let dir = TempDir::new("ranged-oob");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        writer.write_all(b"0123456789").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert!(matches!(
            object.content_range_reader(11, None),
            Err(ObjectFileError::RangeOutOfBounds { .. })
        ));
        assert!(matches!(
            object.content_range_reader(5, Some(6)),
            Err(ObjectFileError::RangeOutOfBounds { .. })
        ));
    }

    #[test]
    fn metadata_can_be_written_during_streaming() {
        // The pattern the object store relies on: reserve room for a digest, stream the content
        // while hashing it, then fill the digest in before publishing.
        let dir = TempDir::new("digest");
        let path = dir.join("object.tuxio");

        let mut metadata = sample_metadata();
        metadata.insert(ETAG.into(), vec![0u8; 16].into());

        let mut writer =
            TuxObject::create(&path, CreateOptions::new().with_metadata(metadata)).unwrap();
        writer.write_all(b"hash me").unwrap();
        writer
            .metadata_mut()
            .insert(ETAG.into(), vec![7u8; 16].into());
        writer.finish().unwrap();

        let object = TuxObject::open(&path).unwrap();
        assert_eq!(
            object.metadata().get_header(&ETAG),
            Some(&ValueType::Bytes(vec![7u8; 16]))
        );
    }

    /// An atomic metadata update still copies the content, but it must not creep the layout
    /// forward — otherwise an object updated repeatedly would keep growing.
    #[test]
    fn growing_metadata_in_reserved_space_preserves_the_layout() {
        let dir = TempDir::new("grow-in-place");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(
            &path,
            CreateOptions::new()
                .with_metadata(sample_metadata())
                .with_tags(sample_tags()),
        )
        .unwrap();
        writer.write_all(b"content stays put").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open_writable(&path).unwrap();
        let original_content_start = object.header().content_start;

        object
            .modify_metadata(|metadata| {
                metadata.insert(
                    http::header::CACHE_CONTROL.into(),
                    "max-age=3600".to_owned().into(),
                );
            })
            .unwrap();

        assert_eq!(object.header().content_start, original_content_start);
        assert_eq!(object.read_content_to_vec().unwrap(), b"content stays put");
        assert_eq!(
            object
                .metadata()
                .get_header(&http::header::CACHE_CONTROL)
                .and_then(ValueType::as_str),
            Some("max-age=3600")
        );
        // The tags survived a metadata-only update.
        assert_eq!(
            object.read_tags().unwrap().get("tag"),
            Some(&"value".to_owned().into())
        );
    }

    #[test]
    fn metadata_beyond_the_reserve_triggers_a_rewrite() {
        let dir = TempDir::new("grow-rewrite");
        let path = dir.join("object.tuxio");

        // A packed layout leaves no slack, so any growth has to move the content.
        let mut writer = TuxObject::create(
            &path,
            CreateOptions::new()
                .with_metadata(sample_metadata())
                .with_tags(sample_tags())
                .with_layout(LayoutOptions::packed()),
        )
        .unwrap();
        writer.write_all(b"content moves").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open_writable(&path).unwrap();
        let original_content_start = object.header().content_start;

        object
            .modify_metadata(|metadata| {
                metadata.insert(
                    http::header::CONTENT_DISPOSITION.into(),
                    "attachment; filename=\"a-fairly-long-file-name.txt\""
                        .to_owned()
                        .into(),
                );
            })
            .unwrap();

        assert!(object.header().content_start > original_content_start);
        assert_eq!(object.read_content_to_vec().unwrap(), b"content moves");
        assert_eq!(object.content_length(), 13);
        assert_eq!(
            object.read_tags().unwrap().get("tag"),
            Some(&"value".to_owned().into())
        );
    }

    #[test]
    fn tags_can_shrink() {
        // Shrinking is what the older store implementation rejected outright.
        let dir = TempDir::new("shrink");
        let path = dir.join("object.tuxio");

        let mut tags = Tags::new();
        for index in 0..8 {
            tags.insert(format!("tag-{index}"), format!("value-{index}").into());
        }

        let mut writer = TuxObject::create(&path, CreateOptions::new().with_tags(tags)).unwrap();
        writer.write_all(b"shrink my tags").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open_writable(&path).unwrap();
        assert_eq!(object.tag_count().unwrap(), 8);

        object.set_tags(Tags::new()).unwrap();
        assert_eq!(object.tag_count().unwrap(), 0);
        assert!(object.read_tags().unwrap().is_empty());
        assert_eq!(object.read_content_to_vec().unwrap(), b"shrink my tags");
    }

    #[test]
    fn in_place_update_rejects_oversized_sections() {
        let dir = TempDir::new("in-place");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(
            &path,
            CreateOptions::new()
                .with_metadata(sample_metadata())
                .with_layout(LayoutOptions::packed()),
        )
        .unwrap();
        writer.write_all(b"do not move me").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open_writable(&path).unwrap();
        let mut metadata = object.metadata().clone();
        metadata.insert(
            http::header::CONTENT_DISPOSITION.into(),
            "attachment; filename=\"much-too-long-for-a-packed-layout.txt\""
                .to_owned()
                .into(),
        );

        assert!(matches!(
            object.set_sections_in_place(metadata, Tags::new()),
            Err(ObjectFileError::ReservedSpaceExceeded { .. })
        ));
        // The rejected update left the object untouched.
        assert_eq!(object.read_content_to_vec().unwrap(), b"do not move me");
    }

    #[test]
    fn in_place_update_succeeds_within_the_reserve() {
        let dir = TempDir::new("in-place-ok");
        let path = dir.join("object.tuxio");

        let mut writer =
            TuxObject::create(&path, CreateOptions::new().with_metadata(sample_metadata()))
                .unwrap();
        writer.write_all(b"still here").unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open_writable(&path).unwrap();
        let content_start = object.header().content_start;
        let mut metadata = object.metadata().clone();
        metadata.insert(ETAG.into(), vec![1u8; 16].into());

        object
            .set_sections_in_place(metadata, sample_tags())
            .unwrap();

        assert_eq!(object.header().content_start, content_start);
        assert_eq!(object.read_content_to_vec().unwrap(), b"still here");
        assert_eq!(
            object.read_tags().unwrap().get("tag"),
            Some(&"value".to_owned().into())
        );
        assert_eq!(
            object.metadata().get_header(&ETAG),
            Some(&ValueType::Bytes(vec![1u8; 16]))
        );
    }

    #[test]
    fn read_only_objects_reject_updates() {
        let dir = TempDir::new("read-only");
        let path = dir.join("object.tuxio");

        TuxObject::create(&path, CreateOptions::new())
            .unwrap()
            .finish()
            .unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert!(matches!(
            object.set_tags(sample_tags()),
            Err(ObjectFileError::ReadOnly(_))
        ));
    }

    #[test]
    fn open_optional_reports_a_missing_file_without_creating_it() {
        let dir = TempDir::new("missing");
        let path = dir.join("nested/does-not-exist.tuxio");

        assert!(TuxObject::open_optional(&path).unwrap().is_none());
        assert!(!path.exists());
        assert!(!dir.join("nested").exists());
    }

    #[test]
    fn empty_object_round_trips() {
        let dir = TempDir::new("empty");
        let path = dir.join("object.tuxio");

        TuxObject::create(&path, CreateOptions::new())
            .unwrap()
            .finish()
            .unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(object.content_length(), 0);
        assert!(object.metadata().is_empty());
        assert_eq!(object.tag_count().unwrap(), 0);
        assert!(object.read_content_to_vec().unwrap().is_empty());
    }

    #[test]
    fn larger_content_round_trips() {
        let dir = TempDir::new("large");
        let path = dir.join("object.tuxio");

        let content: Vec<u8> = (0..(512 * 1024)).map(|index| (index % 251) as u8).collect();
        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        writer.write_all(&content).unwrap();
        writer.finish().unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(object.content_length(), content.len() as u64);
        assert_eq!(object.read_content_to_vec().unwrap(), content);
    }

    #[test]
    fn find_tag_locates_a_single_entry() {
        let dir = TempDir::new("find-tag");
        let path = dir.join("object.tuxio");

        let mut tags = Tags::new();
        tags.insert("first".to_owned(), "one".to_owned().into());
        tags.insert("second".to_owned(), "two".to_owned().into());
        tags.insert("third".to_owned(), 3u32.into());

        TuxObject::create(&path, CreateOptions::new().with_tags(tags))
            .unwrap()
            .finish()
            .unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(
            object.find_tag("second").unwrap(),
            Some("two".to_owned().into())
        );
        assert_eq!(object.find_tag("third").unwrap(), Some(3u32.into()));
        assert_eq!(object.find_tag("absent").unwrap(), None);
    }

    #[test]
    fn compressed_objects_reject_raw_writes() {
        let dir = TempDir::new("raw-write-guard");
        let path = dir.join("object.tuxio");

        let compression =
            crate::CompressionTypes::ZSTD(crate::compression_types::ZStdCompressionType(3));
        if !is_supported(compression) {
            // Without the codec feature, creating the writer is what fails.
            assert!(matches!(
                TuxObject::create(&path, CreateOptions::new().with_compression(compression)),
                Err(ObjectFileError::UnsupportedCompression(_))
            ));
            return;
        }

        let mut writer =
            TuxObject::create(&path, CreateOptions::new().with_compression(compression)).unwrap();
        // Writing raw bytes would store uncompressed content under a header claiming zstd.
        assert!(writer.write_all(b"raw bytes").is_err());
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn zstd_content_round_trips() {
        let dir = TempDir::new("zstd");
        let path = dir.join("object.tuxio");

        // Repetitive content, so the compressed form is definitely smaller.
        let content = b"tuxio ".repeat(4096);
        let compression =
            crate::CompressionTypes::ZSTD(crate::compression_types::ZStdCompressionType(3));

        let mut writer = TuxObject::create(
            &path,
            CreateOptions::new()
                .with_metadata(sample_metadata())
                .with_compression(compression),
        )
        .unwrap();
        let mut encoder = writer.content_encoder().unwrap();
        encoder.write_all(&content).unwrap();
        let uncompressed = encoder.finish().unwrap();
        assert_eq!(uncompressed, content.len() as u64);
        let object = writer.finish().unwrap();

        // The header records the stored (compressed) length.
        assert!(object.content_length() < content.len() as u64);
        drop(object);

        let mut object = TuxObject::open(&path).unwrap();
        assert!(object.is_compressed());
        assert_eq!(
            object.metadata().get_header(&UNCOMPRESSED_LENGTH),
            Some(&ValueType::U64(content.len() as u64))
        );
        assert_eq!(object.read_content_to_vec().unwrap(), content);

        // A byte offset into compressed storage is meaningless, so ranged reads are refused rather
        // than silently returning the wrong bytes.
        assert!(matches!(
            object.content_range_reader(0, Some(8)),
            Err(ObjectFileError::RangedReadOnCompressed)
        ));
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn gzip_content_round_trips() {
        let dir = TempDir::new("gzip");
        let path = dir.join("object.tuxio");

        let content = b"tuxio ".repeat(4096);
        let compression =
            crate::CompressionTypes::Gzip(crate::compression_types::GzipCompressionType(6));

        let mut writer =
            TuxObject::create(&path, CreateOptions::new().with_compression(compression)).unwrap();
        let mut encoder = writer.content_encoder().unwrap();
        encoder.write_all(&content).unwrap();
        encoder.finish().unwrap();
        let object = writer.finish().unwrap();
        assert!(object.content_length() < content.len() as u64);
        drop(object);

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(object.read_content_to_vec().unwrap(), content);
    }

    #[test]
    fn uncompressed_encoder_is_a_pass_through() {
        let dir = TempDir::new("passthrough");
        let path = dir.join("object.tuxio");

        let mut writer = TuxObject::create(&path, CreateOptions::new()).unwrap();
        let mut encoder = writer.content_encoder().unwrap();
        encoder.write_all(b"straight through").unwrap();
        assert_eq!(encoder.finish().unwrap(), 16);
        writer.finish().unwrap();

        let mut object = TuxObject::open(&path).unwrap();
        assert_eq!(object.read_content_to_vec().unwrap(), b"straight through");
        // Nothing to record when the stored length already matches.
        assert!(object.metadata().get_header(&UNCOMPRESSED_LENGTH).is_none());
    }

    #[test]
    fn padding_is_zero_filled() {
        // Padding must be deterministic rather than whatever the filesystem handed back, so two
        // objects built the same way agree byte for byte outside the map sections.
        let dir = TempDir::new("deterministic");

        let build = |name: &str| {
            let path = dir.join(name);
            let mut writer = TuxObject::create(
                &path,
                CreateOptions::new()
                    .with_metadata(sample_metadata())
                    .with_tags(sample_tags()),
            )
            .unwrap();
            writer.write_all(b"same bytes").unwrap();
            writer.finish().unwrap();
            std::fs::read(&path).unwrap()
        };

        let first = build("a.tuxio");
        let second = build("b.tuxio");
        assert_eq!(first.len(), second.len());

        let header =
            <crate::ObjectHeader as crate::ReadableObjectType>::read_from_bytes(&first[..32])
                .unwrap();
        // The header and everything from the content onwards are byte identical. The map sections
        // themselves are `HashMap` backed, so entry order is not stable.
        assert_eq!(&first[..32], &second[..32]);
        assert_eq!(
            &first[header.content_start as usize..],
            &second[header.content_start as usize..]
        );

        // The tail of each section is zero padding.
        let tags_end = 32 + sample_metadata().size();
        assert!(
            first[tags_end..header.tags_start as usize]
                .iter()
                .all(|byte| *byte == 0)
        );
    }
}
