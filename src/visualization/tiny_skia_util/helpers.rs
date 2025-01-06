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

use crate::{
    track_schema::GenericTrackElement,
    visualization::types::{VizArc, VizElement, VizSector},
    FoxHashMap,
};

use crate::visualization::{tiny_skia_util::SkiaStyle, types::VizShape};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

#[inline]
pub fn skia_render_arc(path: &mut PathBuilder, arc: &VizArc, line_to: bool, reverse: bool) {
    match reverse {
        true => {
            if line_to {
                path.line_to(arc.end.x, arc.end.y);
            }
            else {
                path.move_to(arc.end.x, arc.end.y);
            }
            path.cubic_to(arc.cp1.x, arc.cp1.y, arc.cp2.x, arc.cp2.y, arc.start.x, arc.start.y);
            //path.line_to(arc.start.x, arc.start.y);
        }
        false => {
            if line_to {
                path.line_to(arc.start.x, arc.start.y);
            }
            else {
                path.move_to(arc.start.x, arc.start.y);
            }
            path.cubic_to(arc.cp1.x, arc.cp1.y, arc.cp2.x, arc.cp2.y, arc.end.x, arc.end.y);
            //path.line_to(arc.end.x, arc.end.y);
        }
    }
}

#[inline]
pub fn skia_render_sector(path: &mut PathBuilder, sector: &VizSector) {
    // Draw the inner curve from start to end
    skia_render_arc(path, &sector.inner, false, false);
    // Draw the outer curve from end to start
    skia_render_arc(path, &sector.outer, true, false);
    // Draw a line back to the inner curve start
    path.line_to(sector.inner.start.x, sector.inner.start.y);
}

pub fn skia_render_shape(path: &mut PathBuilder, shape: &VizShape) {
    match shape {
        VizShape::CubicArc(arc) => {
            skia_render_arc(path, arc, false, false);
        }
        VizShape::QuadraticArc(arc) => {
            //skia_render_quadratic_arc(data, arc, line_to);
        }
        VizShape::Sector(sector) => {
            skia_render_sector(path, sector);
        }
        VizShape::Circle(circle) => {
            //skia_render_circle(data, circle, line_to);
        }
        VizShape::Line(line) => {
            //skia_render_line(data, line, line_to);
        }
    }
}

pub fn skia_render_element(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    element: &VizElement,
    transform: &Transform,
    palette: &FoxHashMap<GenericTrackElement, SkiaStyle>,
) {
    let mut path = PathBuilder::new();

    //log::debug!("Rendering sector: {:#?}", &element.sector);
    //log::debug!("Rendering element: {:#?}", &element);
    skia_render_shape(&mut path, &element.shape);
    path.close();
    let default_style = SkiaStyle::default();
    let style = palette.get(&element.info.element_type).unwrap_or(&default_style);

    paint.set_color(Color::from(style.fill));

    if let Some(path) = path.finish() {
        if !path.is_empty() {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform.clone(), None);
        }
    }
}
