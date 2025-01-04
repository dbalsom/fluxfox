/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

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

//! RPM (Revolutions Per Minute) related types and functions.

use crate::types::DiskCh;
use std::{
    fmt,
    fmt::{Display, Formatter},
};

/// A [DiskRpm] represents the physical rotation rate of a disk within a drive
/// context.
/// The most common rotation rate used for floppy disks is 300RPM, but this was
/// not universal.
///
/// The most common variant is the 360RPM used by the IBM PC's 5.25" high
/// density floppy drives, even when reading DD disks.  Some of these drives
/// could also operate at 300RPM.
///
/// Some platforms would either halve or double the normal RPM rate as a
/// technical shortcut.
/// The Amiga's Paula chip couldn't handle high density data rates, so Amiga
/// high density disk drives would spin at 150RPM to halve the effective
/// data rate.
///
/// The Macintosh's SWIM controller could only handle fixed bitcell sizes,
/// so the Mac SuperDrive spun DD disks at 600RPM.
///
/// A drive's RPM was not always constant over the surface of the disk.
/// Zoned recording was a common technique used to increase the data density
/// to take advantage of the outer tracks' greater circumference.  This was
/// used on the Apple II and inherited by the Macintosh in GCR mode.
///
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskRpm {
    /// A 150 RPM base rotation rate (Amiga high density).
    Rpm150(f64),
    /// A 300 RPM base rotation rate.
    Rpm300(f64),
    /// A 360 RPM base rotation rate.
    Rpm360(f64),
    /// A 600 RPM base rotation rate (Macintosh SuperDrive reading DD).
    Rpm600(f64),
    /// A Zoned rotation rate, specifying an RPM mapping for each track.
    Zoned(RpmZoneMap, f64),
}

impl From<DiskRpm> for f64 {
    /// Convert a DiskRpm to a floating-point RPM value.
    fn from(rpm: DiskRpm) -> Self {
        use DiskRpm::*;
        match rpm {
            Rpm150(f) => 150.0 * f,
            Rpm300(f) => 300.0 * f,
            Rpm360(f) => 360.0 * f,
            Rpm600(f) => 600.0 * f,
            Zoned(map, f) => map.calculate(DiskCh::default()) as f64 * f,
        }
    }
}

impl Default for DiskRpm {
    fn default() -> Self {
        DiskRpm::Rpm300(1.0)
    }
}

impl Display for DiskRpm {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        let f = self.factor();
        let f_str = if f == 1.0 {
            "".to_string()
        }
        else if f > 1.0 {
            format!(" +{:.3}%", 1.0 - f)
        }
        else {
            format!(" -{:.3}%", f - 1.0)
        };
        match self {
            DiskRpm::Rpm150(_) => write!(fmt, "150RPM{}", f_str),
            DiskRpm::Rpm300(_) => write!(fmt, "300RPM{}", f_str),
            DiskRpm::Rpm360(_) => write!(fmt, "360RPM{}", f_str),
            DiskRpm::Rpm600(_) => write!(fmt, "600RPM{}", f_str),
            DiskRpm::Zoned(_map, _) => write!(fmt, "Zoned RPM"),
        }
    }
}

impl DiskRpm {
    /// Retrieve the adjustment factor for this [DiskRpm].
    pub fn factor(&self) -> f64 {
        match *self {
            DiskRpm::Rpm150(f) => f,
            DiskRpm::Rpm300(f) => f,
            DiskRpm::Rpm360(f) => f,
            DiskRpm::Rpm600(f) => f,
            DiskRpm::Zoned(_map, f) => f,
        }
    }

    /// Try to calculate a [DiskRpm] from the time between index pulses in milliseconds.
    /// Sometimes flux streams report bizarre RPMs, so you will need fallback logic if this
    /// conversion fails.
    ///
    /// This function should not be used on platforms with Zoned RPMs.
    pub fn try_from_index_time(time: f64) -> Option<DiskRpm> {
        let rpm = 60.0 / time;
        // We'd like to support a 15% deviation, but there is a small overlap between 300 +15%
        // and 360 -15%, so we split the difference at 327 RPM.
        match rpm {
            270.0..327.00 => Some(DiskRpm::Rpm300(rpm / 300.0)),
            327.0..414.00 => Some(DiskRpm::Rpm360(rpm / 360.0)),
            _ => None,
        }
    }

    /// Convert a [DiskRpm] to an index time in milliseconds.
    pub fn index_time_ms(&self) -> f64 {
        60.0 / f64::from(*self)
    }

    #[inline]
    pub fn adjust_clock(&self, base_clock: f64) -> f64 {
        // Assume a base clock of 1.5us or greater is a double density disk.
        if matches!(self, DiskRpm::Rpm360(_)) && base_clock >= 1.5e-6 {
            base_clock * (300.0 / 360.0)
        }
        else {
            base_clock
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RpmZoneMap {
    #[default]
    AppleSpeed1,
    AppleSpeed2,
}

impl RpmZoneMap {
    /// Calculate the RPM for a given track with this zone map.
    /// The head number is also required to support any potential weird platforms that might
    /// have different RPMs per side.
    pub fn calculate(&self, ch: DiskCh) -> u32 {
        match self {
            // Values taken from the Mac 400K drive datasheet.
            // Confirmed by Applesauce
            RpmZoneMap::AppleSpeed1 => match ch.c {
                0..16 => 394,
                16..32 => 429,
                32..48 => 472,
                48..64 => 525,
                _ => 590,
            },
            RpmZoneMap::AppleSpeed2 => match ch.c {
                0..16 => 402,
                16..32 => 438,
                32..48 => 482,
                48..64 => 536,
                _ => 603,
            },
        }
    }
}
