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
use crate::style::Style;
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::{VizArc, VizColor, VizElement, VizSector},
        types::VizElementFlags,
    },
    FoxHashMap,
};
use svg::node::{
    element::{path::Data, Path},
    Value,
};

fn viz_color_to_value(color: VizColor) -> Value {
    if color.a < 255 {
        // Convert to rgba() string if alpha is present
        Value::from(format!(
            "rgba({}, {}, {}, {:.3})",
            color.r,
            color.g,
            color.b,
            color.a as f32 / 255.0 // Alpha normalized to [0.0, 1.0]
        ))
    }
    else {
        // Convert to hex string if fully opaque
        Value::from(format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b))
    }
}

fn svg_render_arc(data: Data, arc: &VizArc, line_to: bool) -> Data {
    let data = if line_to {
        data.line_to((arc.start.x, arc.start.y)) // Draw a line to start
    }
    else {
        data.move_to((arc.start.x, arc.start.y)) // Move without drawing
    };
    data.cubic_curve_to(((arc.cp1.x, arc.cp1.y), (arc.cp2.x, arc.cp2.y), (arc.end.x, arc.end.y)))
}

fn svg_render_sector(data: Data, sector: &VizSector) -> Data {
    let mut data = svg_render_arc(data, &sector.inner, false);
    data = svg_render_arc(data, &sector.outer, true);
    data.line_to((sector.inner.start.x, sector.inner.start.y))
}

pub fn svg_render_element(
    element: &VizElement,
    track_style: &Style,
    element_styles: &FoxHashMap<GenericTrackElement, Style>,
) -> Path {
    let mut data = Data::new();
    data = svg_render_sector(data, &element.sector);

    let default_style = Style::default();

    let style = match element.info.element_type {
        GenericTrackElement::NullElement => {
            // Check if this is a track-level element
            if element.flags.contains(VizElementFlags::TRACK) {
                log::warn!(
                    "emitting track element, start: {} end: {} style: {:?}",
                    element.sector.start,
                    element.sector.end,
                    track_style
                );
                track_style
            }
            else {
                &default_style
            }
        }
        _ => element_styles.get(&element.info.element_type).unwrap_or(&default_style),
    };

    Path::new()
        .set("d", data)
        .set("fill", viz_color_to_value(style.fill))
        .set("stroke", viz_color_to_value(style.stroke))
        .set("stroke-width", style.stroke_width)
}
