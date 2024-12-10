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
use crate::StandardFormat;
use std::{
    fmt,
    fmt::{Display, Formatter},
    path::PathBuf,
};

/// The type of computer system that a disk image is intended to be used with - not necessarily the
/// system that the disk image was created on.
///
/// A `Platform` may be used as a hint to a disk image format parser, or provided in a
/// [BitStreamTrackParams] struct to help determine the appropriate [TrackSchema] for a track.
/// A `Platform` may not be specified (or reliable) in all disk image formats, nor can it always
/// be determined from a [DiskImage] (High density MFM Macintosh 3.5" diskettes look nearly
/// identical to PC 3.5" diskettes, unless you examine the boot sector).
/// It may be the most pragmatic option to have the user specify the platform when loading/saving a
/// disk image.
#[repr(usize)]
#[derive(Copy, Clone, Debug, strum::EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Platform {
    #[doc = "IBM PC and compatibles"]
    IbmPc,
    #[doc = "Commodore Amiga"]
    Amiga,
    #[doc = "Apple Macintosh"]
    Macintosh,
}

impl Display for Platform {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Platform::IbmPc => write!(f, "IBM PC"),
            Platform::Amiga => write!(f, "Commodore Amiga"),
            Platform::Macintosh => write!(f, "Apple Macintosh"),
        }
    }
}

impl From<StandardFormat> for Platform {
    fn from(format: StandardFormat) -> Self {
        use StandardFormat::*;
        match format {
            PcFloppy160 | PcFloppy180 | PcFloppy320 | PcFloppy360 | PcFloppy720 | PcFloppy1200 | PcFloppy1440
            | PcFloppy2880 => Platform::IbmPc,
            #[cfg(feature = "amiga")]
            AmigaFloppy880 => Platform::Amiga,
        }
    }
}

/// The resolution of the data in the disk image.
/// fluxfox supports three types of disk images:
/// * MetaSector images hold only sector data along with optional metadata per sector.
/// * BitStream images hold a bitwise representation of each track on a disk.
/// * FluxStream images hold one or more `revolutions` of flux transition delta times per track,
///   which are resolved to a single bitstream.
///
#[repr(usize)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskDataResolution {
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

/// The density of a track on a disk.
/// A disk image may contain tracks with different densities.
///
/// * 'Standard' density: referring to FM encoding, typically used by 8" diskettes.
/// * 'Double' density: referring to MFM encoding at 250/300Kbps. Appeared on 5.25" and 3.5" diskettes.
/// * 'High' density: referring to MFM encoding at 500Kbps. Appeared on 5.25" and 3.5" diskettes.
/// * 'Extended' density: referring to MFM encoding at 1Mbps. Appeared on 3.5" diskettes.
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
        match rate {
            TrackDataRate::Rate125Kbps(_) => TrackDensity::Standard,
            TrackDataRate::Rate250Kbps(_) => TrackDensity::Double,
            TrackDataRate::Rate500Kbps(_) => TrackDensity::High,
            TrackDataRate::Rate1000Kbps(_) => TrackDensity::Extended,
            _ => TrackDensity::Double,
        }
    }
}

impl Display for TrackDensity {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TrackDensity::Standard => write!(f, "Standard"),
            TrackDensity::Double => write!(f, "Double"),
            TrackDensity::High => write!(f, "High"),
            TrackDensity::Extended => write!(f, "Extended"),
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
        match (self, rpm) {
            (TrackDensity::Standard, _) => Some(50_000),
            (TrackDensity::Double, _) => Some(100_000),
            (TrackDensity::High, Some(DiskRpm::Rpm360)) => Some(166_666),
            (TrackDensity::High, Some(DiskRpm::Rpm300) | None) => Some(200_000),
            (TrackDensity::Extended, _) => Some(400_000),
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
        match rate {
            TrackDataRate::Rate125Kbps(f) => (125_000.0 * f) as u32,
            TrackDataRate::Rate250Kbps(f) => (250_000.0 * f) as u32,
            TrackDataRate::Rate300Kbps(f) => (300_000.0 * f) as u32,
            TrackDataRate::Rate500Kbps(f) => (500_000.0 * f) as u32,
            TrackDataRate::Rate1000Kbps(f) => (1_000_000.0 * f) as u32,
            TrackDataRate::RateNonstandard(rate) => rate,
        }
    }
}

/// Implement a conversion from a u32 to a DiskDataRate.
/// An 8-15% rate deviance is allowed for standard rates, otherwise a RateNonstandard is returned.
impl From<u32> for TrackDataRate {
    fn from(rate: u32) -> Self {
        match rate {
            93_750..143_750 => TrackDataRate::Rate125Kbps(rate as f64 / 125_000.0),
            212_000..271_000 => TrackDataRate::Rate250Kbps(rate as f64 / 250_000.0),
            271_000..345_000 => TrackDataRate::Rate300Kbps(rate as f64 / 300_000.0),
            425_000..575_000 => TrackDataRate::Rate500Kbps(rate as f64 / 500_000.0),
            850_000..1_150_000 => TrackDataRate::Rate1000Kbps(rate as f64 / 1_000_000.0),
            _ => TrackDataRate::RateNonstandard(rate),
        }
    }
}

impl From<TrackDensity> for TrackDataRate {
    fn from(density: TrackDensity) -> Self {
        match density {
            TrackDensity::Standard => TrackDataRate::Rate125Kbps(1.0),
            TrackDensity::Double => TrackDataRate::Rate250Kbps(1.0),
            TrackDensity::High => TrackDataRate::Rate500Kbps(1.0),
            TrackDensity::Extended => TrackDataRate::Rate1000Kbps(1.0),
        }
    }
}

impl Display for TrackDataRate {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            TrackDataRate::RateNonstandard(rate) => write!(fmt, "*{}Kbps", rate / 1000),
            TrackDataRate::Rate125Kbps(f) => write!(fmt, "125Kbps (x{:.2})", f),
            TrackDataRate::Rate250Kbps(f) => write!(fmt, "250Kbps (x{:.2})", f),
            TrackDataRate::Rate300Kbps(f) => write!(fmt, "300Kbps (x{:.2})", f),
            TrackDataRate::Rate500Kbps(f) => write!(fmt, "500Kbps (x{:.2})", f),
            TrackDataRate::Rate1000Kbps(f) => write!(fmt, "1000Kbps (x{:.2})", f),
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
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
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
    #[cfg(feature = "adf")]
    AmigaDiskFile,
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
            #[cfg(feature = "adf")]
            AmigaDiskFile => 0,
        }
    }

    pub fn resolution(self) -> DiskDataResolution {
        use DiskImageFileFormat::*;
        match self {
            RawSectorImage => DiskDataResolution::MetaSector,
            ImageDisk => DiskDataResolution::MetaSector,
            PceSectorImage => DiskDataResolution::MetaSector,
            PceBitstreamImage => DiskDataResolution::BitStream,
            MfmBitstreamImage => DiskDataResolution::BitStream,
            #[cfg(feature = "td0")]
            TeleDisk => DiskDataResolution::MetaSector,
            KryofluxStream => DiskDataResolution::FluxStream,
            HfeImage => DiskDataResolution::BitStream,
            F86Image => DiskDataResolution::BitStream,
            TransCopyImage => DiskDataResolution::BitStream,
            SuperCardPro => DiskDataResolution::FluxStream,
            PceFluxImage => DiskDataResolution::FluxStream,
            #[cfg(feature = "mfi")]
            MameFloppyImage => DiskDataResolution::FluxStream,
            #[cfg(feature = "adf")]
            AmigaDiskFile => DiskDataResolution::MetaSector,
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
            #[cfg(feature = "adf")]
            AmigaDiskFile => "Amiga Disk File".to_string(),
        };
        write!(f, "{}", str)
    }
}

/// A `DiskFormat` enumeration describes the format of a disk image.
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

/// An enum that defines the scope of a sector operation.
#[derive(Copy, Clone, Debug)]
pub enum RwSectorScope {
    /// The operation will include the entire data element, including address marker and CRC bytes.
    DataElement,
    /// The operation will include only the sector data, excluding address marker and CRC bytes.
    DataOnly,
    /// The operation will only affect the sector CRC.
    CrcOnly,
}
