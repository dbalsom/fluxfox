/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

    src/image_writer.rs

    Implements an output helper for writing disk images to a file.

*/

use std::path::PathBuf;

use crate::{
    file_parsers::{ImageFormatParser, ParserWriteOptions},
    io::{Cursor, ReadWriteSeek},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
};

pub struct ImageWriter<'img, RWS: ReadWriteSeek> {
    pub image:  &'img mut DiskImage,
    pub writer: Option<RWS>,
    pub path:   Option<PathBuf>,
    pub format: Option<DiskImageFileFormat>,
}

impl<'img, RWS: ReadWriteSeek> ImageWriter<'img, RWS> {
    pub fn new(img: &'img mut DiskImage) -> Self {
        Self {
            image:  img,
            writer: None,
            path:   None,
            format: None,
        }
    }

    pub fn with_format(mut self, format: DiskImageFileFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn with_writer(mut self, writer: RWS) -> Self {
        self.writer = Some(writer);
        self
    }

    pub fn with_path(self, path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..self
        }
    }

    pub fn write(self) -> Result<(), DiskImageError> {
        if self.path.is_none() && self.writer.is_none() {
            log::error!("ImageWriter::write(): No output path or writer provided");
            return Err(DiskImageError::ParameterError);
        }
        if self.format.is_none() {
            log::error!("ImageWriter::write(): No format provided");
            return Err(DiskImageError::ParameterError);
        }

        let format = self.format.unwrap();

        if let Some(mut writer) = self.writer {
            log::debug!("ImageWriter::write(): Saving image to writer...");
            format.save_image(self.image, &ParserWriteOptions::default(), &mut writer)?;
            return Ok(());
        }

        // This is a bit inefficient if both a writer and a path were specified, as we export
        // the image twice - but it's not intended for both a writer and path to be specified,
        // so I'm not terribly concerned about it.
        if let Some(path) = self.path {
            log::debug!("ImageWriter::write(): Saving image to file: {:?}", path);
            let mut buf = Cursor::new(Vec::with_capacity(3_000_000));
            format.save_image(self.image, &ParserWriteOptions::default(), &mut buf)?;

            let data = buf.into_inner();
            std::fs::write(path, data)?;

            return Ok(());
        }

        Ok(())
    }
}
