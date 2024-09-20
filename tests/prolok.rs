mod common;

use fluxfox::bitstream::mfm;
use fluxfox::bitstream::mfm::encode_mfm;
use fluxfox::diskimage::RwSectorScope;
use fluxfox::{DiskChs, DiskImage, DiskImageError, DiskImageFormat, ImageParser};

#[test]
fn test_prolok() {
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\prolok.tc").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut tc_image = DiskImage::load(&mut in_buffer).unwrap();

    println!(
        "Loaded TransCopy image of geometry {}...",
        tc_image.image_format().geometry
    );

    let mut read_sector_result =
        match tc_image.read_sector(DiskChs::from((39, 0, 5)), None, RwSectorScope::DataOnly, false) {
            Ok(result) => result,
            Err(DiskImageError::DataError) => {
                panic!("Data error reading sector.");
            }
            Err(e) => panic!("Error reading sector: {:?}", e),
        };

    let sector_data = read_sector_result.read_buf;

    println!(
        "Read sector data: {:02X?} of length {}",
        &sector_data[0..8],
        sector_data.len()
    );

    assert_eq!(sector_data.len(), 512);

    let original_data = sector_data.clone();

    let encoded_bits = encode_mfm(&sector_data, false, mfm::MfmEncodingType::Data);

    let encoded_bytes = encoded_bits.to_bytes();

    let idx_range: Vec<u8> = (0..16).collect();
    for pair in idx_range.chunks_exact(2) {
        println!(
            "Encoded byte: {:08b}{:08b}",
            encoded_bytes[pair[0] as usize], encoded_bytes[pair[1] as usize]
        );
    }

    let _write_sector_result = match tc_image.write_sector(
        DiskChs::from((39, 0, 5)),
        None,
        &sector_data,
        RwSectorScope::DataOnly,
        false,
    ) {
        Ok(result) => result,
        Err(DiskImageError::DataError) => {
            panic!("Data error writing sector.");
        }
        Err(e) => panic!("Error writing sector: {:?}", e),
    };

    // Read the sector back. It should have different data.
    read_sector_result = match tc_image.read_sector(DiskChs::from((39, 0, 5)), None, RwSectorScope::DataOnly, false) {
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
