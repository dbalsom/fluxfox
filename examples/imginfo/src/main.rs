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

    examples/imginfo/src/main.rs

    This is a simple example of how to use FluxFox to read a disk image and
    print out some basic information about it.
*/
use bpaf::*;
use std::io::Cursor;

use fluxfox::DiskImage;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    sector_list: bool,
    filename: PathBuf,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let sector_list = short('s')
        .long("sector-list")
        .help("List all sectors in the image")
        .switch();

    let filename = short('t')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    construct!(Out {
        debug,
        sector_list,
        filename
    })
    .to_options()
    .descr("imginfo: display info about disk image")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    let mut file_vec = match std::fs::read(opts.filename.clone()) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };
    let mut reader = Cursor::new(&mut file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut reader, Some(&opts.filename)) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            return;
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader, Some(&opts.filename), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            return;
        }
    };

    println!("Disk image info:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_info(&mut std::io::stdout());
    println!();

    println!("Disk analysis:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_analysis(&mut std::io::stdout());

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

    if opts.sector_list {
        let _ = disk.dump_sector_map(&mut std::io::stdout());
    }

    /*    for track in disk.track_pool.iter_mut() {
        match &mut track.data {
            TrackData::BitStream { data, .. } => {
                let elements = System34Parser::scan_track_metadata(data);

                log::trace!("Found {} elements on track.", elements.len());
            }
            _ => {
                println!("Track data is not a bitstream. Skipping track element scan.");
            }
        }
    }*/
}
