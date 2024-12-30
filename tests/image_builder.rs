use fluxfox::{image_builder::ImageBuilder, prelude::*, DiskImageFileFormat, ImageFormatParser, StandardFormat};
use std::io::Cursor;

mod common;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_image_builder() {
    init();

    let mut image = match ImageBuilder::new()
        .with_resolution(TrackDataResolution::BitStream)
        .with_standard_format(StandardFormat::PcFloppy360)
        .with_creator_tag("MartyPC ".as_bytes())
        .with_formatted(true)
        .build()
    {
        Ok(image) => image,
        Err(e) => panic!("Failed to create image: {}", e),
    };

    let mut out_buffer = Cursor::new(Vec::new());
    let output_fmt = DiskImageFileFormat::F86Image;
    match output_fmt.save_image(&mut image, &ParserWriteOptions::default(), &mut out_buffer) {
        Ok(_) => println!("Wrote 86F image."),
        Err(e) => panic!("Failed to write 86F image: {}", e),
    };

    std::fs::write(".\\tests\\images\\test_formatted.86f", out_buffer.get_ref()).unwrap();
}
