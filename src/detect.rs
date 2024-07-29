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

use crate::diskimage::{DiskImageFormat, FloppyFormat};
use crate::file_parsers::ImageParser;
use crate::io::ReadSeek;
use crate::DiskImageError;

const IMAGE_FORMATS: [DiskImageFormat; 6] = [
    DiskImageFormat::ImageDisk,
    DiskImageFormat::TeleDisk,
    DiskImageFormat::PceSectorImage,
    DiskImageFormat::PceBitstreamImage,
    DiskImageFormat::RawSectorImage,
    DiskImageFormat::MfmBitstreamImage,
];

/// Attempt to detect the format of a disk image. If the format cannot be determined, UnknownFormat is returned.
pub fn detect_image_format<T: ReadSeek>(image_io: &mut T) -> Result<DiskImageFormat, DiskImageError> {
    for format in IMAGE_FORMATS.iter() {
        if format.detect(&mut *image_io) {
            return Ok(*format);
        }
    }
    Err(DiskImageError::UnknownFormat)
}

/// Attempt to return a DiskChs structure representing the geometry of a disk image from the size of a raw sector image.
/// Returns None if the size does not match a known raw disk image size.
pub fn chs_from_raw_size(size: usize) -> Option<DiskChs> {
    let raw_size_fmt = FloppyFormat::from(size);
    if raw_size_fmt != FloppyFormat::Unknown {
        return Some(raw_size_fmt.get_chs());
    }
    None
}
