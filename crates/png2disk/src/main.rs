/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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
*/

//! png2disk
//! An entirely useless utility that writes PNG files to disk images, mostly
//! because we can. Have fun making floppy art!

mod args;
mod disk;

use std::io::Cursor;

use fluxfox::{format_from_ext, prelude::*, visualization::prelude::*, DiskImage, ImageBuilder, ImageWriter};

use crate::{args::opts, disk::repair_crcs};

use fluxfox_tiny_skia::tiny_skia::Pixmap;
use tiny_skia::{PixmapPaint, PixmapRef, Transform};

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    let mut disk = if let Some(in_disk) = opts.in_disk {
        let mut file_vec = match std::fs::read(in_disk.clone()) {
            Ok(file_vec) => file_vec,
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                std::process::exit(1);
            }
        };
        let mut reader = Cursor::new(&mut file_vec);

        let disk_image_type = match DiskImage::detect_format(&mut reader, Some(&in_disk)) {
            Ok(disk_image_type) => disk_image_type,
            Err(e) => {
                eprintln!("Error detecting disk image type: {}", e);
                std::process::exit(1);
            }
        };

        println!("Reading disk image: {}", in_disk.display());
        println!("Detected disk image type: {}", disk_image_type);

        match DiskImage::load(&mut reader, Some(&in_disk), None, None) {
            Ok(disk) => disk,
            Err(e) => {
                eprintln!("Error loading disk image: {}", e);
                std::process::exit(1);
            }
        }
    }
    else {
        match ImageBuilder::new()
            .with_resolution(TrackDataResolution::BitStream)
            .with_standard_format(opts.disk_format)
            .with_formatted(opts.formatted)
            .build()
        {
            Ok(disk) => disk,
            Err(e) => {
                eprintln!("Error creating disk image: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Load the image for the first side.
    let mut pixmap0 = match Pixmap::load_png(&opts.in_image0) {
        Ok(pixmap) => pixmap,
        Err(e) => {
            eprintln!("Error loading PNG image: {}", e);
            std::process::exit(1);
        }
    };

    // Load the image for the second side, if specified.
    let mut pixmap1_opt = if let Some(in_image1) = opts.in_image1 {
        let pixmap = match Pixmap::load_png(&in_image1) {
            Ok(pixmap) => pixmap,
            Err(e) => {
                eprintln!("Error loading PNG image: {}", e);
                std::process::exit(1);
            }
        };
        Some(pixmap)
    }
    else {
        None
    };

    if opts.applesauce {
        pixmap0 = rotate_pixmap(pixmap0.as_ref(), 90.0);
        if let Some(pixmap1) = pixmap1_opt.as_mut() {
            *pixmap1 = rotate_pixmap(pixmap1.as_ref(), 90.0);
        }
    }

    let mut common_params = CommonVizParams {
        radius: Some(pixmap0.height() as f32 / 2.0),
        max_radius_ratio: opts.hole_ratio.unwrap_or(match opts.applesauce {
            false => 0.3, // Good hole ratio for HxC and fluxfox
            true => 0.27, // Applesauce has slightly smaller hole
        }),
        min_radius_ratio: 1.0,
        pos_offset: None,
        index_angle: opts.angle,
        track_limit: Some(disk.tracks(0) as usize),
        pin_last_standard_track: false,
        track_gap: 0.0,
        direction: TurningDirection::Clockwise,
        ..CommonVizParams::default()
    };

    let mut data_params = RenderTrackDataParams {
        side: 0,
        // Not used
        decode: false,
        // Whether to mask the image to sector data areas
        sector_mask: opts.sectors_only,
        // Not used
        resolution: Default::default(),
        // Not used
        slices: 0,
        overlap: 0.0,
    };

    // let mut data_params = RenderTrackDataParams {
    //     image_size: (pixmap0.width(), pixmap0.height()),
    //     image_pos: (0, 0),
    //     side: 0,
    //     track_limit: disk.tracks(0) as usize,
    //     min_radius_ratio: opts.hole_ratio.unwrap_or(match opts.applesauce {
    //         false => 0.3, // Good hole ratio for HxC and fluxfox
    //         true => 0.27, // Applesauce has slightly smaller hole
    //     }),
    //     index_angle: opts.angle,
    //     direction: TurningDirection::Clockwise,
    //     sector_mask: opts.sectors_only,
    //     ..Default::default()
    // };

    // If the user specified initial counter-clockwise rotation, change the direction.
    if opts.cc {
        common_params.direction = TurningDirection::CounterClockwise;
    }

    let pixmap_params = PixmapToDiskParams {
        skip_tracks: opts.skip,
        ..Default::default()
    };

    let render = |pixmap: &Pixmap,
                  disk: &mut DiskImage,
                  common_params: &CommonVizParams,
                  pixmap_params: &PixmapToDiskParams,
                  data_params: &RenderTrackDataParams| {
        match opts.grayscale {
            true => match render_pixmap_to_disk_grayscale(pixmap, disk, common_params, data_params, pixmap_params) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Error rendering pixmap to disk: {}", e);
                    std::process::exit(1);
                }
            },
            false => match render_pixmap_to_disk(pixmap, disk, common_params, data_params, pixmap_params) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Error rendering pixmap to disk: {}", e);
                    std::process::exit(1);
                }
            },
        }
    };

    println!("Rendering side 0...");
    // Render the first side.
    render(&pixmap0, &mut disk, &common_params, &pixmap_params, &data_params);

    // Render the second side, if present.
    if let Some(pixmap1) = pixmap1_opt {
        if disk.heads() > 1 {
            // Applesauce doesn't change the rotation direction for the second side.
            if !opts.applesauce {
                common_params.direction = common_params.direction.opposite();
            }
            common_params.track_limit = Some(disk.tracks(1) as usize);
            data_params.side = 1;
            common_params.index_angle = common_params.direction.adjust_angle(opts.angle);
            println!("Rendering side 1...");
            render(
                &pixmap1.to_owned(),
                &mut disk,
                &common_params,
                &pixmap_params,
                &data_params,
            );
        }
    }

    //If we rendered to sector data, repair the sector CRCs now.
    if opts.sectors_only {
        match repair_crcs(&mut disk) {
            Ok(_) => println!("Successfully repaired sector CRCs."),
            Err(e) => {
                eprintln!("Error repairing sector CRCs: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Get extension from output filename
    let output_format = opts
        .out_disk
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(format_from_ext)
        .unwrap_or_else(|| {
            eprintln!("Error: Invalid or unknown output file extension!");
            std::process::exit(1);
        });

    match ImageWriter::new(&mut disk)
        .with_format(output_format)
        .with_path(opts.out_disk)
        .write()
    {
        Ok(_) => {
            println!("Successfully wrote disk image.");
        }
        Err(e) => {
            eprintln!("Error writing disk image: {}", e);
            std::process::exit(1);
        }
    }
}

fn rotate_pixmap(pixmap: PixmapRef, angle: f32) -> Pixmap {
    let mut new_pixmap = Pixmap::new(pixmap.height(), pixmap.width()).unwrap();
    new_pixmap.draw_pixmap(
        0,
        0,
        pixmap,
        &PixmapPaint::default(),
        Transform::from_rotate(angle).post_translate(pixmap.height() as f32, 0.0),
        None,
    );
    new_pixmap
}
