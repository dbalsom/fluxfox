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
pub mod args;

use crate::{args::GlobalOptions, prompt, read_file};
use anyhow::{bail, Error};
use fluxfox::prelude::*;
use std::io::Cursor;

pub(crate) fn run(global: &GlobalOptions, params: &args::ConvertParams) -> Result<(), Error> {
    let mut reader = read_file(&params.in_file.clone())?;

    let disk_image_type = match DiskImage::detect_format(&mut reader, Some(params.in_file.clone())) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            bail!("Error detecting input disk image type: {}", e);
        }
    };

    if !global.silent {
        println!("Input disk image type: {}", disk_image_type);
    }

    // Get extension from output filename
    let output_extension = match params.out_file.extension() {
        Some(ext) => ext,
        None => {
            bail!("Error: A file extension is required for the output file!");
        }
    };

    let ext_str = match output_extension.to_str() {
        Some(ext) => ext,
        None => {
            bail!("Error: Invalid output file extension!");
        }
    };
    let output_format = match format_from_ext(ext_str) {
        Some(format) => format,
        None => {
            bail!("Error: Unknown output file extension: {}", ext_str);
        }
    };

    println!("Output disk image type: {}", output_format);
    //std::process::exit(0);

    // Load disk image
    let mut in_disk = match DiskImage::load(&mut reader, Some(params.in_file.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            bail!("Error loading disk image: {}", e);
        }
    };

    if in_disk.has_weak_bits() {
        println!("Input disk image contains a weak bit mask.");
    }

    if params.prolok {
        in_disk.set_flag(fluxfox::types::DiskImageFlags::PROLOK);
        println!("PROLOK holes will be created in output image.");
    }

    match output_format.can_write(Some(&in_disk)) {
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
    match output_format.save_image(&mut in_disk, &ParserWriteOptions::default(), &mut out_buffer) {
        Ok(_) => {
            let out_inner: Vec<u8> = out_buffer.into_inner();
            match std::fs::write(params.out_file.clone(), out_inner) {
                Ok(_) => {
                    println!("Output image saved to {}", params.out_file.display());
                    Ok(())
                }
                Err(e) => {
                    bail!("Error saving output image: {}", e);
                }
            }
        }
        Err(e) => {
            bail!("Error saving output image: {}", e);
        }
    }
}
