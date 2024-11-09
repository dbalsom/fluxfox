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
#![allow(dead_code)]

use fluxfox::{DiskChs, DiskImage, DiskImageFileFormat, DEFAULT_SECTOR_SIZE};
use hex::encode;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub fn compute_file_hash<P: AsRef<Path>>(path: P) -> String {
    let file_buf = std::fs::read(path).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(file_buf);
    let result = hasher.finalize();

    encode(result)
}

#[allow(dead_code)]
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

pub fn run_sector_test(file_path: PathBuf, fmt: DiskImageFileFormat) {
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(file_path).unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut disk = match DiskImage::load(&mut in_buffer, None, None, None) {
        Ok(image) => image,
        Err(e) => panic!("Failed to load {} image: {}", fmt.to_string(), e),
    };

    println!(
        "Loaded {} image of geometry {}...",
        fmt.to_string(),
        disk.image_format().geometry
    );
    println!("Verifying sectors...");
    assert_eq!(verify_sector_test_sectors(&mut disk), true);
    println!("Success!");
}

/// The sector test image stores a u8 value in each sector that increments for each sector, wrapping.
/// This image was written to a floppy, then read back as a Kryoflux and SCP file via Greaseweazle,
/// then converted to other formats.
///
/// This function reads the sectors of a DiskImage and verifies that the u8 values are correct and
/// incrementing in the same way as the sector test image.
#[allow(dead_code)]
pub fn verify_sector_test_sectors(disk: &mut DiskImage) -> bool {
    let mut sector_byte: u8 = 0;

    // Collect indices to avoid borrowing issues
    let ti_vec: Vec<usize> = disk.track_idx_iter().collect();
    for ti in ti_vec {
        if let Some(td) = disk.track_by_idx_mut(ti) {
            let ch = td.ch();
            //println!("Reading track {}...", ch);
            let rtr = match td.read_all_sectors(ch, 2, 0) {
                Ok(rtr) => rtr,
                Err(e) => panic!("Failed to read track: {}", e),
            };

            if rtr.read_buf.len() != rtr.sectors_read as usize * 512 {
                eprintln!(
                    "Read buffer size mismatch: expected {} bytes, got {} bytes.",
                    rtr.sectors_read as usize * 512,
                    rtr.read_buf.len()
                );
            }

            for si in 0..rtr.sectors_read {
                let sector = &rtr.read_buf[si as usize * 512..(si as usize + 1) * 512];
                for bi in 0..512 {
                    if sector[bi] != sector_byte {
                        eprintln!(
                            "Sector byte mismatch at track {}, sector {}, byte [{}]: expected {}, got {}.",
                            td.ch(),
                            si + 1,
                            bi,
                            sector_byte,
                            sector[bi]
                        );
                        assert_eq!(sector[bi], sector_byte);
                        break;
                    }
                }

                sector_byte = sector_byte.wrapping_add(1);
                //println!("Advancing sector, new sector_byte: {}", sector_byte);
            }
        }
    }
    true
}
