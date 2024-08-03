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

    examples/imgdump/src/main.rs

    This is a simple example of how to use FluxFox to read a disk image and
    dump information from it, such a specified sector or track, in hex or binary
    format.
*/
use bpaf::*;
use fluxfox::{DiskChs, DiskImage};
use std::io::BufWriter;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    filename: PathBuf,
    cylinder: u16,
    head: u8,
    sector: u8,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let filename = short('t')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    let cylinder = short('c')
        .long("cylinder")
        .help("Target cylinder")
        .argument::<u16>("CYLINDER");

    let head = short('h').long("head").help("Target head").argument::<u8>("HEAD");

    let sector = short('s').long("sector").help("Target sector").argument::<u8>("SECTOR");

    construct!(Out {
        debug,
        filename,
        cylinder,
        head,
        sector
    })
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
            std::process::exit(1);
        }
    };

    let mut reader = std::io::BufReader::new(disk_image);

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    println!("Detected disk image type: {:?}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    let handle = std::io::stdout();
    let mut buf = BufWriter::new(handle);

    let chs = DiskChs::new(opts.cylinder, opts.head, opts.sector);

    println!("Dumping sector {} in hex format:", chs);
    match disk.dump_sector_hex(chs, 8, &mut buf) {
        Ok(_) => println!("Sector dumped successfully"),
        Err(e) => eprintln!("Error dumping sector: {}", e),
    }
}
