mod common;

use common::*;
use fluxfox::{DiskImage, DiskImageFormat, ImageParser};

#[test]
fn test_td0() {
    use std::io::Cursor;

    let disk_image_buf = std::fs::read(".\\tests\\images\\Transylvania.td0").unwrap();
    let mut in_buffer = Cursor::new(disk_image_buf);

    let img_image = DiskImage::load(&mut in_buffer).unwrap();

    println!("Loaded TD0 image of geometry {}...", img_image.image_format().geometry);

    let mut out_buffer = Cursor::new(Vec::new());

    let fmt = DiskImageFormat::RawSectorImage;
    fmt.save_image(&img_image, &mut out_buffer).unwrap();

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

    if in_hash != out_hash {
        println!("Hashes do not match!");
        //std::fs::write("test_out.img", out_inner.clone()).unwrap();
    }

    assert_eq!(in_hash, out_hash);
    println!("Hashes match!");
}
