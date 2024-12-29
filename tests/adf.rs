#![cfg(all(feature = "adf", feature = "amiga"))]
mod common;

use common::*;
use fluxfox::prelude::*;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_adf() {
    init();
    test_invertibility(
        ".\\tests\\images\\adf\\flightyfox.adf",
        DiskImageFileFormat::RawSectorImage,
    );
}

#[test]
#[cfg(feature = "gzip")]
fn test_adz() {
    init();
    test_convert_exact(
        ".\\tests\\images\\adf\\flightyfox.adz",
        ".\\tests\\images\\adf\\flightyfox.adf",
        DiskImageFileFormat::RawSectorImage,
    );
}

// #[test]
// fn test_img_sector_test() {
//     init();
//     run_sector_test(
//         PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.img"),
//         DiskImageFileFormat::RawSectorImage,
//     );
//     #[cfg(feature = "zip")]
//     run_sector_test(
//         PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.imz"),
//         DiskImageFileFormat::RawSectorImage,
//     );
//     run_sector_test(
//         PathBuf::from(".\\tests\\images\\sector_test\\sector_test_1200k.img"),
//         DiskImageFileFormat::RawSectorImage,
//     );
// }
