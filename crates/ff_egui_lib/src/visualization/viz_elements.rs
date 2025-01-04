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
use crate::visualization::VizPalette;
use egui::{
    emath::RectTransform,
    epaint::{ColorMode, CubicBezierShape, PathShape, PathStroke},
    Color32,
    Painter,
    Pos2,
    Shape,
    Stroke,
};
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::types::{VizArc, VizElement, VizPoint2d, VizSector},
};

/// Converts a fluxfox `VizPoint2d<f32>` to an egui `Pos2`
#[inline]
fn to_pos2(pt: &VizPoint2d<f32>) -> Pos2 {
    Pos2::new(pt.x, pt.y)
}

#[inline]
fn to_pos2_transformed(pt: &VizPoint2d<f32>, transform: &RectTransform) -> Pos2 {
    let new_pt = Pos2::new(pt.x, pt.y);
    transform.transform_pos(new_pt)
}

fn approximate_wedge_quad(
    center: Pos2,
    outer_radius: f32,
    inner_radius: f32,
    start_angle: f32,
    end_angle: f32,
) -> [Pos2; 4] {
    // p0: outer arc start
    let p0 = Pos2::new(
        center.x + outer_radius * start_angle.cos(),
        center.y + outer_radius * start_angle.sin(),
    );

    // p1: outer arc end
    let p1 = Pos2::new(
        center.x + outer_radius * end_angle.cos(),
        center.y + outer_radius * end_angle.sin(),
    );

    // p2: inner boundary end (same angle as p1)
    let p2 = Pos2::new(
        center.x + inner_radius * end_angle.cos(),
        center.y + inner_radius * end_angle.sin(),
    );

    // p3: inner boundary start (same angle as p0)
    let p3 = Pos2::new(
        center.x + inner_radius * start_angle.cos(),
        center.y + inner_radius * start_angle.sin(),
    );

    [p0, p1, p2, p3]
}

/// Draws a `VizArc` as a cubic Bézier curve.
pub fn make_bezier(
    transform: &RectTransform,
    arc: &VizArc,
    fill_color: Color32,
    stroke: &PathStroke,
) -> CubicBezierShape {
    CubicBezierShape {
        points: [
            to_pos2_transformed(&arc.start, transform),
            to_pos2_transformed(&arc.cp1, transform),
            to_pos2_transformed(&arc.cp2, transform),
            to_pos2_transformed(&arc.end, transform),
        ],
        closed: true,
        fill:   fill_color,
        stroke: stroke.clone(),
    }
}

pub fn draw_arc(transform: &RectTransform, arc: &VizArc, fill_color: Color32, stroke: &PathStroke) -> Vec<Pos2> {
    let bezier = CubicBezierShape {
        points: [
            to_pos2_transformed(&arc.start, transform),
            to_pos2_transformed(&arc.cp1, transform),
            to_pos2_transformed(&arc.cp2, transform),
            to_pos2_transformed(&arc.end, transform),
        ],
        closed: true,
        fill:   fill_color,
        stroke: stroke.clone(),
    };

    bezier
        .to_path_shapes(Some(1.0), None)
        .into_iter()
        .flat_map(|shape| shape.points)
        .collect()
}

/// Renders a sector by drawing its outer and inner arcs and connecting lines.
pub fn paint_sector(
    painter: &Painter,
    transform: &RectTransform,
    sector: &VizSector,
    fill_color: Color32,
    stroke: &PathStroke,
) {
    // Draw outer arc.
    //let bezier_shape = Shape::CubicBezier(make_bezier(transform, &sector.outer, fill_color, stroke));
    //painter.add(bezier_shape);

    // Draw inside wedge

    //log::warn!("paint_sector: outer start: {:?}", sector.outer.start);
    // Draw inner arc from start to end
    let mut points = Vec::new();
    // Draw inner arc
    points.extend(draw_arc(transform, &sector.inner, fill_color, stroke));
    points.push(to_pos2_transformed(&sector.inner.end, transform));
    points.push(to_pos2_transformed(&sector.outer.start, transform));
    // Draw outer arc
    points.extend(draw_arc(transform, &sector.outer, fill_color, stroke));

    points.push(to_pos2_transformed(&sector.outer.end, transform));
    points.push(to_pos2_transformed(&sector.inner.start, transform));

    let shape = PathShape {
        points,
        closed: true,
        fill: Color32::from_gray(128),
        // stroke: PathStroke {
        //     width: 0.5,
        //     color: ColorMode::Solid(Color32::WHITE),
        //     kind:  StrokeKind::Inside,
        // },
        stroke: PathStroke::new(0.5, Color32::BLACK),
    };

    painter.add(Shape::Path(shape));

    // Carve out the inner arc
    //painter.add(make_bezier(transform, &sector.inner, Color32::BLACK, stroke));
}

pub fn paint_elements(painter: &Painter, transform: &RectTransform, palette: &VizPalette, elements: &[VizElement]) {
    let stroke = PathStroke::NONE;

    for element in elements {
        match element.info.element_type {
            GenericTrackElement::SectorData { .. } => {}
            _ => continue,
        }
        if let Some(color) = palette.get(&element.info.element_type) {
            let fill_color = *color;
            paint_sector(painter, transform, &element.sector, fill_color, &stroke);
        }
        else {
            log::warn!("No color found for element type: {:?}", element.info.element_type);
        }
    }
}
