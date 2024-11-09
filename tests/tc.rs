mod common;

use crate::common::run_sector_test;
use fluxfox::DiskImageFileFormat;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_scp_sector_test() {
    init();
    run_sector_test(
        "tests/images/sector_test/sector_test_360k.tc".into(),
        DiskImageFileFormat::TransCopyImage,
    );
}
