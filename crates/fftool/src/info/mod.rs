/*
    fftool
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
use crate::args::GlobalOptions;
use crate::read_file;
use anyhow::{bail, Error};
use fluxfox::DiskImage;

pub mod args;

pub(crate) fn run(_global: &GlobalOptions, params: args::InfoParams) -> Result<(), Error> {
    let mut reader = read_file(&params.in_file)?;

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            bail!("Error detecting disk image type: {}", e);
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader, Some(params.in_file), None) {
        Ok(disk) => disk,
        Err(e) => {
            bail!("Error loading disk image: {}", e);
        }
    };

    println!("Disk image info:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_info(&mut std::io::stdout());
    println!();

    println!("Disk consistency report:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_consistency(&mut std::io::stdout());

    println!("Image can be represented by the following formats with write support:");
    let formats = disk.compatible_formats(true);
    for format in formats {
        println!("  {} [{}]", format.0, format.1.join(", "));
    }

    if let Some(bootsector) = disk.boot_sector() {
        println!("Disk has a boot sector");
        if bootsector.has_valid_bpb() {
            println!("Boot sector with valid BPB detected:");
            println!("{}", "-".repeat(79));
            let _ = bootsector.dump_bpb(&mut std::io::stdout());
        }
        else {
            println!("Boot sector has an invalid BPB");
        }
    }
    println!();

    if params.sector_list {
        let _ = disk.dump_sector_map(&mut std::io::stdout());
    }

    Ok(())
}
