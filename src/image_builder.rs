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

    src/image_builder.rs

    Implements the Builder pattern for DiskImage objects.

    Allows for creation of blank or pre-formatted DiskImages.
*/

use crate::diskimage::DiskImageFlags;
use crate::{DiskCh, DiskDataResolution, DiskImage, DiskImageError, StandardFormat};

/// Implements the Builder pattern for DiskImage objects.
/// Allows for creation of blank or pre-formatted DiskImages.
#[derive(Default)]
pub struct ImageBuilder {
    #[doc = "Specify the [`StandardFormat`] to use for the [`DiskImage`] to be built."]
    pub standard_format: Option<StandardFormat>,
    #[doc = "Specify the [`DiskDataResolution`] to use for the DiskImage to be built."]
    pub resolution: Option<DiskDataResolution>,
    #[doc = "Specify the creator tag to display during boot."]
    pub creator_tag: Option<[u8; 8]>,
    #[doc = "Specify whether the DiskImage should be formatted."]
    pub formatted: bool,
}

impl ImageBuilder {
    pub fn new() -> ImageBuilder {
        Default::default()
    }

    /// Set the [`StandardFormat`] to use for the [`DiskImage`] to be built.
    pub fn with_standard_format(mut self, standard_format: StandardFormat) -> ImageBuilder {
        self.standard_format = Some(standard_format);
        self
    }

    /// Set the [`DiskDataResolution`] to use for the [`DiskImage`] to be built.
    pub fn with_resolution(mut self, resolution: DiskDataResolution) -> ImageBuilder {
        self.resolution = Some(resolution);
        self
    }

    /// Set whether the [`DiskImage`] to be built should be formatted.
    /// If this is not set, the DiskImage will be created as a blank image which must be formatted
    /// before it can be read in a disk drive or emulator.
    pub fn with_formatted(mut self, formatted: bool) -> ImageBuilder {
        self.formatted = formatted;
        self
    }

    /// Set the creator tag for the [`DiskImage`] to be built. This is only used if the [`DiskImage`]
    /// is to be formatted.
    pub fn with_creator_tag(mut self, creator_tag: &[u8]) -> ImageBuilder {
        let mut new_creator_tag = [0x20; 8];
        let max_len = creator_tag.len().min(8);
        new_creator_tag[..max_len].copy_from_slice(&creator_tag[..max_len]);

        self.creator_tag = Some(new_creator_tag);
        self
    }

    /// Build the [`DiskImage`] using the specified parameters.
    pub fn build(self) -> Result<DiskImage, DiskImageError> {
        if self.resolution.is_none() {
            log::error!("DiskDataResolution not set");
            return Err(DiskImageError::ParameterError);
        }

        if self.standard_format.is_some() {
            match self.resolution {
                Some(DiskDataResolution::BitStream) => self.build_bitstream(),
                None | Some(DiskDataResolution::ByteStream) => self.build_bytestream(),
                _ => Err(DiskImageError::UnsupportedFormat),
            }
        } else {
            Err(DiskImageError::UnsupportedFormat)
        }
    }

    fn build_bitstream(self) -> Result<DiskImage, DiskImageError> {
        let format = self.standard_format.unwrap();
        let mut disk_image = DiskImage::create(format);
        disk_image.set_resolution(DiskDataResolution::BitStream);

        let chsn = format.get_chsn();
        let encoding = format.get_encoding();
        let data_rate = format.get_data_rate();
        let bitcell_size = format.get_bitcell_ct();

        for head in 0..chsn.h() {
            for cylinder in 0..chsn.c() {
                let ch = DiskCh::new(cylinder, head);
                disk_image.add_empty_track(ch, encoding, data_rate, bitcell_size)?;
            }
        }

        if self.formatted {
            disk_image.format(format, None, self.creator_tag.as_ref())?;
        }

        // Do post-load processing as normal
        disk_image.post_load_process();

        // Clear dirty flag
        disk_image.clear_flag(DiskImageFlags::DIRTY);

        Ok(disk_image)
    }

    fn build_bytestream(self) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::create(self.standard_format.unwrap());
        disk_image.set_resolution(DiskDataResolution::ByteStream);

        // Do post-load processing as normal
        disk_image.post_load_process();

        // Clear dirty flag
        disk_image.clear_flag(DiskImageFlags::DIRTY);

        Ok(disk_image)
    }
}
