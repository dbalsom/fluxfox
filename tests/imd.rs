mod common;

use common::*;
use fluxfox::{DiskImage, DiskImageFormat, ImageParser};

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
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\Transylvania.imd").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut img_image = DiskImage::load(&mut in_buffer, None, None).unwrap();

    println!("Loaded IMD image of geometry {}...", img_image.image_format().geometry);

    let mut out_buffer = Cursor::new(Vec::new());

    let fmt = DiskImageFormat::RawSectorImage;
    fmt.save_image(&mut img_image, &mut out_buffer).unwrap();

    //let in_inner: Vec<u8> = in_buffer.into_inner();
    let out_inner: Vec<u8> = out_buffer.into_inner();

    let in_hash = compute_file_hash(".\\tests\\images\\Transylvania.img");

    //println!("Input file is {} bytes.", in_inner.len());
    //println!("First bytes of input file: {:02X?}", &in_inner[0..16]);
    println!("Input file SHA1: {}", in_hash);

    //println!("Output file is {} bytes.", out_inner.len());
    //println!("First bytes of output file: {:02X?}", &out_inner[0..16]);
    //std::fs::write("test_out.img", out_inner.clone()).unwrap();
    let out_hash = compute_slice_hash(&out_inner);
    println!("Output file SHA1: {:}", out_hash);

    assert_eq!(in_hash, out_hash);
    println!("Hashes match!");
}
