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
use crate::chs::DiskChs;

use crate::containers::zip::{detect_zip, extract_first_file};
use crate::containers::DiskImageContainer;
use crate::diskimage::DiskImageFormat;
use crate::file_parsers::ImageParser;
use crate::io::ReadSeek;
use crate::standard_format::StandardFormat;
use crate::DiskImageError;

const IMAGE_FORMATS: [DiskImageFormat; 8] = [
    DiskImageFormat::ImageDisk,
    DiskImageFormat::TeleDisk,
    DiskImageFormat::PceSectorImage,
    DiskImageFormat::PceBitstreamImage,
    DiskImageFormat::RawSectorImage,
    DiskImageFormat::MfmBitstreamImage,
    DiskImageFormat::HfeImage,
    DiskImageFormat::F86Image,
];

/// Returns a list of advertised file extensions supported by available image format parsers.
/// This is a convenience function for use in file dialogs - internal image detection is not based
/// on file extension, but by image file content and size.
pub fn supported_extensions() -> Vec<&'static str> {
    IMAGE_FORMATS.iter().flat_map(|f| f.extensions()).collect()
}

/// Attempt to detect the format of a disk image. If the format cannot be determined, UnknownFormat is returned.
pub fn detect_image_format<T: ReadSeek>(image_io: &mut T) -> Result<DiskImageContainer, DiskImageError> {
    // If the zip feature is present, we can look into and identify images in zip files.
    // Most common of these are WinImage's "Compressed Disk Image" format, IMZ, which is simply
    // a zip file containing a single raw disk image with .IMA extension.
    // However, we can extend this to support simple zip containers of other file image formats.
    // Note only the first file in the ZIP is checked.

    // Given MartyPC's feature set, it can either mount the files within a zip as a FAT filesystem,
    // or it can let fluxfox mount the image inside the zip instead. Choosing which is the correct
    // behavior desired may not be trivial; so we assume a compressed disk image will only contain
    // one file. You could override loading by adding a second dummy file.

    // The exception to this are Kryoflux images which span multiple files, but these  are easily
    // differentiated due to their large size and regular naming conventions. When/if support is
    // added for Kryoflux, Kryoflux images will be treated as zip files themselves, instead of
    // containers to be examined.
    #[cfg(feature = "zip")]
    {
        // First of all, is the input file a zip?
        let (is_zip, file_ct) = detect_zip(image_io);

        // If it is a zip, we need to check if it contains a supported image format
        if is_zip && file_ct == 1 {
            let file_buf = match extract_first_file(image_io) {
                Ok(buf) => buf,
                Err(e) => return Err(e),
            };

            // Wrap buffer in Cursor, and send it through all the format detectors.
            let mut file_io = std::io::Cursor::new(file_buf);
            for format in IMAGE_FORMATS.iter() {
                if format.detect(&mut file_io) {
                    return Ok(DiskImageContainer::Zip(*format));
                }
            }

            return Err(DiskImageError::UnknownFormat);
        }
    }

    for format in IMAGE_FORMATS.iter() {
        if format.detect(&mut *image_io) {
            return Ok(DiskImageContainer::Raw(*format));
        }
    }
    Err(DiskImageError::UnknownFormat)
}

/// Attempt to return a DiskChs structure representing the geometry of a disk image from the size of a raw sector image.
/// Returns None if the size does not match a known raw disk image size.
pub fn chs_from_raw_size(size: usize) -> Option<DiskChs> {
    let raw_size_fmt = StandardFormat::from(size);
    if raw_size_fmt != StandardFormat::Invalid {
        return Some(raw_size_fmt.get_chs());
    }
    None
}
