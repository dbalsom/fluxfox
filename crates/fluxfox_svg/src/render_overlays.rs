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

//! Module to render an SVG file as a Document to be drawn as an overlay over
//! a side visualization. These overlays are typically wireframe depictions of
//! physical floppy media.

use crate::{render_elements::viz_color_to_value, styles::ElementStyle};
use fluxfox::visualization::prelude::VizRect;
use svg::{
    node::element::{Circle, Group, Path},
    parser::Event,
    Document,
};

pub fn svg_to_group(svg_data: &str, viewbox: &VizRect<f32>, style: &ElementStyle) -> Result<Group, String> {
    log::debug!("svg_to_group: Got svg string: {:?}", svg_data);
    let parser = svg::read(svg_data).map_err(|e| format!("Failed to parse overlay SVG: {}", e))?;

    let mut group = Group::new();

    //Scale overlay viewbox (0,0)-(100,100) to the provided viewbox
    let scale_x = viewbox.width() / 100.0;
    let scale_y = viewbox.height() / 100.0;
    let transform = format!("scale({:.3},{:.3})", scale_x, scale_y);

    let mut group = Group::new().set("transform", transform);

    //let mut group = Group::new();
    for event in parser {
        match event {
            // Handle <path> elements
            Event::Tag("path", _, attributes) => {
                if let Some(d) = attributes.get("d") {
                    let path = Path::new()
                        .set("d", d.clone())
                        .set("fill", viz_color_to_value(style.fill))
                        .set("stroke", viz_color_to_value(style.stroke))
                        .set("stroke-width", style.stroke_width);
                    group = group.add(path);
                }
            }
            // Handle <circle> elements
            Event::Tag("circle", _, attributes) => {
                let cx = attributes
                    .get("cx")
                    .map(|v| v.parse::<f32>().unwrap_or(0.0))
                    .unwrap_or(0.0);
                let cy = attributes
                    .get("cy")
                    .map(|v| v.parse::<f32>().unwrap_or(0.0))
                    .unwrap_or(0.0);
                let r = attributes
                    .get("r")
                    .map(|v| v.parse::<f32>().unwrap_or(0.0))
                    .unwrap_or(0.0);

                let circle = Circle::new()
                    .set("cx", cx)
                    .set("cy", cy)
                    .set("r", r)
                    .set("fill", viz_color_to_value(style.fill))
                    .set("stroke", viz_color_to_value(style.stroke))
                    .set("stroke-width", style.stroke_width);
                group = group.add(circle);
            }
            _ => {}
        }
    }

    Ok(group)
}
