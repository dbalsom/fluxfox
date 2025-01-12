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

use crate::styles::ElementStyle;
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::{VizArc, VizColor, VizDataSlice, VizElement, VizQuadraticArc, VizSector},
        types::shapes::{VizElementFlags, VizShape},
    },
    FoxHashMap,
};

use svg::node::{
    element::{path::Data, Circle, Path},
    Value,
};

#[derive(Clone, Debug)]
pub enum RenderNode {
    Path(Path),
    Circle(Circle),
}

pub(crate) fn viz_color_to_value(color: VizColor) -> Value {
    if color.a == 0 {
        // Fully transparent, return 'none' to prevent rendering
        Value::from("none")
    }
    else if color.a < 255 {
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

fn svg_render_quadratic_arc(data: Data, arc: &VizQuadraticArc, line_to: bool) -> Data {
    let data = if line_to {
        data.line_to((arc.start.x, arc.start.y)) // Draw a line to start
    }
    else {
        data.move_to((arc.start.x, arc.start.y)) // Move without drawing
    };
    data.quadratic_curve_to(((arc.cp.x, arc.cp.y), (arc.end.x, arc.end.y)))
}

fn svg_render_sector(data: Data, sector: &VizSector) -> Data {
    let mut data = svg_render_arc(data, &sector.inner, false);
    data = svg_render_arc(data, &sector.outer, true);
    data.line_to((sector.inner.start.x, sector.inner.start.y))
}

/// Render shapes as paths. Notably we do not render circles here as they are not paths!
fn svg_render_shape(data: Data, shape: &VizShape) -> Data {
    match shape {
        VizShape::CubicArc(arc, _h) => svg_render_arc(data, arc, false),
        VizShape::QuadraticArc(arc, _h) => svg_render_quadratic_arc(data, arc, false),
        VizShape::Sector(sector) => svg_render_sector(data, sector),
        _ => data,
    }
}

pub fn svg_render_element(
    element: &VizElement,
    track_style: &ElementStyle,
    element_styles: &FoxHashMap<GenericTrackElement, ElementStyle>,
) -> RenderNode {
    let mut data = Data::new();
    let default_style = ElementStyle::default();
    let style = match element.info.element_type {
        GenericTrackElement::NullElement => {
            // Check if this is a track-level element
            if element.flags.contains(VizElementFlags::TRACK) {
                track_style
            }
            else {
                &default_style
            }
        }
        _ => element_styles.get(&element.info.element_type).unwrap_or(&default_style),
    };

    match element.shape {
        VizShape::CubicArc(_, _) | VizShape::QuadraticArc(_, _) | VizShape::Sector(_) => {
            data = svg_render_shape(data, &element.shape);
        }
        VizShape::Circle(circle, _) => {
            // Circles are not paths, so we do not add to data.
            let new_circle = Circle::new()
                .set("cx", circle.center.x)
                .set("cy", circle.center.y)
                .set("r", circle.radius)
                .set("fill", viz_color_to_value(style.fill))
                .set("stroke", viz_color_to_value(style.stroke))
                .set("stroke-width", style.stroke_width);

            return RenderNode::Circle(new_circle);
        }
        _ => {}
    };

    RenderNode::Path(
        Path::new()
            .set("d", data)
            .set("fill", viz_color_to_value(style.fill))
            .set("stroke", viz_color_to_value(style.stroke))
            .set("stroke-width", style.stroke_width),
    )
}

/// Render a single data slice as an SVG path. Unlike a sector element, a data slice is a single
/// arc with a stroke rendered at the track width.
pub fn svg_render_data_slice(slice: &VizDataSlice, stroke: f32) -> Path {
    let mut data = Data::new();
    data = svg_render_quadratic_arc(data, &slice.arc, false);

    // Boost contrast by increasing density by 50%
    let adjusted_density = (slice.density * 1.5).clamp(0.0, 1.0);
    let value_u8 = (adjusted_density * 255.0) as u8;
    let fill_color = VizColor::from_value(value_u8, 255);
    Path::new()
        .set("d", data)
        .set("stroke", viz_color_to_value(fill_color))
        .set("stroke-width", stroke)
}
