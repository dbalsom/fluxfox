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
//! fluxfox is a library crate for reading, writing, and manipulating floppy disk images of the
//! kind used with vintage IBM Personal Computers and compatibles.
//!
//! fluxfox is primarily designed for emulator authors who may be writing a PC emulator and would
//! like to support disk images in a variety of formats, however it can be used for visualization,
//! dumping, editing, and other disk image tasks.
//!
//! fluxfox currently supports several different disk image formats, both modern and vintage, of
//! flux, bitstream and sector-based resolution.
//!
//! The main interface to fluxfox is via a [`DiskImage`] object, which can be created by loading
//! a disk image file, or by creating a new disk image from scratch.
//!
//! It is recommended to use the [`image_builder::ImageBuilder`] interface to load or create a disk image.

extern crate core;

mod bit_ring;
pub mod bitstream;
pub mod boot_sector;
mod chs;
mod containers;
mod detect;
pub mod diskimage;
mod file_parsers;
pub mod image_builder;
pub mod io;
mod random;
pub mod standard_format;
pub mod structure_parsers;
pub mod util;

mod copy_protection;
pub mod file_system;
pub mod flux;
mod image_writer;
pub mod prelude;
mod range_check;
pub mod track;
#[cfg(feature = "viz")]
pub mod visualization;

use std::{
    fmt,
    fmt::{Display, Formatter},
    hash::RandomState,
    sync::Arc,
};
use thiserror::Error;

pub const MAXIMUM_SECTOR_SIZE: usize = 8192;
pub const DEFAULT_SECTOR_SIZE: usize = 512;
pub const ASCII_EOF: u8 = 0x1A;

#[allow(unused)]
pub type FoxHashMap<K, V, S = RandomState> = std::collections::HashMap<K, V, S>;
#[allow(unused)]
type FoxHashSet<T, S = RandomState> = std::collections::HashSet<T, S>;

/// The status of a disk image loading operation, for file parsers that support progress reporting.
pub enum LoadingStatus {
    /// Emitted by file parsers that support progress updates. This is sent before any other task
    /// is performed, to allow the caller time to prepare and display a progress bar.
    ProgressSupport,
    /// Emitted by file parsers that support progress updates to inform the caller of the current progress.
    /// The value is a floating-point number between 0.0 and 1.0, where 1.0 represents full completion.
    /// Note: The value 1.0 is not guaranteed to be emitted.
    Progress(f64),
    /// Emitted by file parsers to inform the caller that the loading operation is complete.
    Complete,
    /// Emitted by file parsers to inform the caller that an error occurred during the loading operation.
    Error,
}

pub type LoadingCallback = Arc<dyn Fn(LoadingStatus) + Send + Sync>;

#[derive(Clone, Debug, Error)]
pub enum DiskImageError {
    #[error("An IO error occurred reading or writing the disk image: {0}")]
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
    #[error("Flux track has not been resolved")]
    ResolveError,
    #[error("An error occurred reading a multi-disk archive: {0}")]
    MultiDiskError(String),
    #[error("An error occurred attempting to lock a resource: {0}")]
    SyncError(String),
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskDataResolution {
    #[default]
    MetaSector = 0,
    BitStream = 1,
    FluxStream = 2,
}

/// The base bitcell encoding method of the data in a disk image.
/// Note that some disk images may contain tracks with different encodings.
#[derive(Default, Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// The density of the disk image.
///
/// * 8" diskettes were FM-encoded and standard density.
/// * 5.25" diskettes were available in double and high densities.
/// * 3.5" diskettes were available in double, high and extended densities.
#[derive(Default, Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
            DiskDataRate::Rate125Kbps(_) => DiskDensity::Standard,
            DiskDataRate::Rate250Kbps(_) => DiskDensity::Double,
            DiskDataRate::Rate500Kbps(_) => DiskDensity::High,
            DiskDataRate::Rate1000Kbps(_) => DiskDensity::Extended,
            _ => DiskDensity::Double,
        }
    }
}

impl Display for DiskDensity {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskDensity::Standard => write!(f, "Standard"),
            DiskDensity::Double => write!(f, "Double"),
            DiskDensity::High => write!(f, "High"),
            DiskDensity::Extended => write!(f, "Extended"),
        }
    }
}

impl DiskDensity {
    /// Return the base number of bitcells for a given disk density.
    /// It is ideal to provide the disk RPM to get the most accurate bitcell count as high
    /// density 5.25 disks have different bitcell counts than high density 3.5 disks.
    ///
    /// The value provided is only an estimate for the ideal bitcell count. The actual bitcell
    /// may vary depending on variances in the disk drive used to write the diskette.
    pub fn bitcells(&self, rpm: Option<DiskRpm>) -> Option<usize> {
        match (self, rpm) {
            (DiskDensity::Standard, _) => Some(50_000),
            (DiskDensity::Double, _) => Some(100_000),
            (DiskDensity::High, Some(DiskRpm::Rpm360)) => Some(166_666),
            (DiskDensity::High, Some(DiskRpm::Rpm300) | None) => Some(200_000),
            (DiskDensity::Extended, _) => Some(400_000),
        }
    }

    /// Return a value in seconds representing the base clock of a PLL for a given disk density.
    /// A `DiskRpm` must be provided for double density disks, as the clock is adjusted for
    /// double-density disks read in high-density 360RPM drives.
    pub fn base_clock(&self, rpm: Option<DiskRpm>) -> f64 {
        match (self, rpm) {
            (DiskDensity::Standard, _) => 4e-6,
            (DiskDensity::Double, None | Some(DiskRpm::Rpm300)) => 2e-6,
            (DiskDensity::Double, Some(DiskRpm::Rpm360)) => 1.666e-6,
            (DiskDensity::High, _) => 1e-6,
            (DiskDensity::Extended, _) => 5e-7,
        }
    }

    /// Attempt to determine the disk density from the base clock of a PLL.
    pub fn from_base_clock(clock: f64) -> Option<DiskDensity> {
        match clock {
            0.375e-6..0.625e-6 => Some(DiskDensity::Extended),
            0.75e-6..1.25e-6 => Some(DiskDensity::High),
            1.5e-6..2.5e-6 => Some(DiskDensity::Double),
            _ => None,
        }
    }
}

/// DiskDataRate defines the data rate of the disk image - for MFM and FM encoding, this is the
/// bit rate / 2.
/// DiskDataRate defines standard data rate categories, while storing a clock adjustment factor to
/// make possible calculation of the exact data rate if required.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskDataRate {
    RateNonstandard(u32),
    Rate125Kbps(f64),
    Rate250Kbps(f64),
    Rate300Kbps(f64),
    Rate500Kbps(f64),
    Rate1000Kbps(f64),
}

impl Default for DiskDataRate {
    fn default() -> Self {
        DiskDataRate::Rate250Kbps(1.0)
    }
}

impl From<DiskDataRate> for u32 {
    fn from(rate: DiskDataRate) -> Self {
        match rate {
            DiskDataRate::Rate125Kbps(f) => (125_000.0 * f) as u32,
            DiskDataRate::Rate250Kbps(f) => (250_000.0 * f) as u32,
            DiskDataRate::Rate300Kbps(f) => (300_000.0 * f) as u32,
            DiskDataRate::Rate500Kbps(f) => (500_000.0 * f) as u32,
            DiskDataRate::Rate1000Kbps(f) => (1_000_000.0 * f) as u32,
            DiskDataRate::RateNonstandard(rate) => rate,
        }
    }
}

/// Implement a conversion from a u32 to a DiskDataRate.
/// An 8-15% rate deviance is allowed for standard rates, otherwise a RateNonstandard is returned.
impl From<u32> for DiskDataRate {
    fn from(rate: u32) -> Self {
        match rate {
            93_750..143_750 => DiskDataRate::Rate125Kbps(rate as f64 / 125_000.0),
            212_000..271_000 => DiskDataRate::Rate250Kbps(rate as f64 / 250_000.0),
            271_000..345_000 => DiskDataRate::Rate300Kbps(rate as f64 / 300_000.0),
            425_000..575_000 => DiskDataRate::Rate500Kbps(rate as f64 / 500_000.0),
            850_000..1_150_000 => DiskDataRate::Rate1000Kbps(rate as f64 / 1_000_000.0),
            _ => DiskDataRate::RateNonstandard(rate),
        }
    }
}

impl From<DiskDensity> for DiskDataRate {
    fn from(density: DiskDensity) -> Self {
        match density {
            DiskDensity::Standard => DiskDataRate::Rate125Kbps(1.0),
            DiskDensity::Double => DiskDataRate::Rate250Kbps(1.0),
            DiskDensity::High => DiskDataRate::Rate500Kbps(1.0),
            DiskDensity::Extended => DiskDataRate::Rate1000Kbps(1.0),
        }
    }
}

impl Display for DiskDataRate {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            DiskDataRate::RateNonstandard(rate) => write!(fmt, "*{}Kbps", rate / 1000),
            DiskDataRate::Rate125Kbps(f) => write!(fmt, "125Kbps (x{:.2})", f),
            DiskDataRate::Rate250Kbps(f) => write!(fmt, "250Kbps (x{:.2})", f),
            DiskDataRate::Rate300Kbps(f) => write!(fmt, "300Kbps (x{:.2})", f),
            DiskDataRate::Rate500Kbps(f) => write!(fmt, "500Kbps (x{:.2})", f),
            DiskDataRate::Rate1000Kbps(f) => write!(fmt, "1000Kbps (x{:.2})", f),
        }
    }
}

/// A `DiskRpm` may represent the standard rotation speed of a standard disk image, or the actual
/// rotation speed of a disk drive while reading a disk. Double density 5.25" disk drives rotate
/// at 300RPM, but a double-density disk read in a high-density 5.25" drive may rotate at 360RPM.
///
/// All PC floppy disk drives typically rotate at 300 RPM, except for high density 5.25\" drives
/// which rotate at 360 RPM.
///
/// Macintosh disk drives may have variable rotation rates while reading a single disk.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskRpm {
    /// A 300 RPM base rotation rate.
    #[default]
    Rpm300,
    /// A 360 RPM base rotation rate.
    Rpm360,
}

impl From<DiskRpm> for f64 {
    /// Convert a DiskRpm to a floating-point RPM value.
    fn from(rpm: DiskRpm) -> Self {
        match rpm {
            DiskRpm::Rpm300 => 300.0,
            DiskRpm::Rpm360 => 360.0,
        }
    }
}

impl Display for DiskRpm {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DiskRpm::Rpm300 => write!(f, "300RPM"),
            DiskRpm::Rpm360 => write!(f, "360RPM"),
        }
    }
}

impl DiskRpm {
    /// Try to determine the disk RPM from the time between index pulses.
    /// Sometimes flux streams report bizarre RPMs, so you will need fallback logic if this
    /// conversion fails.
    pub fn from_index_time(time: f64) -> Option<DiskRpm> {
        let rpm = 60.0 / time;
        // We'd like to support a 15% deviation, but there is a small overlap between 300 +15%
        // and 360 -15%, so we split the difference at 327 RPM.
        match rpm {
            270.0..327.00 => Some(DiskRpm::Rpm300),
            327.0..414.00 => Some(DiskRpm::Rpm360),
            _ => None,
        }
    }

    #[inline]
    pub fn adjust_clock(&self, base_clock: f64) -> f64 {
        // Assume a base clock of 1.5us or greater is a double density disk.
        if matches!(self, DiskRpm::Rpm360) && base_clock >= 1.5e-6 {
            base_clock * (300.0 / 360.0)
        }
        else {
            base_clock
        }
    }
}

// Re-export tiny_skia for convenience
#[cfg(feature = "viz")]
pub use tiny_skia;

pub use crate::{
    chs::{DiskCh, DiskChs, DiskChsn, DiskChsnQuery},
    diskimage::{DiskImage, DiskImageFileFormat, SectorMapEntry},
    file_parsers::{format_from_ext, supported_extensions, ImageParser, ParserWriteCompatibility},
    image_builder::ImageBuilder,
    image_writer::ImageWriter,
    standard_format::StandardFormat,
};

pub type SectorId = DiskChsn;
pub type SectorIdQuery = DiskChsnQuery;
pub type DiskSectorMap = Vec<Vec<Vec<SectorMapEntry>>>;
