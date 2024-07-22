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

mod chs;
mod diskimage;
mod sector;

mod detect;
mod io;
mod parsers;
mod util;

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
    #[error("The disk image format parser encountered an error")]
    FormatParseError,
    #[error("The requested sector could not be found")]
    SeekError,
}

#[repr(usize)]
#[derive(Default, PartialEq, Eq, Hash)]
pub enum TrackDataType {
    #[default]
    ByteStream = 0,
    BitStream = 1,
    FluxStream = 2,
}

#[derive(Default, Copy, Clone)]
pub enum DiskDataEncoding {
    #[default]
    Fm,
    Mfm,
}

impl Display for DiskDataEncoding {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskDataEncoding::Fm => write!(f, "FM"),
            DiskDataEncoding::Mfm => write!(f, "MFM"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum DiskDataRate {
    Rate250Kbps,
    #[default]
    Rate300Kbps,
    Rate500Kbps,
}

impl Display for DiskDataRate {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskDataRate::Rate250Kbps => write!(f, "250Kbps"),
            DiskDataRate::Rate300Kbps => write!(f, "300Kbps"),
            DiskDataRate::Rate500Kbps => write!(f, "500Kbps"),
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
