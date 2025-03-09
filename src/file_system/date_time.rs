/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------
*/

#[cfg(feature = "fat")]
use fluxfox_fat;
use std::{
    fmt::{self, Display, Formatter},
    time::SystemTime,
};

use thiserror;

#[derive(thiserror::Error, Debug)]
pub enum DateTimeError {
    #[error("Year is before 1970")]
    Before1970,
    #[error("Year is too large")]
    YearTooLarge,
    #[error("Month or Day out of range")]
    InternalOutOfRange,
}

#[derive(Clone, Debug)]
pub struct FsDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

impl Default for FsDateTime {
    fn default() -> Self {
        Self {
            year: 1980,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
        }
    }
}

impl Display for FsDateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}/{:02}/{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(feature = "fat")]
impl From<fluxfox_fat::DateTime> for FsDateTime {
    fn from(dt: fluxfox_fat::DateTime) -> Self {
        Self {
            year: dt.date.year,
            month: dt.date.month as u8,
            day: dt.date.day as u8,
            hour: dt.time.hour as u8,
            minute: dt.time.min as u8,
            second: dt.time.sec as u8,
            millisecond: dt.time.millis,
        }
    }
}

#[cfg(feature = "fat")]
impl From<FsDateTime> for fluxfox_fat::DateTime {
    fn from(dt: FsDateTime) -> Self {
        let date = fluxfox_fat::Date::new(dt.year, dt.month as u16, dt.day as u16);
        let time = fluxfox_fat::Time::new(dt.hour as u16, dt.minute as u16, dt.second as u16, dt.millisecond);
        fluxfox_fat::DateTime::new(date, time)
    }
}

impl TryFrom<SystemTime> for FsDateTime {
    type Error = DateTimeError;

    fn try_from(st: SystemTime) -> Result<Self, Self::Error> {
        // If st is before the Unix epoch, duration_since will Err
        let duration = st
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| DateTimeError::Before1970)?;

        // Extract total whole seconds and sub-second nanos
        let total_secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        let millis = nanos / 1_000_000;

        // We'll only allow 0..999 for milliseconds
        debug_assert!(millis < 1000);

        // Break out H/M/S within the day
        let days = total_secs / 86400;
        let mut leftover = total_secs % 86400;

        let hour = (leftover / 3600) as u8;
        leftover %= 3600;

        let minute = (leftover / 60) as u8;
        leftover %= 60;

        let second = leftover as u8;

        // Now figure out which calendar date corresponds to that many days past 1970-01-01
        let (year, day_of_year) = days_to_ymd(days)?;

        // Convert day_of_year into month/day
        let (month, day) = day_of_year_to_month_day(year, day_of_year)?;

        Ok(FsDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond: millis as u16,
        })
    }
}

/// Convert `days_since_1970` into (year, day_in_year_0_based).
///
/// Returns an error if the computed year would exceed u16::MAX.
fn days_to_ymd(mut days_since_1970: u64) -> Result<(u16, u16), DateTimeError> {
    let mut year: u16 = 1970;
    loop {
        let year_days = days_in_year(year) as u64; // 365 or 366
        if days_since_1970 < year_days {
            // We found our year
            let day_of_year_0_based = days_since_1970 as u16;
            return Ok((year, day_of_year_0_based));
        }
        days_since_1970 -= year_days;

        // Attempt next year
        if year == u16::MAX {
            return Err(DateTimeError::YearTooLarge);
        }
        year += 1;
    }
}

/// Convert a (year, day_in_year_0_based) to (month, day).
///
/// Expects day_in_year to be in [0, 365 or 366).
/// Returns an error if something is inconsistent.
fn day_of_year_to_month_day(year: u16, day_of_year_0_based: u16) -> Result<(u8, u8), DateTimeError> {
    let leap = is_leap_year(year);
    let month_lengths = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    }
    else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut leftover = day_of_year_0_based;
    for (i, &mlen) in month_lengths.iter().enumerate() {
        if leftover < mlen {
            let month = (i + 1) as u8; // i is 0-based, month is 1-based
            let day = leftover + 1; // day is 1-based
            return Ok((month, day as u8));
        }
        else {
            leftover -= mlen;
        }
    }

    // Should never happen if day_of_year_0_based fits in the year
    Err(DateTimeError::InternalOutOfRange)
}

/// Number of days in a given year (365 or 366).
fn days_in_year(y: u16) -> u16 {
    if is_leap_year(y) {
        366
    }
    else {
        365
    }
}

/// Standard proleptic‐Gregorian leap‐year rule
fn is_leap_year(year: u16) -> bool {
    if year % 400 == 0 {
        true
    }
    else if year % 100 == 0 {
        false
    }
    else if year % 4 == 0 {
        true
    }
    else {
        false
    }
}
