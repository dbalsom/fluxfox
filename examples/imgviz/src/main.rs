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
mod args;
mod render;
mod text;

use crossbeam::channel;
use std::{
    collections::HashMap,
    io::{Cursor, Write},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};
use tiny_skia::{BlendMode, Color, FilterQuality, Pixmap, PixmapPaint, Transform};

use fluxfox::{
    structure_parsers::DiskStructureGenericElement,
    visualization::{render_track_metadata_quadrant, RenderTrackMetadataParams, ResolutionType, RotationDirection},
    DiskImage,
};

use crate::{
    args::{opts, substitute_title},
    render::render_side,
    text::{calculate_scaled_font_size, create_font, measure_text, render_text, Justification},
};

fn main() {
    let mut legend_height = None;

    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    // Perform argument substitution.
    let title = substitute_title(opts.title, &opts.in_filename);

    // Load font.
    let font_data = include_bytes!("../../../resources/PTN57F.ttf");
    let font = match create_font(font_data) {
        Ok(font) => font,
        Err(e) => {
            eprintln!("Error loading font: {}", e);
            std::process::exit(1);
        }
    };

    if !is_power_of_two(opts.resolution) {
        eprintln!("Image size must be a power of two");
        return;
    }

    match opts.supersample {
        1 => opts.resolution,
        2 => opts.resolution * 2,
        4 => opts.resolution * 4,
        8 => opts.resolution * 8,
        _ => {
            eprintln!("Supersample must be 2, 4, or 8");
            std::process::exit(1);
        }
    };

    let mut file_vec = match std::fs::read(opts.in_filename.clone()) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };
    let mut reader = Cursor::new(&mut file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    println!("Reading disk image: {}", opts.in_filename.display());
    println!("Detected disk image type: {}", disk_image_type);

    let disk = match DiskImage::load(&mut reader, Some(opts.in_filename), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    // let direction = match &opts.cc {
    //     true => RotationDirection::CounterClockwise,
    //     false => RotationDirection::Clockwise,
    // };

    let resolution = ResolutionType::Byte; // Change to Bit if needed
    let min_radius_fraction = opts.hole_ratio; // Minimum radius as a fraction of the image size

    let render_track_gap = 0.10; // Fraction of the track width to leave transparent as a gap between tracks (0.0-1.0)

    let heads;
    let sides;
    let mut head: u32 = 0;
    if let Some(side) = opts.side {
        if disk.heads() < side {
            eprintln!("Disk image does not have side {}", side);
            std::process::exit(1);
        }
        heads = 1;
        sides = opts.sides.unwrap_or(1) as u32;
        head = side as u32;
    }
    else {
        heads = if disk.heads() > 1 { 2 } else { 1 };
        sides = opts.sides.unwrap_or(if disk.heads() > 1 { 2 } else { 1 }) as u32;
    }

    println!("Rendering {} heads, {} tracks...", heads, disk.tracks(0));

    #[rustfmt::skip]
    let pixmap_pool: Vec<Arc<Mutex<Pixmap>>> = vec![
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution / 2, opts.resolution / 2).unwrap())),
    ];

    let null_color = Color::from_rgba8(0, 0, 0, 0);

    //let viz_red: Color = Color::from_rgba8(255, 0, 0, 255);
    //let viz_green: Color = Color::from_rgba8(0, 255, 0, 255);
    //let viz_blue: Color = Color::from_rgba8(0, 0, 255, 255);

    //let viz_light_blue: Color = Color::from_rgba8(0, 0, 180, 255);
    //let viz_light_green: Color = Color::from_rgba8(0, 180, 0, 255);
    let viz_light_red: Color = Color::from_rgba8(180, 0, 0, 255);

    //let viz_orange: Color = Color::from_rgba8(255, 100, 0, 255);
    let vis_purple: Color = Color::from_rgba8(180, 0, 180, 255);
    //let viz_cyan: Color = Color::from_rgba8(70, 200, 200, 255);
    //let vis_light_purple: Color = Color::from_rgba8(185, 0, 255, 255);

    let pal_medium_green = Color::from_rgba8(0x38, 0xb7, 0x64, 0xff);
    let pal_dark_green = Color::from_rgba8(0x25, 0x71, 0x79, 0xff);
    //let pal_dark_blue = Color::from_rgba8(0x29, 0x36, 0x6f, 0xff);
    let pal_medium_blue = Color::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
    let pal_light_blue = Color::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
    //let pal_dark_purple = Color::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
    let pal_orange = Color::from_rgba8(0xef, 0x7d, 0x57, 0xff);
    //let pal_dark_red = Color::from_rgba8(0xb1, 0x3e, 0x53, 0xff);
    let pal_weak_bits = Color::from_rgba8(70, 200, 200, 255);
    let pal_error_bits = Color::from_rgba8(255, 0, 0, 255);

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

    // Determine size of legend. Currently, we only have a title.
    let font_h;
    let mut font_size = 0.0;
    if let Some(title_string) = title.clone() {
        font_size = calculate_scaled_font_size(40.0, image_size, 1024);
        (_, font_h) = measure_text(&font, &title_string, font_size);
        legend_height = Some(font_h * 3); // 3 lines of text. Title will be centered within.
        log::debug!("Using title: {}", title_string);
    }

    for side in head..heads {
        let disk = Arc::clone(&a_disk);

        // Render data if data flag was passed.
        let mut pixmap = if opts.data {
            let params = render::RenderParams {
                bg_color: opts.img_bg_color,
                track_bg_color: Some(Color::from_rgba8(0, 0, 0, 255)),
                render_size: image_size,
                supersample: opts.supersample as u8,
                side,
                min_radius: min_radius_fraction,
                angle: opts.angle,
                track_limit: track_ct,
                track_gap: render_track_gap,
                decode: opts.decode,
                weak: opts.weak,
                errors: opts.errors,
                weak_color: Some(pal_weak_bits),
                error_color: Some(pal_error_bits),
                resolution_type: resolution,
            };

            match render_side(&disk.lock().unwrap(), params) {
                Ok(pixmap) => pixmap,
                Err(e) => {
                    eprintln!("Error rendering side: {}", e);
                    std::process::exit(1);
                }
            }
        }
        else {
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

                    let render_params = RenderTrackMetadataParams {
                        quadrant,
                        head: side as u8,
                        min_radius_fraction,
                        index_angle: opts.angle,
                        track_limit: track_ct,
                        track_gap: render_track_gap,
                        direction,
                        palette,
                        draw_empty_tracks: false,
                        pin_last_standard_track: true,
                        draw_sector_lookup: false,
                    };
                    _ = render_track_metadata_quadrant(&l_disk, &mut pixmap, &render_params);

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
                        opacity:    1.0,
                        blend_mode: BlendMode::HardLight,
                        quality:    FilterQuality::Nearest,
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

    // Combine both sides into a single image, if we have two sides.
    let (mut composited_image, composited_width) = if (rendered_pixmaps.len() > 1) || (sides == 2) {
        let final_size = (
            image_size * sides + horiz_gap * (sides - 1),
            image_size + legend_height.unwrap_or(0) as u32,
        );

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();
        if let Some(color) = opts.img_bg_color {
            final_image.fill(color);
        }

        println!("Compositing sides...");
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
        (final_image, final_size.0)
    }
    else if let Some(height) = legend_height {
        // Just one side, but we have a legend.
        let final_size = (image_size, image_size + height as u32);

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();
        if let Some(color) = opts.img_bg_color {
            final_image.fill(color);
        }

        println!("Compositing side...");
        final_image.draw_pixmap(
            0,
            0,
            rendered_pixmaps.pop().unwrap().as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );

        println!("Saving final image as {}", opts.out_filename.display());
        (final_image, image_size)
    }
    else {
        // Just one side, and no legend. Nothing to composite.
        println!("Saving final image as {}", opts.out_filename.display());
        (rendered_pixmaps.pop().unwrap(), image_size)
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

    // Render text if requested.
    if let Some(title_string) = title.clone() {
        let (_, font_h) = measure_text(&font, &title_string, font_size);

        let legend_height = legend_height.unwrap_or(0);
        let x = (composited_width / 2) as i32;
        let y = image_size as i32 + legend_height - font_h; // Draw text one 'line' up from bottom of image.

        log::debug!("Rendering text at ({}, {})", x, y);
        let font_color = Color::from_rgba8(255, 255, 255, 255);
        render_text(
            &mut composited_image,
            &font,
            font_size,
            &title_string,
            x,
            y,
            Justification::Center,
            font_color,
        );
    }

    // Save image to disk.
    composited_image.save_png(opts.out_filename.clone()).unwrap();

    //println!("Metadata layer rendered in: {:?}", metadata_render_start_time.elapsed());

    //total_render_duration += total_render_start_time.elapsed();
    //println!("Total render time: {:?}", total_render_duration);

    //colorize_image.save_png(opts.out_filename.clone()).unwrap();
}

fn is_power_of_two(n: u32) -> bool {
    n > 0 && (n & (n - 1)) == 0
}
