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

    tests/common/mod.rs

    Common support routines for tests
*/

use fluxfox::{DiskChs, DEFAULT_SECTOR_SIZE};
use hex::encode;
use sha1::{Digest, Sha1};
use std::path::Path;

#[allow(dead_code)]
pub fn compute_file_hash<P: AsRef<Path>>(path: P) -> String {
    let file_buf = std::fs::read(path).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(file_buf);
    let result = hasher.finalize();

    encode(result)
}

pub fn compute_slice_hash(slice: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(slice);
    let result = hasher.finalize();

    encode(result)
}

#[allow(dead_code)]
pub fn get_raw_image_address(chs: DiskChs, geom: DiskChs) -> usize {
    if chs.s() == 0 {
        log::warn!("Invalid sector == 0");
        return 0;
    }
    let hpc = geom.h() as usize;
    let spt = geom.s() as usize;
    let lba: usize = (chs.c() as usize * hpc + (chs.h() as usize)) * spt + (chs.s() as usize - 1);
    lba * DEFAULT_SECTOR_SIZE
}
