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
use crate::{
    io::{ReadSeek, ReadWriteSeek},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
};
use bitflags::bitflags;

pub mod compression;
pub mod f86;
pub mod hfe;
pub mod imd;
pub mod kryoflux;
mod mfi;
pub mod mfm;
pub mod pfi;
pub mod pri;
pub mod psi;
pub mod raw;
pub mod scp;
pub mod tc;
pub mod td0;

bitflags! {
    /// Bit flags representing the capabilities of a specific image format. Used to determine if a
    /// specific image format can represent a particular DiskImage.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        const CAP_HOLES             = 0b0000_0100_0000_0000; // Can store hole information
        const CAP_ENCODING_FM       = 0b0000_1000_0000_0000; // Can store FM encoding
        const CAP_ENCODING_MFM      = 0b0001_0000_0000_0000; // Can store MFM encoding
        const CAP_ENCODING_GCR      = 0b0010_0000_0000_0000; // Can store GCR encoding
        const CAP_NO_DAM            = 0b0100_0000_0000_0000; // Can store IDAM with no DAM
    }
}

/// Return a set of FormatCaps flags implicitly supported by the nature of any bitstream format.
pub fn bitstream_flags() -> FormatCaps {
    FormatCaps::CAP_VARIABLE_SPT
        | FormatCaps::CAP_VARIABLE_SSPT
        | FormatCaps::CAP_ADDRESS_CRC
        | FormatCaps::CAP_DATA_CRC
        | FormatCaps::CAP_DATA_DELETED
        | FormatCaps::CAP_SID_OVERRIDE
        | FormatCaps::CAP_NO_DAM
}

pub enum ParserWriteCompatibility {
    Ok,
    DataLoss,
    Incompatible,
    UnsupportedFormat,
}

pub(crate) const IMAGE_FORMATS: [DiskImageFileFormat; 12] = [
    DiskImageFileFormat::ImageDisk,
    DiskImageFileFormat::TeleDisk,
    DiskImageFileFormat::PceSectorImage,
    DiskImageFileFormat::PceBitstreamImage,
    DiskImageFileFormat::MfmBitstreamImage,
    DiskImageFileFormat::HfeImage,
    DiskImageFileFormat::F86Image,
    DiskImageFileFormat::TransCopyImage,
    DiskImageFileFormat::SuperCardPro,
    //DiskImageFormat::PceFluxImage,
    DiskImageFileFormat::MameFloppyImage,
    DiskImageFileFormat::KryofluxStream,
    DiskImageFileFormat::RawSectorImage,
];

/// Returns a list of advertised file extensions supported by available image format parsers.
/// This is a convenience function for use in file dialogs - internal image detection is not based
/// on file extension, but by image file content and size.
pub fn supported_extensions() -> Vec<&'static str> {
    IMAGE_FORMATS.iter().flat_map(|f| f.extensions()).collect()
}

/// Returns a DiskImageFormat enum variant based on the file extension provided. If the extension
/// is not recognized, None is returned.
pub fn format_from_ext(ext: &str) -> Option<DiskImageFileFormat> {
    for format in IMAGE_FORMATS.iter() {
        if format.extensions().contains(&ext.to_lowercase().as_str()) {
            return Some(*format);
        }
    }
    None
}

/// Returns a list of image formats and their associated file extensions that support the specified
/// capabilities.
pub fn formats_from_caps(caps: FormatCaps) -> Vec<(DiskImageFileFormat, Vec<String>)> {
    // if caps.is_empty() {
    //     log::warn!("formats_from_caps(): called with empty capabilities");
    // }

    let format_vec = IMAGE_FORMATS
        .iter()
        .filter(|f| caps.is_empty() || f.capabilities().contains(caps))
        .map(|f| (*f, f.extensions().iter().map(|s| s.to_string()).collect()))
        .collect();

    format_vec
}

pub fn filter_writable(image: &DiskImage, formats: Vec<DiskImageFileFormat>) -> Vec<DiskImageFileFormat> {
    formats
        .into_iter()
        .filter(|f| matches!(f.can_write(image), ParserWriteCompatibility::Ok))
        .collect()
}

/// Currently called via enum dispatch - implement on parsers directly?
pub trait ImageParser {
    /// Return the capability flags for this format.
    fn capabilities(&self) -> FormatCaps;
    /// Detect and return true if the image is of a format that the parser can read.
    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool;
    /// Return a list of file extensions associated with the parser.
    fn extensions(&self) -> Vec<&'static str>;
    /// Load a disk image file into an empty (default) DiskImage, or append a disk image file to an
    /// existing DiskImage.
    fn load_image<RWS: ReadSeek>(
        &self,
        read_buf: RWS,
        image: &mut DiskImage,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError>;
    /// Return true if the parser can write the specified disk image. Not all formats are writable
    /// at all, and not all DiskImages can be represented in the specified format.
    fn can_write(&self, image: &DiskImage) -> ParserWriteCompatibility;
    fn save_image<RWS: ReadWriteSeek>(self, image: &mut DiskImage, image_buf: &mut RWS) -> Result<(), DiskImageError>;
}

impl ImageParser for DiskImageFileFormat {
    fn capabilities(&self) -> FormatCaps {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::capabilities(),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::capabilities(),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::capabilities(),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::capabilities(),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::capabilities(),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::capabilities(),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::capabilities(),
            DiskImageFileFormat::F86Image => f86::F86Format::capabilities(),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::capabilities(),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::capabilities(),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::capabilities(),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::capabilities(),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::capabilities(),
        }
    }

    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::detect(image_buf),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::detect(image_buf),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::detect(image_buf),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::detect(image_buf),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::detect(image_buf),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::detect(image_buf),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::detect(image_buf),
            DiskImageFileFormat::F86Image => f86::F86Format::detect(image_buf),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::detect(image_buf),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::detect(image_buf),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::detect(image_buf),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::detect(image_buf),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::detect(image_buf),
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::extensions(),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::extensions(),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::extensions(),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::extensions(),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::extensions(),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::extensions(),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::extensions(),
            DiskImageFileFormat::F86Image => f86::F86Format::extensions(),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::extensions(),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::extensions(),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::extensions(),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::extensions(),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::extensions(),
        }
    }

    fn load_image<RWS: ReadSeek>(
        &self,
        read_buf: RWS,
        image: &mut DiskImage,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::load_image(read_buf, image, callback),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::F86Image => f86::F86Format::load_image(read_buf, image, callback),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::load_image(read_buf, image, callback),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::load_image(read_buf, image, callback),
        }
    }

    fn can_write(&self, image: &DiskImage) -> ParserWriteCompatibility {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::can_write(image),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::can_write(image),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::can_write(image),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::can_write(image),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::can_write(image),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::can_write(image),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::can_write(image),
            DiskImageFileFormat::F86Image => f86::F86Format::can_write(image),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::can_write(image),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::can_write(image),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::can_write(image),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::can_write(image),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::can_write(image),
        }
    }

    fn save_image<RWS: ReadWriteSeek>(self, image: &mut DiskImage, write_buf: &mut RWS) -> Result<(), DiskImageError> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::save_image(image, write_buf),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::save_image(image, write_buf),
            DiskImageFileFormat::TeleDisk => td0::Td0Format::save_image(image, write_buf),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::save_image(image, write_buf),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::save_image(image, write_buf),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::save_image(image, write_buf),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::save_image(image, write_buf),
            DiskImageFileFormat::F86Image => f86::F86Format::save_image(image, write_buf),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::save_image(image, write_buf),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::save_image(image, write_buf),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::save_image(image, write_buf),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::save_image(image, write_buf),
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::save_image(image, write_buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_ext_tc() {
        let ext = "tc";
        let expected_format = DiskImageFileFormat::TransCopyImage;
        let result = format_from_ext(ext);
        assert_eq!(result, Some(expected_format));
    }
}
