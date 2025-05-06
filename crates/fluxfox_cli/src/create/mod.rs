/*
    fftool
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
*/
pub mod args;

use crate::args::GlobalOptions;
use anyhow::{anyhow, bail, Error};
use fluxfox::{file_system::FileSystemType, prelude::*};
use std::fs::File;

pub(crate) fn run(global: &GlobalOptions, params: &args::CreateParams) -> Result<(), Error> {
    // Get extension from output filename
    let ext_str = params
        .out_file
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("Error: Invalid or missing output file extension!"))?;

    let output_format =
        format_from_ext(ext_str).ok_or_else(|| anyhow!("Error: Unknown output file extension: {}", ext_str))?;

    global.loud(|| println!("Output disk image format requested: {}", output_format));

    if matches!(
        output_format.can_write(None),
        ParserWriteCompatibility::UnsupportedFormat
    ) {
        bail!("Requested format {} does not support image writing.", output_format);
    }

    // Override the resolution for raw sector images, at least until we implement formatting MetaSector images.
    let create_resolution = match output_format {
        DiskImageFileFormat::RawSectorImage => TrackDataResolution::BitStream,
        _ => output_format.resolution(),
    };

    global.loud(|| println!("Creating disk image of resolution {:?}", create_resolution));

    // Create a DiskImage using ImageBuilder
    let mut builder = ImageBuilder::new()
        .with_resolution(create_resolution)
        .with_standard_format(params.disk_format);

    if params.formatted | params.sector_test {
        builder = builder.with_filesystem(FileSystemType::Fat12);
    }

    if let Some(dir) = &params.from_dir {
        builder = builder.with_filesystem_from_path(dir, FileSystemType::Fat12, false, true, false);
    }

    let mut disk = match builder.build() {
        Ok(disk) => {
            log::debug!("Disk image created of Ch: {}", disk.geometry());
            global.loud(|| println!("Disk image created of Ch: {}", disk.geometry()));

            disk
        }
        Err(e) => {
            bail!("Error creating disk image: {}", e);
        }
    };

    // Create a sector test image if requested.
    if params.sector_test {
        // Iterate through all sectors, skipping the boot sector, and write the sector index to the sector.
        let layout = StandardFormat::from(params.disk_format).layout();
        for (idx, sector) in layout.chsn_iter().skip(1).enumerate() {
            let sector_value = (idx + 1) as u8; // Let the sector value roll over at 255.

            // Write the sector value to the sector.
            match disk.write_sector_basic(sector.ch(), sector.into(), None, &vec![sector_value; layout.size()]) {
                Ok(()) => {
                    global.loud(|| println!("Wrote sector {} with value {}", sector, sector_value));
                }
                Err(e) => {
                    bail!("Error writing sector {}: {}", sector, e);
                }
            }
        }
    }

    // Write the image with an ImageWriter
    match ImageWriter::<File>::new(&mut disk)
        .with_format(output_format)
        .with_path(params.out_file.clone())
        .write()
    {
        Ok(()) => {
            global.loud(|| println!("Disk image successfully written to {}", params.out_file.display()));
            Ok(())
        }
        Err(e) => {
            bail!("Error opening disk image for writing: {}", e);
        }
    }
}
