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
use crate::visualization::VizPalette;
use egui::{
    emath::RectTransform,
    epaint::{CubicBezierShape, PathShape, PathStroke, QuadraticBezierShape},
    Color32,
    Painter,
    Pos2,
    Shape,
    Stroke,
};
use fluxfox::visualization::{prelude::*, VizRotate};

#[inline]
fn to_pos2_transformed(pt: &VizPoint2d<f32>, transform: &RectTransform) -> Pos2 {
    let new_pt = Pos2::new(pt.x, pt.y);
    transform.transform_pos(new_pt)
}

// Creates a [CubicBezierShape] from a [VizArc].
pub fn make_arc(
    transform: &RectTransform,
    rotation: &VizRotation,
    arc: &VizArc,
    stroke: &PathStroke,
) -> CubicBezierShape {
    let arc = arc.rotate(rotation);
    CubicBezierShape {
        points: [
            to_pos2_transformed(&arc.start, transform),
            to_pos2_transformed(&arc.cp1, transform),
            to_pos2_transformed(&arc.cp2, transform),
            to_pos2_transformed(&arc.end, transform),
        ],
        closed: false,
        fill:   Color32::TRANSPARENT,
        stroke: stroke.clone(),
    }
}

// Creates a [QuadraticBezierShape] from a [VizQuadraticArc].
pub fn make_quadratic_arc(
    transform: &RectTransform,
    rotation: &VizRotation,
    arc: &VizQuadraticArc,
    stroke: &PathStroke,
) -> QuadraticBezierShape {
    let arc = arc.rotate(rotation);
    QuadraticBezierShape {
        points: [
            to_pos2_transformed(&arc.start, transform),
            to_pos2_transformed(&arc.cp, transform),
            to_pos2_transformed(&arc.end, transform),
        ],
        closed: false,
        fill:   Color32::TRANSPARENT,
        stroke: stroke.clone(),
    }
}

pub fn arc_to_points(transform: &RectTransform, rotation: &VizRotation, arc: &VizArc) -> Vec<Pos2> {
    let arc = arc.rotate(rotation);
    let bezier = CubicBezierShape {
        points: [
            to_pos2_transformed(&arc.start, transform),
            to_pos2_transformed(&arc.cp1, transform),
            to_pos2_transformed(&arc.cp2, transform),
            to_pos2_transformed(&arc.end, transform),
        ],
        closed: false,
        fill:   Color32::TRANSPARENT,
        stroke: PathStroke::NONE,
    };

    bezier.flatten(Some(0.1))
}

/// Paint a shape - dispatch to the appropriate function based on the shape type.
pub fn paint_shape(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    shape: &VizShape,
    fill_color: Color32,
    stroke: &PathStroke,
) {
    match shape {
        VizShape::Sector(sector) => paint_sector(painter, transform, rotation, sector, fill_color, stroke),
        VizShape::CubicArc(arc, thickness) => {
            // For an arc, stroke becomes the "fill" and the fill color is the stroke color.
            let stroke = PathStroke::from(Stroke::new(*thickness * transform.scale().x, fill_color));
            paint_arc(painter, transform, rotation, arc, &stroke)
        }
        VizShape::QuadraticArc(arc, thickness) => {
            // For an arc, stroke becomes the "fill" and the fill color is the stroke color.
            let stroke = PathStroke::from(Stroke::new(*thickness * transform.scale().x, fill_color));
            paint_quadratic_arc(painter, transform, rotation, arc, &stroke)
        }
        VizShape::Circle(circle, thickness) => {
            // For a circle, stroke becomes the "fill" and the fill color is the stroke color.
            let stroke = Stroke::new(*thickness * transform.scale().x, fill_color);
            paint_circle(painter, transform, circle, &stroke);
        }
        _ => {}
    }
}

/// Renders a sector by drawing its outer and inner arcs and connecting lines.
pub fn paint_sector(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    sector: &VizSector,
    _fill_color: Color32,
    stroke: &PathStroke,
) {
    // Draw outer arc.
    //let bezier_shape = Shape::CubicBezier(make_bezier(transform, &sector.outer, fill_color, stroke));
    //painter.add(bezier_shape);

    // Draw inside wedge

    //log::warn!("paint_sector: outer start: {:?}", sector.outer.start);
    // Draw inner arc from start to end
    let mut points = Vec::new();
    // We need clockwise winding - start with the outer arc, which should be drawn from start to end
    // point in clockwise winding.
    points.extend(arc_to_points(transform, rotation, &sector.outer));
    // Then draw the inner arc
    points.extend(arc_to_points(transform, rotation, &sector.inner));

    let shape = PathShape {
        points,
        closed: true,
        fill: Color32::TRANSPARENT, // egui cannot fill a concave path.
        stroke: stroke.clone(),
    };

    painter.add(Shape::Path(shape));

    // Carve out the inner arc
    //painter.add(make_bezier(transform, &sector.inner, Color32::BLACK, stroke));
}

/// Renders an arc by drawing a cubic Bézier curve.
pub fn paint_arc(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    arc: &VizArc,
    stroke: &PathStroke,
) {
    let shape = make_arc(transform, rotation, arc, stroke);
    painter.add(Shape::CubicBezier(shape));
}

/// Renders an arc by drawing a cubic Bézier curve.
pub fn paint_quadratic_arc(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    arc: &VizQuadraticArc,
    stroke: &PathStroke,
) {
    //let shape = make_quadratic_arc(transform, rotation, arc, stroke);
    //painter.add(Shape::QuadraticBezier(shape));

    painter.line(
        vec![
            to_pos2_transformed(&arc.start.rotate(rotation), transform),
            to_pos2_transformed(&arc.end.rotate(rotation), transform),
        ],
        stroke.clone(),
    );
}

/// Renders a circle. Note no rotation parameter as rotating a circle does nothing.
pub fn paint_circle(painter: &Painter, transform: &RectTransform, circle: &VizCircle, stroke: &Stroke) {
    painter.circle(
        to_pos2_transformed(&circle.center, transform),
        circle.radius,
        Color32::TRANSPARENT,
        *stroke,
    );
}

pub fn paint_elements(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    palette: &VizPalette,
    elements: &[VizElement],
    multiply: bool,
) {
    let stroke = PathStroke::NONE;

    match multiply {
        true => {
            // Check track flag and draw as gray
            for element in elements {
                let fill_color = if element.flags.contains(VizElementFlags::TRACK) {
                    Color32::from_gray(128)
                }
                else if let Some(color) = palette.get(&element.info.element_type) {
                    *color
                }
                else {
                    // Use a warning color to indicate missing palette entry.
                    Color32::RED
                };
                paint_shape(painter, transform, rotation, &element.shape, fill_color, &stroke);
            }
        }
        false => {
            // Paint normally.
            for element in elements {
                let fill_color = if element.flags.contains(VizElementFlags::HIGHLIGHT) {
                    //log::warn!("Highlighting element: {:?}", element.info.element_type);
                    Color32::from_white_alpha(80)
                }
                else if let Some(color) = palette.get(&element.info.element_type) {
                    *color
                }
                else {
                    // Use a warning color to indicate missing palette entry.
                    Color32::RED
                };
                paint_shape(painter, transform, rotation, &element.shape, fill_color, &stroke);
            }
        }
    }
}

pub fn paint_data(
    painter: &Painter,
    transform: &RectTransform,
    rotation: &VizRotation,
    slices: &[VizDataSlice],
    width: f32,
    multiply: bool,
) {
    let stroke = PathStroke::NONE;
    match multiply {
        true => {
            // Multiply mode: render as black, density as alpha.
            for slice in slices {
                let fill_color =
                    Color32::from_black_alpha((((1.0 - slice.density * 2.0).clamp(0.0, 1.0)) * 255.0) as u8);
                paint_shape(
                    painter,
                    transform,
                    rotation,
                    &VizShape::QuadraticArc(slice.arc, width),
                    fill_color,
                    &stroke,
                );
            }
        }
        false => {
            // Normal mode; full alpha, grayscale rendering.
            for slice in slices {
                let fill_color = Color32::from_gray(((slice.density * 1.5).clamp(0.0, 1.0) * 255.0) as u8);
                paint_shape(
                    painter,
                    transform,
                    rotation,
                    &VizShape::QuadraticArc(slice.arc, width),
                    fill_color,
                    &stroke,
                );
            }
        }
    }
}
