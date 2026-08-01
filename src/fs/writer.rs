use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    CompressionTypes, MetadataMap, ObjectHeader, Tags, TuxIOType, WritableObjectType,
    fs::{HEADER_SIZE, LayoutOptions, ObjectFileError, ObjectFileResult, SectionLayout, TuxObject},
};

/// Distinguishes temp files created by concurrent writers in the same directory.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Next value for a temporary file name. Shared with the async writer so the two cannot collide.
pub(crate) fn next_temp_counter() -> u64 {
    TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// How a new object should be laid out and what it starts with.
#[derive(Debug, Clone, Default)]
pub struct CreateOptions {
    pub metadata: MetadataMap,
    pub tags: Tags,
    pub layout: LayoutOptions,
    pub compression: CompressionTypes,
    /// `fsync` the file before publishing it. Costs a flush per object but means a completed write
    /// survives a power loss.
    pub sync: bool,
}

impl CreateOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_metadata(mut self, metadata: MetadataMap) -> Self {
        self.metadata = metadata;
        self
    }
    pub fn with_tags(mut self, tags: Tags) -> Self {
        self.tags = tags;
        self
    }
    pub fn with_layout(mut self, layout: LayoutOptions) -> Self {
        self.layout = layout;
        self
    }
    pub fn with_compression(mut self, compression: CompressionTypes) -> Self {
        self.compression = compression;
        self
    }
    pub fn with_sync(mut self, sync: bool) -> Self {
        self.sync = sync;
        self
    }
}

/// Writes a complete object file.
///
/// The content is streamed first, into space reserved by the layout, and the header, metadata and
/// tag sections are written last — once the final content length is known. That ordering is what
/// lets a caller keep mutating [ObjectWriter::metadata_mut] while streaming (to fill in a digest,
/// for instance) without a second pass over the content.
///
/// Writes land in a temporary file that is renamed over the destination by [ObjectWriter::finish],
/// so a reader never observes a half-written object. Dropping the writer without finishing removes
/// the temporary file.
pub struct ObjectWriter {
    file: File,
    /// `Some` until the object is published or abandoned; [Drop] uses it to clean up.
    temp_path: Option<PathBuf>,
    final_path: PathBuf,
    layout: SectionLayout,
    layout_options: LayoutOptions,
    compression: CompressionTypes,
    metadata: MetadataMap,
    tags: Tags,
    content_length: u64,
    sync: bool,
    /// Guards against writing raw bytes into an object whose header claims a codec, which would
    /// leave a file that reads back as garbage. Cleared for compressed objects until
    /// [ObjectWriter::content_encoder] hands out an encoder.
    allow_raw_writes: bool,
}

impl ObjectWriter {
    /// Creates a new object at `path`, replacing any existing file when finished.
    ///
    /// The parent directory must already exist — this deliberately does not create directories, so
    /// a read path can never bring one into being as a side effect.
    pub fn create(path: impl Into<PathBuf>, options: CreateOptions) -> ObjectFileResult<Self> {
        let final_path = path.into();
        crate::fs::ensure_supported(options.compression)?;
        let layout = options
            .layout
            .compute(options.metadata.size(), options.tags.size())?;

        let (file, temp_path) = create_temp_file(&final_path)?;
        let allow_raw_writes = matches!(options.compression, CompressionTypes::None(_));

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
            allow_raw_writes,
        };
        // Leave the prefix untouched for now; it gets written by `finish`.
        writer
            .file
            .seek(SeekFrom::Start(writer.layout.content_start as u64))?;
        Ok(writer)
    }

    /// The metadata this object will be published with. Mutable until [ObjectWriter::finish].
    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }
    pub fn metadata_mut(&mut self) -> &mut MetadataMap {
        &mut self.metadata
    }
    /// The tags this object will be published with. Mutable until [ObjectWriter::finish].
    pub fn tags(&self) -> &Tags {
        &self.tags
    }
    pub fn tags_mut(&mut self) -> &mut Tags {
        &mut self.tags
    }
    /// Content bytes written so far.
    pub fn content_length(&self) -> u64 {
        self.content_length
    }
    /// Where the object will be published.
    pub fn path(&self) -> &Path {
        &self.final_path
    }
    /// Bytes still available to the metadata and tag sections without moving the content.
    pub fn reserved_space(&self) -> usize {
        self.layout.prefix_size() - HEADER_SIZE
    }
    /// The compression the finished object will declare.
    pub fn compression(&self) -> CompressionTypes {
        self.compression
    }

    /// An encoder that compresses content on its way into this writer.
    ///
    /// This is the only way to write the content of a compressed object — writing to the
    /// [Write] impl directly is rejected while a codec is configured, since it would store raw
    /// bytes under a header claiming they were compressed. For an uncompressed object the encoder
    /// is a pass-through, so the same code path works either way.
    ///
    /// [crate::fs::ContentEncoder::finish] must be called to flush the codec.
    pub fn content_encoder(&mut self) -> ObjectFileResult<crate::fs::ContentEncoder<'_>> {
        let compression = self.compression;
        // The encoder borrows the writer exclusively, so nothing else can write raw bytes while it
        // is alive.
        self.allow_raw_writes = true;
        crate::fs::ContentEncoder::new(self, compression)
    }

    /// Writes the prefix, publishes the object, and reopens it for reading.
    pub fn finish(mut self) -> ObjectFileResult<TuxObject> {
        let metadata_size = self.metadata.size();
        let tags_size = self.tags.size();

        // The content is already on disk at `layout.content_start`. If the sections have outgrown
        // the reserved prefix, the content has to move, which means rewriting the file.
        let layout = match LayoutOptions::repartition(
            self.layout.content_start,
            metadata_size,
            tags_size,
            self.layout_options.alignment,
        ) {
            Some(layout) => layout,
            None => return self.finish_by_rewriting(),
        };
        self.layout = layout;

        let header = self.build_header();
        let prefix = encode_prefix(&header, &self.metadata, &self.tags, layout)?;

        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&prefix)?;
        self.file.flush()?;
        if self.sync {
            self.file.sync_all()?;
        }

        self.publish()?;

        Ok(TuxObject::from_parts(
            self.file.try_clone()?,
            self.final_path.clone(),
            header,
            self.metadata.clone(),
            true,
        ))
    }

    /// Abandons the write, removing the temporary file.
    pub fn abort(mut self) -> ObjectFileResult<()> {
        if let Some(temp_path) = self.temp_path.take() {
            match std::fs::remove_file(temp_path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    fn build_header(&self) -> ObjectHeader {
        ObjectHeader {
            version: 0,
            compression_type: self.compression,
            tags_start: self.layout.tags_start,
            content_start: self.layout.content_start,
            content_length: self.content_length,
            bit_flags: 0,
        }
    }

    /// Renames the temporary file over the destination.
    fn publish(&mut self) -> ObjectFileResult<()> {
        let Some(temp_path) = self.temp_path.take() else {
            return Ok(());
        };
        std::fs::rename(&temp_path, &self.final_path)?;
        // Renames are only durable once the directory entry itself is flushed.
        if self.sync
            && let Some(parent) = self.final_path.parent()
            && let Ok(dir) = File::open(parent)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    /// Slow path for when the metadata and tags outgrew the reserved prefix: lay the object out
    /// again with the larger sections and copy the content across.
    fn finish_by_rewriting(mut self) -> ObjectFileResult<TuxObject> {
        let mut options = CreateOptions {
            metadata: std::mem::take(&mut self.metadata),
            tags: std::mem::take(&mut self.tags),
            layout: self.layout_options,
            compression: self.compression,
            sync: self.sync,
        };
        // Make sure the fresh layout actually has room for what we are carrying over, even if the
        // caller configured no reserve at all.
        options.layout.metadata_reserve =
            options.layout.metadata_reserve.max(DEFAULT_REWRITE_RESERVE);
        options.layout.tag_reserve = options.layout.tag_reserve.max(DEFAULT_REWRITE_RESERVE);

        let mut replacement = ObjectWriter::create(&self.final_path, options)?;

        self.file.flush()?;
        self.file
            .seek(SeekFrom::Start(self.layout.content_start as u64))?;
        let mut source = crate::fs::ContentReader::new(&mut self.file, self.content_length);
        std::io::copy(&mut source, &mut replacement)?;

        // `self` still owns the original temp file; dropping it removes it.
        replacement.finish()
    }
}

/// Extra slack handed to a rewrite so the very next metadata update does not trigger another one.
const DEFAULT_REWRITE_RESERVE: usize = 256;

impl Write for ObjectWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.allow_raw_writes {
            return Err(std::io::Error::other(
                "this object declares a compression codec; write its content through \
                 ObjectWriter::content_encoder",
            ));
        }
        let written = self.file.write(buf)?;
        self.content_length += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Drop for ObjectWriter {
    fn drop(&mut self) {
        if let Some(temp_path) = self.temp_path.take() {
            let _ = std::fs::remove_file(temp_path);
        }
    }
}

/// Encodes the header, metadata and tag sections into one buffer covering `0..content_start`.
///
/// Padding is zero filled, so two objects with the same sections produce identical prefixes.
pub(crate) fn encode_prefix(
    header: &ObjectHeader,
    metadata: &MetadataMap,
    tags: &Tags,
    layout: SectionLayout,
) -> ObjectFileResult<Vec<u8>> {
    let metadata_size = metadata.size();
    let tags_size = tags.size();
    if metadata_size > layout.metadata_space() {
        return Err(ObjectFileError::ReservedSpaceExceeded {
            required: metadata_size,
            available: layout.metadata_space(),
        });
    }
    if tags_size > layout.tags_space() {
        return Err(ObjectFileError::ReservedSpaceExceeded {
            required: tags_size,
            available: layout.tags_space(),
        });
    }

    let mut buffer = vec![0u8; layout.prefix_size()];
    let mut cursor = Cursor::new(buffer.as_mut_slice());
    header.write_to_writer(&mut cursor)?;
    cursor.set_position(HEADER_SIZE as u64);
    metadata.write_to_writer(&mut cursor)?;
    cursor.set_position(layout.tags_start as u64);
    tags.write_to_writer(&mut cursor)?;
    Ok(buffer)
}

/// Creates a uniquely named temp file next to `final_path`, so the eventual rename stays within one
/// filesystem and is therefore atomic.
fn create_temp_file(final_path: &Path) -> ObjectFileResult<(File, PathBuf)> {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "object".to_owned());
    let pid = std::process::id();

    // Retry rather than trust the counter alone: another process could hold the same name.
    for _ in 0..32 {
        let counter = next_temp_counter();
        let candidate = parent.join(format!(".{stem}.{pid}.{counter}.tuxtmp"));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&candidate)
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
