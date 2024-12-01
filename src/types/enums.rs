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
use crate::{DiskChs, DiskDataResolution, StandardFormat};
use std::{fmt::Display, path::PathBuf};

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
}

impl DiskImageFileFormat {
    /// Return the priority of the disk image format. Higher values are higher priority.
    /// Used to sort returned lists of disk image formats, hopefully returning the most desirable
    /// format first.
    pub fn priority(self) -> usize {
        match self {
            DiskImageFileFormat::KryofluxStream => 0,
            // Supported bytestream formats (low priority)
            DiskImageFileFormat::RawSectorImage => 1,
            DiskImageFileFormat::TeleDisk => 0,
            DiskImageFileFormat::ImageDisk => 0,

            DiskImageFileFormat::PceSectorImage => 1,
            // Supported bitstream formats (high priority)
            DiskImageFileFormat::TransCopyImage => 0,
            DiskImageFileFormat::MfmBitstreamImage => 0,
            DiskImageFileFormat::HfeImage => 0,
            DiskImageFileFormat::PceBitstreamImage => 7,
            DiskImageFileFormat::F86Image => 8,
            // Flux images (not supported for writes)
            DiskImageFileFormat::SuperCardPro => 0,
            DiskImageFileFormat::PceFluxImage => 0,
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => 0,
        }
    }

    pub fn resolution(self) -> DiskDataResolution {
        match self {
            DiskImageFileFormat::RawSectorImage => DiskDataResolution::MetaSector,
            DiskImageFileFormat::ImageDisk => DiskDataResolution::MetaSector,
            DiskImageFileFormat::PceSectorImage => DiskDataResolution::MetaSector,
            DiskImageFileFormat::PceBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::MfmBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::TeleDisk => DiskDataResolution::MetaSector,
            DiskImageFileFormat::KryofluxStream => DiskDataResolution::FluxStream,
            DiskImageFileFormat::HfeImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::F86Image => DiskDataResolution::BitStream,
            DiskImageFileFormat::TransCopyImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::SuperCardPro => DiskDataResolution::FluxStream,
            DiskImageFileFormat::PceFluxImage => DiskDataResolution::FluxStream,
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => DiskDataResolution::FluxStream,
        }
    }
}

impl Display for DiskImageFileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFileFormat::RawSectorImage => "Raw Sector".to_string(),
            DiskImageFileFormat::PceSectorImage => "PCE Sector".to_string(),
            DiskImageFileFormat::PceBitstreamImage => "PCE Bitstream".to_string(),
            DiskImageFileFormat::ImageDisk => "ImageDisk Sector".to_string(),
            DiskImageFileFormat::TeleDisk => "TeleDisk Sector".to_string(),
            DiskImageFileFormat::KryofluxStream => "Kryoflux Flux Stream".to_string(),
            DiskImageFileFormat::MfmBitstreamImage => "HxC MFM Bitstream".to_string(),
            DiskImageFileFormat::HfeImage => "HFEv1 Bitstream".to_string(),
            DiskImageFileFormat::F86Image => "86F Bitstream".to_string(),
            DiskImageFileFormat::TransCopyImage => "TransCopy Bitstream".to_string(),
            DiskImageFileFormat::SuperCardPro => "SuperCard Pro Flux".to_string(),
            DiskImageFileFormat::PceFluxImage => "PCE Flux Stream".to_string(),
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => "MAME Flux Stream".to_string(),
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
    Nonstandard(DiskChs),
    /// A standard disk format. This format is used for disk images that conform to a standard
    /// IBM PC format type, determined by a `StandardFormat` enum.
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
