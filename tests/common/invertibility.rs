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

//! File formats that support reading and writing should be able to demonstrate
//! invertibility - that is, a file that is read from and written back to the
//! same format should be identical to the original file (assuming that the source
//! file does not contain missing or invalid metadata that fluxfox is able to
//! reconstruct).

use crate::common::compute_slice_hash;
use fluxfox::{prelude::ParserWriteOptions, DiskImage, DiskImageFileFormat, ImageFormatParser};
use std::path::PathBuf;

pub fn test_invertibility(in_path: impl Into<PathBuf>, fmt: DiskImageFileFormat) {
    use std::io::Cursor;
    let in_path = in_path.into();
    let ext = in_path
        .extension()
        .map(|os| os.to_string_lossy().to_string())
        .unwrap_or("".to_string());

    let disk_image_buf = std::fs::read(in_path).unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);
    let mut disk = DiskImage::load(&mut in_buffer, None, None, None).unwrap();

    let geometry = disk.image_format().geometry;

    println!("Loaded \"{}\" file of geometry {}...", ext, geometry);
    let format = disk.closest_format(false).unwrap();
    println!("Closest format is {:?}", format);

    //assert_eq!(format, StandardFormat::AmigaFloppy880);

    let mut out_buffer = Cursor::new(Vec::new());

    fmt.save_image(&mut disk, &ParserWriteOptions::default(), &mut out_buffer)
        .unwrap();

    let in_inner: Vec<u8> = in_buffer.into_inner();
    let out_inner: Vec<u8> = out_buffer.into_inner();

    let in_hash = compute_slice_hash(&in_inner);

    //println!("Input file is {} bytes.", in_inner.len());
    //println!("First bytes of input file: {:02X?}", &in_inner[0..16]);
    println!("Input file SHA1: {}", in_hash);

    let out_hash = compute_slice_hash(&out_inner);
    println!("Output file SHA1: {:}", out_hash);

    assert_eq!(in_hash, out_hash);
    println!("Hashes match!");
}
