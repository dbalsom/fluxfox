mod common;

use crate::common::run_sector_test;
use fluxfox::DiskImageFileFormat;
use std::path::PathBuf;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_kryoflux_sector_test() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_kryoflux_360k.zip"),
        DiskImageFileFormat::KryofluxStream,
    );
}
