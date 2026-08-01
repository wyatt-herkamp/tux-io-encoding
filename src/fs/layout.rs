use crate::fs::ObjectFileError;

/// The encoded size of an [crate::ObjectHeader], which is also the offset the metadata section
/// starts at.
pub const HEADER_SIZE: usize = 32;
/// Default alignment applied to the tag and content section offsets.
pub const DEFAULT_ALIGNMENT: usize = 32;
/// Default number of spare bytes left after the metadata section.
///
/// Metadata is the section that gets rewritten most (ETags, last-modified, user metadata), so
/// leaving slack means those updates never have to move the content.
pub const DEFAULT_METADATA_RESERVE: usize = 256;
/// Default number of spare bytes left after the tag section.
pub const DEFAULT_TAG_RESERVE: usize = 256;

/// Where each section of an object file begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionLayout {
    /// Byte offset the tag section starts at. Metadata occupies `HEADER_SIZE..tags_start`.
    pub tags_start: u16,
    /// Byte offset the content starts at. Tags occupy `tags_start..content_start`.
    pub content_start: u32,
}

impl SectionLayout {
    /// Total bytes available to the metadata section, including padding.
    pub fn metadata_space(&self) -> usize {
        self.tags_start as usize - HEADER_SIZE
    }
    /// Total bytes available to the tag section, including padding.
    pub fn tags_space(&self) -> usize {
        self.content_start as usize - self.tags_start as usize
    }
    /// Total bytes in front of the content.
    pub fn prefix_size(&self) -> usize {
        self.content_start as usize
    }
}

/// How much room to leave for the metadata and tag sections when creating or rewriting an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    /// Spare bytes left after the metadata section.
    pub metadata_reserve: usize,
    /// Spare bytes left after the tag section.
    pub tag_reserve: usize,
    /// Alignment applied to `tags_start` and `content_start`. Values below 1 are treated as 1.
    pub alignment: usize,
    /// Reuse this content offset when the sections still fit in front of it, and never place the
    /// content before it.
    ///
    /// Set this to an existing object's `content_start` to rewrite it without disturbing the
    /// layout: [LayoutOptions::compute] will re-partition the existing prefix instead of applying
    /// the reserves again, which is what stops a repeatedly updated object from creeping forward.
    pub min_content_start: u32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            metadata_reserve: DEFAULT_METADATA_RESERVE,
            tag_reserve: DEFAULT_TAG_RESERVE,
            alignment: DEFAULT_ALIGNMENT,
            min_content_start: 0,
        }
    }
}

impl LayoutOptions {
    /// No slack and no alignment — the smallest possible file.
    ///
    /// Only worth using for objects whose metadata will never change, since any growth then forces
    /// a full rewrite.
    pub fn packed() -> Self {
        Self {
            metadata_reserve: 0,
            tag_reserve: 0,
            alignment: 1,
            min_content_start: 0,
        }
    }

    /// Reserve enough room for the given sections plus the configured slack.
    pub fn compute(
        &self,
        metadata_size: usize,
        tags_size: usize,
    ) -> Result<SectionLayout, ObjectFileError> {
        let alignment = self.alignment.max(1);

        // When a prefix to reuse was named and the sections still fit inside it, keep it rather
        // than applying the reserves on top of the new sizes and pushing the content out.
        if self.min_content_start > 0
            && let Some(layout) =
                Self::repartition(self.min_content_start, metadata_size, tags_size, alignment)
        {
            return Ok(layout);
        }

        let tags_start = align_up(
            HEADER_SIZE + metadata_size + self.metadata_reserve,
            alignment,
        );
        if tags_start > u16::MAX as usize {
            return Err(ObjectFileError::SectionOffsetTooLarge {
                required: tags_start,
                limit: u16::MAX as usize,
            });
        }

        let content_start = align_up(tags_start + tags_size + self.tag_reserve, alignment)
            .max(align_up(self.min_content_start as usize, alignment));
        if content_start > u32::MAX as usize {
            return Err(ObjectFileError::SectionOffsetTooLarge {
                required: content_start,
                limit: u32::MAX as usize,
            });
        }

        Ok(SectionLayout {
            tags_start: tags_start as u16,
            content_start: content_start as u32,
        })
    }

    /// Fit the sections inside an existing prefix without moving the content.
    ///
    /// Re-partitions `HEADER_SIZE..content_start` between the two sections, so metadata can grow
    /// into space the tags are no longer using (and the other way round). Returns `None` when the
    /// two sections simply do not fit in front of the existing content.
    pub fn repartition(
        content_start: u32,
        metadata_size: usize,
        tags_size: usize,
        alignment: usize,
    ) -> Option<SectionLayout> {
        let alignment = alignment.max(1);
        let content_start_usize = content_start as usize;
        if HEADER_SIZE + metadata_size + tags_size > content_start_usize {
            return None;
        }

        // Prefer an aligned tag section, but fall back to packing it directly after the metadata
        // when alignment would push the tags past the content.
        let aligned = align_up(HEADER_SIZE + metadata_size, alignment);
        let tags_start = if aligned + tags_size <= content_start_usize {
            aligned
        } else {
            HEADER_SIZE + metadata_size
        };
        if tags_start > u16::MAX as usize {
            return None;
        }

        Some(SectionLayout {
            tags_start: tags_start as u16,
            content_start,
        })
    }
}

/// Rounds `value` up to the next multiple of `alignment`.
pub fn align_up(value: usize, alignment: usize) -> usize {
    let alignment = alignment.max(1);
    value.div_ceil(alignment) * alignment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_to_boundary() {
        assert_eq!(align_up(0, 32), 0);
        assert_eq!(align_up(1, 32), 32);
        assert_eq!(align_up(32, 32), 32);
        assert_eq!(align_up(33, 32), 64);
        assert_eq!(align_up(33, 1), 33);
        // An alignment of zero must not divide by zero.
        assert_eq!(align_up(33, 0), 33);
    }

    #[test]
    fn compute_leaves_reserve_and_aligns() {
        let layout = LayoutOptions::default().compute(40, 10).unwrap();
        // 32 header + 40 metadata + 256 reserve = 328 -> aligned to 352
        assert_eq!(layout.tags_start, 352);
        // 352 + 10 tags + 256 reserve = 618 -> aligned to 640
        assert_eq!(layout.content_start, 640);
        assert!(layout.metadata_space() >= 40);
        assert!(layout.tags_space() >= 10);
    }

    #[test]
    fn packed_computes_exact_offsets() {
        let layout = LayoutOptions::packed().compute(40, 10).unwrap();
        assert_eq!(layout.tags_start, 72);
        assert_eq!(layout.content_start, 82);
        assert_eq!(layout.metadata_space(), 40);
        assert_eq!(layout.tags_space(), 10);
    }

    #[test]
    fn compute_honors_min_content_start() {
        let options = LayoutOptions {
            min_content_start: 4096,
            ..LayoutOptions::packed()
        };
        let layout = options.compute(10, 10).unwrap();
        assert_eq!(layout.content_start, 4096);
    }

    #[test]
    fn compute_rejects_metadata_past_u16() {
        let error = LayoutOptions::packed()
            .compute(u16::MAX as usize, 0)
            .unwrap_err();
        assert!(matches!(
            error,
            ObjectFileError::SectionOffsetTooLarge { .. }
        ));
    }

    #[test]
    fn repartition_moves_tags_to_make_room_for_metadata() {
        // Metadata grew to 200 bytes while tags shrank to 4; both still fit before offset 640.
        let layout = LayoutOptions::repartition(640, 200, 4, 32).unwrap();
        assert_eq!(layout.content_start, 640);
        assert_eq!(layout.tags_start, 256); // align_up(32 + 200, 32)
        assert!(layout.metadata_space() >= 200);
        assert!(layout.tags_space() >= 4);
    }

    #[test]
    fn repartition_falls_back_to_unaligned_tags() {
        // Alignment would push tags_start to 64 leaving only 6 bytes, so it packs at 62 instead.
        let layout = LayoutOptions::repartition(70, 30, 8, 32).unwrap();
        assert_eq!(layout.tags_start, 62);
        assert_eq!(layout.tags_space(), 8);
    }

    #[test]
    fn repartition_rejects_sections_that_do_not_fit() {
        assert!(LayoutOptions::repartition(100, 60, 40, 32).is_none());
    }
}
