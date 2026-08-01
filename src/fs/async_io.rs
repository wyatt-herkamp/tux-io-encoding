//! Async object file access for tokio.
//!
//! The header, metadata and tag sections are small and are read in one shot into a buffer, then
//! decoded with the crate's ordinary synchronous codecs — there is nothing to gain from decoding
//! those incrementally. Only the content is genuinely streamed.
//!
//! ```no_run
//! use tokio::io::AsyncWriteExt;
//! use tux_io_encoding::fs::{AsyncTuxObject, CreateOptions};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut writer = AsyncTuxObject::create("object.tuxio", CreateOptions::new()).await?;
//! writer.write_all(b"Hello, world!").await?;
//! let mut object = writer.finish().await?;
//!
//! assert_eq!(object.read_content_to_vec().await?, b"Hello, world!");
//! # Ok(())
//! # }
//! ```

use std::{
    io::Cursor,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, ReadBuf},
};

use crate::{
    CompressionTypes, MetadataMap, ObjectHeader, ReadableObjectType, Tags, TuxIOType, ValueType,
    fs::{
        HEADER_SIZE, LayoutOptions, ObjectFileError, ObjectFileResult, SectionLayout,
        ensure_supported, writer::encode_prefix,
    },
};

/// The async counterpart of [crate::fs::TuxObject].
#[derive(Debug)]
pub struct AsyncTuxObject {
    file: File,
    path: PathBuf,
    header: ObjectHeader,
    metadata: MetadataMap,
    writable: bool,
}

impl AsyncTuxObject {
    /// Opens an object for reading.
    pub async fn open(path: impl Into<PathBuf>) -> ObjectFileResult<Self> {
        let path = path.into();
        let file = OpenOptions::new().read(true).open(&path).await?;
        Self::from_file(file, path, false).await
    }

    /// Opens an object for reading and writing.
    pub async fn open_writable(path: impl Into<PathBuf>) -> ObjectFileResult<Self> {
        let path = path.into();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await?;
        Self::from_file(file, path, true).await
    }

    /// Opens an object, returning `None` when the file does not exist.
    pub async fn open_optional(path: impl Into<PathBuf>) -> ObjectFileResult<Option<Self>> {
        match Self::open(path).await {
            Ok(object) => Ok(Some(object)),
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Starts writing a new object. The parent directory must already exist.
    pub async fn create(
        path: impl Into<PathBuf>,
        options: crate::fs::CreateOptions,
    ) -> ObjectFileResult<AsyncObjectWriter> {
        AsyncObjectWriter::create(path, options).await
    }

    async fn from_file(mut file: File, path: PathBuf, writable: bool) -> ObjectFileResult<Self> {
        file.rewind().await?;

        let mut header_bytes = [0u8; HEADER_SIZE];
        file.read_exact(&mut header_bytes).await?;
        let header = <ObjectHeader as ReadableObjectType>::read_from_bytes(&header_bytes)?;

        let metadata_space = (header.tags_start as usize).saturating_sub(HEADER_SIZE);
        let metadata = if metadata_space == 0 {
            MetadataMap::new()
        } else {
            let mut buffer = vec![0u8; metadata_space];
            file.read_exact(&mut buffer).await?;
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
    pub async fn file_size(&self) -> ObjectFileResult<u64> {
        Ok(self.file.metadata().await?.len())
    }

    // -- tags ----------------------------------------------------------------------------------

    async fn read_tag_section(&mut self) -> ObjectFileResult<Option<Vec<u8>>> {
        let space = self.layout().tags_space();
        if space == 0 {
            return Ok(None);
        }
        self.file
            .seek(std::io::SeekFrom::Start(self.header.tags_start as u64))
            .await?;
        let mut buffer = vec![0u8; space];
        self.file.read_exact(&mut buffer).await?;
        Ok(Some(buffer))
    }

    /// Reads the whole tag section.
    pub async fn read_tags(&mut self) -> ObjectFileResult<Tags> {
        match self.read_tag_section().await? {
            None => Ok(Tags::new()),
            Some(buffer) => Ok(Tags::read_from_reader(&mut Cursor::new(&buffer))?),
        }
    }

    /// Number of tags without decoding their values.
    pub async fn tag_count(&mut self) -> ObjectFileResult<u16> {
        if self.layout().tags_space() == 0 {
            return Ok(0);
        }
        self.file
            .seek(std::io::SeekFrom::Start(self.header.tags_start as u64))
            .await?;
        let mut buffer = [0u8; 2];
        self.file.read_exact(&mut buffer).await?;
        Ok(u16::from_le_bytes(buffer))
    }

    /// Looks up a single tag.
    pub async fn find_tag(&mut self, key: &str) -> ObjectFileResult<Option<ValueType>> {
        match self.read_tag_section().await? {
            None => Ok(None),
            Some(buffer) => Ok(Tags::find_from_reader(
                &mut Cursor::new(&buffer),
                &key.to_owned(),
            )?),
        }
    }

    // -- section updates -----------------------------------------------------------------------

    /// Replaces the metadata section, keeping the tags. Atomic.
    pub async fn set_metadata(&mut self, metadata: MetadataMap) -> ObjectFileResult<()> {
        let tags = self.read_tags().await?;
        self.rewrite(metadata, tags).await
    }

    /// Replaces the tag section, keeping the metadata. Atomic.
    pub async fn set_tags(&mut self, tags: Tags) -> ObjectFileResult<()> {
        let metadata = self.metadata.clone();
        self.rewrite(metadata, tags).await
    }

    /// Replaces both sections at once. Atomic.
    pub async fn set_sections(
        &mut self,
        metadata: MetadataMap,
        tags: Tags,
    ) -> ObjectFileResult<()> {
        self.rewrite(metadata, tags).await
    }

    /// Overwrites both sections without moving the content.
    ///
    /// Cheap but not crash safe — see [crate::fs::TuxObject::set_sections_in_place].
    pub async fn set_sections_in_place(
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
            crate::fs::DEFAULT_ALIGNMENT,
        )
        .ok_or(ObjectFileError::ReservedSpaceExceeded {
            required: HEADER_SIZE + metadata_size + tags_size,
            available: self.header.content_start as usize,
        })?;

        let mut header = self.header.clone();
        header.tags_start = layout.tags_start;
        header.content_start = layout.content_start;

        let prefix = encode_prefix(&header, &metadata, &tags, layout)?;
        self.file.rewind().await?;
        self.file.write_all(&prefix).await?;
        self.file.flush().await?;

        self.header = header;
        self.metadata = metadata;
        Ok(())
    }

    async fn rewrite(&mut self, metadata: MetadataMap, tags: Tags) -> ObjectFileResult<()> {
        self.ensure_writable()?;

        let options = crate::fs::CreateOptions {
            metadata,
            tags,
            layout: LayoutOptions {
                min_content_start: self.header.content_start,
                ..LayoutOptions::default()
            },
            compression: self.header.compression_type,
            sync: true,
        };
        let mut writer = AsyncObjectWriter::create(&self.path, options).await?;
        {
            let mut reader = self.stored_content_reader().await?;
            tokio::io::copy(&mut reader, &mut writer).await?;
        }
        *self = writer.finish().await?;
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

    /// Streams the content exactly as stored.
    pub async fn stored_content_reader(&mut self) -> ObjectFileResult<AsyncContentReader<'_>> {
        let remaining = self.seek_to_content_start().await?;
        Ok(ContentSection {
            file: &mut self.file,
            remaining,
        })
    }

    /// Streams a byte range of the stored content. Rejects compressed objects.
    pub async fn content_range_reader(
        &mut self,
        offset: u64,
        length: Option<u64>,
    ) -> ObjectFileResult<AsyncContentReader<'_>> {
        let remaining = self.seek_within_content(offset, length).await?;
        Ok(ContentSection {
            file: &mut self.file,
            remaining,
        })
    }

    /// Consumes the object and streams its content exactly as stored.
    ///
    /// [AsyncTuxObject::stored_content_reader] borrows the object, so the reader cannot outlive it.
    /// That is fine while reading into a local buffer but not when the reader has to be handed to
    /// something that owns its source — an HTTP response body, say, which outlives the handler that
    /// opened the object. This variant takes the file with it.
    pub async fn into_content_reader(mut self) -> ObjectFileResult<AsyncOwnedContentReader> {
        let remaining = self.seek_to_content_start().await?;
        Ok(ContentSection {
            file: self.file,
            remaining,
        })
    }

    /// Consumes the object and streams a byte range of its content. Rejects compressed objects.
    pub async fn into_content_range_reader(
        mut self,
        offset: u64,
        length: Option<u64>,
    ) -> ObjectFileResult<AsyncOwnedContentReader> {
        let remaining = self.seek_within_content(offset, length).await?;
        Ok(ContentSection {
            file: self.file,
            remaining,
        })
    }

    /// Positions the file at the start of the content, returning the whole stored length.
    async fn seek_to_content_start(&mut self) -> ObjectFileResult<u64> {
        self.file
            .seek(std::io::SeekFrom::Start(self.header.content_start as u64))
            .await?;
        Ok(self.header.content_length)
    }

    /// Positions the file `offset` bytes into the content, returning how much may be read.
    async fn seek_within_content(
        &mut self,
        offset: u64,
        length: Option<u64>,
    ) -> ObjectFileResult<u64> {
        // A compressed object's stored bytes do not correspond to content offsets, so a range over
        // them would be meaningless rather than merely inefficient.
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
            .seek(std::io::SeekFrom::Start(
                self.header.content_start as u64 + offset,
            ))
            .await?;
        Ok(length)
    }

    /// Reads the whole content into memory.
    ///
    /// Compressed objects are decoded on a blocking thread, since the codecs are synchronous.
    pub async fn read_content_to_vec(&mut self) -> ObjectFileResult<Vec<u8>> {
        let mut stored = Vec::with_capacity(self.header.content_length as usize);
        self.stored_content_reader()
            .await?
            .read_to_end(&mut stored)
            .await?;

        match self.header.compression_type {
            CompressionTypes::None(_) => Ok(stored),
            other => decode_blocking(stored, other).await,
        }
    }
}

/// Decodes a compressed content buffer on the blocking pool.
async fn decode_blocking(
    stored: Vec<u8>,
    compression: CompressionTypes,
) -> ObjectFileResult<Vec<u8>> {
    ensure_supported(compression)?;
    tokio::task::spawn_blocking(move || -> ObjectFileResult<Vec<u8>> {
        use std::io::Read;
        let mut decoded = Vec::new();
        match compression {
            CompressionTypes::None(_) => decoded = stored,
            #[cfg(feature = "zstd")]
            CompressionTypes::ZSTD(_) => {
                zstd::stream::read::Decoder::new(Cursor::new(stored))
                    .map_err(ObjectFileError::IO)?
                    .read_to_end(&mut decoded)?;
            }
            #[cfg(feature = "gzip")]
            CompressionTypes::Gzip(_) => {
                flate2::read::GzDecoder::new(Cursor::new(stored)).read_to_end(&mut decoded)?;
            }
            #[allow(unreachable_patterns)]
            other => return Err(ObjectFileError::UnsupportedCompression(other)),
        }
        Ok(decoded)
    })
    .await
    .map_err(|err| ObjectFileError::IO(std::io::Error::other(err)))?
}

/// An [AsyncRead] bounded to the content section of an object file.
///
/// Generic over how the file is held so the borrowing and owning readers share one implementation of
/// the bound — the bound is the part that must not be got wrong, and duplicating it would mean
/// duplicating the `unsafe` in [ContentSection::poll_read].
#[derive(Debug)]
pub struct ContentSection<File> {
    file: File,
    remaining: u64,
}

/// A content reader borrowing the object it reads from.
pub type AsyncContentReader<'object> = ContentSection<&'object mut File>;
/// A content reader that owns its file, for a body that outlives the object handle.
pub type AsyncOwnedContentReader = ContentSection<File>;

impl<F> ContentSection<F> {
    /// Bytes still available from this reader.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub fn is_empty(&self) -> bool {
        self.remaining == 0
    }
}

impl<F: AsyncRead + Unpin> AsyncRead for ContentSection<F> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.remaining == 0 {
            return Poll::Ready(Ok(()));
        }

        // Never let the inner file read past the end of the content section.
        let limit = self.remaining.min(buf.remaining() as u64) as usize;
        let mut limited = buf.take(limit);

        match Pin::new(&mut self.file).poll_read(cx, &mut limited) {
            Poll::Ready(Ok(())) => {
                let read = limited.filled().len();
                // SAFETY: `limited` borrows the unfilled section of `buf`, so the `read` bytes it
                // reports as filled are initialised in `buf`'s backing storage too.
                unsafe { buf.assume_init(read) };
                buf.advance(read);
                self.remaining -= read as u64;
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

/// The async counterpart of [crate::fs::ObjectWriter].
///
/// Same ordering as the sync writer: content first, prefix last, published by rename on
/// [AsyncObjectWriter::finish]. Dropping without finishing leaves the temporary file behind — async
/// drop cannot await the removal — so prefer [AsyncObjectWriter::abort] on the error path.
pub struct AsyncObjectWriter {
    file: File,
    temp_path: Option<PathBuf>,
    final_path: PathBuf,
    layout: SectionLayout,
    layout_options: LayoutOptions,
    compression: CompressionTypes,
    metadata: MetadataMap,
    tags: Tags,
    content_length: u64,
    sync: bool,
}

impl AsyncObjectWriter {
    /// Creates a new object at `path`. The parent directory must already exist.
    pub async fn create(
        path: impl Into<PathBuf>,
        options: crate::fs::CreateOptions,
    ) -> ObjectFileResult<Self> {
        let final_path = path.into();
        ensure_supported(options.compression)?;
        if !matches!(options.compression, CompressionTypes::None(_)) {
            // The codecs are synchronous; compressed content has to be produced with the blocking
            // writer and handed over, rather than streamed through this type.
            return Err(ObjectFileError::UnsupportedCompression(options.compression));
        }
        let layout = options
            .layout
            .compute(options.metadata.size(), options.tags.size())?;

        let (file, temp_path) = create_temp_file(&final_path).await?;

        let mut writer = Self {
            file,
            temp_path: Some(temp_path),
            final_path,
            layout,
            layout_options: options.layout,
            compression: options.compression,
            metadata: options.metadata,
            tags: options.tags,
            content_length: 0,
            sync: options.sync,
        };
        writer
            .file
            .seek(std::io::SeekFrom::Start(writer.layout.content_start as u64))
            .await?;
        Ok(writer)
    }

    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }
    pub fn metadata_mut(&mut self) -> &mut MetadataMap {
        &mut self.metadata
    }
    pub fn tags(&self) -> &Tags {
        &self.tags
    }
    pub fn tags_mut(&mut self) -> &mut Tags {
        &mut self.tags
    }
    pub fn content_length(&self) -> u64 {
        self.content_length
    }
    pub fn path(&self) -> &Path {
        &self.final_path
    }

    /// Writes the prefix, publishes the object, and reopens it for reading.
    pub async fn finish(mut self) -> ObjectFileResult<AsyncTuxObject> {
        let metadata_size = self.metadata.size();
        let tags_size = self.tags.size();

        let layout = match LayoutOptions::repartition(
            self.layout.content_start,
            metadata_size,
            tags_size,
            self.layout_options.alignment,
        ) {
            Some(layout) => layout,
            None => return self.finish_by_rewriting().await,
        };
        self.layout = layout;

        let header = ObjectHeader {
            version: 0,
            compression_type: self.compression,
            tags_start: layout.tags_start,
            content_start: layout.content_start,
            content_length: self.content_length,
            bit_flags: 0,
        };
        let prefix = encode_prefix(&header, &self.metadata, &self.tags, layout)?;

        self.file.flush().await?;
        self.file.rewind().await?;
        self.file.write_all(&prefix).await?;
        self.file.flush().await?;
        if self.sync {
            self.file.sync_all().await?;
        }

        self.publish().await?;

        let file = self.file.try_clone().await?;
        Ok(AsyncTuxObject {
            file,
            path: self.final_path.clone(),
            header,
            metadata: self.metadata.clone(),
            writable: true,
        })
    }

    /// Abandons the write, removing the temporary file.
    pub async fn abort(mut self) -> ObjectFileResult<()> {
        if let Some(temp_path) = self.temp_path.take() {
            match tokio::fs::remove_file(temp_path).await {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    async fn publish(&mut self) -> ObjectFileResult<()> {
        let Some(temp_path) = self.temp_path.take() else {
            return Ok(());
        };
        tokio::fs::rename(&temp_path, &self.final_path).await?;
        if self.sync
            && let Some(parent) = self.final_path.parent()
            && let Ok(dir) = File::open(parent).await
        {
            let _ = dir.sync_all().await;
        }
        Ok(())
    }

    /// Slow path for sections that outgrew the reserved prefix.
    async fn finish_by_rewriting(mut self) -> ObjectFileResult<AsyncTuxObject> {
        let mut options = crate::fs::CreateOptions {
            metadata: std::mem::take(&mut self.metadata),
            tags: std::mem::take(&mut self.tags),
            layout: self.layout_options,
            compression: self.compression,
            sync: self.sync,
        };
        options.layout.metadata_reserve = options.layout.metadata_reserve.max(256);
        options.layout.tag_reserve = options.layout.tag_reserve.max(256);

        let mut replacement = AsyncObjectWriter::create(&self.final_path, options).await?;

        self.file.flush().await?;
        self.file
            .seek(std::io::SeekFrom::Start(self.layout.content_start as u64))
            .await?;
        let mut source = AsyncContentReader {
            file: &mut self.file,
            remaining: self.content_length,
        };
        tokio::io::copy(&mut source, &mut replacement).await?;

        // Boxed to break the `finish` -> `finish_by_rewriting` -> `finish` cycle. The replacement
        // was created with larger reserves, so it does not take this path again.
        let object = Box::pin(replacement.finish()).await?;
        // The original temp file is no longer needed.
        let _ = self.abort().await;
        Ok(object)
    }
}

/// Removes the temporary file when a write was neither finished nor aborted.
///
/// [AsyncObjectWriter::abort] is the ordinary way to give up and the one that reports a failure; this is
/// the safety net for the path that cannot call it, a write abandoned by `?` on the way out. Without it,
/// every upload that fails part way through leaves a `.tuxtmp` file behind for nobody to collect.
///
/// The unlink is synchronous because a [Drop] cannot await. That is acceptable for removing one small
/// file, and it is what the blocking writer already does.
impl Drop for AsyncObjectWriter {
    fn drop(&mut self) {
        if let Some(temp_path) = self.temp_path.take() {
            let _ = std::fs::remove_file(temp_path);
        }
    }
}

impl AsyncWrite for AsyncObjectWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::new(&mut self.file).poll_write(cx, buf) {
            Poll::Ready(Ok(written)) => {
                self.content_length += written as u64;
                Poll::Ready(Ok(written))
            }
            other => other,
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_shutdown(cx)
    }
}

/// Creates a uniquely named temp file next to `final_path`.
async fn create_temp_file(final_path: &Path) -> ObjectFileResult<(File, PathBuf)> {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "object".to_owned());
    let pid = std::process::id();

    for _ in 0..32 {
        let counter = crate::fs::next_temp_counter();
        let candidate = parent.join(format!(".{stem}.{pid}.{counter}.tuxtmp"));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&candidate)
            .await
        {
            Ok(file) => return Ok((file, candidate)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(ObjectFileError::IO(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not find an unused temporary file name",
    )))
}

#[cfg(test)]
mod tests {
    use http::header::CONTENT_TYPE;

    use super::*;
    use crate::fs::CreateOptions;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tux-io-encoding-async-{}-{}-{}",
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
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn create_and_read_back() {
        let dir = TempDir::new("round-trip");
        let path = dir.join("object.tuxio");

        let mut metadata = MetadataMap::new();
        metadata.insert(CONTENT_TYPE.into(), "text/plain".to_owned().into());
        let mut tags = Tags::new();
        tags.insert("tag".to_owned(), "value".to_owned().into());

        let mut writer = AsyncTuxObject::create(
            &path,
            CreateOptions::new().with_metadata(metadata).with_tags(tags),
        )
        .await
        .unwrap();
        writer.write_all(b"Hello, world!").await.unwrap();
        let object = writer.finish().await.unwrap();
        assert_eq!(object.content_length(), 13);
        drop(object);

        let mut object = AsyncTuxObject::open(&path).await.unwrap();
        assert_eq!(
            object
                .metadata()
                .get_header(&CONTENT_TYPE)
                .and_then(ValueType::as_str),
            Some("text/plain")
        );
        assert_eq!(object.tag_count().await.unwrap(), 1);
        assert_eq!(
            object.read_tags().await.unwrap().get("tag"),
            Some(&"value".to_owned().into())
        );
        assert_eq!(
            object.read_content_to_vec().await.unwrap(),
            b"Hello, world!"
        );
    }

    #[tokio::test]
    async fn ranged_reads_stop_at_the_end_of_the_content() {
        let dir = TempDir::new("ranged");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"0123456789").await.unwrap();
        writer.finish().await.unwrap();

        let mut object = AsyncTuxObject::open(&path).await.unwrap();

        let mut buffer = Vec::new();
        object
            .content_range_reader(3, Some(4))
            .await
            .unwrap()
            .read_to_end(&mut buffer)
            .await
            .unwrap();
        assert_eq!(buffer, b"3456");

        // A full-length read must not spill into whatever follows the content on disk.
        buffer.clear();
        object
            .content_range_reader(0, None)
            .await
            .unwrap()
            .read_to_end(&mut buffer)
            .await
            .unwrap();
        assert_eq!(buffer, b"0123456789");

        assert!(matches!(
            object.content_range_reader(11, None).await,
            Err(ObjectFileError::RangeOutOfBounds { .. })
        ));
    }

    /// The owned reader has to behave exactly like the borrowing one, and keep working after the
    /// object handle it came from is gone — which is the entire reason it exists.
    #[tokio::test]
    async fn an_owned_reader_outlives_the_object_handle() {
        let dir = TempDir::new("owned-reader");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"0123456789").await.unwrap();
        writer.finish().await.unwrap();

        let mut reader = AsyncTuxObject::open(&path)
            .await
            .unwrap()
            .into_content_reader()
            .await
            .unwrap();
        assert_eq!(reader.remaining(), 10);

        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await.unwrap();
        assert_eq!(buffer, b"0123456789");
        assert!(reader.is_empty());

        // The same bound applies to a range, and must not spill into what follows the content.
        let mut ranged = AsyncTuxObject::open(&path)
            .await
            .unwrap()
            .into_content_range_reader(3, Some(4))
            .await
            .unwrap();
        buffer.clear();
        ranged.read_to_end(&mut buffer).await.unwrap();
        assert_eq!(buffer, b"3456");

        assert!(matches!(
            AsyncTuxObject::open(&path)
                .await
                .unwrap()
                .into_content_range_reader(11, None)
                .await,
            Err(ObjectFileError::RangeOutOfBounds { .. })
        ));
    }

    #[tokio::test]
    async fn larger_content_round_trips() {
        let dir = TempDir::new("large");
        let path = dir.join("object.tuxio");

        let content: Vec<u8> = (0..(512 * 1024)).map(|index| (index % 251) as u8).collect();
        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(&content).await.unwrap();
        writer.finish().await.unwrap();

        let mut object = AsyncTuxObject::open(&path).await.unwrap();
        assert_eq!(object.content_length(), content.len() as u64);
        assert_eq!(object.read_content_to_vec().await.unwrap(), content);
    }

    #[tokio::test]
    async fn publishes_atomically() {
        let dir = TempDir::new("atomic");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"partial").await.unwrap();
        assert!(!path.exists());
        writer.finish().await.unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn abort_removes_the_temp_file() {
        let dir = TempDir::new("abort");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"aborted").await.unwrap();
        writer.abort().await.unwrap();

        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn metadata_updates_preserve_content_and_layout() {
        let dir = TempDir::new("metadata");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"content stays put").await.unwrap();
        writer.finish().await.unwrap();

        let mut object = AsyncTuxObject::open_writable(&path).await.unwrap();
        let content_start = object.header().content_start;

        let mut metadata = object.metadata().clone();
        metadata.insert(CONTENT_TYPE.into(), "application/json".to_owned().into());
        object.set_metadata(metadata).await.unwrap();

        assert_eq!(object.header().content_start, content_start);
        assert_eq!(
            object.read_content_to_vec().await.unwrap(),
            b"content stays put"
        );
        assert_eq!(
            object
                .metadata()
                .get_header(&CONTENT_TYPE)
                .and_then(ValueType::as_str),
            Some("application/json")
        );
    }

    /// The sync and async layers must produce interchangeable files.
    #[tokio::test]
    async fn sync_written_objects_are_readable_asynchronously() {
        use std::io::Write;

        let dir = TempDir::new("interop");
        let path = dir.join("object.tuxio");

        let mut metadata = MetadataMap::new();
        metadata.insert(CONTENT_TYPE.into(), "text/csv".to_owned().into());
        let mut writer =
            crate::fs::TuxObject::create(&path, CreateOptions::new().with_metadata(metadata))
                .unwrap();
        writer.write_all(b"a,b,c").unwrap();
        writer.finish().unwrap();

        let mut object = AsyncTuxObject::open(&path).await.unwrap();
        assert_eq!(
            object
                .metadata()
                .get_header(&CONTENT_TYPE)
                .and_then(ValueType::as_str),
            Some("text/csv")
        );
        assert_eq!(object.read_content_to_vec().await.unwrap(), b"a,b,c");
    }

    #[tokio::test]
    async fn async_written_objects_are_readable_synchronously() {
        let dir = TempDir::new("interop-reverse");
        let path = dir.join("object.tuxio");

        let mut writer = AsyncTuxObject::create(&path, CreateOptions::new())
            .await
            .unwrap();
        writer.write_all(b"written by tokio").await.unwrap();
        writer.finish().await.unwrap();

        let mut object = crate::fs::TuxObject::open(&path).unwrap();
        assert_eq!(object.read_content_to_vec().unwrap(), b"written by tokio");
    }
}
