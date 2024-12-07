#![cfg(feature = "mfi")]
mod common;

use crate::common::run_sector_test;
use fluxfox::{DiskImage, DiskImageFileFormat, ImageParser};
use std::path::PathBuf;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_mfi_sector_test_360k() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.mfi"),
        DiskImageFileFormat::MameFloppyImage,
    );
}

#[test]
fn test_mfi_sector_test_1200k() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_1200k.mfi"),
        DiskImageFileFormat::MameFloppyImage,
    );
}
