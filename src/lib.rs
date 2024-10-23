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

//! # fluxfox
//!
//! fluxfox is a Rust library for reading, writing, and manipulating disk images of the kind used
//! with vintage IBM Personal Computers, compatibles, and emulators thereof.
//!
//! It is primarily designed for emulator authors who may be writing a PC emulator in Rust and would
//! like to support disk images in a variety of formats, however it can be used for visualization,
//! dumping, editing, conversion, and other disk image tasks.
//!
//! fluxfox currently supports several different disk image formats, both modern and vintage, of
//! bitstream and sector-based resolution. Internally, fluxfox disk images can exist as either
//! byte or bit representations, and up-conversion is possible depending on the format and sector
//! encoding.
//!
//! The main interface to fluxfox is via a [`DiskImage`] object, which can be created by loading
//! a disk image file, or by creating a new disk image from scratch.
//!
//! It is recommended to use the [`image_builder::ImageBuilder`] interface to load or create a disk image.
mod bit_ring;
pub mod bitstream;
mod boot_sector;
mod chs;
mod containers;
mod detect;
pub mod diskimage;
mod file_parsers;
pub mod image_builder;
mod io;
mod random;
pub mod standard_format;
pub mod structure_parsers;
pub mod util;

mod copy_protection;
mod fluxstream;
mod image_writer;
mod track;
#[cfg(feature = "viz")]
pub mod visualization;

use std::fmt;
use std::fmt::{Display, Formatter};
use std::hash::RandomState;

use thiserror::Error;

pub const MAXIMUM_SECTOR_SIZE: usize = 8192;
pub const DEFAULT_SECTOR_SIZE: usize = 512;
pub const ASCII_EOF: u8 = 0x1A;

#[allow(unused)]
type FoxHashMap<K, V, S = RandomState> = std::collections::HashMap<K, V, S>;
#[allow(unused)]
type FoxHashSet<T, S = RandomState> = std::collections::HashSet<T, S>;

pub enum LoadingStatus {
    Progress(f64),
    Complete,
    Error,
}

type LoadingCallback = Box<dyn Fn(LoadingStatus) + Send + 'static>;

#[derive(Debug, Error)]
pub enum DiskImageError {
    #[error("An IO error occurred reading or writing the disk image")]
    IoError(String),
    #[error("A filesystem error occurred or path not found")]
    FsError,
    #[error("Unknown disk image format")]
    UnknownFormat,
    #[error("Unsupported disk image format for requested operation")]
    UnsupportedFormat,
    #[error("The disk image is valid but contains incompatible disk information")]
    IncompatibleImage,
    #[error("The disk image format parser encountered an error")]
    FormatParseError,
    #[error("The disk image format parser determined the image was corrupt")]
    ImageCorruptError,
    #[error("The requested head or cylinder could not be found")]
    SeekError,
    #[error("An error occurred addressing the track bitstream")]
    BitstreamError,
    #[error("The requested sector ID could not be found")]
    IdError,
    #[error("The requested operation matched multiple sector IDs")]
    UniqueIdError,
    #[error("No sectors were found on the current track")]
    DataError,
    #[error("A CRC error was detected in the disk image")]
    CrcError,
    #[error("An invalid function parameter was supplied")]
    ParameterError,
    #[error("Write-protect status prevents writing to the disk image")]
    WriteProtectError,
}

// Manually implement `From<io::Error>` for `DiskImageError`
impl From<io::Error> for DiskImageError {
    fn from(err: io::Error) -> Self {
        DiskImageError::IoError(err.to_string()) // You could convert in a different way
    }
}

// Manually implement `From<binrw::Error>` for `DiskImageError`
impl From<binrw::Error> for DiskImageError {
    fn from(err: binrw::Error) -> Self {
        DiskImageError::IoError(err.to_string()) // Again, you could convert differently
    }
}

#[derive(Debug, Error)]
pub enum DiskVisualizationError {
    #[error("An invalid parameter was supplied")]
    InvalidParameter,
    #[error("The disk image is not a valid format for visualization")]
    NoTracks,
}

/// The resolution of the data in the disk image.
/// Currently only ByteStream and BitStream are implemented.
#[repr(usize)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub enum DiskDataResolution {
    #[default]
    MetaSector = 0,
    BitStream = 1,
    FluxStream = 2,
}

/// The base bitcell encoding method of the data in a disk image.
/// Note that some disk images may contain tracks with different encodings.
#[derive(Default, Copy, Clone, Debug)]
pub enum DiskDataEncoding {
    #[default]
    #[doc = "Frequency Modulation encoding. Used by older 8&quot; diskettes, and duplication tracks on some 5.25&quot; diskettes."]
    Fm,
    #[doc = "Modified Frequency Modulation encoding. Used by almost all 5.25&quot; and 3.5&quot; diskettes."]
    Mfm,
    #[doc = "Group Code Recording encoding. Used by Apple and Macintosh diskettes."]
    Gcr,
}

impl DiskDataEncoding {
    pub fn byte_size(&self) -> usize {
        match self {
            DiskDataEncoding::Fm => 16,
            DiskDataEncoding::Mfm => 16,
            DiskDataEncoding::Gcr => 0,
        }
    }

    pub fn marker_size(&self) -> usize {
        match self {
            DiskDataEncoding::Fm => 64,
            DiskDataEncoding::Mfm => 64,
            DiskDataEncoding::Gcr => 0,
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

/// The physical dimensions of a disk corresponding to the format of the image.
/// This is rarely stored by disk image formats, so it is determined automatically.
#[derive(Default, Copy, Clone, Debug)]
pub enum DiskPhysicalDimensions {
    #[doc = "An 8\" Diskette"]
    Dimension8,
    #[default]
    #[doc = "A 5.25\" Diskette"]
    Dimension5_25,
    #[doc = "A 3.5\" Diskette"]
    Dimension3_5,
}

/// The density of the disk image. Only 8" diskettes were available in standard density.
///
/// * 5.25" diskettes were available in double and high densities.
/// * 3.5" diskettes were available in double, high and extended densities.
#[derive(Default, Copy, Clone, Debug)]
pub enum DiskDensity {
    Standard,
    #[default]
    Double,
    High,
    Extended,
}

impl From<DiskDataRate> for DiskDensity {
    fn from(rate: DiskDataRate) -> Self {
        match rate {
            DiskDataRate::Rate125Kbps => DiskDensity::Standard,
            DiskDataRate::Rate250Kbps => DiskDensity::Double,
            DiskDataRate::Rate500Kbps => DiskDensity::High,
            DiskDataRate::Rate1000Kbps => DiskDensity::Extended,
            _ => DiskDensity::Standard,
        }
    }
}

impl DiskDensity {
    /// Return the number of bitcells for a given disk density.
    /// It is ideal to provide the disk dimensions to get the most accurate bitcell count as high
    /// density 5.25 disks have different bitcell counts than high density 3.5 disks.
    pub fn bitcells(&self, dimensions: Option<DiskPhysicalDimensions>) -> Option<usize> {
        match (self, dimensions) {
            (DiskDensity::Standard, _) => Some(50_000),
            (DiskDensity::Double, _) => Some(100_000),
            (DiskDensity::High, Some(DiskPhysicalDimensions::Dimension5_25)) => Some(166_666),
            (DiskDensity::High, Some(DiskPhysicalDimensions::Dimension3_5) | None) => Some(200_000),
            (DiskDensity::Extended, _) => Some(400_000),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum EncodingPhase {
    Even,
    Odd,
}

impl From<EncodingPhase> for usize {
    fn from(phase: EncodingPhase) -> Self {
        match phase {
            EncodingPhase::Even => 0,
            EncodingPhase::Odd => 1,
        }
    }
}

impl From<EncodingPhase> for bool {
    fn from(phase: EncodingPhase) -> Self {
        match phase {
            EncodingPhase::Even => false,
            EncodingPhase::Odd => true,
        }
    }
}

impl From<bool> for EncodingPhase {
    fn from(phase: bool) -> Self {
        match phase {
            false => EncodingPhase::Even,
            true => EncodingPhase::Odd,
        }
    }
}

impl From<usize> for EncodingPhase {
    fn from(phase: usize) -> Self {
        match phase {
            0 => EncodingPhase::Even,
            _ => EncodingPhase::Odd,
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

impl From<DiskDataRate> for u32 {
    fn from(rate: DiskDataRate) -> Self {
        match rate {
            DiskDataRate::Rate125Kbps => 125_000,
            DiskDataRate::Rate250Kbps => 250_000,
            DiskDataRate::Rate300Kbps => 300_000,
            DiskDataRate::Rate500Kbps => 500_000,
            DiskDataRate::Rate1000Kbps => 1_000_000,
            DiskDataRate::RateNonstandard(rate) => rate,
        }
    }
}

impl From<u32> for DiskDataRate {
    fn from(rate: u32) -> Self {
        match rate {
            125_000 => DiskDataRate::Rate125Kbps,
            250_000 => DiskDataRate::Rate250Kbps,
            300_000 => DiskDataRate::Rate300Kbps,
            500_000 => DiskDataRate::Rate500Kbps,
            1_000_000 => DiskDataRate::Rate1000Kbps,
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
            DiskDataRate::RateNonstandard(rate) => write!(f, "*{}Kbps", rate / 1000),
            DiskDataRate::Rate125Kbps => write!(f, "125Kbps"),
            DiskDataRate::Rate250Kbps => write!(f, "250Kbps"),
            DiskDataRate::Rate300Kbps => write!(f, "300Kbps"),
            DiskDataRate::Rate500Kbps => write!(f, "500Kbps"),
            DiskDataRate::Rate1000Kbps => write!(f, "1000Kbps"),
        }
    }
}

/// The nominal rotational speed of the disk.
///
/// All PC floppy disk drives typically rotate at 300 RPM, except for high density 5.25\" drives
/// which rotate at 360 RPM.
///
/// Macintosh disk drives may have variable rotation rates per-track.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
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

pub use crate::chs::{DiskCh, DiskChs, DiskChsn};
pub use crate::diskimage::{DiskImage, DiskImageFormat, SectorMapEntry};
pub use crate::file_parsers::{format_from_ext, supported_extensions, ImageParser, ParserWriteCompatibility};
pub use crate::image_builder::ImageBuilder;
pub use crate::image_writer::ImageWriter;
pub use crate::standard_format::StandardFormat;
pub use crate::track::TrackConsistency;

pub type DiskSectorMap = Vec<Vec<Vec<SectorMapEntry>>>;
