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

//! Module to render disk visualization to SVG using the `svg` crate.

use crate::{args::VizArgs, config::StyleConfig, style::style_map_to_fluxfox_svg};

use fluxfox::{
    visualization::{prelude::*, TurningDirection},
    DiskImage,
};

use anyhow::{anyhow, Error};

use crate::legend::VizLegend;
use fluxfox_svg::prelude::*;

pub(crate) fn render_svg(
    disk: &DiskImage,
    starting_head: u32,
    sides_to_render: u32,
    opts: &VizArgs,
    style: &StyleConfig,
    _legend: &VizLegend,
) -> Result<(), Error> {
    // Render with fluxfox_svg's SvgRenderer

    let render_timer = std::time::Instant::now();
    let mut renderer = SvgRenderer::new()
        .side_by_side(true, 20.0)
        .with_radius_ratios(0.55, 0.88)
        .with_track_gap(opts.track_gap.unwrap_or(style.track_gap))
        .with_data_layer(opts.data, Some(opts.data_slices))
        .decode_data(opts.decode)
        .with_metadata_layer(opts.metadata)
        .with_index_angle(opts.angle)
        .with_layer_stack(true)
        .with_side_view_box(VizRect::from((
            0.0,
            0.0,
            opts.resolution as f32,
            opts.resolution as f32,
        )))
        .with_styles(style_map_to_fluxfox_svg(&style.element_styles))
        .with_blend_mode(style.blend_mode.into())
        .with_overlay(Overlay::Overlay5_25)
        .with_initial_turning(if opts.cc {
            TurningDirection::CounterClockwise
        }
        else {
            TurningDirection::Clockwise
        });

    if sides_to_render == 1 {
        renderer = renderer.with_side(starting_head as u8);
    }
    else {
        renderer = renderer.side_by_side(true, opts.side_spacing);
    }

    let documents = renderer
        .render(disk)
        .map_err(|e| anyhow!("Error rendering SVG documents: {}", e))?
        .create_documents()
        .map_err(|e| anyhow!("Error rendering SVG documents: {}", e))?;

    let render_time = render_timer.elapsed();
    log::debug!("SVG rendering took {:.3}ms", render_time.as_secs_f64() * 1000.0);

    if documents.is_empty() {
        return Err(anyhow!("No SVG documents were created!"));
    }

    if documents.len() > 1 {
        log::warn!("Multiple SVG documents were created, but only the first will be saved.");
    }

    println!("Saving SVG to {}...", opts.out_filename.display());
    svg::save(&opts.out_filename, &documents[0].document)?;

    Ok(())
}
