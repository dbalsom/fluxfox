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
#![allow(dead_code)]
//! A module that implements a builder pattern for [DiskImage] designed around
//! creation of a [DiskImage] from an existing disk image file.
//! It is possible to call DiskImage::load() directly, but this module provides
//! a simpler interface for doing so which is less likely to break in future
//! versions of the library.

use crate::{prelude::DiskDataResolution, types::Platform, DiskImage, DiskImageError, DiskImageFileFormat};
use std::path::PathBuf;

/// Implements the Builder pattern for DiskImage objects.
/// Allows for creation of blank or pre-formatted DiskImages.
#[derive(Default)]
pub struct ImageLoader {
    /// Restrict loading of disk images to disks identified to belong to a
    /// specific platform.
    pub(crate) platform: Option<Platform>,
    /// Restrict loading of disk images to the specified image file format.
    /// This will bypass automatic format detection.
    pub(crate) format: Option<DiskImageFileFormat>,
    /// If a disk image can be resolved to different resolutions, this field
    /// will determine which resolution to use. If a disk image format does
    /// not support multiple resolutions, this field will be ignored.
    pub(crate) resolution: Option<DiskDataResolution>,
    /// Control whether to parse containers/archives while loading.
    /// If false, we can only handle raw disk images.
    /// This will disable archived disk images like IMZ and ADZ.
    pub(crate) parse_containers: bool,
    /// If an image (or image container) can contain multiple volumes, this
    /// field will determine which volume to load, by index. The list of
    /// volumes can be returned in a `ImageLoaderError::MultiVolume` error.
    pub(crate) volume_index: Option<usize>,
    /// Similar to volume_index, but allows for specifying the volume by path.
    /// The path is significant only to the relative archive filesystem.
    /// If both index and path are specified, the path will take precedence.
    pub(crate) volume_path: Option<PathBuf>,
    /// Create a source map during import, if the parser supports doing so.
    pub(crate) create_source_map: bool,
}

impl ImageLoader {
    pub fn new() -> ImageLoader {
        Default::default()
    }

    pub fn with_platform(mut self, platform: Platform) -> ImageLoader {
        self.platform = Some(platform);
        self
    }

    pub fn with_file_format(mut self, format: DiskImageFileFormat) -> ImageLoader {
        self.format = Some(format);
        self
    }

    /// Set the [`DiskDataResolution`] to use for the [`DiskImage`] to be built.
    pub fn with_resolution(mut self, resolution: DiskDataResolution) -> ImageLoader {
        self.resolution = Some(resolution);
        self
    }

    pub fn with_volume_index(mut self, volume_index: usize) -> ImageLoader {
        self.volume_index = Some(volume_index);
        self
    }

    pub fn with_volume_path(mut self, volume_path: PathBuf) -> ImageLoader {
        self.volume_path = Some(volume_path);
        self
    }

    pub fn with_source_map(mut self, state: bool) -> ImageLoader {
        self.create_source_map = state;
        self
    }

    pub fn with_container(mut self, state: bool) -> ImageLoader {
        self.parse_containers = state;
        self
    }

    pub fn load(mut self) -> Result<DiskImage, DiskImageError> {
        unimplemented!()
    }
}
