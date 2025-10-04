use crate::{EncodingError, RawDate, RawTime, RawTimeZone};
#[derive(thiserror::Error, Debug)]
pub enum ChronoError {
    #[error("Unable to convert {0:?} to NaiveDate")]
    InvalidNaiveDate(RawDate),
    #[error("Unable to convert {0:?} to NaiveTime")]
    InvalidNaiveTime(RawTime),
    #[error("Unable to convert {0:?} to FixedOffset")]
    InvalidFixedOffset(RawTimeZone),
}

mod date {
    use chrono::{Datelike, NaiveDate};

    use crate::{
        ConstTypedObjectType, RawDate, ReadableObjectType, TuxIOType, TypedObjectType,
        WritableObjectType, types::time::chrono_impl::ChronoError,
    };

    use super::EncodingError;

    impl TuxIOType for NaiveDate {
        fn size(&self) -> usize {
            4
        }
        fn const_size(&self) -> Option<usize> {
            Some(4)
        }
    }
    impl TypedObjectType for NaiveDate {
        fn type_key() -> u8 {
            13
        }
    }
    impl ConstTypedObjectType for NaiveDate {
        const TYPE_KEY: u8 = 13;
    }
    impl ReadableObjectType for NaiveDate {
        fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
            Ok(4)
        }
        fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError> {
            let raw_date = RawDate::read_from_reader(reader)?;
            let Some(date) = NaiveDate::from_ymd_opt(
                raw_date.year as i32,
                raw_date.month as u32,
                raw_date.day as u32,
            ) else {
                return Err(EncodingError::other(ChronoError::InvalidNaiveDate(
                    raw_date,
                )));
            };
            Ok(date)
        }
    }
    impl WritableObjectType for NaiveDate {
        fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
            let year = self.year();
            let month = self.month();
            let day = self.day();
            RawDate {
                year: year as u16,
                month: month as u8,
                day: day as u8,
            }
            .write_to_writer(writer)?;
            Ok(())
        }
    }
    impl From<NaiveDate> for RawDate {
        fn from(value: NaiveDate) -> Self {
            RawDate {
                year: value.year() as u16,
                month: value.month() as u8,
                day: value.day() as u8,
            }
        }
    }
    impl From<&NaiveDate> for RawDate {
        fn from(value: &NaiveDate) -> Self {
            RawDate {
                year: value.year() as u16,
                month: value.month() as u8,
                day: value.day() as u8,
            }
        }
    }
    impl TryFrom<RawDate> for NaiveDate {
        type Error = ChronoError;
        fn try_from(value: RawDate) -> Result<Self, Self::Error> {
            let Some(date) =
                NaiveDate::from_ymd_opt(value.year as i32, value.month as u32, value.day as u32)
            else {
                return Err(ChronoError::InvalidNaiveDate(value));
            };
            Ok(date)
        }
    }
    impl TryFrom<&RawDate> for NaiveDate {
        type Error = ChronoError;
        fn try_from(value: &RawDate) -> Result<Self, Self::Error> {
            let Some(date) =
                NaiveDate::from_ymd_opt(value.year as i32, value.month as u32, value.day as u32)
            else {
                return Err(ChronoError::InvalidNaiveDate(*value));
            };
            Ok(date)
        }
    }
}
mod time {
    use chrono::{NaiveTime, Timelike};

    use crate::{
        ConstTypedObjectType, RawTime, ReadableObjectType, TuxIOType, TypedObjectType, ValueType,
        WritableObjectType, types::time::chrono_impl::ChronoError,
    };

    use super::EncodingError;

    impl TuxIOType for NaiveTime {
        fn size(&self) -> usize {
            8
        }
        fn const_size(&self) -> Option<usize> {
            Some(8)
        }
    }
    impl TypedObjectType for NaiveTime {
        fn type_key() -> u8 {
            14
        }
    }
    impl ConstTypedObjectType for NaiveTime {
        const TYPE_KEY: u8 = 14;
    }
    impl ReadableObjectType for NaiveTime {
        fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
            Ok(8)
        }
        fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError> {
            let raw_time = RawTime::read_from_reader(reader)?;
            let Some(time) = NaiveTime::from_num_seconds_from_midnight_opt(
                raw_time.seconds_from_midnight,
                raw_time.nanoseconds,
            ) else {
                return Err(EncodingError::other(ChronoError::InvalidNaiveTime(
                    raw_time,
                )));
            };
            Ok(time)
        }
    }
    impl WritableObjectType for NaiveTime {
        fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
            let raw_time = RawTime {
                seconds_from_midnight: self.num_seconds_from_midnight(),
                nanoseconds: self.nanosecond(),
            };
            raw_time.write_to_writer(writer)?;
            Ok(())
        }
    }
    impl TryFrom<RawTime> for NaiveTime {
        type Error = ChronoError;
        fn try_from(value: RawTime) -> Result<Self, Self::Error> {
            let Some(time) = NaiveTime::from_num_seconds_from_midnight_opt(
                value.seconds_from_midnight,
                value.nanoseconds,
            ) else {
                return Err(ChronoError::InvalidNaiveTime(value));
            };
            Ok(time)
        }
    }
    impl From<NaiveTime> for RawTime {
        fn from(value: NaiveTime) -> Self {
            RawTime {
                seconds_from_midnight: value.num_seconds_from_midnight(),
                nanoseconds: value.nanosecond(),
            }
        }
    }
    impl From<&NaiveTime> for RawTime {
        fn from(value: &NaiveTime) -> Self {
            RawTime {
                seconds_from_midnight: value.num_seconds_from_midnight(),
                nanoseconds: value.nanosecond(),
            }
        }
    }
    impl From<NaiveTime> for ValueType {
        fn from(value: NaiveTime) -> Self {
            ValueType::Time(RawTime::from(value))
        }
    }
}
mod fixed_offset {
    use chrono::FixedOffset;

    use crate::{
        ConstTypedObjectType, RawTimeZone, ReadableObjectType, TuxIOType, TypedObjectType,
        WritableObjectType, types::time::chrono_impl::ChronoError,
    };

    use super::EncodingError;

    impl TuxIOType for FixedOffset {
        fn size(&self) -> usize {
            4
        }
        fn const_size(&self) -> Option<usize> {
            Some(4)
        }
    }
    impl TypedObjectType for FixedOffset {
        fn type_key() -> u8 {
            15
        }
    }
    impl ConstTypedObjectType for FixedOffset {
        const TYPE_KEY: u8 = 15;
    }
    impl ReadableObjectType for FixedOffset {
        fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
            Ok(4)
        }
        fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError> {
            let raw_time_zone = RawTimeZone::read_from_reader(reader)?;
            let Some(offset) = FixedOffset::east_opt(raw_time_zone.offset) else {
                return Err(EncodingError::other(ChronoError::InvalidFixedOffset(
                    raw_time_zone,
                )));
            };
            Ok(offset)
        }
    }
    impl WritableObjectType for FixedOffset {
        fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
            RawTimeZone::from(*self).write_to_writer(writer)?;
            Ok(())
        }
    }
    impl TryFrom<RawTimeZone> for FixedOffset {
        type Error = ChronoError;
        fn try_from(value: RawTimeZone) -> Result<Self, Self::Error> {
            let Some(offset) = FixedOffset::east_opt(value.offset) else {
                return Err(ChronoError::InvalidFixedOffset(value));
            };
            Ok(offset)
        }
    }
    impl From<FixedOffset> for RawTimeZone {
        fn from(value: FixedOffset) -> Self {
            RawTimeZone {
                offset: value.local_minus_utc(),
            }
        }
    }
    impl From<&FixedOffset> for RawTimeZone {
        fn from(value: &FixedOffset) -> Self {
            RawTimeZone {
                offset: value.local_minus_utc(),
            }
        }
    }
}
mod date_time {

    use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

    use crate::{
        ConstTypedObjectType, EncodingError, RawDateTime, ReadableObjectType, TuxIOType,
        TypedObjectType, ValueType, WritableObjectType, types::time::chrono_impl::ChronoError,
    };

    type DateTimeFixed = chrono::DateTime<chrono::FixedOffset>;
    impl TuxIOType for DateTimeFixed {
        fn size(&self) -> usize {
            16
        }
        fn const_size(&self) -> Option<usize> {
            Some(16)
        }
    }
    impl TypedObjectType for DateTimeFixed {
        fn type_key() -> u8 {
            16
        }
    }
    impl ConstTypedObjectType for DateTimeFixed {
        const TYPE_KEY: u8 = 16;
    }
    impl ReadableObjectType for DateTimeFixed {
        fn read_size<R: std::io::Read>(_: &mut R) -> Result<usize, EncodingError> {
            Ok(16)
        }
        fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError> {
            let raw_date_time = RawDateTime::read_from_reader(reader)?;
            raw_date_time.try_into().map_err(EncodingError::other)
        }
    }
    impl WritableObjectType for DateTimeFixed {
        fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
            let raw_date_time: RawDateTime = (*self).into();
            raw_date_time.write_to_writer(writer)?;
            Ok(())
        }
    }
    impl TryFrom<RawDateTime> for DateTimeFixed {
        type Error = ChronoError;
        fn try_from(value: RawDateTime) -> Result<Self, Self::Error> {
            let date: NaiveDate = value.date.try_into()?;
            let time: NaiveTime = value.time.try_into()?;
            let date_time = NaiveDateTime::new(date, time);
            let timezone: FixedOffset = value.timezone.try_into()?;
            Ok(timezone.from_utc_datetime(&date_time))
        }
    }
    impl From<DateTimeFixed> for RawDateTime {
        fn from(value: DateTimeFixed) -> Self {
            let value_utc = value.with_timezone(&Utc);
            RawDateTime {
                date: value_utc.date_naive().into(),
                time: value_utc.time().into(),
                timezone: (*value.offset()).into(),
            }
        }
    }
    impl From<DateTimeFixed> for ValueType {
        fn from(value: DateTimeFixed) -> Self {
            ValueType::RawDateTime(RawDateTime::from(value))
        }
    }
    impl RawDateTime {
        /// Get the local time via [chrono::Local::now]
        pub fn now_chrono_local() -> Self {
            let now = chrono::Local::now().fixed_offset();
            RawDateTime::from(now)
        }
        /// Get the UTC time via [chrono::Utc::now]
        pub fn now_chrono_utc() -> Self {
            let now = chrono::Utc::now().fixed_offset();
            RawDateTime::from(now)
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime};

    use crate::{ConstTypedObjectType, RawDate, RawDateTime, RawTime, RawTimeZone};
    #[test]
    fn assert_type_keys_match() {
        assert_eq!(NaiveDate::TYPE_KEY, RawDate::TYPE_KEY);
        assert_eq!(NaiveTime::TYPE_KEY, RawTime::TYPE_KEY);
        assert_eq!(chrono::FixedOffset::TYPE_KEY, RawTimeZone::TYPE_KEY);
        assert_eq!(
            chrono::DateTime::<chrono::FixedOffset>::TYPE_KEY,
            RawDateTime::TYPE_KEY
        );
    }
}
