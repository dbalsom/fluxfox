mod common;

use common::*;
use fluxfox::{DiskImage, DiskImageFileFormat, ImageParser};
use std::path::PathBuf;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_img() {
    init();
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\transylvania\\Transylvania.img").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let mut img_image = DiskImage::load(&mut in_buffer, None, None, None).unwrap();

    let geometry = img_image.image_format().geometry;

    println!("Loaded IMG of geometry {}...", geometry);
    let format = img_image.closest_format(true);
    println!("Closest format is {:?}", format);

    let mut out_buffer = Cursor::new(Vec::new());
    let fmt = DiskImageFileFormat::RawSectorImage;

    fmt.save_image(&mut img_image, &mut out_buffer).unwrap();

    let in_inner: Vec<u8> = in_buffer.into_inner();
    let out_inner: Vec<u8> = out_buffer.into_inner();

    let in_hash = compute_slice_hash(&in_inner);

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

#[test]
fn test_img_sector_test() {
    init();
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.img"),
        DiskImageFileFormat::RawSectorImage,
    );
    run_sector_test(
        PathBuf::from(".\\tests\\images\\sector_test\\sector_test_360k.imz"),
        DiskImageFileFormat::RawSectorImage,
    );
}
