mod common;

use common::*;
use fluxfox::prelude::*;
use std::path::PathBuf;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_img() {
    init();
    test_invertibility(
        ".\\tests\\images\\transylvania\\Transylvania.img",
        DiskImageFileFormat::RawSectorImage,
    );
}

#[test]
fn test_img_sector_test() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.img"),
        DiskImageFileFormat::RawSectorImage,
    );
    #[cfg(feature = "zip")]
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.imz"),
        DiskImageFileFormat::RawSectorImage,
    );
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_1200k.img"),
        DiskImageFileFormat::RawSectorImage,
    );
}
