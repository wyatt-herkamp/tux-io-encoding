use crate::{
    ConstTypedObjectType, EncodingError, ReadableObjectType, TuxIOType, TypedObjectType,
    WritableObjectType, typed_object_type,
};
mod alloc;
#[cfg(feature = "bytes")]
mod bytes;
mod map;
mod num;
mod option;
mod time;
#[cfg(feature = "uuid")]
mod uuid_impl;
pub use time::*;
impl TuxIOType for bool {
    fn const_size(&self) -> Option<usize> {
        Some(1)
    }
    fn size(&self) -> usize {
        1
    }
}
typed_object_type!(
    bool => 10
);
impl WritableObjectType for bool {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        writer.write_all(&[*self as u8])?;
        Ok(())
    }
}
impl ReadableObjectType for bool {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(1)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let mut buffer = [0u8; 1];
        reader.read_exact(&mut buffer)?;
        match buffer[0] {
            0 => Ok(false),
            1 => Ok(true),
            // Anything else is a corrupt byte, not a short read — which is what this used to report.
            byte => Err(EncodingError::InvalidValue {
                type_name: "bool",
                byte,
            }),
        }
    }
}
/// A fixed-size byte array encodes as the byte-block type: a `u16` length, then the bytes.
///
/// The length is redundant for a `[u8; N]` — `N` is known at compile time — but the type key is shared
/// with [Vec]`<u8>` and, under the `bytes` feature, `Bytes`. One wire form for all three is what lets
/// a value written as any of them read back as any other, so the prefix stays.
impl<const N: usize> TuxIOType for [u8; N] {
    fn const_size(&self) -> Option<usize> {
        Some(N + 2)
    }
    /// `N` plus the two-byte length prefix.
    ///
    /// This returned a bare `N`, disagreeing with both the bytes written and `read_size`.
    fn size(&self) -> usize {
        N + 2
    }
}
// Written out rather than going through `typed_object_type!`, which cannot introduce the `const N`
// parameter. Both traits are implemented, unlike before: `ConstTypedObjectType` was missing, which is
// what a `ValueType` variant is matched on, so a `[u8; N]` could not be one.
impl<const N: usize> TypedObjectType for [u8; N] {
    fn type_key() -> u8 {
        Self::TYPE_KEY
    }
}
impl<const N: usize> ConstTypedObjectType for [u8; N] {
    const TYPE_KEY: u8 = 11;
}
impl<const N: usize> WritableObjectType for [u8; N] {
    fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
        length_is_allowed("[u8; N]", N)?;
        (N as u16).write_to_writer(writer)?;
        writer.write_all(self)?;
        Ok(())
    }
}
impl<const N: usize> ReadableObjectType for [u8; N] {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
        Ok(N + 2)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
    where
        Self: Sized,
    {
        let length = u16::read_from_reader(reader)? as usize;
        if length != N {
            // A byte block of the wrong width. Not a short read: the prefix was there and said
            // something this array cannot hold.
            return Err(EncodingError::LengthMismatch {
                type_name: "[u8; N]",
                expected: N,
                found: length,
            });
        }
        let mut buffer = [0u8; N];
        reader.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}
/// The largest value a `u16` length prefix or entry count can address.
pub const MAX_PREFIXED_LENGTH: usize = u16::MAX as usize;

/// Checks an encoded *byte length* against what its `u16` prefix can hold.
///
/// Kept separate from [count_is_allowed] because the two used to share one check and one error whose
/// message said "bytes" — so a map with too many entries reported a byte-size problem.
#[inline(always)]
pub(crate) fn length_is_allowed(type_name: &'static str, size: usize) -> Result<(), EncodingError> {
    if size > MAX_PREFIXED_LENGTH {
        return Err(EncodingError::TypeTooLarge {
            type_name,
            size,
            limit: MAX_PREFIXED_LENGTH,
        });
    }
    Ok(())
}

/// Checks a collection's *entry count* against what its `u16` count can address.
#[inline(always)]
pub(crate) fn count_is_allowed(type_name: &'static str, count: usize) -> Result<(), EncodingError> {
    if count > MAX_PREFIXED_LENGTH {
        return Err(EncodingError::TooManyEntries {
            type_name,
            count,
            limit: MAX_PREFIXED_LENGTH,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Corrupt input has to be distinguishable from truncated input.
    //!
    //! Every decoder here used to answer [EncodingError::UnexpectedEof] for both, which reads as "the
    //! file is short" — so a single flipped byte looked like a truncated write, and the two call for
    //! opposite responses: re-fetch versus quarantine.

    use std::io::Cursor;

    use super::*;

    #[test]
    fn a_bool_byte_other_than_zero_or_one_is_a_corrupt_value() {
        assert!(matches!(
            bool::read_from_reader(&mut Cursor::new([2u8])),
            Err(EncodingError::InvalidValue {
                type_name: "bool",
                byte: 2
            })
        ));

        // Whereas no byte at all really is a short read.
        assert!(matches!(
            bool::read_from_reader(&mut Cursor::new([0u8; 0])),
            Err(EncodingError::IOError(_))
        ));
    }

    #[test]
    fn a_string_holding_invalid_utf8_says_so() {
        // A two-byte length prefix followed by a lone continuation byte.
        let encoded = [1u8, 0, 0xFF];
        let error = String::read_from_reader(&mut Cursor::new(encoded))
            .expect_err("0xFF is not valid UTF-8");
        assert!(
            matches!(error, EncodingError::InvalidUtf8(_)),
            "got {error:?}"
        );
        // And the message names the actual problem rather than claiming the buffer ended.
        assert!(error.to_string().contains("UTF-8"), "{error}");
    }

    #[test]
    fn a_byte_array_of_the_wrong_width_reports_the_mismatch() {
        // Prefix says 3 bytes; the target holds 4.
        let encoded = [3u8, 0, 1, 2, 3];
        let error = <[u8; 4]>::read_from_reader(&mut Cursor::new(encoded))
            .expect_err("a 3-byte block cannot fill a [u8; 4]");
        assert!(
            matches!(
                error,
                EncodingError::LengthMismatch {
                    expected: 4,
                    found: 3,
                    ..
                }
            ),
            "got {error:?}"
        );
    }

    /// The limits are on the length *prefix*, so the messages have to say which limit was hit.
    #[test]
    fn the_two_limits_report_differently() {
        let too_long = length_is_allowed("String", MAX_PREFIXED_LENGTH + 1).unwrap_err();
        assert!(
            matches!(too_long, EncodingError::TypeTooLarge { .. }),
            "{too_long:?}"
        );
        assert!(too_long.to_string().contains("bytes"), "{too_long}");

        let too_many = count_is_allowed("Tags", MAX_PREFIXED_LENGTH + 1).unwrap_err();
        assert!(
            matches!(too_many, EncodingError::TooManyEntries { .. }),
            "{too_many:?}"
        );
        assert!(too_many.to_string().contains("entries"), "{too_many}");

        // Exactly at the limit is fine, on both.
        assert!(length_is_allowed("String", MAX_PREFIXED_LENGTH).is_ok());
        assert!(count_is_allowed("Tags", MAX_PREFIXED_LENGTH).is_ok());
    }
}
