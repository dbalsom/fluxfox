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

    examples/imgconvert/src/main.rs

    This is a simple example of how to use FluxFox to convert one disk image
    format to another.

    WARNING: fluxfox is not primarily intended for the purpose of disk image
    conversion. I am not responsible for any data loss or corruption that may
    result as a consequence of using this tool.
*/
mod prompt;

use bpaf::*;
use fluxfox::diskimage::DiskImageFlags;
use fluxfox::{format_from_ext, DiskImage, ImageParser, ParserWriteCompatibility};
use std::io::Cursor;
use std::path::PathBuf;
use std::ptr::read;

pub const WARNING: &str = "WARNING: fluxfox is not primarily intended for the purpose of disk\n\
image conversion. I am not responsible for any data loss or corruption\n\
that may result as a consequence of using this tool.\n";

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    in_filename: PathBuf,
    out_filename: PathBuf,
    prolok: bool,
    debug: bool,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let in_filename = short('i')
        .long("in_filename")
        .help("Filename of source image")
        .argument::<PathBuf>("IN_FILE");

    let out_filename = short('o')
        .long("out_filename")
        .help("Filename of destination image")
        .argument::<PathBuf>("OUT_FILE");

    let prolok = long("prolok")
        .help("Create PROLOK holes for compatible formats")
        .switch();

    construct!(Out {
        in_filename,
        out_filename,
        prolok,
        debug,
    })
    .to_options()
    .descr("imgconvert: display info about disk image")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();
    let disk_image = match std::fs::File::open(&opts.in_filename) {
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
            eprintln!("Error detecting input disk image type: {}", e);
            std::process::exit(1);
        }
    };

    println!("Input disk image type: {}", disk_image_type);

    // Get extension from output filename
    let output_extension = match opts.out_filename.extension() {
        Some(ext) => ext,
        None => {
            eprintln!("Error: A file extension is required for the output file!");
            std::process::exit(1);
        }
    };

    let ext_str = match output_extension.to_str() {
        Some(ext) => ext,
        None => {
            eprintln!("Error: Invalid output file extension!");
            std::process::exit(1);
        }
    };
    let output_format = match format_from_ext(ext_str) {
        Some(format) => format,
        None => {
            eprintln!("Error: Unknown output file extension: {}", ext_str);
            std::process::exit(1);
        }
    };

    println!("Output disk image type: {}", output_format);
    //std::process::exit(0);

    // Load disk image
    let mut in_disk = match DiskImage::load(&mut reader) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    if in_disk.has_weak_bits() {
        println!("Input disk image contains a weak bit mask.");
    }

    if opts.prolok {
        in_disk.set_flag(DiskImageFlags::PROLOK);
        println!("PROLOK holes will be created in output image.");
    }

    match output_format.can_write(&in_disk) {
        ParserWriteCompatibility::Ok => {
            println!("Output format is compatible with input image.");
        }
        ParserWriteCompatibility::Incompatible | ParserWriteCompatibility::UnsupportedFormat => {
            eprintln!("Error: Output format {} cannot write specified image!", output_format);
            std::process::exit(1);
        }
        ParserWriteCompatibility::DataLoss => {
            eprintln!("Warning: Output format {} may lose data!", output_format);
            prompt::prompt("Continue with potential data loss? (y/n)");
        }
    }

    // Create an output buffer
    let mut out_buffer = Cursor::new(Vec::new());
    match output_format.save_image(&mut in_disk, &mut out_buffer) {
        Ok(_) => {
            let out_inner: Vec<u8> = out_buffer.into_inner();
            match std::fs::write(opts.out_filename.clone(), out_inner) {
                Ok(_) => {
                    println!("Output image saved to {}", opts.out_filename.display());
                }
                Err(e) => {
                    eprintln!("Error saving output image: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error saving output image: {}", e);
            std::process::exit(1);
        }
    }
}
