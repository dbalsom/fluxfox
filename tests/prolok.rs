mod common;

use fluxfox::{diskimage::RwSectorScope, DiskCh, DiskChsnQuery, DiskImage, DiskImageError};

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_prolok() {
    use std::io::Cursor;
    init();

    let disk_image_buf = std::fs::read(".\\tests\\images\\tc\\catprot.tc").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut tc_image = DiskImage::load(&mut in_buffer, None, None, None).unwrap();

    println!(
        "Loaded TransCopy image of geometry {}...",
        tc_image.image_format().geometry
    );

    let mut read_sector_result = match tc_image.read_sector(
        DiskCh::new(39, 0),
        DiskChsnQuery::new(39, 0, 5, None),
        None,
        None,
        RwSectorScope::DataOnly,
        false,
    ) {
        Ok(result) => result,
        Err(DiskImageError::DataError) => {
            panic!("Data error reading sector.");
        }
        Err(e) => panic!("Error reading sector: {:?}", e),
    };

    let sector_data = read_sector_result.read_buf;
    let original_data = sector_data.clone();

    println!(
        "Read sector data: {:02X?} of length {}",
        &sector_data[0..8],
        sector_data.len()
    );

    assert_eq!(sector_data.len(), 512);

    match tc_image.write_sector(
        DiskCh::new(39, 0),
        DiskChsnQuery::new(39, 0, 5, 2),
        None,
        &sector_data,
        RwSectorScope::DataOnly,
        false,
        false,
    ) {
        Ok(result) => result,
        Err(DiskImageError::DataError) => {
            panic!("Data error writing sector.");
        }
        Err(e) => panic!("Error writing sector: {:?}", e),
    };

    // Read the sector back. It should have different data.
    read_sector_result = match tc_image.read_sector(
        DiskCh::new(39, 0),
        DiskChsnQuery::new(39, 0, 5, 2),
        None,
        None,
        RwSectorScope::DataOnly,
        false,
    ) {
        Ok(result) => result,
        Err(DiskImageError::DataError) => {
            panic!("Data error reading sector.");
        }
        Err(e) => panic!("Error reading sector: {:?}", e),
    };

    let sector_data = read_sector_result.read_buf;

    println!("Original data: {:02X?}", &original_data[0..8]);
    println!("Post-write data: {:02X?}", &sector_data[0..8]);

    if sector_data == original_data {
        panic!("Data read back from written sector did not change - no hole detected!");
    }

    println!("Data read back from written sector changed - hole detected!");
}
