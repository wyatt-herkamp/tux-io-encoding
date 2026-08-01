use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    CompressionTypes, MetadataMap, ObjectHeader, ReadableObjectType, Tags, TuxIOType, ValueType,
    fs::{
        ContentReader, CreateOptions, DEFAULT_ALIGNMENT, DecodedContentReader, HEADER_SIZE,
        LayoutOptions, ObjectFileError, ObjectFileResult, ObjectWriter, SectionLayout,
        writer::encode_prefix,
    },
};

/// A complete TuxIO object on the filesystem.
///
/// Opening reads the header and the metadata section eagerly (both are small and almost always
/// wanted); tags and content are read on demand.
#[derive(Debug)]
pub struct TuxObject {
    file: File,
    path: PathBuf,
    header: ObjectHeader,
    metadata: MetadataMap,
    writable: bool,
}

impl TuxObject {
    /// Opens an object for reading.
    pub fn open(path: impl Into<PathBuf>) -> ObjectFileResult<Self> {
        let path = path.into();
        let file = OpenOptions::new().read(true).open(&path)?;
        Self::from_file(file, path, false)
    }

    /// Opens an object for reading and writing, so the metadata and tag sections can be updated.
    pub fn open_writable(path: impl Into<PathBuf>) -> ObjectFileResult<Self> {
        let path = path.into();
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        Self::from_file(file, path, true)
    }

    /// Opens an object, returning `None` when the file does not exist.
    ///
    /// Unlike a plain `open`, this never creates anything as a side effect of a missing path.
    pub fn open_optional(path: impl Into<PathBuf>) -> ObjectFileResult<Option<Self>> {
        match Self::open(path) {
            Ok(object) => Ok(Some(object)),
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Starts writing a new object at `path`.
    ///
    /// The parent directory must already exist.
    pub fn create(
        path: impl Into<PathBuf>,
        options: CreateOptions,
    ) -> ObjectFileResult<ObjectWriter> {
        ObjectWriter::create(path, options)
    }

    pub(crate) fn from_parts(
        file: File,
        path: PathBuf,
        header: ObjectHeader,
        metadata: MetadataMap,
        writable: bool,
    ) -> Self {
        Self {
            file,
            path,
            header,
            metadata,
            writable,
        }
    }

    fn from_file(mut file: File, path: PathBuf, writable: bool) -> ObjectFileResult<Self> {
        file.seek(SeekFrom::Start(0))?;

        let mut header_bytes = [0u8; HEADER_SIZE];
        file.read_exact(&mut header_bytes)?;
        let header = <ObjectHeader as ReadableObjectType>::read_from_bytes(&header_bytes)?;

        // One read for the whole metadata section beats many small reads straight off the file.
        let metadata_space = (header.tags_start as usize).saturating_sub(HEADER_SIZE);
        let metadata = if metadata_space == 0 {
            MetadataMap::new()
        } else {
            let mut buffer = vec![0u8; metadata_space];
            file.read_exact(&mut buffer)?;
            MetadataMap::read_from_reader(&mut Cursor::new(&buffer))?
        };

        Ok(Self {
            file,
            path,
            header,
            metadata,
            writable,
        })
    }

    pub fn header(&self) -> &ObjectHeader {
        &self.header
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }
    /// Length of the stored content, which for a compressed object is the compressed length.
    pub fn content_length(&self) -> u64 {
        self.header.content_length
    }
    pub fn compression(&self) -> CompressionTypes {
        self.header.compression_type
    }
    pub fn is_compressed(&self) -> bool {
        !matches!(self.header.compression_type, CompressionTypes::None(_))
    }
    pub fn layout(&self) -> SectionLayout {
        SectionLayout {
            tags_start: self.header.tags_start,
            content_start: self.header.content_start,
        }
    }
    /// Total size of the file on disk, including the prefix and any padding.
    pub fn file_size(&self) -> ObjectFileResult<u64> {
        Ok(self.file.metadata()?.len())
    }

    // -- tags ----------------------------------------------------------------------------------

    /// Reads the whole tag section.
    pub fn read_tags(&mut self) -> ObjectFileResult<Tags> {
        let space = self.layout().tags_space();
        if space == 0 {
            return Ok(Tags::new());
        }
        self.file
            .seek(SeekFrom::Start(self.header.tags_start as u64))?;
        let mut buffer = vec![0u8; space];
        self.file.read_exact(&mut buffer)?;
        Ok(Tags::read_from_reader(&mut Cursor::new(&buffer))?)
    }

    /// Number of tags without decoding their values.
    pub fn tag_count(&mut self) -> ObjectFileResult<u16> {
        if self.layout().tags_space() == 0 {
            return Ok(0);
        }
        self.file
            .seek(SeekFrom::Start(self.header.tags_start as u64))?;
        let mut buffer = [0u8; 2];
        self.file.read_exact(&mut buffer)?;
        Ok(u16::from_le_bytes(buffer))
    }

    /// Looks up a single tag, skipping over the values it does not need.
    pub fn find_tag(&mut self, key: &str) -> ObjectFileResult<Option<ValueType>> {
        let space = self.layout().tags_space();
        if space == 0 {
            return Ok(None);
        }
        self.file
            .seek(SeekFrom::Start(self.header.tags_start as u64))?;
        let mut buffer = vec![0u8; space];
        self.file.read_exact(&mut buffer)?;
        let mut cursor = Cursor::new(&buffer);
        Ok(Tags::find_from_reader(&mut cursor, &key.to_owned())?)
    }

    // -- section updates -----------------------------------------------------------------------

    /// Replaces the metadata section, keeping the tags as they are.
    ///
    /// Atomic: the object is rewritten to a temporary file and renamed into place, so a crash
    /// mid-update leaves the previous version intact.
    pub fn set_metadata(&mut self, metadata: MetadataMap) -> ObjectFileResult<()> {
        let tags = self.read_tags()?;
        self.rewrite(metadata, tags)
    }

    /// Replaces the tag section, keeping the metadata as it is. Atomic, as with
    /// [TuxObject::set_metadata].
    pub fn set_tags(&mut self, tags: Tags) -> ObjectFileResult<()> {
        let metadata = self.metadata.clone();
        self.rewrite(metadata, tags)
    }

    /// Replaces both sections at once. Atomic.
    pub fn set_sections(&mut self, metadata: MetadataMap, tags: Tags) -> ObjectFileResult<()> {
        self.rewrite(metadata, tags)
    }

    /// Applies a change to the metadata and persists it atomically.
    pub fn modify_metadata<F>(&mut self, modify: F) -> ObjectFileResult<()>
    where
        F: FnOnce(&mut MetadataMap),
    {
        let mut metadata = self.metadata.clone();
        modify(&mut metadata);
        self.set_metadata(metadata)
    }

    /// Overwrites the metadata and tag sections in place, without touching the content.
    ///
    /// Much cheaper than [TuxObject::set_metadata] because the content never moves, but **not**
    /// crash safe: a torn write can leave the prefix inconsistent. Use it for data you can rebuild
    /// (a cache entry, say), and prefer the atomic setters for anything authoritative.
    ///
    /// Returns [ObjectFileError::ReservedSpaceExceeded] when the new sections do not fit in the
    /// space already reserved — it deliberately does not fall back to the expensive rewrite.
    pub fn set_sections_in_place(
        &mut self,
        metadata: MetadataMap,
        tags: Tags,
    ) -> ObjectFileResult<()> {
        self.ensure_writable()?;

        let metadata_size = metadata.size();
        let tags_size = tags.size();
        let layout = LayoutOptions::repartition(
            self.header.content_start,
            metadata_size,
            tags_size,
            DEFAULT_ALIGNMENT,
        )
        .ok_or(ObjectFileError::ReservedSpaceExceeded {
            required: HEADER_SIZE + metadata_size + tags_size,
            available: self.header.content_start as usize,
        })?;

        let mut header = self.header.clone();
        header.tags_start = layout.tags_start;
        header.content_start = layout.content_start;

        let prefix = encode_prefix(&header, &metadata, &tags, layout)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&prefix)?;
        self.file.flush()?;

        self.header = header;
        self.metadata = metadata;
        Ok(())
    }

    /// Rewrites the object with fresh section sizes, copying the content across.
    fn rewrite(&mut self, metadata: MetadataMap, tags: Tags) -> ObjectFileResult<()> {
        self.ensure_writable()?;

        let options = CreateOptions {
            metadata,
            tags,
            layout: LayoutOptions {
                // Treat the current prefix as a floor. Without this every metadata edit would lay
                // the file out from scratch and creep the content forward, so an object updated
                // repeatedly would keep growing even when the sections did not.
                min_content_start: self.header.content_start,
                ..LayoutOptions::default()
            },
            compression: self.header.compression_type,
            sync: true,
        };
        let mut writer = ObjectWriter::create(&self.path, options)?;
        {
            let mut reader = self.stored_content_reader()?;
            std::io::copy(&mut reader, &mut writer)?;
        }
        let replacement = writer.finish()?;
        *self = replacement;
        Ok(())
    }

    fn ensure_writable(&self) -> ObjectFileResult<()> {
        if self.writable {
            Ok(())
        } else {
            Err(ObjectFileError::ReadOnly(self.path.clone()))
        }
    }

    // -- content -------------------------------------------------------------------------------

    /// Reads the content exactly as stored, which for a compressed object means the compressed
    /// bytes. See [TuxObject::decompressed_content_reader] to read through the codec.
    pub fn stored_content_reader(&mut self) -> ObjectFileResult<ContentReader<'_>> {
        let content_start = self.header.content_start as u64;
        let content_length = self.header.content_length;
        self.file.seek(SeekFrom::Start(content_start))?;
        Ok(ContentReader::new(&mut self.file, content_length))
    }

    /// Reads a byte range of the stored content.
    ///
    /// `length` of `None` reads to the end. Rejects compressed objects, where a byte offset into
    /// the stored bytes does not correspond to an offset in the content.
    pub fn content_range_reader(
        &mut self,
        offset: u64,
        length: Option<u64>,
    ) -> ObjectFileResult<ContentReader<'_>> {
        if self.is_compressed() {
            return Err(ObjectFileError::RangedReadOnCompressed);
        }
        let content_length = self.header.content_length;
        let available =
            content_length
                .checked_sub(offset)
                .ok_or(ObjectFileError::RangeOutOfBounds {
                    offset,
                    end: offset,
                    content_length,
                })?;
        let length = length.unwrap_or(available);
        if length > available {
            return Err(ObjectFileError::RangeOutOfBounds {
                offset,
                end: offset.saturating_add(length),
                content_length,
            });
        }

        self.file
            .seek(SeekFrom::Start(self.header.content_start as u64 + offset))?;
        Ok(ContentReader::new(&mut self.file, length))
    }

    /// Reads the content through the object's codec, or straight through when uncompressed.
    pub fn decompressed_content_reader(&mut self) -> ObjectFileResult<DecodedContentReader<'_>> {
        let compression = self.header.compression_type;
        let reader = self.stored_content_reader()?;
        match compression {
            CompressionTypes::None(_) => Ok(DecodedContentReader::Stored(reader)),
            #[cfg(feature = "zstd")]
            CompressionTypes::ZSTD(_) => Ok(DecodedContentReader::Zstd(Box::new(
                zstd::stream::read::Decoder::new(reader).map_err(ObjectFileError::IO)?,
            ))),
            #[cfg(feature = "gzip")]
            CompressionTypes::Gzip(_) => Ok(DecodedContentReader::Gzip(Box::new(
                flate2::read::GzDecoder::new(reader),
            ))),
            #[allow(unreachable_patterns)]
            other => Err(ObjectFileError::UnsupportedCompression(other)),
        }
    }

    /// Reads the whole content into memory, decompressing when needed.
    pub fn read_content_to_vec(&mut self) -> ObjectFileResult<Vec<u8>> {
        let capacity = self.header.content_length as usize;
        let mut buffer = Vec::with_capacity(capacity);
        let mut reader = self.decompressed_content_reader()?;
        reader.read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}
