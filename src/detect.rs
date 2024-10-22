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
use crate::containers::zip;
use crate::containers::DiskImageContainer;
use crate::file_parsers::{kryoflux::KfxFormat, ImageParser, IMAGE_FORMATS};

use crate::io::ReadSeek;
use crate::standard_format::StandardFormat;
use crate::{DiskImageError, DiskImageFormat};
use std::path::PathBuf;

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
    // differentiated due to their large size and regular naming conventions.

    // The smallest kryoflux set I have seen is of a 160K disk, 5_741_334 bytes uncompressed.
    // Therefore, I assume a cutoff point of 5MB for Kryoflux sets.
    #[cfg(feature = "zip")]
    {
        // First of all, is the input file a zip?
        let (is_zip, file_ct, total_size) = zip::detect_zip(image_io);

        // If it is a zip, we need to check if it contains a supported image format
        if is_zip & (file_ct > 0) {
            log::debug!("ZIP file detected with {} files, total size: {}", file_ct, total_size);
            if file_ct == 1 {
                let file_buf = match zip::extract_first_file(image_io) {
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
            } else if total_size > 5_000_000 {
                // Multiple files in the zip, of at least 5MB
                // Get the file listing

                let zip_listing = zip::file_listing(image_io)?;
                let path_vec: Vec<PathBuf> = zip_listing
                    .files
                    .iter()
                    .map(|entry| PathBuf::from(&entry.name))
                    .collect();

                // Get the first file in the listing with a 'raw' extension
                let first_raw = path_vec
                    .iter()
                    .find(|&path| path.extension().unwrap_or_default() == "raw");

                if let Some(raw_path) = first_raw {
                    log::debug!("Found .raw file in zip: {:?}", raw_path);
                    let kryo_set = KfxFormat::expand_kryoflux_set(raw_path.clone(), Some(path_vec))?;
                    log::debug!(
                        "Expanded to kryoflux set of {} files, geometry: {}",
                        kryo_set.0.len(),
                        kryo_set.1
                    );

                    // We could assume we need at least 40 files to be a kryoflux set
                    if kryo_set.0.len() > 39 {
                        return Ok(DiskImageContainer::ZippedKryofluxSet(kryo_set.0, kryo_set.1));
                    }
                }
            }
        }
    }

    for format in IMAGE_FORMATS.iter() {
        if format.detect(&mut *image_io) {
            if let DiskImageFormat::KryofluxStream = format {
                return Ok(DiskImageContainer::KryofluxSet);
            }
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
