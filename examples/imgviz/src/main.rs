/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------

    examples/imgviz/src/main.rs

    This is a simple example of how to use FluxFox to produce a graphical
    visualization of a disk image.
*/
use bpaf::*;
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, Resizer};
use fluxfox::visualization::render_tracks;
use fluxfox::visualization::ResolutionType;
use fluxfox::visualization::RotationDirection;
use fluxfox::DiskImage;
use image::{ImageBuffer, Rgba};
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    in_filename: PathBuf,
    out_filename: PathBuf,
    resolution: u32,
    cc: bool,
    supersample: u32,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let in_filename = short('i')
        .long("in_filename")
        .help("Filename of disk image to read")
        .argument::<PathBuf>("IN_FILE");

    let out_filename = short('o')
        .long("out_filename")
        .help("Filename of image to write")
        .argument::<PathBuf>("OUT_FILE");

    let resolution = short('r')
        .long("resolution")
        .help("Size of resulting image, in pixels")
        .argument::<u32>("SIZE");

    let supersample = long("ss")
        .help("Supersample (2,4,8)")
        .argument::<u32>("FACTOR")
        .fallback(1);

    let cc = long("cc").help("Wrap data counter-clockwise").switch();

    construct!(Out {
        debug,
        in_filename,
        out_filename,
        resolution,
        cc,
        supersample
    })
    .to_options()
    .descr("imgviz: generate a graphical visualization of a disk image")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    if !is_power_of_two(opts.resolution) {
        eprintln!("Image size must be a power of two");
        return;
    }

    let render_size = match opts.supersample {
        1 => opts.resolution,
        2 => opts.resolution * 2,
        4 => opts.resolution * 4,
        8 => opts.resolution * 8,
        _ => {
            eprintln!("Supersample must be 2, 4, or 8");
            std::process::exit(1);
        }
    };

    let disk_image = match std::fs::File::open(&opts.in_filename) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Error opening file: {}", e);
            std::process::exit(1);
        }
    };

    let mut reader = std::io::BufReader::new(disk_image);

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let disk = match DiskImage::load(&mut reader) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    let direction = match &opts.cc {
        true => RotationDirection::CounterClockwise,
        false => RotationDirection::Clockwise,
    };

    let resolution = ResolutionType::Byte; // Change to Bit if needed
    let min_radius_fraction = 0.33; // Minimum radius as a fraction of the image size
    let track_gap_weight = 2.0; // Thickness of the boundary circles

    let heads = if disk.heads() > 1 { 2 } else { 1 };
    let high_res_size = (render_size, render_size); // High-resolution image size
    let final_size = (opts.resolution * heads, opts.resolution);

    println!("Rendering {} heads, {} tracks...", heads, disk.track_map[0].len());

    let rendered_image = if heads > 1 {
        let mut imgbuf = ImageBuffer::new(render_size * heads, render_size);

        match render_tracks(
            &disk,
            &mut imgbuf,
            0,
            high_res_size,
            (0, 0),
            min_radius_fraction,
            track_gap_weight,
            direction,
            resolution,
            true,
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };

        match render_tracks(
            &disk,
            &mut imgbuf,
            1,
            high_res_size,
            (high_res_size.0, 0),
            min_radius_fraction,
            track_gap_weight,
            direction.opposite(),
            resolution,
            true,
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };

        imgbuf
    } else {
        let mut imgbuf = ImageBuffer::new(render_size, render_size);

        match render_tracks(
            &disk,
            &mut imgbuf,
            0,
            high_res_size,
            (0, 0),
            min_radius_fraction,
            track_gap_weight,
            direction,
            resolution,
            true,
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };

        imgbuf
    };

    let rendered_image = match opts.supersample {
        1 => match rendered_image.save(opts.out_filename.clone()) {
            Ok(_) => {
                println!("Output image saved to {}", opts.out_filename.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error saving image: {}", e);
                std::process::exit(1);
            }
        },
        _ => {
            let mut src_image = match FirImage::from_vec_u8(
                rendered_image.width(),
                rendered_image.height(),
                rendered_image.into_raw(),
                PixelType::U8x4,
            ) {
                Ok(image) => image,
                Err(e) => {
                    eprintln!("Error converting image: {}", e);
                    std::process::exit(1);
                }
            };
            let mut dst_image = FirImage::new(final_size.0, final_size.1, PixelType::U8x4);

            let mut resizer = Resizer::new();
            let resize_opts =
                fast_image_resize::ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));

            println!("Resampling output image...");
            match resizer.resize(&mut src_image, &mut dst_image, &resize_opts) {
                Ok(_) => {
                    let save_image: ImageBuffer<Rgba<u8>, &[u8]> =
                        ImageBuffer::from_raw(dst_image.width(), dst_image.height(), dst_image.buffer()).unwrap();
                    match save_image.save(opts.out_filename.clone()) {
                        Ok(_) => {
                            println!("Output image saved to {}", opts.out_filename.display());
                        }
                        Err(e) => {
                            eprintln!("Error saving image: {}", e);
                            std::process::exit(1);
                        }
                    }
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Error resizing image: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };
}

fn is_power_of_two(n: u32) -> bool {
    n > 0 && (n & (n - 1)) == 0
}
