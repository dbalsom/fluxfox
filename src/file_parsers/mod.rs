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
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::{DiskImage, DiskImageError, DiskImageFormat};
use bitflags::bitflags;

pub mod compression;
pub mod hfe;
pub mod imd;
pub mod mfm;
pub mod pri;
pub mod psi;
pub mod raw;
pub mod td0;

bitflags! {
    /// Bit flags representing the capabilities of a specific image format. Used to determine if a
    /// specific image format can represent a particular DiskImage.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[rustfmt::skip]
    pub struct FormatCaps: u32 {
        const CAP_VARIABLE_SPT      = 0b0000_0000_0000_0001; // Can support variable sector counts per track
        const CAP_VARIABLE_SSPT     = 0b0000_0000_0000_0010; // Can support variable sector sizes
        const CAP_ADDRESS_CRC       = 0b0000_0000_0000_0100; // Encodes sector address mark CRC status
        const CAP_DATA_CRC          = 0b0000_0000_0000_1000; // Encodes sector data CRC status
        const CAP_DATA_DELETED      = 0b0000_0000_0001_0000; // Encodes 'Deleted address' marks
        const CAP_SID_OVERRIDE      = 0b0000_0000_0010_0000; // Can specify the sector ID parameters (chs, size) independent of sector order
        const CAP_COMMENT           = 0b0000_0000_0100_0000; // Can store a text comment field
        const CAP_TRACK_ENCODING    = 0b0000_0000_1000_0000; // Can store per-track encoding type
        const CAP_TRACK_DATA_RATE   = 0b0000_0001_0000_0000; // Can store per-track data rate
        const CAP_WEAK_BITS         = 0b0000_0010_0000_0000; // Can store weak bit information
    }
}

pub enum ParserWriteCompatibility {
    Ok,
    DataLoss,
    Incompatible,
    UnsupportedFormat,
}

/// A trait to be implemented by disk image parsers. Called via enum dispatch.
pub trait ImageParser {
    /// Return the capability flags for this format.
    fn capabilities(&self) -> FormatCaps;
    /// Detect and return true if the image is of a format that the parser can read.
    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool;
    /// Return a list of file extensions associated with the parser.
    fn extensions(&self) -> Vec<&'static str>;
    /// Create a DiskImage from the specified image buffer, or DiskImageError if the format is not supported.
    fn load_image<RWS: ReadSeek>(&self, image_buf: RWS) -> Result<DiskImage, DiskImageError>;
    /// Return true if the parser can write the specified disk image. Not all formats are writable
    /// at all, and not all DiskImages can be represented in the specified format.
    fn can_save(&self, image: &DiskImage) -> ParserWriteCompatibility;
    fn save_image<RWS: ReadWriteSeek>(self, image: &DiskImage, image_buf: &mut RWS) -> Result<(), DiskImageError>;
}

impl ImageParser for DiskImageFormat {
    fn capabilities(&self) -> FormatCaps {
        match self {
            DiskImageFormat::RawSectorImage => raw::RawFormat::capabilities(),
            DiskImageFormat::ImageDisk => imd::ImdFormat::capabilities(),
            DiskImageFormat::TeleDisk => td0::Td0Format::capabilities(),
            DiskImageFormat::PceSectorImage => psi::PsiFormat::capabilities(),
            DiskImageFormat::PceBitstreamImage => pri::PriFormat::capabilities(),
            DiskImageFormat::MfmBitstreamImage => mfm::MfmFormat::capabilities(),
            DiskImageFormat::HfeImage => hfe::HfeFormat::capabilities(),
            _ => FormatCaps::empty(),
        }
    }

    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool {
        match self {
            DiskImageFormat::RawSectorImage => raw::RawFormat::detect(image_buf),
            DiskImageFormat::ImageDisk => imd::ImdFormat::detect(image_buf),
            DiskImageFormat::TeleDisk => td0::Td0Format::detect(image_buf),
            DiskImageFormat::PceSectorImage => psi::PsiFormat::detect(image_buf),
            DiskImageFormat::PceBitstreamImage => pri::PriFormat::detect(image_buf),
            DiskImageFormat::MfmBitstreamImage => mfm::MfmFormat::detect(image_buf),
            DiskImageFormat::HfeImage => hfe::HfeFormat::detect(image_buf),
            _ => false,
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        match self {
            DiskImageFormat::RawSectorImage => vec!["img", "ima", "dsk", "bin"],
            DiskImageFormat::ImageDisk => vec!["imd"],
            DiskImageFormat::TeleDisk => vec!["td0"],
            DiskImageFormat::PceSectorImage => vec!["psi"],
            DiskImageFormat::PceBitstreamImage => vec!["pri"],
            DiskImageFormat::MfmBitstreamImage => vec!["mfm"],
            DiskImageFormat::HfeImage => vec!["hfe"],
            _ => vec![],
        }
    }

    fn load_image<RWS: ReadSeek>(&self, image_buf: RWS) -> Result<DiskImage, DiskImageError> {
        match self {
            DiskImageFormat::RawSectorImage => raw::RawFormat::load_image(image_buf),
            DiskImageFormat::ImageDisk => imd::ImdFormat::load_image(image_buf),
            DiskImageFormat::TeleDisk => td0::Td0Format::load_image(image_buf),
            DiskImageFormat::PceSectorImage => psi::PsiFormat::load_image(image_buf),
            DiskImageFormat::PceBitstreamImage => pri::PriFormat::load_image(image_buf),
            DiskImageFormat::MfmBitstreamImage => mfm::MfmFormat::load_image(image_buf),
            DiskImageFormat::HfeImage => hfe::HfeFormat::load_image(image_buf),
            _ => Err(DiskImageError::UnknownFormat),
        }
    }

    fn can_save(&self, image: &DiskImage) -> ParserWriteCompatibility {
        match self {
            DiskImageFormat::RawSectorImage => raw::RawFormat::can_write(image),
            DiskImageFormat::ImageDisk => imd::ImdFormat::can_write(image),
            DiskImageFormat::TeleDisk => td0::Td0Format::can_write(image),
            DiskImageFormat::PceSectorImage => psi::PsiFormat::can_write(image),
            DiskImageFormat::PceBitstreamImage => pri::PriFormat::can_write(image),
            DiskImageFormat::MfmBitstreamImage => mfm::MfmFormat::can_write(image),
            DiskImageFormat::HfeImage => hfe::HfeFormat::can_write(image),
            _ => ParserWriteCompatibility::UnsupportedFormat,
        }
    }

    fn save_image<RWS: ReadWriteSeek>(self, image: &DiskImage, image_buf: &mut RWS) -> Result<(), DiskImageError> {
        match self {
            DiskImageFormat::RawSectorImage => raw::RawFormat::save_image(image, image_buf),
            DiskImageFormat::ImageDisk => imd::ImdFormat::save_image(image, image_buf),
            DiskImageFormat::TeleDisk => td0::Td0Format::save_image(image, image_buf),
            DiskImageFormat::PceSectorImage => psi::PsiFormat::save_image(image, image_buf),
            DiskImageFormat::PceBitstreamImage => pri::PriFormat::save_image(image, image_buf),
            DiskImageFormat::MfmBitstreamImage => mfm::MfmFormat::save_image(image, image_buf),
            DiskImageFormat::HfeImage => hfe::HfeFormat::save_image(image, image_buf),
            _ => Err(DiskImageError::UnknownFormat),
        }
    }
}
