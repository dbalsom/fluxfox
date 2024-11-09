mod common;

use crate::common::run_sector_test;
use fluxfox::{DiskImage, DiskImageFileFormat, ImageParser};
use std::path::PathBuf;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_86f_write() {
    init();
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\transylvania\\Transylvania.86f").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut f86_image = DiskImage::load(&mut in_buffer, None, None, None).unwrap();

    println!("Loaded 86F image of geometry {}...", f86_image.image_format().geometry);

    let mut out_buffer = Cursor::new(Vec::new());
    let fmt = DiskImageFileFormat::F86Image;

    match fmt.save_image(&mut f86_image, &mut out_buffer) {
        Ok(_) => println!("Saved 86F image."),
        Err(e) => panic!("Failed to save 86F image: {}", e),
    }
    let out_inner: Vec<u8> = out_buffer.into_inner();
    std::fs::write(".\\tests\\images\\temp\\temp_out.86f", out_inner).unwrap();

    // let readback_disk_image_buf = std::fs::read(".\\tests\\images\\temp\\temp_out.86f").unwrap();
    // let mut readback_in_buffer = Cursor::new(readback_disk_image_buf);
    //
    // let mut f86_image = match DiskImage::load(&mut readback_in_buffer) {
    //     Ok(image) => image,
    //     Err(e) => panic!("Failed to re-load new 86F image: {}", e),
    // };
}

#[test]
fn test_86f_sector_tests() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.86f"),
        DiskImageFileFormat::F86Image,
    );
}
