mod common;

use crate::common::{run_sector_test, test_convert_exact};
use fluxfox::DiskImageFileFormat;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

// #[test]
// fn test_scp_sector_test_360k() {
//     init();
//     run_sector_test(
//         "tests/images/sector_test/sector_test_360k.scp",
//         DiskImageFileFormat::SuperCardPro,
//     );
// }
//
// #[test]
// fn test_scp_trans() {
//     init();
//     test_convert_exact(
//         ".\\tests\\images\\transylvania\\Transylvania.scp",
//         ".\\tests\\images\\transylvania\\Transylvania.img",
//         DiskImageFileFormat::RawSectorImage,
//     );
// }
