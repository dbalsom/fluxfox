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
*/

//! Module to render disk visualization to SVG using the `svg` crate.

use crate::{
    args::VizArgs,
    config::StyleConfig,
    style::{style_map_to_skia, Style},
    svg_helpers::*,
};
use anyhow::Error;
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::{skia_render_element, vectorize_disk_elements_by_quadrants, VizRect},
        vectorize_disk_elements,
        CommonVizParams,
        RenderTrackMetadataParams,
        ResolutionType,
        TurningDirection,
        VizElementDisplayList,
    },
    DiskImage,
};
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    sync::{Arc, RwLock},
    time::Instant,
};
use svg::{
    node::element::{path::Data, Group},
    Document,
};
use tiny_skia::{BlendMode, Paint, Pixmap, Transform};

pub(crate) fn render_svg(
    disk: &DiskImage,
    starting_head: u32,
    sides_to_render: u32,
    opts: &VizArgs,
    style: &StyleConfig,
    title: &Option<String>,
) -> Result<(), Error> {
    let resolution = ResolutionType::Byte; // Change to Bit if needed
    let min_radius_fraction = opts.hole_ratio; // Minimum radius as a fraction of the image size
                                               // TODO: Make this a command line parameter
    let render_track_gap = 0.10; // Fraction of the track width to leave transparent as a gap between tracks (0.0-1.0)
    let image_size = opts.resolution;
    let track_ct = disk.tracks(0) as usize;

    // Default to clockwise turning, unless --cc flag is passed.
    let direction = if opts.cc {
        TurningDirection::CounterClockwise
    }
    else {
        TurningDirection::Clockwise
    };

    let mut common_params = CommonVizParams {
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

    // Set our viewbox to the size of the image
    let viewbox = VizRect::from((
        0.0,
        0.0,
        image_size as f32 * sides_to_render as f32 + opts.side_spacing,
        image_size as f32,
    ));
    let mut groups = VecDeque::new();

    for si in 0..sides_to_render {
        let side = si + starting_head;

        if opts.metadata {
            log::debug!("Rendering metadata for side {}...", side);

            let metadata_params = RenderTrackMetadataParams {
                quadrant: None,
                head: side as u8,
                draw_empty_tracks: false,
                draw_sector_lookup: false,
            };

            let list_start_time = Instant::now();
            let display_list = match vectorize_disk_elements_by_quadrants(disk, &common_params, &metadata_params) {
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

            let group = render_display_list_as_svg(
                VizRect::from((0.0, 0.0, image_size as f32, image_size as f32)),
                opts.angle,
                &display_list,
                &style.track_style,
                &style.element_styles,
            );

            groups.push_back(group);
        }

        // Change turning direction for next side, unless --dont-reverse flag is passed.
        if !opts.dont_reverse {
            common_params.direction = direction.opposite();
        }
    }

    // Create a new Document
    let document = Document::new().set("viewBox", viewbox.to_tuple());

    // Add first group to the document (side 0)
    let document = if let Some(group) = groups.pop_front() {
        document.add(group)
    }
    else {
        document
    };

    // Add second group to the document (side 1)
    let document = if let Some(group) = groups.pop_front() {
        document.add(group.set(
            "transform",
            format!("translate({:.3}, 0)", opts.side_spacing + image_size as f32),
        ))
    }
    else {
        document
    };

    svg::save(&opts.out_filename, &document)?;
    Ok(())
}

pub fn render_display_list_as_svg(
    viewbox: VizRect<f32>,
    angle: f32,
    display_list: &VizElementDisplayList,
    track_style: &Style,
    element_styles: &HashMap<GenericTrackElement, Style>,
) -> Group {
    let center = viewbox.center();
    let angle_degrees = angle.to_degrees();

    // Rotate around the center of the viewbox
    let mut group = svg::node::element::Group::new().set(
        "transform",
        format!("rotate({} {:.3} {:.3})", angle_degrees, center.x, center.y),
    );

    for element in display_list.iter() {
        let path = svg_render_element(element, track_style, element_styles);
        group = group.add(path);
    }

    group
}
