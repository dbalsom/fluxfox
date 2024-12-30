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

    enums.rs

    Defines common enum types
*/
use crate::{types::IntegrityField, StandardFormat};
use std::{
    fmt,
    fmt::{Display, Formatter},
    path::PathBuf,
};

pub use crate::platform::Platform;

/// The level of data resolution for a given track.
/// fluxfox supports three types of data resolutions:
/// * MetaSector tracks hold only sector data along with optional metadata per sector.
/// * BitStream tracks hold a bitwise representation of each track on a disk.
/// * FluxStream tracks hold one or more `revolutions` of flux transition delta times per track,
///   which are resolved to a single bitstream.
///
/// It is possible for some image formats to contain a combination of BitStream and FluxStream
/// tracks.
#[repr(usize)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackDataResolution {
    #[default]
    #[doc = "MetaSector images hold only sector data along with optional metadata per sector."]
    MetaSector = 0,
    #[doc = "BitStream images hold a bitwise representation of each track on a disk."]
    BitStream = 1,
    #[doc = "FluxStream images hold one or more `revolutions` of flux transition delta times per track, which are resolved to a single bitstream."]
    FluxStream = 2,
}

/// The type of data encoding used by a track in a disk image.
/// Note that some disk images may contain tracks with different encodings.
/// fluxfox supports two types of data encodings:
/// * Fm: Frequency Modulation encoding. Used by older 8" diskettes, and 'duplication mark' tracks
///   on some 3.5" and 5.25" diskettes.
/// * Mfm: Modified Frequency Modulation encoding. Used by almost all PC 5.25" and 3.5" diskettes,
///   Amiga 3.5" diskettes, and Macintosh 1.44MB 3.5" diskettes.
///
/// Not implemented are:
/// * Gcr: Group Code Recording encoding. Used by Apple and Macintosh diskettes.
#[derive(Default, Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackDataEncoding {
    #[default]
    #[doc = "Frequency Modulation encoding. Used by older 8&quot; diskettes, and duplication tracks on some 5.25&quot; diskettes."]
    Fm,
    #[doc = "Modified Frequency Modulation encoding. Used by almost all 5.25&quot; and 3.5&quot; diskettes."]
    Mfm,
    #[doc = "Group Code Recording encoding. Used by Apple and Macintosh diskettes."]
    Gcr,
}

impl TrackDataEncoding {
    pub fn byte_size(&self) -> usize {
        match self {
            TrackDataEncoding::Fm => 16,
            TrackDataEncoding::Mfm => 16,
            TrackDataEncoding::Gcr => 0,
        }
    }

    pub fn marker_size(&self) -> usize {
        match self {
            TrackDataEncoding::Fm => 64,
            TrackDataEncoding::Mfm => 64,
            TrackDataEncoding::Gcr => 0,
        }
    }
}

impl Display for TrackDataEncoding {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TrackDataEncoding::Fm => write!(f, "FM"),
            TrackDataEncoding::Mfm => write!(f, "MFM"),
            TrackDataEncoding::Gcr => write!(f, "GCR"),
        }
    }
}

/// The physical dimensions of a disk.
/// A few disk image formats such as MFI have a metadata field to specify a disk's dimensions.
/// There is not a perfect way to determine  this heuristically, but one can take a pretty good
/// guess based on the cylinder count, density, data rate, RPM, and other parameters.
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

/// The density of data recording on a disk track.
/// A disk image may contain tracks with different densities.
///
/// * `Standard` density: typically referring to FM encoding, typically used by 8" diskettes.
/// * `Double` density: typically referring to MFM encoding at 250/300Kbps. Appeared on 5.25" and 3.5" diskettes.
/// * `High` density: typically referring to MFM encoding at 500Kbps. Appeared on 5.25" and 3.5" diskettes.
/// * `Extended` density: typically referring to MFM encoding at 1Mbps. Appeared on 3.5" diskettes.
#[derive(Default, Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackDensity {
    Standard,
    #[default]
    Double,
    High,
    Extended,
}

impl From<TrackDataRate> for TrackDensity {
    fn from(rate: TrackDataRate) -> Self {
        use TrackDataRate::*;
        match rate {
            Rate125Kbps(_) => TrackDensity::Standard,
            Rate250Kbps(_) => TrackDensity::Double,
            Rate300Kbps(_) => TrackDensity::Double,
            Rate500Kbps(_) => TrackDensity::High,
            Rate1000Kbps(_) => TrackDensity::Extended,
            _ => TrackDensity::Double,
        }
    }
}

impl Display for TrackDensity {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use TrackDensity::*;
        match self {
            Standard => write!(f, "Standard"),
            Double => write!(f, "Double"),
            High => write!(f, "High"),
            Extended => write!(f, "Extended"),
        }
    }
}

impl TrackDensity {
    /// Return the base number of bitcells for a given disk density.
    /// It is ideal to provide the disk RPM to get the most accurate bitcell count as high
    /// density 5.25 disks have different bitcell counts than high density 3.5 disks.
    ///
    /// The value provided is only an estimate for the ideal bitcell count. The actual bitcell
    /// may vary depending on variances in the disk drive used to write the diskette.
    pub fn bitcells(&self, rpm: Option<DiskRpm>) -> Option<usize> {
        use TrackDensity::*;
        match (self, rpm) {
            (Standard, _) => Some(50_000),
            (Double, _) => Some(100_000),
            (High, Some(DiskRpm::Rpm360)) => Some(166_666),
            (High, Some(DiskRpm::Rpm300) | None) => Some(200_000),
            (Extended, _) => Some(400_000),
        }
    }

    pub fn from_bitcells(bitcells: u32) -> Option<TrackDensity> {
        match bitcells {
            40_000..60_000 => Some(TrackDensity::Standard),
            80_000..120_000 => Some(TrackDensity::Double),
            150_000..250_000 => Some(TrackDensity::High),
            350_000..450_000 => Some(TrackDensity::Extended),
            _ => None,
        }
    }

    /// Return a value in seconds representing the base clock of a PLL for a given disk density.
    /// A `DiskRpm` must be provided for double density disks, as the clock is adjusted for
    /// double-density disks read in high-density 360RPM drives.
    pub fn base_clock(&self, rpm: Option<DiskRpm>) -> f64 {
        match (self, rpm) {
            (TrackDensity::Standard, _) => 4e-6,
            (TrackDensity::Double, None | Some(DiskRpm::Rpm300)) => 2e-6,
            (TrackDensity::Double, Some(DiskRpm::Rpm360)) => 1.666e-6,
            (TrackDensity::High, _) => 1e-6,
            (TrackDensity::Extended, _) => 5e-7,
        }
    }

    /// Attempt to determine the disk density from the base clock of a PLL.
    pub fn from_base_clock(clock: f64) -> Option<TrackDensity> {
        match clock {
            0.375e-6..0.625e-6 => Some(TrackDensity::Extended),
            0.75e-6..1.25e-6 => Some(TrackDensity::High),
            1.5e-6..2.5e-6 => Some(TrackDensity::Double),
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
pub enum TrackDataRate {
    RateNonstandard(u32),
    Rate125Kbps(f64),
    Rate250Kbps(f64),
    Rate300Kbps(f64),
    Rate500Kbps(f64),
    Rate1000Kbps(f64),
}

impl Default for TrackDataRate {
    fn default() -> Self {
        TrackDataRate::Rate250Kbps(1.0)
    }
}

impl From<TrackDataRate> for u32 {
    fn from(rate: TrackDataRate) -> Self {
        use TrackDataRate::*;
        match rate {
            Rate125Kbps(f) => (125_000.0 * f) as u32,
            Rate250Kbps(f) => (250_000.0 * f) as u32,
            Rate300Kbps(f) => (300_000.0 * f) as u32,
            Rate500Kbps(f) => (500_000.0 * f) as u32,
            Rate1000Kbps(f) => (1_000_000.0 * f) as u32,
            RateNonstandard(rate) => rate,
        }
    }
}

/// Implement a conversion from a u32 to a DiskDataRate.
/// An 8-15% rate deviance is allowed for standard rates, otherwise a RateNonstandard is returned.
impl From<u32> for TrackDataRate {
    fn from(rate: u32) -> Self {
        use TrackDataRate::*;
        match rate {
            93_750..143_750 => Rate125Kbps(rate as f64 / 125_000.0),
            212_000..271_000 => Rate250Kbps(rate as f64 / 250_000.0),
            271_000..345_000 => Rate300Kbps(rate as f64 / 300_000.0),
            425_000..575_000 => Rate500Kbps(rate as f64 / 500_000.0),
            850_000..1_150_000 => Rate1000Kbps(rate as f64 / 1_000_000.0),
            _ => RateNonstandard(rate),
        }
    }
}

impl From<TrackDensity> for TrackDataRate {
    fn from(density: TrackDensity) -> Self {
        use TrackDensity::*;
        match density {
            Standard => TrackDataRate::Rate125Kbps(1.0),
            Double => TrackDataRate::Rate250Kbps(1.0),
            High => TrackDataRate::Rate500Kbps(1.0),
            Extended => TrackDataRate::Rate1000Kbps(1.0),
        }
    }
}

impl Display for TrackDataRate {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        use TrackDataRate::*;
        match self {
            RateNonstandard(rate) => write!(fmt, "*{}Kbps", rate / 1000),
            Rate125Kbps(f) => write!(fmt, "125Kbps (x{:.2})", f),
            Rate250Kbps(f) => write!(fmt, "250Kbps (x{:.2})", f),
            Rate300Kbps(f) => write!(fmt, "300Kbps (x{:.2})", f),
            Rate500Kbps(f) => write!(fmt, "500Kbps (x{:.2})", f),
            Rate1000Kbps(f) => write!(fmt, "1000Kbps (x{:.2})", f),
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
    /// Try to calculate a [DiskRpm] from the time between index pulses in milliseconds.
    /// Sometimes flux streams report bizarre RPMs, so you will need fallback logic if this
    /// conversion fails.
    pub fn try_from_index_time(time: f64) -> Option<DiskRpm> {
        let rpm = 60.0 / time;
        // We'd like to support a 15% deviation, but there is a small overlap between 300 +15%
        // and 360 -15%, so we split the difference at 327 RPM.
        match rpm {
            270.0..327.00 => Some(DiskRpm::Rpm300),
            327.0..414.00 => Some(DiskRpm::Rpm360),
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
        if matches!(self, DiskRpm::Rpm360) && base_clock >= 1.5e-6 {
            base_clock * (300.0 / 360.0)
        }
        else {
            base_clock
        }
    }
}

/// A DiskSelection enumeration is used to select a disk image by either index or path when dealing
/// with containers that contain multiple disk images.
#[derive(Clone, Debug)]
pub enum DiskSelection {
    /// Specify a disk image by index into a list of normally sorted path names within the container.
    Index(usize),
    /// Specify a disk image by path within the container.
    Path(PathBuf),
}

impl Display for DiskSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskSelection::Index(idx) => write!(f, "(Index: {})", idx),
            DiskSelection::Path(path) => write!(f, "(Path: {})", path.display()),
        }
    }
}

/// `DiskImageFileFormat` is an enumeration listing the various disk image file formats that can be
/// read or written by FluxFox.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, strum::EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskImageFileFormat {
    /// A raw sector image. Typically, has extensions IMG, IMA, DSK.
    RawSectorImage,
    /// An ImageDisk sector image. Typically has extension IMD.
    ImageDisk,
    /// A PCE sector image. Typically, has extension PSI.
    PceSectorImage,
    /// A PCE bitstream image. Typically, has extension PRI,
    PceBitstreamImage,
    /// A PCE flux stream image. Typically, has extension PFI.
    PceFluxImage,
    /// An MFM bitstream image. Typically, has extension MFM.
    MfmBitstreamImage,
    /// A TeleDisk sector image. Typically, has extension TD0.
    #[cfg(feature = "td0")]
    TeleDisk,
    /// A Kryoflux flux stream image. Typically, has extension RAW.
    KryofluxStream,
    /// An HFEv1 bitstream image. Typically, has extension HFE.
    HfeImage,
    /// An 86F bitstream image. Typically, has extension 86F.
    F86Image,
    /// A TransCopy bitstream image. Typically, has extension TC.
    TransCopyImage,
    /// A SuperCard Pro flux stream image. Typically, has extension SCP.
    SuperCardPro,
    /// A MAME floppy image. Typically, has extension MFI.
    #[cfg(feature = "mfi")]
    MameFloppyImage,
    /// Interchangeable Preservation Format image. Typically, has extension IPF.
    #[cfg(feature = "ipf")]
    IpfImage,
    /// MOOF - Applesauce Macintosh Disk Image
    #[cfg(feature = "moof")]
    MoofImage,
}

impl DiskImageFileFormat {
    /// Return the priority of the disk image format. Higher values are higher priority.
    /// Used to sort returned lists of disk image formats, hopefully returning the most desirable
    /// format first.
    pub fn priority(self) -> usize {
        use DiskImageFileFormat::*;
        match self {
            KryofluxStream => 0,
            // Supported bytestream formats (low priority)
            RawSectorImage => 1,
            #[cfg(feature = "td0")]
            TeleDisk => 0,
            ImageDisk => 0,

            PceSectorImage => 1,
            // Supported bitstream formats (high priority)
            TransCopyImage => 0,
            MfmBitstreamImage => 0,
            HfeImage => 0,
            PceBitstreamImage => 7,
            F86Image => 8,
            // Flux images (not supported for writes)
            SuperCardPro => 0,
            PceFluxImage => 0,
            #[cfg(feature = "mfi")]
            MameFloppyImage => 0,
            #[cfg(feature = "ipf")]
            IpfImage => 0,
            #[cfg(feature = "moof")]
            MoofImage => 0,
        }
    }

    pub fn resolution(self) -> TrackDataResolution {
        use DiskImageFileFormat::*;
        match self {
            RawSectorImage => TrackDataResolution::MetaSector,
            ImageDisk => TrackDataResolution::MetaSector,
            PceSectorImage => TrackDataResolution::MetaSector,
            PceBitstreamImage => TrackDataResolution::BitStream,
            MfmBitstreamImage => TrackDataResolution::BitStream,
            #[cfg(feature = "td0")]
            TeleDisk => TrackDataResolution::MetaSector,
            KryofluxStream => TrackDataResolution::FluxStream,
            HfeImage => TrackDataResolution::BitStream,
            F86Image => TrackDataResolution::BitStream,
            TransCopyImage => TrackDataResolution::BitStream,
            SuperCardPro => TrackDataResolution::FluxStream,
            PceFluxImage => TrackDataResolution::FluxStream,
            #[cfg(feature = "mfi")]
            MameFloppyImage => TrackDataResolution::FluxStream,
            #[cfg(feature = "ipf")]
            IpfImage => TrackDataResolution::BitStream,
            #[cfg(feature = "moof")]
            MoofImage => TrackDataResolution::BitStream,
        }
    }
}

impl Display for DiskImageFileFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use DiskImageFileFormat::*;
        let str = match self {
            RawSectorImage => "Raw Sector".to_string(),
            PceSectorImage => "PCE Sector".to_string(),
            PceBitstreamImage => "PCE Bitstream".to_string(),
            ImageDisk => "ImageDisk Sector".to_string(),
            #[cfg(feature = "td0")]
            TeleDisk => "TeleDisk Sector".to_string(),
            KryofluxStream => "Kryoflux Flux Stream".to_string(),
            MfmBitstreamImage => "HxC MFM Bitstream".to_string(),
            HfeImage => "HFEv1 Bitstream".to_string(),
            F86Image => "86F Bitstream".to_string(),
            TransCopyImage => "TransCopy Bitstream".to_string(),
            SuperCardPro => "SuperCard Pro Flux".to_string(),
            PceFluxImage => "PCE Flux Stream".to_string(),
            #[cfg(feature = "mfi")]
            MameFloppyImage => "MAME Flux Stream".to_string(),
            #[cfg(feature = "ipf")]
            IpfImage => "IPF Disk".to_string(),
            #[cfg(feature = "moof")]
            MoofImage => "MOOF Disk".to_string(),
        };
        write!(f, "{}", str)
    }
}

/// A [DiskFormat] enumeration describes the format of a disk image.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum DiskFormat {
    /// An unknown format. This is the default format for a disk image before a disk's format can
    /// be determined.
    Unknown,
    /// A non-standard disk format. This format is used for disk images that do not conform to a
    /// standard format, such a copy-protected titles that may have varying track lengths,
    /// non-consecutive sectors, or other non-standard features.
    Nonstandard,
    /// A standard disk format. This format is used for disk images that conform to a standard
    /// format type, determined by a `StandardFormat` enum.
    Standard(StandardFormat),
}

/// An enum that defines the scope of a track element read/write operation.
/// Not all operations for any given [TrackSchema] may support all scopes.
#[derive(Copy, Clone, Debug)]
pub enum RwScope {
    /// The operation will include the entire track data element, including address marker,
    /// CRC/checksum, or other track schema metadata.
    EntireElement,
    /// The operation will include only the element data. For sector data elements, this would
    /// return just the sector data, excluding address marker and CRC bytes.
    DataOnly,
    /// The operation will only affect the element CRC or Checksum.
    CrcOnly,
}

/// An enum that encompasses data integrity verification strategies.
/// Some track schemas may use a CRC to verify the integrity of the data on a track, others may
/// use a checksum.  Other types can be added here as needed as support for new track schemas is
/// added.
#[derive(Copy, Clone, Debug)]
pub enum IntegrityCheck {
    /// Represents the result of a 16-bit CRC (Cyclic Redundancy Check)
    Crc16(IntegrityField<u16>),
    /// Represents the result of a 16-bit checksum
    Checksum16(IntegrityField<u16>),
}

impl Display for IntegrityCheck {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use IntegrityCheck::*;
        match self {
            Crc16(result) if result.is_valid() => write!(f, "Valid"),
            Crc16(_) => write!(f, "Invalid"),
            Checksum16(result) if result.is_valid() => write!(f, "Valid"),
            Checksum16(_) => write!(f, "Invalid"),
        }
    }
}

impl IntegrityCheck {
    pub fn is_valid(&self) -> bool {
        use IntegrityCheck::*;
        match self {
            Crc16(result) => result.is_valid(),
            Checksum16(result) => result.is_valid(),
        }
    }
    pub fn is_error(&self) -> bool {
        !self.is_valid()
    }
}
