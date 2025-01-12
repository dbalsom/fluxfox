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
mod legend;
mod palette;
mod render_bitmap;
mod render_svg;
mod style;
mod text;

use crate::{
    args::{opts, substitute_title},
    palette::{default_error_bit_color, default_style_config, default_weak_bit_color},
    render_bitmap::rasterize_data_layer,
    text::{calculate_scaled_font_size, create_font, measure_text, render_text, Justification},
};
use rusttype::Font;
use std::{
    io::{Cursor, Write},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use crate::legend::VizLegend;
use fluxfox::{
    visualization::{
        prelude::*,
        CommonVizParams,
        RenderRasterizationParams,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        ResolutionType,
        TurningDirection,
    },
    DiskImage,
};

pub const MAX_SLICES: usize = 2880;
pub const DEFAULT_DATA_SLICES: usize = 1440;

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    // Perform argument substitution.
    let title = substitute_title(opts.title.clone(), &opts.in_filename);

    // Create a VizLegend.
    let mut legend = VizLegend::new(&title.unwrap_or("".to_string()), opts.resolution);

    // Load default title font and add it to the legend.
    let font_data = include_bytes!("../../../resources/PTN57F.ttf");
    let font: Font<'static> = match create_font(font_data) {
        Some(font) => font,
        None => {
            eprintln!("Error loading font.");
            std::process::exit(1);
        }
    };

    legend.set_title_font(font, 40.0);

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

    let image_size = opts.resolution;
    let track_ct = disk.tracks(0) as usize;
    log::trace!("Image has {} tracks.", track_ct);

    // Determine whether our output format is SVG or PNG. Only do so if `use_svg` is enabled.
    #[cfg(feature = "use_svg")]
    if let Some(extension) = opts.out_filename.extension() {
        if extension == "svg" {
            log::debug!("Rendering SVG...");
            match render_svg::render_svg(&disk, starting_head, sides_to_render, &opts, &style_config, &legend) {
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

    #[cfg(feature = "use_tiny_skia")]
    if let Some(extension) = opts.out_filename.extension() {
        if extension == "png" {
            log::debug!("Rendering bitmap...");
            match render_bitmap::render_bitmap(disk, starting_head, sides_to_render, &opts, &style_config, &mut legend)
            {
                Ok(_) => {
                    log::debug!("Saved bitmap to: {}", opts.out_filename.display());
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Error rendering bitmap: {}", e);
                    std::process::exit(1);
                }
            }
        }
        else {
            eprintln!("Unknown extension: {}. Did you mean .PNG?", extension.to_string_lossy());
        }
    }
}

fn is_power_of_two(n: u32) -> bool {
    n > 0 && (n & (n - 1)) == 0
}
