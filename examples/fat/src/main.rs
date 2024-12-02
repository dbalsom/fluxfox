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

    examples/fat/src/main.rs

    This is a simple example of how to use FluxFox to mount and read a FAT
    filesystem using a DiskImage and a StandardSectorView.
*/

use std::path::PathBuf;

use bpaf::*;
use fluxfox::{file_system::fat::fat_fs::FatFileSystem, io::Cursor, prelude::*};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug:    bool,
    silent:   bool,
    filename: PathBuf,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let silent = long("silent")
        .help("Suppress all output except errors and requested data")
        .switch();

    let filename = short('i')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    construct!(Out {
        debug,
        silent,
        filename,
    })
    .to_options()
    .descr("fat_example: list all files on a disk containing a DOS FAT12 filesystem")
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
    let mut cursor = Cursor::new(&mut file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut cursor) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    if !opts.silent {
        println!("Detected disk image type: {}", disk_image_type);
    }

    let disk = match DiskImage::load(&mut cursor, Some(opts.filename.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    // Attempt to determine the disk format. Trust the BPB for this purpose.
    let format = match disk.closest_format(true) {
        Some(format) => format,
        None => {
            eprintln!("Couldn't detect disk format. Disk may not contain a FAT filesystem.");
            std::process::exit(1);
        }
    };

    if !opts.silent {
        println!("Disk format: {:?}", format);
    }

    let disk_arc = DiskImage::into_arc(disk);

    // Mount the filesystem
    let fs = match FatFileSystem::mount(disk_arc.clone(), None) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error mounting filesystem: {}", e);
            std::process::exit(1);
        }
    };

    // Get file listing.
    let files = fs.list_all_files();

    for file in files {
        println!("{}", file);
    }

    if !opts.silent {
        println!("Done!");
    }
}
