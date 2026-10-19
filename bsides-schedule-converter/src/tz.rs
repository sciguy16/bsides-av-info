//! Some formats (*cough* sessionize *cough*) give offsets instead of proper time zones, so we have to cover both cases

use std::{borrow::Borrow, fmt::Display, str::FromStr};

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, TimeZone};
use chrono_tz::{Tz, TzOffset};

use color_eyre::{Result, eyre::{self, eyre}};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeAs, DeserializeFromStr, SerializeAs, SerializeDisplay};

pub fn fixed_offset_fromstr(s: &str) -> Result<FixedOffset> {
    let trimmed = s.strip_prefix("UTC").unwrap_or(s);
    let sign =
        match trimmed.chars().next() {
            Some('+') => 1i32,
            Some('-') => -1i32,
            Some(other) => return Err(eyre!("Invalid UTC offset sign {other:?}")),
            None => return Err(eyre!("Empty timezone string"))
        };
    // chrono::FixedOffset::from_str doesn't allow UTC+01, it needs minutes for some reason
    let time: u32 = trimmed[1..].splitn(3, ":")
        .map(u32::from_str)
        .process_results(|i|
            i.enumerate()
            .map(|(idx, value)| 60u32.pow(2 - idx as u32) * value)
            .sum()
        )?;
    chrono::FixedOffset::east_opt(time as i32 * sign)
    // .map(Self::FixedOffset)
    .ok_or_else(|| eyre!("Value was neither a valid time zone nor a valid UTC offset"))
}

#[derive(Clone, Copy, Debug, SerializeDisplay)]
pub enum TzOrOffsetOffset {
    TimeZone(TzOffset),
    FixedOffset(FixedOffset)
}
impl Default for TzOrOffsetOffset {
    fn default() -> Self {
        Self::FixedOffset(FixedOffset::east_opt(0).unwrap())
    }
}
impl Offset for TzOrOffsetOffset {
    fn fix(&self) -> FixedOffset {
        match self {
            TzOrOffsetOffset::TimeZone(tz_offset) => tz_offset.fix(),
            TzOrOffsetOffset::FixedOffset(fixed_offset) => *fixed_offset,
        }
    }
}
impl Display for TzOrOffsetOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TzOrOffsetOffset::TimeZone(tz_offset) => tz_offset.fmt(f),
            TzOrOffsetOffset::FixedOffset(fixed_offset) => fixed_offset.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(SerializeDisplay, DeserializeFromStr)]
pub enum TzOrOffset {
    TimeZone(Tz),
    FixedOffset(FixedOffset)
}
impl Default for TzOrOffset {
    fn default() -> Self {
        Self::FixedOffset(FixedOffset::east_opt(0).unwrap())
    }
}
impl TimeZone for TzOrOffset {
    type Offset = TzOrOffsetOffset;

    fn from_offset(offset: &Self::Offset) -> Self {
        match offset {
            TzOrOffsetOffset::TimeZone(tz_offset) => Self::TimeZone(Tz::from_offset(tz_offset)),
            TzOrOffsetOffset::FixedOffset(fixed_offset) => Self::FixedOffset(*fixed_offset),
        }
    }

    fn offset_from_local_date(&self, local: &chrono::prelude::NaiveDate) -> chrono::MappedLocalTime<Self::Offset> {
        match self {
            TzOrOffset::TimeZone(tz) => tz.offset_from_local_date(local).map(Self::Offset::TimeZone),
            TzOrOffset::FixedOffset(fixed_offset) => fixed_offset.offset_from_local_date(local).map(Self::Offset::FixedOffset),
        }
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> chrono::MappedLocalTime<Self::Offset> {
        match self {
            TzOrOffset::TimeZone(tz) => tz.offset_from_local_datetime(local).map(Self::Offset::TimeZone),
            TzOrOffset::FixedOffset(fixed_offset) => fixed_offset.offset_from_local_datetime(local).map(Self::Offset::FixedOffset),
        }
    }

    fn offset_from_utc_date(&self, utc: &chrono::prelude::NaiveDate) -> Self::Offset {
        match self {
            TzOrOffset::TimeZone(tz) => Self::Offset::TimeZone(tz.offset_from_utc_date(utc)),
            TzOrOffset::FixedOffset(fixed_offset) => Self::Offset::FixedOffset(fixed_offset.offset_from_utc_date(utc)),
        }
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
        match self {
            TzOrOffset::TimeZone(tz) => Self::Offset::TimeZone(tz.offset_from_utc_datetime(utc)),
            TzOrOffset::FixedOffset(fixed_offset) => Self::Offset::FixedOffset(fixed_offset.offset_from_utc_datetime(utc)),
        }
    }
}
impl FromStr for TzOrOffset {
    type Err = color_eyre::eyre::Error;

    fn from_str(s: &str) -> Result<Self> {
        Tz::from_str(s).map(Self::TimeZone).map_err(color_eyre::eyre::Error::from)
        .or_else(|_| fixed_offset_fromstr(s).map(Self::FixedOffset))
    }
}
impl Display for TzOrOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TzOrOffset::TimeZone(tz) => tz.fmt(f),
            TzOrOffset::FixedOffset(fixed_offset) => write!(f, "UTC{fixed_offset}"),
        }
    }
}
impl From<chrono::Utc> for TzOrOffset {
    fn from(_value: chrono::Utc) -> Self {
        Default::default()
    }
}

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, SerializeDisplay, DeserializeFromStr, Debug)]
pub struct Rfc9557DateTime<Base: Borrow<DateTime<TzOrOffset>> = DateTime<TzOrOffset>>(Base);
impl<Base: Borrow<DateTime<TzOrOffset>>> Display for Rfc9557DateTime<Base> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base = self.0.borrow();
        // We could cast to fixed offset, but that loses the timezone name
        base.date_naive().fmt(f)?;
        write!(f, "T")?;
        base.time().fmt(f)?;

        let offset = base.offset();
        match offset {
            TzOrOffsetOffset::FixedOffset(fixed_offset) => fixed_offset.fmt(f),
            TzOrOffsetOffset::TimeZone(tz_offset) => {
                tz_offset.fix().fmt(f)?;
                write!(f, "[{}]", Tz::from_offset(tz_offset).name())
            }
        }
    }
}
impl FromStr for Rfc9557DateTime {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self> {
        // Find the timezone splitter
        let delim = s.rfind(['+', '-']).ok_or_else(|| eyre!("Missing time zone offset"))?;
        // Parse the date
        let date = NaiveDateTime::from_str(&s[..delim])?;
        let s = &s[delim..];
        // See if we have the timezone
        if let Some(timezone_start_idx) = s.find('[') {
            if !s.ends_with(']') || timezone_start_idx == s.len() - 1 {
                return Err(eyre!("Time zone name is malformed"));
            }
            let tz_fixed_offset = fixed_offset_fromstr(&s[..timezone_start_idx])?;
            let time_zone = Tz::from_str(&s[timezone_start_idx+1..s.len() - 1])?;
            // FIXME: gracefully handle insane ambiguity here
            let ret = date.and_local_timezone(TzOrOffset::TimeZone(time_zone)).unwrap();
            if *ret.fixed_offset().offset() != tz_fixed_offset {
                return Err(eyre!("Given offset did not match time zone offset"));
            }
            Ok(Rfc9557DateTime(ret))
        }
        else {
            let offset = fixed_offset_fromstr(s)?;
            // FIXME: gracefully handle insane ambiguity here
            Ok(Rfc9557DateTime(date.and_local_timezone(TzOrOffset::FixedOffset(offset)).unwrap()))
        }
    }
}
impl From<DateTime<TzOrOffset>> for Rfc9557DateTime {
    fn from(value: DateTime<TzOrOffset>) -> Self {
        Self(value)
    }
}
impl From<Rfc9557DateTime> for DateTime<TzOrOffset> {
    fn from(value: Rfc9557DateTime) -> Self {
        value.0
    }
}
impl SerializeAs<DateTime<TzOrOffset>> for Rfc9557DateTime {
    fn serialize_as<S: serde::Serializer>(source: &DateTime<TzOrOffset>, serializer: S) -> std::prelude::v1::Result<S::Ok, S::Error>  {
        Rfc9557DateTime(source).serialize(serializer)
    }
}
impl<'de> DeserializeAs<'de, DateTime<TzOrOffset>> for Rfc9557DateTime {
    fn deserialize_as<D: serde::Deserializer<'de>>(deserializer: D) -> std::prelude::v1::Result<DateTime<TzOrOffset>, D::Error> {
        Rfc9557DateTime::deserialize(deserializer).map(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_deserialise_tz() {
        assert_eq!("Europe/London".parse::<TzOrOffset>().expect("Could not parse time zone"), TzOrOffset::TimeZone(Tz::Europe__London))
    }


    #[test]
    fn test_deserialise_offset() {
        assert_eq!("UTC+01:00".parse::<TzOrOffset>().expect("Could not parse offset"), TzOrOffset::FixedOffset(FixedOffset::east_opt(3600).unwrap()));
        assert_eq!("UTC+01".parse::<TzOrOffset>().expect("Could not parse offset"), TzOrOffset::FixedOffset(FixedOffset::east_opt(3600).unwrap()));
        assert_eq!("UTC-03".parse::<TzOrOffset>().expect("Could not parse offset"), TzOrOffset::FixedOffset(FixedOffset::west_opt(3600 * 3).unwrap()));
        assert_eq!("UTC-03:15".parse::<TzOrOffset>().expect("Could not parse offset"), TzOrOffset::FixedOffset(FixedOffset::west_opt(3600* 3 + 15 * 60).unwrap()));
    }

    #[test]
    fn test_serialise_tz() {
        assert_eq!(TzOrOffset::TimeZone(Tz::Europe__London).to_string(), "Europe/London")
    }

    #[test]
    fn test_serialise_offset() {
        assert_eq!(TzOrOffset::FixedOffset(FixedOffset::east_opt(3600).unwrap()).to_string(), "UTC+01:00")
    }


    #[test]
    fn test_serialize_datetime_tz() {
        let naive_dt = NaiveDateTime::from_str("2026-10-05T16:20:00").unwrap();
        let dt = naive_dt.and_local_timezone(TzOrOffset::TimeZone(Tz::Europe__London)).unwrap();
        assert_eq!("2026-10-05T16:20:00+01:00[Europe/London]", Rfc9557DateTime(dt).to_string());
    }
    #[test]
    fn test_serialize_datetime_offset() {
        let naive_dt = NaiveDateTime::from_str("2025-09-02T11:33:00").unwrap();
        let dt = naive_dt.and_local_timezone(TzOrOffset::FixedOffset(FixedOffset::east_opt(3600 * 3).unwrap())).unwrap();
        assert_eq!("2025-09-02T11:33:00+03:00", Rfc9557DateTime(dt).to_string());
    }

        #[test]
    fn test_deserialize_datetime_tz() {
        let naive_dt = NaiveDateTime::from_str("2026-10-05T16:20:00").unwrap();
        let dt = naive_dt.and_local_timezone(TzOrOffset::TimeZone(Tz::Europe__London)).unwrap();
        assert_eq!(Rfc9557DateTime::from_str("2026-10-05T16:20:00+01:00[Europe/London]").unwrap(), Rfc9557DateTime(dt));
    }
    #[test]
    fn test_deserialize_datetime_offset() {
        let naive_dt = NaiveDateTime::from_str("2025-09-02T11:33:00").unwrap();
        let dt = naive_dt.and_local_timezone(TzOrOffset::FixedOffset(FixedOffset::east_opt(3600 * 3).unwrap())).unwrap();
        assert_eq!(Rfc9557DateTime::from_str("2025-09-02T11:33:00+03:00").unwrap(), Rfc9557DateTime(dt));
    }
}
