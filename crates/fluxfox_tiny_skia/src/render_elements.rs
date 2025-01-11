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

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::styles::SkiaStyle;
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::{VizArc, VizDataSlice, VizElement, VizElementDisplayList, VizQuadraticArc, VizSector},
        types::shapes::{VizElementFlags, VizShape},
    },
    FoxHashMap,
};

#[inline]
pub fn skia_render_arc(path: &mut PathBuilder, arc: &VizArc, line_to: bool) {
    if line_to {
        path.line_to(arc.start.x, arc.start.y);
    }
    else {
        path.move_to(arc.start.x, arc.start.y);
    }
    path.cubic_to(arc.cp1.x, arc.cp1.y, arc.cp2.x, arc.cp2.y, arc.end.x, arc.end.y);
}

#[inline]
pub fn skia_render_quadratic_arc(path: &mut PathBuilder, arc: &VizQuadraticArc, line_to: bool) {
    if line_to {
        path.line_to(arc.start.x, arc.start.y);
    }
    else {
        path.move_to(arc.start.x, arc.start.y);
    }
    path.quad_to(arc.cp.x, arc.cp.y, arc.end.x, arc.end.y);
}

#[inline]
pub fn skia_render_sector(path: &mut PathBuilder, sector: &VizSector) {
    // Draw the inner curve from start to end
    skia_render_arc(path, &sector.inner, false);
    // Draw the outer curve from end to start
    skia_render_arc(path, &sector.outer, true);
    // Draw a line back to the inner curve start
    path.line_to(sector.inner.start.x, sector.inner.start.y);
}

pub fn skia_render_shape(path: &mut PathBuilder, shape: &VizShape) {
    match shape {
        VizShape::CubicArc(arc, _thickness) => {
            skia_render_arc(path, arc, false);
        }
        VizShape::QuadraticArc(arc, _thickness) => {
            skia_render_quadratic_arc(path, arc, false);
        }
        VizShape::Sector(sector) => {
            skia_render_sector(path, sector);
        }
        VizShape::Circle(_circle, _thickness) => {
            //skia_render_circle(data, circle, line_to);
        }
        VizShape::Line(_line, _thickness) => {
            //skia_render_line(data, line, line_to);
        }
    }
}

pub fn skia_render_display_list(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    transform: &Transform,
    display_list: &VizElementDisplayList,
    track_style: &SkiaStyle,
    palette: &FoxHashMap<GenericTrackElement, SkiaStyle>,
) {
    for element in display_list.iter() {
        skia_render_element(pixmap, paint, transform, track_style, palette, element);
    }
}

pub fn skia_render_element(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    transform: &Transform,
    track_style: &SkiaStyle,
    palette: &FoxHashMap<GenericTrackElement, SkiaStyle>,
    element: &VizElement,
) {
    let mut path = PathBuilder::new();

    //log::debug!("Rendering sector: {:#?}", &element.sector);
    //log::debug!("Rendering element: {:#?}", &element);
    skia_render_shape(&mut path, &element.shape);
    path.close();
    let default_style = SkiaStyle::default();

    let style = if element.flags.contains(VizElementFlags::TRACK) {
        track_style
    }
    else if let Some(style) = palette.get(&element.info.element_type) {
        style
    }
    else {
        &default_style
    };

    paint.set_color(Color::from(style.fill));

    if let Some(path) = path.finish() {
        if !path.is_empty() {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform.clone(), None);
        }
    }
}

/// Render a single data slice as an SVG path. Unlike a sector element, a data slice is a single
/// arc with a stroke rendered at the track width.
pub fn skia_render_data_slice(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    stroke: &mut Stroke,
    transform: &Transform,
    slice: &VizDataSlice,
) {
    let mut path = PathBuilder::new();
    skia_render_quadratic_arc(&mut path, &slice.arc, false);

    let v = ((slice.density * 1.5).clamp(0.0, 1.0) * 255.0) as u8;
    paint.set_color(Color::from_rgba8(v, v, v, 255));

    if let Some(path) = path.finish() {
        if !path.is_empty() {
            pixmap.stroke_path(&path, &paint, stroke, transform.clone(), None);
        }
    }
}
