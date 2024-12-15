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

//! Demonstrate the ability for a file parser that supports writing to produce
//! output to be matched against a reference image. Normally we would test
//! invertibility, but for example we may wish to test reading a compressed
//! image, but comparing the write result to an equivalent uncompressed image.

use crate::common::compute_slice_hash;
use fluxfox::{prelude::ParserWriteOptions, DiskImage, DiskImageFileFormat, ImageFormatParser};
use std::path::PathBuf;

pub fn test_convert_exact(in_path: impl Into<PathBuf>, reference_image: impl Into<PathBuf>, fmt: DiskImageFileFormat) {
    use std::io::Cursor;
    let in_path = in_path.into();
    let reference_image = reference_image.into();
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

    let mut out_buffer = Cursor::new(Vec::new());

    fmt.save_image(&mut disk, &ParserWriteOptions::default(), &mut out_buffer)
        .unwrap();

    let out_inner: Vec<u8> = out_buffer.into_inner();

    let ref_image_buf = std::fs::read(reference_image).unwrap();
    let ref_buffer = Cursor::new(ref_image_buf);
    let ref_inner: Vec<u8> = ref_buffer.into_inner();
    let ref_hash = compute_slice_hash(&ref_inner);

    //println!("Input file is {} bytes.", in_inner.len());
    //println!("First bytes of input file: {:02X?}", &in_inner[0..16]);
    println!("Reference file SHA1: {}", ref_hash);

    let out_hash = compute_slice_hash(&out_inner);
    println!("Output file SHA1: {:}", out_hash);

    assert_eq!(ref_hash, out_hash);
    println!("Hashes match!");
}
