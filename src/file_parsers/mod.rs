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
use crate::io::SeekFrom;
use bitflags::bitflags;

pub mod compression;
pub mod f86;
pub mod hfe;
pub mod imd;
#[cfg(feature = "ipf")]
pub mod ipf;
pub mod kryoflux;
#[cfg(feature = "mfi")]
pub mod mfi;
pub mod mfm;
#[cfg(feature = "moof")]
pub mod moof;
pub mod pce;
pub mod raw;
pub mod scp;
pub mod tc;
#[cfg(feature = "td0")]
pub mod td0;

#[cfg(feature = "async")]
use std::sync::{Arc, Mutex};

use pce::{pfi, pri, psi};

use crate::{
    io::{ReadSeek, ReadWriteSeek},
    types::Platform,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
};

use strum::IntoEnumIterator;

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct ParserReadOptions {
    platform: Option<Platform>, // If we know the platform, we can give it to the parser as a hint if the platform is otherwise ambiguous.
    flags:    ReadFlags,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct ParserWriteOptions {
    platform: Option<Platform>, // If we know the platform, we can give it to the parser as a hint if the platform is otherwise ambiguous.
}

bitflags! {
    /// Bit flags representing reading options passed to a disk image file parser.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[rustfmt::skip]
    pub struct ReadFlags: u32 {
        const ERRORS_TO_WEAK_BITS     = 0b0000_0000_0000_0001; // Convert MFM errors to weak bits
        const NFA_TO_WEAK_BITS        = 0b0000_0000_0000_0010; // Convert NFA zones to weak bits
        const DETECT_WEAK_BITS        = 0b0000_0000_0000_0100; // Analyze multiple revolutions for weak bits (requires flux image)
        const WEAK_BITS_TO_HOLES      = 0b0000_0000_0000_1000; // Convert weak bits to holes
        const CREATE_SOURCE_MAP       = 0b0000_0000_0001_0000; // Generate a SourceMap for the image (not all parsers support)
    }
}

bitflags! {
    /// Bit flags representing writing options passed to a disk image file parser.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[rustfmt::skip]
    pub struct WriteFlags: u32 {
        const REUSE_SOURCE_FLUX = 0b0000_0000_0000_0001; // Reuse existing flux data if track is unmodified
        const RESOLVE_FLUX      = 0b0000_0000_0000_0010; // Write a single revolution to a flux image
    }
}

bitflags! {
    /// Bit flags representing the capabilities of a specific image format. Used to determine if a
    /// specific image format can represent a particular [DiskImage].
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Describes the basic write compatibility of a [DiskImage] disk image as determined by a specific
/// file format parser.
/// - `Ok`: The image is compatible with the parser and can be read or written without data loss.
/// - `DataLoss`: The image is compatible with the parser, but some data may be lost when reading or
///    writing.
/// - `Incompatible`: The image is not compatible with the parser and cannot be written.
/// - `UnsupportedFormat`: The parser does not support writing.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ParserWriteCompatibility {
    Ok,
    DataLoss,
    Incompatible,
    UnsupportedFormat,
}

/// Returns a list of advertised file extensions supported by available image format parsers.
/// This is a convenience function for use in file dialogs - internal image detection is not based
/// on file extension, but by image file content (and occasionally size, in the case of raw sector
/// images)
pub fn supported_extensions() -> Vec<&'static str> {
    let mut ext_vec: Vec<&str> = DiskImageFileFormat::iter().flat_map(|f| f.extensions()).collect();
    ext_vec.sort();
    ext_vec.dedup();
    ext_vec
}

/// Returns a DiskImageFormat enum variant based on the file extension provided. If the extension
/// is not recognized, None is returned.
pub fn format_from_ext(ext: &str) -> Option<DiskImageFileFormat> {
    for format in DiskImageFileFormat::iter() {
        if format.extensions().contains(&ext.to_lowercase().as_str()) {
            return Some(format);
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
    let format_vec = DiskImageFileFormat::iter()
        .filter(|f| caps.is_empty() || f.capabilities().contains(caps))
        .map(|f| (f, f.extensions().iter().map(|s| s.to_string()).collect()))
        .collect();

    format_vec
}

pub fn filter_writable(image: &DiskImage, formats: Vec<DiskImageFileFormat>) -> Vec<DiskImageFileFormat> {
    formats
        .into_iter()
        .filter(|f| matches!(f.can_write(Some(image)), ParserWriteCompatibility::Ok))
        .collect()
}

/// A trait interface for defining a disk image file format parser.
/// An [ImageFormatParser] should not be used directly - a disk image should be loaded using an [ImageLoader] struct.
pub trait ImageFormatParser {
    /// Return the [DiskImageFileFormat] enum variant associated with the parser.
    fn format(&self) -> DiskImageFileFormat;

    /// Return the capability flags for this format.
    fn capabilities(&self) -> FormatCaps;

    /// Return a list of [Platform]s that are supported by the image format.
    fn platforms(&self) -> Vec<Platform>;

    /// Detect and return true if the image is of a format that the parser can read.
    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool;
    /// Return a list of file extensions associated with the parser.
    fn extensions(&self) -> Vec<&'static str>;
    /// Load a disk image file into an empty [DiskImage], or append a disk image file to an
    /// existing [DiskImage].
    fn load_image<RWS: ReadSeek>(
        &self,
        read_buf: RWS,
        image: &mut DiskImage,
        opts: &ParserReadOptions,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError>;

    /// Load a disk image file into an empty [DiskImage], or append a disk image file to an
    /// existing [DiskImage]. This function is async and should be used in async contexts.
    #[cfg(feature = "async")]
    #[allow(async_fn_in_trait)]
    async fn load_image_async<RWS: ReadSeek + Send + 'static>(
        &self,
        read_buf: RWS,
        image: Arc<Mutex<DiskImage>>,
        opts: &ParserReadOptions,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError>;

    /// Determine if the parser can write an image back to its own format.
    /// # Arguments
    /// * `image` - An `Option<DiskImage>` either specifying the [DiskImage] to check for write
    ///             compatibility, or `None` if the parser should check for general write support.
    fn can_write(&self, image: Option<&DiskImage>) -> ParserWriteCompatibility;
    fn save_image<RWS: ReadWriteSeek>(
        self,
        image: &mut DiskImage,
        opts: &ParserWriteOptions,
        image_buf: &mut RWS,
    ) -> Result<(), DiskImageError>;
}

impl ImageFormatParser for DiskImageFileFormat {
    fn format(&self) -> DiskImageFileFormat {
        *self
    }

    fn capabilities(&self) -> FormatCaps {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::capabilities(),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::capabilities(),
            #[cfg(feature = "td0")]
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
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::capabilities(),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::capabilities(),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::capabilities(),
        }
    }

    fn platforms(&self) -> Vec<Platform> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::platforms(),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::platforms(),
            #[cfg(feature = "td0")]
            DiskImageFileFormat::TeleDisk => td0::Td0Format::platforms(),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::platforms(),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::platforms(),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::platforms(),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::platforms(),
            DiskImageFileFormat::F86Image => f86::F86Format::platforms(),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::platforms(),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::platforms(),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::platforms(),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::platforms(),
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::platforms(),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::platforms(),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::platforms(),
        }
    }

    fn detect<RWS: ReadSeek>(&self, image_buf: RWS) -> bool {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::detect(image_buf),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::detect(image_buf),
            #[cfg(feature = "td0")]
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
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::detect(image_buf),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::detect(image_buf),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::detect(image_buf),
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::extensions(),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::extensions(),
            #[cfg(feature = "td0")]
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
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::extensions(),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::extensions(),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::extensions(),
        }
    }

    fn load_image<RWS: ReadSeek>(
        &self,
        read_buf: RWS,
        image: &mut DiskImage,
        opts: &ParserReadOptions,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::load_image(read_buf, image, opts, callback),
            #[cfg(feature = "td0")]
            DiskImageFileFormat::TeleDisk => td0::Td0Format::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::F86Image => f86::F86Format::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::load_image(read_buf, image, opts, callback),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::load_image(read_buf, image, opts, callback),
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::load_image(read_buf, image, opts, callback),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::load_image(read_buf, image, opts, callback),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::load_image(read_buf, image, opts, callback),
        }
    }

    #[cfg(feature = "async")]
    async fn load_image_async<RWS: ReadSeek + Send + 'static>(
        &self,
        read_buf: RWS,
        image: Arc<Mutex<DiskImage>>,
        opts: &ParserReadOptions,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        // For WASM, use `spawn_local` to run synchronously on the main thread
        #[cfg(target_arch = "wasm32")]
        {
            let self_clone = self.clone();
            let opts_clone = opts.clone();
            let task = async move {
                let mut img = image.lock().unwrap();
                match self_clone.load_image(read_buf, &mut img, &opts_clone, callback) {
                    Ok(_) => (),
                    Err(e) => log::error!("Error loading image: {:?}", e),
                }
            };
            wasm_bindgen_futures::spawn_local(task);
            // RustRover gets confused about the conditional compilation here
            #[allow(clippy::needless_return)]
            return Ok(());
        }

        // For non-WASM, use `tokio::task::spawn_blocking` to avoid blocking the async runtime
        #[cfg(feature = "tokio-async")]
        {
            let self_clone = self.clone();
            let opts_clone = opts.clone();
            tokio::task::spawn_blocking(move || {
                let mut img = image.lock().unwrap();
                self_clone.load_image(read_buf, &mut img, &opts_clone, callback)
            })
            .await
            .map_err(|e| DiskImageError::IoError(e.to_string()))?
        }
    }

    fn can_write(&self, image: Option<&DiskImage>) -> ParserWriteCompatibility {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::can_write(image),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::can_write(image),
            #[cfg(feature = "td0")]
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
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::can_write(image),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::can_write(image),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::can_write(image),
        }
    }

    fn save_image<RWS: ReadWriteSeek>(
        self,
        image: &mut DiskImage,
        opts: &ParserWriteOptions,
        write_buf: &mut RWS,
    ) -> Result<(), DiskImageError> {
        match self {
            DiskImageFileFormat::RawSectorImage => raw::RawFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::ImageDisk => imd::ImdFormat::save_image(image, opts, write_buf),
            #[cfg(feature = "td0")]
            DiskImageFileFormat::TeleDisk => td0::Td0Format::save_image(image, opts, write_buf),
            DiskImageFileFormat::PceSectorImage => psi::PsiFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::PceBitstreamImage => pri::PriFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::MfmBitstreamImage => mfm::MfmFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::HfeImage => hfe::HfeFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::F86Image => f86::F86Format::save_image(image, opts, write_buf),
            DiskImageFileFormat::TransCopyImage => tc::TCFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::SuperCardPro => scp::ScpFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::PceFluxImage => pfi::PfiFormat::save_image(image, opts, write_buf),
            DiskImageFileFormat::KryofluxStream => kryoflux::KfxFormat::save_image(image, opts, write_buf),
            #[cfg(feature = "mfi")]
            DiskImageFileFormat::MameFloppyImage => mfi::MfiFormat::save_image(image, opts, write_buf),
            #[cfg(feature = "ipf")]
            DiskImageFileFormat::IpfImage => ipf::IpFormat::save_image(image, opts, write_buf),
            #[cfg(feature = "moof")]
            DiskImageFileFormat::MoofImage => moof::MoofFormat::save_image(image, opts, write_buf),
        }
    }
}

// Helper function to retrieve the length of a reader
fn reader_len<R: ReadSeek>(reader: &mut R) -> Result<u64, DiskImageError> {
    let pos = reader.seek(SeekFrom::Current(0))?;
    let len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(pos))?;
    Ok(len)
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
