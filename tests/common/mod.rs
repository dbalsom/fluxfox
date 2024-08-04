use fluxfox::{DiskChs, DEFAULT_SECTOR_SIZE};
use hex::encode;
use sha1::{Digest, Sha1};
use std::path::Path;

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
