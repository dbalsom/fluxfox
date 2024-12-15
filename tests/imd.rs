mod common;

use common::*;
use fluxfox::prelude::*;
use std::path::PathBuf;

fn init() {
    match env_logger::builder().is_test(true).try_init() {
        Ok(_) => {
            println!("Logger initialized. A debug log should follow:");
            log::debug!("Logger initialized.");
        }
        Err(e) => eprintln!("Failed to initialize logger: {}", e),
    }
}

#[test]
fn test_imd() {
    init();
    test_convert_exact(
        ".\\tests\\images\\transylvania\\Transylvania.imd",
        ".\\tests\\images\\transylvania\\Transylvania.img",
        DiskImageFileFormat::RawSectorImage,
    );
}

#[test]
fn test_imd_sector_test_360k() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.imd"),
        DiskImageFileFormat::ImageDisk,
    );
}
