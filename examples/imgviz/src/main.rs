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

use anyhow::bail;
use bpaf::*;
use crossbeam::channel;
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, Resizer};
use fluxfox::structure_parsers::DiskStructureGenericElement;
use fluxfox::visualization::RotationDirection;
use fluxfox::visualization::{render_track_data, render_track_metadata_quadrant};
use fluxfox::visualization::{render_track_weak_bits, ResolutionType};
use fluxfox::DiskImage;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tiny_skia::{BlendMode, Color, FilterQuality, IntSize, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    in_filename: PathBuf,
    out_filename: PathBuf,
    resolution: u32,
    side: Option<u8>,
    hole_ratio: f32,
    angle: f32,
    data: bool,
    weak: bool,
    metadata: bool,
    index_hole: bool,
    decode: bool,
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

    let side = short('s')
        .long("side")
        .help("Side to render. Omit to render both sides")
        .argument::<u8>("SIDE")
        .guard(|side| *side < 2, "Side must be 0 or 1")
        .optional();

    let hole_ratio = short('h')
        .long("hole_ratio")
        .help("Ratio of inner radius to outer radius")
        .argument::<f32>("RATIO")
        .fallback(0.33);

    let angle = short('a')
        .long("angle")
        .help("Angle of rotation")
        .argument::<f32>("ANGLE")
        .fallback(0.0);

    let data = long("data").help("Render data").switch();

    let weak = short('w').long("weak").help("Render weak bits").switch();

    let metadata = long("metadata").help("Render metadata").switch();

    let decode = long("decode").help("Decode data").switch();

    let index_hole = long("index_hole").help("Render index hole").switch();

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
        side,
        hole_ratio,
        angle,
        data,
        weak,
        metadata,
        index_hole,
        decode,
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

    println!("Reading disk image: {}", opts.in_filename.display());
    println!("Detected disk image type: {}", disk_image_type);

    let disk = match DiskImage::load(&mut reader, Some(opts.in_filename), None) {
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
    let min_radius_fraction = opts.hole_ratio; // Minimum radius as a fraction of the image size

    let render_track_gap = 0.10; // Fraction of the track width to leave transparent as a gap between tracks (0.0-1.0)

    let heads;
    let mut head: u32 = 0;
    if let Some(side) = opts.side {
        if disk.heads() < side {
            eprintln!("Disk image does not have side {}", side);
            std::process::exit(1);
        }
        heads = 1;
        head = side as u32;
    } else {
        heads = if disk.heads() > 1 { 2 } else { 1 };
    }

    let high_res_size = (render_size, render_size); // High-resolution image size
    let final_size = (opts.resolution * heads, opts.resolution);

    println!("Rendering {} heads, {} tracks...", heads, disk.tracks(0));

    #[rustfmt::skip]
    let pixmap_pool: Vec<Arc<Mutex<Pixmap>>> = vec![
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
    ];

    let null_color = Color::from_rgba8(0, 0, 0, 0);

    let viz_red: Color = Color::from_rgba8(255, 0, 0, 255);
    let viz_green: Color = Color::from_rgba8(0, 255, 0, 255);
    let viz_blue: Color = Color::from_rgba8(0, 0, 255, 255);

    let viz_light_blue: Color = Color::from_rgba8(0, 0, 180, 255);
    let viz_light_green: Color = Color::from_rgba8(0, 180, 0, 255);
    let viz_light_red: Color = Color::from_rgba8(180, 0, 0, 255);

    let viz_orange: Color = Color::from_rgba8(255, 100, 0, 255);
    let vis_purple: Color = Color::from_rgba8(180, 0, 180, 255);
    let viz_cyan: Color = Color::from_rgba8(70, 200, 200, 255);
    let vis_light_purple: Color = Color::from_rgba8(185, 0, 255, 255);

    let pal_medium_green = Color::from_rgba8(0x38, 0xb7, 0x64, 0xff);
    let pal_dark_green = Color::from_rgba8(0x25, 0x71, 0x79, 0xff);
    let pal_dark_blue = Color::from_rgba8(0x29, 0x36, 0x6f, 0xff);
    let pal_medium_blue = Color::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
    let pal_light_blue = Color::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
    let pal_dark_purple = Color::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
    let pal_orange = Color::from_rgba8(0xef, 0x7d, 0x57, 0xff);
    let pal_dark_red = Color::from_rgba8(0xb1, 0x3e, 0x53, 0xff);

    let pal_weak_bits = PremultipliedColorU8::from_rgba(70, 200, 200, 255).unwrap();

    #[rustfmt::skip]
    let palette = HashMap::from([
        (DiskStructureGenericElement::SectorData, pal_medium_green),
        (DiskStructureGenericElement::SectorBadData, pal_orange),
        (DiskStructureGenericElement::SectorDeletedData, pal_dark_green),
        (DiskStructureGenericElement::SectorBadDeletedData, viz_light_red),
        (DiskStructureGenericElement::SectorHeader, pal_light_blue),
        (DiskStructureGenericElement::SectorBadHeader, pal_medium_blue),
        (DiskStructureGenericElement::Marker, vis_purple),
    ]);

    let _total_render_start_time = Instant::now();
    let data_render_start_time = Instant::now();
    let mut rendered_pixmaps = Vec::new();

    let image_size = opts.resolution;
    let track_ct = disk.tracks(0) as usize;
    log::trace!("Image has {} tracks.", track_ct);
    let a_disk = Arc::new(Mutex::new(disk));

    for side in head..heads {
        let disk = Arc::clone(&a_disk);

        // Render data if data flag was passed.
        let mut pixmap = if opts.data {
            match render_side(
                &disk.lock().unwrap(),
                image_size,
                opts.supersample as u8,
                side,
                min_radius_fraction,
                opts.angle,
                track_ct,
                render_track_gap,
                opts.decode,
                opts.weak,
                pal_weak_bits,
                resolution,
            ) {
                Ok(pixmap) => pixmap,
                Err(e) => {
                    eprintln!("Error rendering side: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            Pixmap::new(image_size, image_size).unwrap()
        };

        drop(disk);

        if opts.metadata {
            let (sender, receiver) = channel::unbounded::<u8>();
            for quadrant in 0..4 {
                let disk = Arc::clone(&a_disk);
                let pixmap = Arc::clone(&pixmap_pool[quadrant as usize]);
                let sender = sender.clone();
                let palette = palette.clone();
                let direction = match side {
                    0 => RotationDirection::CounterClockwise,
                    1 => RotationDirection::Clockwise,
                    _ => panic!("Invalid side"),
                };
                thread::spawn(move || {
                    let mut pixmap = pixmap.lock().unwrap();
                    let l_disk = disk.lock().unwrap();
                    _ = render_track_metadata_quadrant(
                        &l_disk,
                        &mut pixmap,
                        quadrant,
                        side as u8,
                        min_radius_fraction,
                        opts.angle,
                        track_ct,
                        render_track_gap,
                        direction,
                        palette,
                        false,
                    );

                    //println!("Sending quadrant over channel...");
                    match sender.send(quadrant) {
                        Ok(_) => {
                            //println!("...Sent!");
                        }
                        Err(e) => {
                            eprintln!("Error sending quadrant: {}", e);
                            std::process::exit(1);
                        }
                    }
                });
            }

            println!("Rendering metadata quadrants...");
            _ = std::io::stdout().flush();

            //std::thread::sleep(std::time::Duration::from_secs(10));

            //        std::process::exit(1);
            for (q, quadrant) in receiver.iter().enumerate() {
                //println!("Received quadrant {}, compositing...", quadrant);
                let (x, y) = match quadrant {
                    0 => (0, 0),
                    1 => (image_size / 2, 0),
                    2 => (0, image_size / 2),
                    3 => (image_size / 2, image_size / 2),
                    _ => panic!("Invalid quadrant"),
                };

                // pixmap_pool[quadrant as usize]
                //     .lock()
                //     .unwrap()
                //     .save_png(format!("metadata_quadrant_{}.png", quadrant))
                //     .unwrap();

                let paint = match opts.data {
                    true => PixmapPaint {
                        opacity: 1.0,
                        blend_mode: BlendMode::HardLight,
                        quality: FilterQuality::Nearest,
                    },
                    false => PixmapPaint::default(),
                };

                pixmap.draw_pixmap(
                    x as i32,
                    y as i32,
                    pixmap_pool[quadrant as usize].lock().unwrap().as_ref(), // Convert &Pixmap to PixmapRef
                    &paint,
                    Transform::identity(),
                    None,
                );

                if side == 0 {
                    pixmap_pool[quadrant as usize].lock().unwrap().as_mut().fill(null_color);
                }

                if q == 3 {
                    break;
                }
            }
        }

        rendered_pixmaps.push(pixmap);
    }

    println!("Finished data layer in {:?}", data_render_start_time.elapsed());

    let horiz_gap = 0;

    let mut composited_image = if rendered_pixmaps.len() > 0 {
        let final_size = (image_size * heads + horiz_gap * (heads - 1), image_size);

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();

        for (i, pixmap) in rendered_pixmaps.iter().enumerate() {
            //println!("Compositing pixmap #{}", i);
            final_image.draw_pixmap(
                (image_size + horiz_gap) as i32 * i as i32,
                0,
                pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }

        println!("Saving final image as {}", opts.out_filename.display());
        final_image
    } else {
        println!("Saving final image as {}", opts.out_filename.display());
        rendered_pixmaps.pop().unwrap()
    };

    // Render index hole if requested.
    if opts.index_hole {
        fluxfox::visualization::draw_index_hole(
            &mut composited_image,
            0.39,
            2.88,
            10.0,
            1.0,
            Color::from_rgba8(255, 255, 255, 255),
            RotationDirection::CounterClockwise,
        );
    }

    // Save image to disk.
    composited_image.save_png(opts.out_filename.clone()).unwrap();

    //println!("Metadata layer rendered in: {:?}", metadata_render_start_time.elapsed());

    //total_render_duration += total_render_start_time.elapsed();
    //println!("Total render time: {:?}", total_render_duration);

    //colorize_image.save_png(opts.out_filename.clone()).unwrap();
}

fn render_side(
    disk: &DiskImage,
    render_size: u32,
    supersample: u8,
    side: u32,
    min_radius: f32,
    angle: f32,
    track_limit: usize,
    track_gap: f32,
    decode: bool,
    weak: bool,
    weak_color: PremultipliedColorU8,
    resolution_type: ResolutionType,
) -> Result<Pixmap, anyhow::Error> {
    let direction = match side {
        0 => RotationDirection::Clockwise,
        1 => RotationDirection::CounterClockwise,
        _ => {
            bail!("Invalid side: {}", side);
        }
    };

    let supersample_size = match supersample {
        1 => render_size,
        2 => render_size * 2,
        4 => render_size * 4,
        8 => render_size * 8,
        _ => {
            bail!("Invalid supersample factor: {}", supersample);
        }
    };

    let mut rendered_image = Pixmap::new(supersample_size, supersample_size).unwrap();
    let data_render_start_time = Instant::now();
    match render_track_data(
        &disk,
        &mut rendered_image,
        side as u8,
        (supersample_size, supersample_size),
        (0, 0),
        min_radius,
        angle,
        track_limit,
        track_gap,
        direction,
        decode,
        resolution_type,
    ) {
        Ok(_) => {
            println!("Rendered data layer in {:?}", data_render_start_time.elapsed());
        }
        Err(e) => {
            eprintln!("Error rendering tracks: {}", e);
            std::process::exit(1);
        }
    };

    // Render weak bits on composited image if requested.
    if weak {
        let weak_render_start_time = Instant::now();
        println!("Rendering weak bits layer...");
        match render_track_weak_bits(
            &disk,
            &mut rendered_image,
            side as u8,
            (supersample_size, supersample_size),
            (0, 0),
            min_radius,
            angle,
            track_limit,
            track_gap,
            direction,
            weak_color,
        ) {
            Ok(_) => {
                println!("Rendered weak bits layer in {:?}", weak_render_start_time.elapsed());
            }
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };
    }

    let resampled_image = match supersample {
        1 => rendered_image,
        _ => {
            let resample_start_time = Instant::now();

            let mut src_image = match FirImage::from_slice_u8(
                rendered_image.width(),
                rendered_image.height(),
                rendered_image.data_mut(),
                PixelType::U8x4,
            ) {
                Ok(image) => image,
                Err(e) => {
                    eprintln!("Error converting image: {}", e);
                    std::process::exit(1);
                }
            };
            let mut dst_image = FirImage::new(render_size, render_size, PixelType::U8x4);

            let mut resizer = Resizer::new();
            let resize_opts =
                fast_image_resize::ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));

            println!("Resampling output image...");
            match resizer.resize(&mut src_image, &mut dst_image, &resize_opts) {
                Ok(_) => {
                    println!(
                        "Resampled image to {} in {:?}",
                        render_size,
                        resample_start_time.elapsed()
                    );
                    Pixmap::from_vec(
                        dst_image.into_vec(),
                        IntSize::from_wh(render_size, render_size).unwrap(),
                    )
                    .unwrap()
                }
                Err(e) => {
                    eprintln!("Error resizing image: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    Ok(resampled_image)
}

fn is_power_of_two(n: u32) -> bool {
    n > 0 && (n & (n - 1)) == 0
}
