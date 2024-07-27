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

mod bitstream;
mod chs;
mod detect;
mod diskimage;
mod io;
mod parsers;
mod sector;
mod util;

#[cfg(feature = "viz")]
pub mod visualization;

use std::fmt;
use std::fmt::{Display, Formatter};
use std::hash::RandomState;

use thiserror::Error;

pub const DEFAULT_SECTOR_SIZE: usize = 512;
pub const ASCII_EOF: u8 = 0x1A;

#[allow(unused)]
type FoxHashMap<K, V, S = RandomState> = std::collections::HashMap<K, V, S>;
#[allow(unused)]
type FoxHashSet<T, S = RandomState> = std::collections::HashSet<T, S>;

#[derive(Debug, Error)]
pub enum DiskImageError {
    #[error("An IO error occurred reading or writing the disk image")]
    IoError,
    #[error("Unknown disk image format")]
    UnknownFormat,
    #[error("Unsupported disk image format for requested operation")]
    UnsupportedFormat,
    #[error("The disk image format parser encountered an error")]
    FormatParseError,
    #[error("The disk image format parser determined the image was corrupt")]
    ImageCorruptError,
    #[error("The requested sector could not be found")]
    SeekError,
    #[error("A CRC error was detected in the disk image")]
    CrcError,
    #[error("Invalid parameters were specified to a library function")]
    ParameterError,
}

#[repr(usize)]
#[derive(Default, PartialEq, Eq, Hash)]
pub enum TrackDataType {
    #[default]
    ByteStream = 0,
    BitStream = 1,
    FluxStream = 2,
}

#[derive(Default, Copy, Clone, Debug)]
pub enum DiskDataEncoding {
    #[default]
    Fm,
    Mfm,
    Gcr,
}

#[derive(Default, Copy, Clone, Debug)]
pub enum DiskDensity {
    Standard,
    #[default]
    Double,
    High,
    Extended,
}

pub enum EncodingSync {
    Even,
    Odd,
}

impl From<EncodingSync> for usize {
    fn from(sync: EncodingSync) -> Self {
        match sync {
            EncodingSync::Even => 0,
            EncodingSync::Odd => 1,
        }
    }
}

impl Display for DiskDataEncoding {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskDataEncoding::Fm => write!(f, "FM"),
            DiskDataEncoding::Mfm => write!(f, "MFM"),
            DiskDataEncoding::Gcr => write!(f, "GCR"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum DiskDataRate {
    RateNonstandard(u32),
    Rate125Kbps,
    #[default]
    Rate250Kbps,
    Rate300Kbps,
    Rate500Kbps,
    Rate1000Kbps,
}

impl From<u32> for DiskDataRate {
    fn from(rate: u32) -> Self {
        match rate {
            125000 => DiskDataRate::Rate125Kbps,
            250000 => DiskDataRate::Rate250Kbps,
            300000 => DiskDataRate::Rate300Kbps,
            500000 => DiskDataRate::Rate500Kbps,
            1000000 => DiskDataRate::Rate1000Kbps,
            _ => DiskDataRate::RateNonstandard(rate),
        }
    }
}

impl From<DiskDensity> for DiskDataRate {
    fn from(density: DiskDensity) -> Self {
        match density {
            DiskDensity::Standard => DiskDataRate::Rate125Kbps,
            DiskDensity::Double => DiskDataRate::Rate250Kbps,
            DiskDensity::High => DiskDataRate::Rate500Kbps,
            DiskDensity::Extended => DiskDataRate::Rate1000Kbps,
        }
    }
}

impl Display for DiskDataRate {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskDataRate::RateNonstandard(rate) => write!(f, "{}Kbps", rate / 1000),
            DiskDataRate::Rate125Kbps => write!(f, "125Kbps"),
            DiskDataRate::Rate250Kbps => write!(f, "250Kbps"),
            DiskDataRate::Rate300Kbps => write!(f, "300Kbps"),
            DiskDataRate::Rate500Kbps => write!(f, "500Kbps"),
            DiskDataRate::Rate1000Kbps => write!(f, "1000Kbps"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum DiskRpm {
    #[default]
    Rpm300,
    Rpm360,
}

impl Display for DiskRpm {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskRpm::Rpm300 => write!(f, "300RPM"),
            DiskRpm::Rpm360 => write!(f, "360RPM"),
        }
    }
}

pub use crate::chs::{DiskCh, DiskChs};
pub use crate::diskimage::{DiskImage, DiskImageFormat};
pub use crate::parsers::ImageParser;
