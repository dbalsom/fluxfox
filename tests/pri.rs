mod common;

use fluxfox::{DiskImage, DiskImageFileFormat, ImageParser};

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_pri_write() {
    init();
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\Transylvania.86f").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut f86_image = DiskImage::load(&mut in_buffer, None, None, None).unwrap();

    println!("Loaded 86F image of geometry {}...", f86_image.image_format().geometry);

    let mut out_buffer = Cursor::new(Vec::new());
    let fmt = DiskImageFileFormat::PceBitstreamImage;

    match fmt.save_image(&mut f86_image, &mut out_buffer) {
        Ok(_) => println!("Saved PRI image."),
        Err(e) => panic!("Failed to save PRI image: {}", e),
    }
    let out_inner: Vec<u8> = out_buffer.into_inner();
    std::fs::write(".\\tests\\images\\temp\\temp_out.pri", out_inner).unwrap();

    // let readback_disk_image_buf = std::fs::read(".\\tests\\images\\temp\\temp_out.pri").unwrap();
    // let mut readback_in_buffer = Cursor::new(readback_disk_image_buf);
    //
    // let mut f86_image = match DiskImage::load(&mut readback_in_buffer) {
    //     Ok(image) => image,
    //     Err(e) => panic!("Failed to re-load new PRI image: {}", e),
    // };
}

#[test]
fn test_pri_sector_reads() {
    init();
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\pri\\sector_test.pri").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut pri_image = match DiskImage::load(&mut in_buffer, None, None, None) {
        Ok(image) => image,
        Err(e) => panic!("Failed to load PRI image: {}", e),
    };

    println!("Loaded PRI image of geometry {}...", pri_image.image_format().geometry);

    let mut sector_byte: u8 = 0;

    // Collect indices to avoid borrowing issues
    let ti_vec: Vec<usize> = pri_image.track_idx_iter().collect();
    for ti in ti_vec {
        if let Some(td) = pri_image.track_by_idx_mut(ti) {
            let ch = td.ch();
            println!("Reading track {}...", ch);
            let rtr = match td.read_all_sectors(ch, 2, 0) {
                Ok(rtr) => rtr,
                Err(e) => panic!("Failed to read track: {}", e),
            };

            // pub struct ReadTrackResult {
            //     pub not_found: bool,
            //     pub sectors_read: u16,
            //     pub read_buf: Vec<u8>,
            //     pub deleted_mark: bool,
            //     pub address_crc_error: bool,
            //     pub data_crc_error: bool,
            // }

            if rtr.read_buf.len() != rtr.sectors_read as usize * 512 {
                eprintln!(
                    "Read buffer size mismatch: expected {} bytes, got {} bytes.",
                    rtr.sectors_read as usize * 512,
                    rtr.read_buf.len()
                );
            }

            for si in 0..rtr.sectors_read {
                let sector = &rtr.read_buf[si as usize * 512..(si as usize + 1) * 512];
                for bi in 0..512 {
                    if sector[bi] != sector_byte {
                        eprintln!(
                            "Sector byte mismatch at track {}, sector {}, byte [{}]: expected {}, got {}.",
                            td.ch(),
                            si + 1,
                            bi,
                            sector_byte,
                            sector[bi]
                        );
                        assert_eq!(sector[bi], sector_byte);
                        break;
                    }
                }

                sector_byte = sector_byte.wrapping_add(1);
                println!("Advancing sector, new sector_byte: {}", sector_byte);
            }
        }
    }
}
