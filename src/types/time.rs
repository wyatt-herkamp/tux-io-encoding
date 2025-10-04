//! Implementation of the time types used in tux-io. These are written as library agnostic.
use std::fmt::Debug;

use tux_io_encoding_macros::ObjectType;
#[cfg(feature = "chrono")]
mod chrono_impl;

use crate::{
    ConstTypedObjectType, ReadableObjectType, TuxIOType, TypedObjectType, WritableObjectType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ObjectType)]
#[object_type(const_size = 4, type_key = 13)]
pub struct RawDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}
impl WritableObjectType for RawDate {
    fn write_to_writer<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), super::EncodingError> {
        self.year.write_to_writer(writer)?;
        self.month.write_to_writer(writer)?;
        self.day.write_to_writer(writer)?;
        Ok(())
    }
}
impl ReadableObjectType for RawDate {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, super::EncodingError> {
        // The size is constant for RawDate
        Ok(4)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, super::EncodingError> {
        let year = u16::read_from_reader(reader)?;
        let month = u8::read_from_reader(reader)?;
        let day = u8::read_from_reader(reader)?;
        Ok(RawDate { year, month, day })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ObjectType)]
#[object_type(const_size = 8, type_key = 14)]
pub struct RawTime {
    pub seconds_from_midnight: u32,
    pub nanoseconds: u32,
}

impl WritableObjectType for RawTime {
    fn write_to_writer<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), super::EncodingError> {
        self.seconds_from_midnight.write_to_writer(writer)?;
        self.nanoseconds.write_to_writer(writer)?;
        Ok(())
    }
}
impl ReadableObjectType for RawTime {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, super::EncodingError> {
        // The size is constant for RawTime
        Ok(8)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, super::EncodingError> {
        let seconds_from_midnight = u32::read_from_reader(reader)?;
        let nanoseconds = u32::read_from_reader(reader)?;
        Ok(RawTime {
            seconds_from_midnight,
            nanoseconds,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ObjectType)]
#[object_type(const_size = 4, type_key = 15)]
pub struct RawTimeZone {
    pub offset: i32,
}
impl WritableObjectType for RawTimeZone {
    fn write_to_writer<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), super::EncodingError> {
        self.offset.write_to_writer(writer)?;
        Ok(())
    }
}
impl ReadableObjectType for RawTimeZone {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, super::EncodingError> {
        // The size is constant for RawTimeZone
        Ok(4)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, super::EncodingError> {
        let offset = i32::read_from_reader(reader)?;
        Ok(RawTimeZone { offset })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, ObjectType)]
#[object_type(const_size = 16, type_key = 16)]
pub struct RawDateTime {
    pub date: RawDate,
    pub time: RawTime,
    pub timezone: RawTimeZone,
}
impl WritableObjectType for RawDateTime {
    fn write_to_writer<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), super::EncodingError> {
        self.date.write_to_writer(writer)?;
        self.time.write_to_writer(writer)?;
        self.timezone.write_to_writer(writer)?;
        Ok(())
    }
}
impl ReadableObjectType for RawDateTime {
    fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, super::EncodingError> {
        // The size is constant for RawDateTime
        Ok(16)
    }
    fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, super::EncodingError> {
        let date = RawDate::read_from_reader(reader)?;
        let time = RawTime::read_from_reader(reader)?;
        let timezone = RawTimeZone::read_from_reader(reader)?;
        Ok(RawDateTime {
            date,
            time,
            timezone,
        })
    }
}

impl Debug for RawDateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let RawDateTime {
            date: RawDate { year, month, day },
            time:
                RawTime {
                    seconds_from_midnight,
                    nanoseconds,
                },
            timezone: RawTimeZone { offset },
        } = self;
        f.debug_struct("RawDateTime")
            .field("year", year)
            .field("month", month)
            .field("day", day)
            .field("seconds_from_midnight", seconds_from_midnight)
            .field("nanoseconds", nanoseconds)
            .field("timezone_offset", offset)
            .finish()
    }
}

#[cfg(feature = "get-size2")]
mod get_size_impl {
    use get_size2::GetSize;

    use super::*;

    impl GetSize for RawDateTime {}
    impl GetSize for RawDate {}
    impl GetSize for RawTime {}
    impl GetSize for RawTimeZone {}
}
