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
mod config;
mod palette;
mod render_bitmap;
mod render_svg;
mod style;
mod svg_helpers;
mod text;

use std::{
    io::{Cursor, Write},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use crate::{
    args::{opts, substitute_title},
    palette::{default_error_bit_color, default_style_config, default_weak_bit_color},
    render_bitmap::render_side,
    text::{calculate_scaled_font_size, create_font, measure_text, render_text, Justification},
};

use fluxfox::{
    visualization::{
        tiny_skia::{BlendMode, Color, FilterQuality, Pixmap, PixmapPaint, Transform},
        tiny_skia_util::*,
        types::VizDimensions,
        vectorize_disk_elements,
        CommonVizParams,
        RenderRasterizationParams,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        ResolutionType,
        TurningDirection,
    },
    DiskImage,
};

use crate::render_bitmap::rasterize_display_list;
use crossbeam::channel;

fn main() {
    let mut legend_height = None;

    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    // Perform argument substitution.
    let title = substitute_title(opts.title.clone(), &opts.in_filename);

    // Load default font.
    let font_data = include_bytes!("../../../resources/PTN57F.ttf");
    let font = match create_font(font_data) {
        Ok(font) => font,
        Err(e) => {
            eprintln!("Error loading font: {}", e);
            std::process::exit(1);
        }
    };

    // Enforce power of two image size. This isn't strictly required, but it avoids some aliasing
    // issues when rasterizing.
    if !is_power_of_two(opts.resolution) {
        eprintln!("Image size must be a power of two");
        return;
    }

    // Limit supersampling from 1-8x.
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

    // Read the style configuration file, if specified.
    let style_config = if let Some(style_filename) = opts.style_filename.clone() {
        match config::load_style_config(&style_filename) {
            Ok(style_config) => style_config,
            Err(e) => {
                eprintln!("Error loading style configuration: {}", e);
                std::process::exit(1);
            }
        }
    }
    else {
        default_style_config()
    };

    // Read in the entire input file and put it in a Cursor.
    let mut file_vec = match std::fs::read(opts.in_filename.clone()) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };
    let mut reader = Cursor::new(&mut file_vec);

    // Detect the image file format, or bail.
    let disk_image_type = match DiskImage::detect_format(&mut reader, Some(&opts.in_filename)) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    println!("Reading disk image: {}", opts.in_filename.display());
    println!("Detected disk image type: {}", disk_image_type);

    // Load the disk image or bail.
    let disk = match DiskImage::load(&mut reader, Some(&opts.in_filename), None, None) {
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
                                               // TODO: Make this a command line parameter
    let render_track_gap = 0.10; // Fraction of the track width to leave transparent as a gap between tracks (0.0-1.0)

    let sides_to_render: u32;
    let starting_head: u32;

    // If the user specifies a side, we assume that only that side will be rendered.
    // However, the 'sides' parameter can be overridden to 2 to leave a blank space for the missing
    // empty side. This is useful when rendering slideshows.
    if let Some(side) = opts.side {
        log::debug!("Side {} specified.", side);
        if side >= disk.heads() {
            eprintln!("Disk image does not have requested side: {}", side);
            std::process::exit(1);
        }
        sides_to_render = opts.sides.unwrap_or(1) as u32;
        starting_head = side as u32;

        println!("Visualizing side {}/{}...", starting_head, sides_to_render);
    }
    else {
        // No side was specified. We'll render all sides, starting at side 0.
        sides_to_render = opts.sides.unwrap_or(disk.heads()) as u32;
        starting_head = 0;
        println!("Visualizing {} sides...", sides_to_render);
    }

    // New pool for metadata rendering. Don't bother with quadrants - just render full sides
    let meta_pixmap_pool = [
        Arc::new(Mutex::new(Pixmap::new(opts.resolution, opts.resolution).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution, opts.resolution).unwrap())),
    ];

    let _total_render_start_time = Instant::now();
    let data_render_start_time = Instant::now();
    let mut rendered_pixmaps = Vec::new();

    let image_size = opts.resolution;
    let track_ct = disk.tracks(0) as usize;
    log::trace!("Image has {} tracks.", track_ct);

    // Determine size of legend. Currently, we only have a title.
    let font_h;
    let mut font_size = 0.0;
    if let Some(title_string) = title.clone() {
        font_size = calculate_scaled_font_size(40.0, image_size, 1024);
        (_, font_h) = measure_text(&font, &title_string, font_size);
        legend_height = Some(font_h * 3); // 3 lines of text. Title will be centered within.
        log::debug!("Using title: {}", title_string);
    }

    // Determine whether our output format is SVG or PNG. Only do so if `use_svg` is enabled.
    #[cfg(feature = "use_svg")]
    if let Some(extension) = opts.out_filename.extension() {
        if extension == "svg" {
            log::debug!("Rendering SVG...");
            match render_svg::render_svg(&disk, starting_head, sides_to_render, &opts, &style_config, &title) {
                Ok(_) => {
                    log::debug!("Saved SVG to: {}", opts.out_filename.display());
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Error rendering SVG: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    let a_disk = disk.into_arc();

    for si in 0..sides_to_render {
        let side = si + starting_head;
        log::debug!(" >>> Rendering side {} of {}...", side, sides_to_render);
        let disk = Arc::clone(&a_disk);

        // Default to clockwise turning, unless --cc flag is passed.
        let mut direction = if opts.cc {
            TurningDirection::CounterClockwise
        }
        else {
            TurningDirection::Clockwise
        };

        // Reverse side 1 as long as --dont_reverse flag is not present.
        if side > 0 && !opts.dont_reverse {
            direction = direction.opposite();
        }

        log::debug!("Rendering display list at resolution: {}", opts.resolution);
        let common_params = CommonVizParams {
            radius: Some(image_size as f32 / 2.0),
            max_radius_ratio: 1.0,
            min_radius_ratio: min_radius_fraction,
            pos_offset: None,
            index_angle: direction.adjust_angle(opts.angle),
            track_limit: Some(track_ct),
            pin_last_standard_track: true,
            track_gap: render_track_gap,
            absolute_gap: false,
            direction,
        };

        let render_params = RenderTrackDataParams {
            side: si as u8,
            decode: opts.decode,
            sector_mask: true,
            resolution,
            ..Default::default()
        };

        let rasterization_params = RenderRasterizationParams {
            image_size: VizDimensions::from((image_size, image_size)),
            supersample: opts.supersample,
            image_bg_color: opts.img_bg_color,
            disk_bg_color: opts.track_bg_color,
            mask_color: None,
            palette: None,
            pos_offset: None,
        };

        let weak_color = default_weak_bit_color();
        let error_color = default_error_bit_color();

        // Render data if data flag was passed.
        let mut pixmap = if opts.data {
            match render_side(
                &disk.read().unwrap(),
                &opts,
                &common_params,
                &render_params,
                &rasterization_params,
                weak_color,
                error_color,
            ) {
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

        // Render metadata if requested.
        /*
        if opts.metadata {
            let (sender, receiver) = channel::unbounded::<u8>();
            for quadrant in 0..4 {
                let disk = Arc::clone(&a_disk);
                let pixmap = Arc::clone(&pixmap_pool[quadrant as usize]);
                let sender = sender.clone();
                let palette = palette.clone();
                let direction = match side {
                    0 => TurningDirection::CounterClockwise,
                    1 => TurningDirection::Clockwise,
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

        */

        let (sender, receiver) = channel::unbounded::<u8>();

        if opts.metadata {
            log::debug!("Rendering metadata for side {}...", side);
            let inner_disk = Arc::clone(&a_disk);
            let inner_pixmap = Arc::clone(&meta_pixmap_pool[side as usize]);
            let palette = style_config.element_styles.clone();

            let render_debug = opts.debug;
            let inner_params = common_params.clone();

            thread::spawn(move || {
                let mut metadata_pixmap = inner_pixmap.lock().unwrap();

                // We set the angle to 0.0 here, because tiny_skia can rotate the resulting
                // display list for us.
                let mut common_params = inner_params.clone();
                common_params.index_angle = 0.0;

                let metadata_params = RenderTrackMetadataParams {
                    quadrant: None,
                    head: side as u8,
                    draw_empty_tracks: false,
                    draw_sector_lookup: false,
                };

                let metadata_disk = inner_disk.read().unwrap();

                let list_start_time = Instant::now();
                let display_list = match vectorize_disk_elements(&metadata_disk, &common_params, &metadata_params) {
                    Ok(display_list) => display_list,
                    Err(e) => {
                        eprintln!("Error rendering metadata: {}", e);
                        std::process::exit(1);
                    }
                };

                println!(
                    "visualize_disk_elements() returned a display list of length {} in {:.3}ms",
                    display_list.len(),
                    list_start_time.elapsed().as_secs_f64() * 1000.0
                );

                // Set the index angle for rasterization.
                let angle = inner_params.index_angle;
                rasterize_display_list(&mut metadata_pixmap, angle, &display_list, &palette);

                if render_debug {
                    metadata_pixmap.save_png(format!("new_metadata{}.png", side)).unwrap();
                }

                println!("Sending rendered metadata pixmap for side: {} over channel...", side);
                if let Err(e) = sender.send(side as u8) {
                    eprintln!("Error sending metadata pixmap: {}", e);
                    std::process::exit(1);
                }
            });

            println!("Waiting for metadata pixmap for side {}...", side);
            std::io::stdout().flush().unwrap();
            for (p, recv_side) in receiver.iter().enumerate() {
                // let (x, y) = match recv_side {
                //     0 => (0, 0u32),
                //     1 => (image_size, 0u32),
                //     _ => panic!("Invalid side"),
                // };
                println!("Received metadata pixmap for side {}...", recv_side);
                std::io::stdout().flush().unwrap();

                let paint = match opts.data {
                    true => PixmapPaint {
                        opacity:    1.0,
                        blend_mode: BlendMode::HardLight,
                        quality:    FilterQuality::Nearest,
                    },
                    false => PixmapPaint::default(),
                };

                pixmap.draw_pixmap(
                    0,
                    0,
                    meta_pixmap_pool[recv_side as usize].lock().unwrap().as_ref(), // Convert &Pixmap to PixmapRef
                    &paint,
                    Transform::identity(),
                    None,
                );

                if p == sides_to_render.saturating_sub(1) as usize {
                    break;
                }
            }
        }

        log::debug!("Adding rendered side {} to vector...", side);
        rendered_pixmaps.push(pixmap);
    }

    if rendered_pixmaps.is_empty() {
        eprintln!("No sides rendered!");
        std::process::exit(1);
    }
    println!("Finished data layer in {:?}", data_render_start_time.elapsed());

    let horiz_gap = 0;

    // Combine both sides into a single image, if we have two sides.
    let (mut composited_image, composited_width) = if (rendered_pixmaps.len() > 1) || (sides_to_render == 2) {
        let final_size = (
            image_size * sides_to_render + horiz_gap * (sides_to_render - 1),
            image_size + legend_height.unwrap_or(0) as u32,
        );

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();
        if let Some(color) = opts.img_bg_color {
            final_image.fill(Color::from(color));
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
            final_image.fill(Color::from(color));
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
        draw_index_hole(
            &mut composited_image,
            0.39,
            2.88,
            10.0,
            1.0,
            Color::from_rgba8(255, 255, 255, 255),
            TurningDirection::CounterClockwise,
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
