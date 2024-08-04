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

    examples/imginfo/src/main.rs

    This is a simple example of how to use FluxFox to read a disk image and
    print out some basic information about it.
*/
use bpaf::*;

use fluxfox::DiskImage;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    filename: PathBuf,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let filename = short('t')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    construct!(Out { debug, filename })
        .to_options()
        .descr("imginfo: display info about disk image")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    let disk_image = match std::fs::File::open(&opts.filename) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Error opening file: {}", e);
            return;
        }
    };

    let mut reader = std::io::BufReader::new(disk_image);

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            return;
        }
    };

    println!("Detected disk image type: {:?}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            return;
        }
    };

    println!("Disk image info:");
    println!("----------------");
    let _ = disk.dump_info(&mut std::io::stdout());
    let _ = disk.dump_sector_map(&mut std::io::stdout());

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
