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

    tests/common/mod.rs

    Common support routines for tests
*/
#![allow(dead_code)]
#![allow(unused_imports)]
pub mod convert_exact;
pub mod invertibility;

pub use convert_exact::test_convert_exact;
pub use invertibility::test_invertibility;

use fluxfox::{io::Read, prelude::*, DiskImage, DiskImageFileFormat, DEFAULT_SECTOR_SIZE};

use hex::encode;
use sha1::{Digest, Sha1};
use std::{
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

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

pub fn run_sector_test(file_path: impl Into<PathBuf>, fmt: DiskImageFileFormat) {
    use std::io::Cursor;
    let file_path = file_path.into();
    let disk_image_buf = std::fs::read(file_path).unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let disk = match DiskImage::load(&mut in_buffer, None, None, None) {
        Ok(image) => image,
        Err(e) => panic!("Failed to load {} image: {}", fmt, e),
    };

    println!("Loaded {} image of geometry {}...", fmt, disk.image_format().geometry);
    println!("Verifying sectors...");
    verify_sector_test_sectors(DiskImage::into_arc(disk));
    println!("Success!");
}

pub fn verify_sector_test_sectors(disk_lock: Arc<RwLock<DiskImage>>) {
    {
        let mut disk = disk_lock.write().unwrap();
        verify_sector_test_sectors_direct(&mut disk);
    }

    verify_sector_test_sectors_via_view(disk_lock);
}

/// The sector test image stores a u8 value in each sector that increments for each sector, wrapping.
/// This image was written to a floppy, then read back as a Kryoflux and SCP file via Greaseweazle,
/// then converted to other formats.
///
/// This function reads the sectors of a DiskImage and verifies that the u8 values are correct and
/// incrementing in the same way as the sector test image.
#[allow(dead_code)]
pub fn verify_sector_test_sectors_direct(disk: &mut DiskImage) {
    let layout = disk.closest_format(false).unwrap().layout();
    for (si, sector) in layout.chsn_iter().skip(1).enumerate() {
        let sector_byte = si + 1;
        let sector_data = disk
            .read_sector_basic(sector.ch(), sector.into(), None)
            .unwrap_or_else(|e| panic!("Failed to read sector {}: {}", sector.ch(), e));

        for byte in &sector_data {
            if *byte != sector_byte as u8 {
                eprintln!(
                    "Sector byte mismatch at sector {}: expected {}, got {}.",
                    sector, sector_byte, *byte,
                );
                assert_eq!(*byte, sector_byte as u8);
            }
        }
    }
}

/// The sector test image stores a u8 value in each sector that increments for each sector, wrapping.
/// This image was written to a floppy, then read back as a Kryoflux and SCP file via Greaseweazle,
/// then converted to other formats.
///
/// This function reads the sectors of a DiskImage and verifies that the u8 values are correct and
/// incrementing in the same way as the sector test image, using a StandardSectorView.
#[allow(dead_code)]
pub fn verify_sector_test_sectors_via_view(disk_lock: Arc<RwLock<DiskImage>>) {
    let format = {
        let disk = disk_lock.read().unwrap();
        match disk.closest_format(true) {
            Some(f) => f,
            None => panic!("Couldn't detect disk format."),
        }
    };

    let mut view = match StandardSectorView::new(disk_lock.clone(), format) {
        Ok(view) => view,
        Err(e) => panic!("Failed to create StandardSectorView: {}", e),
    };

    let chs = DiskChs::from(format);
    let sector_ct = chs.sector_count() as usize;

    let mut sector_buf = vec![0u8; format.sector_size()];

    // Skip the boot sector
    view.seek(SeekFrom::Start(format.chsn().n_size() as u64)).unwrap();
    // Start counting from 1 as we skipped boot sector
    for sector_idx in 1..sector_ct {
        //let offset = sector_idx * format.sector_size();

        // Read the sector
        view.read_exact(&mut sector_buf) // Read the sector
            .unwrap_or_else(|e| panic!("Failed to read sector {}: {}", sector_idx, e));

        for byte in &sector_buf {
            if *byte != sector_idx as u8 {
                eprintln!(
                    "Sector byte mismatch at sector {}: expected {}, got {}.",
                    sector_idx, sector_idx, byte
                );
                assert_eq!(*byte, sector_idx as u8);
            }
        }
    }
}
