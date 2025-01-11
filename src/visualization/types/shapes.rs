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

//! Methods to construct cubic and quadratic Bezier approximations of circular arcs.
//! These methods are used to represent track elements in the visualization layer.
//! The cubic approximation is used for longer arcs, while the quadratic approximation is used for
//! shorter arcs.
//!
//! Derived constants are used to generate quadrant (90 degree) arcs, taken from:
//! https://spencermortensen.com/articles/bezier-circle/
//!
//! I'm not the greatest at math so if you spot any optimizations here, please let me know!

use std::{
    fmt::{Display, Formatter},
    ops::{Add, Div, Range},
};

use crate::{
    track_schema::GenericTrackElement,
    types::DiskCh,
    visualization::{types::color::VizColor, RenderWinding, VizRotate},
};

use bitflags::bitflags;
use core::fmt;
use num_traits::Num;
use std::ops::Mul;

#[cfg(feature = "tiny_skia")]
impl From<VizColor> for tiny_skia::Color {
    #[inline]
    fn from(color: VizColor) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(color.r, color.g, color.b, color.a)
    }
}

bitflags! {
    #[derive (Clone, Debug, Default)]
    pub struct VizElementFlags: u32 {
        // No flags set
        const NONE = 0b0000_0000;
        // This element represents a section of an entire track and can be used to draw the track background
        const TRACK = 0b0000_0001;
        /// This element represents a section of an empty track
        const EMPTY_TRACK = 0b0000_0010;
        // This element crosses the index
        const OVERLAP = 0b0000_0100;
        // This element crosses the index, and is sufficiently long that it should be faded out
        const OVERLAP_LONG = 0b0000_1000;
        // This element represents a highlighted element
        const HIGHLIGHT = 0b0001_0000;
        // This element represents a selected element
        const SELECTED = 0b0010_0000;
    }
}

/// A [VizDimensions] represents the width and height of a rectangular region, such as a pixmap.
pub type VizDimensions = VizPoint2d<u32>;

/// A VizShape represents a shape that can be rendered in a visualization. This is a simple enum
/// that can represent a cubic Bezier arc, a quadratic Bezier arc, a sector, a circle, or a line.
/// The second parameter, if present, is the thickness of the shape. This should be used for the
/// stroke parameter during rendering.
#[derive(Copy, Clone, Debug)]
pub enum VizShape {
    CubicArc(VizArc, f32),
    QuadraticArc(VizQuadraticArc, f32),
    Sector(VizSector),
    Circle(VizCircle, f32),
    Line(VizLine<f32>, f32),
}

impl VizRotate for VizShape {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizShape {
        match self {
            VizShape::CubicArc(arc, t) => VizShape::from((arc.rotate(rot), t)),
            VizShape::QuadraticArc(quadratic, t) => VizShape::from((quadratic.rotate(rot), t)),
            VizShape::Sector(sector) => sector.rotate(rot).into(),
            VizShape::Circle(_, _) => self,
            VizShape::Line(_, _) => self,
        }
    }
}

/// A [VizLine] represents a line segment in 2D space.
#[derive(Copy, Clone, Debug)]
pub struct VizLine<T: Num + Copy + Default + Into<f64>> {
    pub start: VizPoint2d<T>,
    pub end:   VizPoint2d<T>,
}

impl<T: Num + Copy + Default> VizLine<T>
where
    f64: From<T>,
{
    pub fn new(start: VizPoint2d<T>, end: VizPoint2d<T>) -> VizLine<T> {
        VizLine { start, end }
    }

    pub fn length(&self) -> f64 {
        let dx = f64::from(self.end.x - self.start.x);
        let dy = f64::from(self.end.y - self.start.y);
        (dx * dx + dy * dy).sqrt()
    }
}

impl<T: Num + Copy + Default> From<(T, T, T, T)> for VizLine<T>
where
    f64: From<T>,
{
    fn from(tuple: (T, T, T, T)) -> Self {
        VizLine {
            start: VizPoint2d::from((tuple.0, tuple.1)),
            end:   VizPoint2d::from((tuple.2, tuple.3)),
        }
    }
}

/// A [VizRect] represents a rectangle in 2D space. It is generic across numeric types, using
/// `num_traits`.
///
/// The rectangle is defined by two points, the top-left and bottom-right corners.
/// Methods are provided for calculating the width and height of the rectangle.
#[derive(Clone, Default, Debug)]
pub struct VizRect<T: Num + Copy + PartialOrd + Default> {
    pub top_left: VizPoint2d<T>,
    pub bottom_right: VizPoint2d<T>,
}

impl<T: Num + Copy + PartialOrd + Default> VizRect<T> {
    #[inline]
    fn min(a: T, b: T) -> T {
        if a < b {
            a
        }
        else {
            b
        }
    }

    #[inline]
    fn max(a: T, b: T) -> T {
        if a > b {
            a
        }
        else {
            b
        }
    }

    pub fn new(top_left: VizPoint2d<T>, bottom_right: VizPoint2d<T>) -> VizRect<T> {
        VizRect { top_left, bottom_right }
    }

    pub fn from_tuple(top_left: (T, T), bottom_right: (T, T)) -> VizRect<T> {
        VizRect {
            top_left: VizPoint2d::from(top_left),
            bottom_right: VizPoint2d::from(bottom_right),
        }
    }

    pub fn width(&self) -> T {
        self.bottom_right.x - self.top_left.x
    }

    pub fn height(&self) -> T {
        self.bottom_right.y - self.top_left.y
    }

    /// Returns the intersection of two [VizRect] as a [VizRect], or returns `None` if they do not
    /// intersect.
    pub fn intersection(&self, other: &VizRect<T>) -> Option<VizRect<T>> {
        let top_left = VizPoint2d::new(
            Self::max(self.top_left.x, other.top_left.x),
            Self::max(self.top_left.y, other.top_left.y),
        );

        let bottom_right = VizPoint2d::new(
            Self::min(self.bottom_right.x, other.bottom_right.x),
            Self::min(self.bottom_right.y, other.bottom_right.y),
        );

        if top_left.x <= bottom_right.x && top_left.y <= bottom_right.y {
            Some(VizRect::new(top_left, bottom_right))
        }
        else {
            None
        }
    }

    /// Returns bounding box that includes both [VizRect]s
    pub fn bounding_box(&self, other: &VizRect<T>) -> VizRect<T> {
        let top_left = VizPoint2d::new(
            Self::min(self.top_left.x, other.top_left.x),
            Self::min(self.top_left.y, other.top_left.y),
        );

        let bottom_right = VizPoint2d::new(
            Self::max(self.bottom_right.x, other.bottom_right.x),
            Self::max(self.bottom_right.y, other.bottom_right.y),
        );

        VizRect::new(top_left, bottom_right)
    }

    /// Return whether the specified point is within Self
    pub fn contains_point(&self, point: &VizPoint2d<T>) -> bool {
        point.x >= self.top_left.x
            && point.x <= self.bottom_right.x
            && point.y >= self.top_left.y
            && point.y <= self.bottom_right.y
    }

    /// Return whether the specified rectangle is within Self
    pub fn contains_rect(&self, other: &VizRect<T>) -> bool {
        self.contains_point(&other.top_left) && self.contains_point(&other.bottom_right)
    }

    /// Grow the rectangle by a factor, preserving the top-left corner position.
    /// If the factor is negative, the rectangle will flip across the top-left corner.
    pub fn grow_pinned(&self, factor: T) -> VizRect<T> {
        let new_width = self.width() * factor;
        let new_height = self.height() * factor;

        let new_rect = VizRect {
            top_left: VizPoint2d::new(self.top_left.x, self.top_left.y),
            bottom_right: VizPoint2d::new(self.top_left.x + new_width, self.bottom_right.y + new_height),
        };

        new_rect.normalize()
    }

    /// Ensure that the top left coordinate is less than the bottom right coordinate.
    pub fn normalize(&self) -> VizRect<T> {
        let top_left = VizPoint2d::new(
            Self::min(self.top_left.x, self.bottom_right.x),
            Self::min(self.top_left.y, self.bottom_right.y),
        );

        let bottom_right = VizPoint2d::new(
            Self::max(self.top_left.x, self.bottom_right.x),
            Self::max(self.top_left.y, self.bottom_right.y),
        );
        VizRect::new(top_left, bottom_right)
    }

    pub fn to_tuple(&self) -> (T, T, T, T) {
        (
            self.top_left.x,
            self.top_left.y,
            self.bottom_right.x,
            self.bottom_right.y,
        )
    }
}

impl<T> VizRect<T>
where
    T: Num + Copy + Add<T, Output = T> + Div<T, Output = T> + From<f32> + PartialOrd + Default,
{
    pub fn center(&self) -> VizPoint2d<T> {
        VizPoint2d::new(
            self.top_left.x + self.width() / T::from(2.0),
            self.top_left.y + self.height() / T::from(2.0),
        )
    }
}

impl From<(f32, f32, f32, f32)> for VizRect<f32> {
    fn from(tuple: (f32, f32, f32, f32)) -> Self {
        VizRect {
            top_left: VizPoint2d::new(tuple.0, tuple.1),
            bottom_right: VizPoint2d::new(tuple.2, tuple.3),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VizRotation {
    pub angle:  f32,
    pub sin:    f32,
    pub cos:    f32,
    pub center: VizPoint2d<f32>,
}

impl VizRotation {
    pub fn new(angle: f32, center: VizPoint2d<f32>) -> VizRotation {
        let (sin, cos) = angle.sin_cos();
        VizRotation {
            angle,
            sin,
            cos,
            center,
        }
    }
}

/// A [VizPoint2d] represents a point in 2D space in the range `[(0,0), (1,1)]`.
/// It is generic across numeric types, using `num_traits`.
#[derive(Copy, Clone, Debug)]
pub struct VizPoint2d<T> {
    pub x: T,
    pub y: T,
}

impl<T: Num + Copy + Default + Display> Display for VizPoint2d<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl<T: Num + Copy + Default> Default for VizPoint2d<T> {
    fn default() -> Self {
        VizPoint2d {
            x: T::default(),
            y: T::default(),
        }
    }
}

impl<T: Num + Copy + Default> From<(T, T)> for VizPoint2d<T> {
    fn from(tuple: (T, T)) -> Self {
        VizPoint2d { x: tuple.0, y: tuple.1 }
    }
}

impl<T: Num + Copy + Default> VizPoint2d<T> {
    pub fn new(x: T, y: T) -> Self {
        VizPoint2d { x, y }
    }

    pub fn to_tuple(&self) -> (T, T) {
        (self.x, self.y)
    }

    pub fn scale(&self, factor: T) -> VizPoint2d<T> {
        VizPoint2d {
            x: self.x * factor,
            y: self.y * factor,
        }
    }
}

impl<T, Rhs> Mul<Rhs> for VizPoint2d<T>
where
    T: Num + Copy + Default + Mul<Rhs, Output = T>,
    Rhs: Num + Copy + Default + Mul<T>,
{
    type Output = VizPoint2d<<T as Mul<Rhs>>::Output>;
    fn mul(self, rhs: Rhs) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl VizRotate for VizPoint2d<f32> {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizPoint2d<f32> {
        let dx = self.x - rot.center.x;
        let dy = self.y - rot.center.y;

        VizPoint2d {
            x: rot.center.x + dx * rot.cos - dy * rot.sin,
            y: rot.center.y + dx * rot.sin + dy * rot.cos,
        }
    }
}

/// A [VizArc] represents a cubic Bezier curve in 2D space.
#[derive(Copy, Clone, Debug)]
pub struct VizArc {
    pub start: VizPoint2d<f32>, // Start point of arc
    pub end:   VizPoint2d<f32>, // End point of arc
    pub cp1:   VizPoint2d<f32>, // 1st control point
    pub cp2:   VizPoint2d<f32>, // 2nd control point
}

impl VizArc {
    /// Calculate cubic Bézier parameters from a center point, radius, and start and end angles.
    /// This assumes the curve represents a segment of a circle.
    pub fn from_angles(center: &VizPoint2d<f32>, radius: f32, start_angle: f32, end_angle: f32) -> VizArc {
        // Calculate start and end points with simple trigonometry
        let x1 = center.x + radius * start_angle.cos();
        let y1 = center.y + radius * start_angle.sin();
        let x4 = center.x + radius * end_angle.cos();
        let y4 = center.y + radius * end_angle.sin();

        // Compute relative vectors
        let ax = x1 - center.x;
        let ay = y1 - center.y;
        let bx = x4 - center.x;
        let by = y4 - center.y;

        // Circular cubic approximation using (4/3).
        // q1 = |A|^2 = ax² + ay²
        // q2 = q1 + (A · B) = q1 + ax*bx + ay*by
        let q1 = ax * ax + ay * ay;
        let q2 = q1 + ax * bx + ay * by;
        let k2 = (4.0 / 3.0) * ((2.0 * q1 * q2).sqrt() - q2) / (ax * by - ay * bx);

        // Reapply center offset
        let (x2, y2) = (center.x + ax - k2 * ay, center.y + ay + k2 * ax);
        let (x3, y3) = (center.x + bx + k2 * by, center.y + by - k2 * bx);

        VizArc {
            start: VizPoint2d { x: x1, y: y1 },
            end:   VizPoint2d { x: x4, y: y4 },
            cp1:   VizPoint2d { x: x2, y: y2 },
            cp2:   VizPoint2d { x: x3, y: y3 },
        }
    }
}

impl From<(VizArc, f32)> for VizShape {
    #[inline]
    fn from(tuple: (VizArc, f32)) -> VizShape {
        VizShape::CubicArc(tuple.0, tuple.1)
    }
}

impl VizRotate for VizArc {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizArc {
        VizArc {
            start: self.start.rotate(rot),
            end:   self.end.rotate(rot),
            cp1:   self.cp1.rotate(rot),
            cp2:   self.cp2.rotate(rot),
        }
    }
}

/// A [VizQuadraticArc] represents a quadratic Bézier curve in 2D space.
/// A lower order curve than a cubic Bézier, a Quadratic Bézier requires more curves to represent
/// the same shapes as a cubic Bezier, but is computationally simpler and requires one fewer control
/// point.
/// We can optimize the representation of short arcs with quadratic Bézier curves and use cubic
/// Bézier curves for longer arcs.
#[derive(Copy, Clone, Debug)]
pub struct VizQuadraticArc {
    pub start: VizPoint2d<f32>, // Start point of arc
    pub end:   VizPoint2d<f32>, // End point of arc
    pub cp:    VizPoint2d<f32>, // Control point
}

impl VizQuadraticArc {
    /// Calculate quadratic Bézier parameters from a center point, radius, and start and end angles.
    /// This assumes the curve represents a segment of a circle.
    pub fn from_angles(center: &VizPoint2d<f32>, radius: f32, start_angle: f32, end_angle: f32) -> VizQuadraticArc {
        // Calculate start and end points with simple trigonometry
        let x1 = center.x + radius * start_angle.cos();
        let y1 = center.y + radius * start_angle.sin();
        let x2 = center.x + radius * end_angle.cos();
        let y2 = center.y + radius * end_angle.sin();

        // Calculate the midpoint of the arc
        let mid_angle = (start_angle + end_angle) * 0.5;
        let mx = center.x + radius * mid_angle.cos();
        let my = center.y + radius * mid_angle.sin();

        // Calculate the control point to represent a circular arc
        let cx = 2.0 * mx - 0.5 * (x1 + x2);
        let cy = 2.0 * my - 0.5 * (y1 + y2);

        VizQuadraticArc {
            start: VizPoint2d { x: x1, y: y1 },
            end:   VizPoint2d { x: x2, y: y2 },
            cp:    VizPoint2d { x: cx, y: cy },
        }
    }
}

impl From<(VizQuadraticArc, f32)> for VizShape {
    #[inline]
    fn from(tuple: (VizQuadraticArc, f32)) -> VizShape {
        VizShape::QuadraticArc(tuple.0, tuple.1)
    }
}

impl VizRotate for VizQuadraticArc {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizQuadraticArc {
        VizQuadraticArc {
            start: self.start.rotate(rot),
            end:   self.end.rotate(rot),
            cp:    self.cp.rotate(rot),
        }
    }
}

/// A [VizCircle] represents a simple circle with center point and radius.
#[derive(Copy, Clone, Debug)]
pub struct VizCircle {
    pub center: VizPoint2d<f32>,
    pub radius: f32,
}

impl From<(VizCircle, f32)> for VizShape {
    #[inline]
    fn from(tuple: (VizCircle, f32)) -> VizShape {
        VizShape::Circle(tuple.0, tuple.1)
    }
}

/// A [VizSector] represents an arc with thickness, or an 'annular sector'. This may be literally
/// be a sector on a disk, but may represent other track elements or regions as well.
#[derive(Copy, Clone, Debug)]
pub struct VizSector {
    pub start: f32, // The angle at which the sector starts
    pub end:   f32, // The angle at which the sector ends
    pub outer: VizArc,
    pub inner: VizArc,
}

impl VizSector {
    /// Calculate a [VizSector] from a center point, start and end angles in radians, and an inner and
    /// outer radius.
    #[inline]
    pub fn from_angles(
        center: &VizPoint2d<f32>,
        render_winding: RenderWinding,
        start_angle: f32,
        end_angle: f32,
        inner_radius: f32,
        outer_radius: f32,
    ) -> VizSector {
        let (outer, inner) = match render_winding {
            RenderWinding::Clockwise => {
                let outer = VizArc::from_angles(center, outer_radius, start_angle, end_angle);
                let inner = VizArc::from_angles(center, inner_radius, end_angle, start_angle);
                (outer, inner)
            }
            RenderWinding::CounterClockwise => {
                let outer = VizArc::from_angles(center, outer_radius, end_angle, start_angle);
                let inner = VizArc::from_angles(center, inner_radius, start_angle, end_angle);
                (outer, inner)
            }
        };
        VizSector::from((start_angle, end_angle, outer, inner))
    }
}

impl From<VizSector> for VizShape {
    #[inline]
    fn from(sector: VizSector) -> VizShape {
        VizShape::Sector(sector)
    }
}

impl VizRotate for VizSector {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizSector {
        VizSector {
            start: self.start + rot.angle,
            end:   self.end + rot.angle,
            outer: self.outer.rotate(rot),
            inner: self.inner.rotate(rot),
        }
    }
}

/// A [VizElementInfo] represents all the information needed to render a track element in a visualization
/// as well as resolve the element back to the track, useful for interactive visualizations (e.g.,
/// selecting sectors with the mouse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VizElementInfo {
    /// The type of element as [GenericTrackElement]
    pub element_type: GenericTrackElement,
    /// The physical track containing the element
    pub ch: DiskCh,
    /// The bit index of the element within the track.
    pub bit_range: Option<Range<usize>>,
    /// The index of the element within the track's element list.
    pub element_idx: Option<usize>,
    /// The physical index of the sector on the track, starting at 0 at the index.
    pub sector_idx: Option<usize>,
}

impl VizElementInfo {
    pub fn new(
        element_type: GenericTrackElement,
        ch: DiskCh,
        bit_range: Option<Range<usize>>,
        element_idx: Option<usize>,
        sector_idx: Option<usize>,
    ) -> VizElementInfo {
        VizElementInfo {
            element_type,
            ch,
            bit_range,
            element_idx,
            sector_idx,
        }
    }
}

impl Default for VizElementInfo {
    fn default() -> VizElementInfo {
        VizElementInfo {
            element_type: GenericTrackElement::NullElement,
            ch: DiskCh::default(),
            bit_range: None,
            element_idx: None,
            sector_idx: None,
        }
    }
}

/// A [VizElement] represents a [VizSector] with additional metadata, such as color and
/// track location.
#[derive(Clone, Debug)]
pub struct VizElement {
    pub shape: VizShape,        // The shape of the element
    pub flags: VizElementFlags, // Flags to control rendering of the element
    pub info:  VizElementInfo,  // Metadata fields for the element
}

impl VizElement {
    pub fn new(shape: impl Into<VizShape>, flags: VizElementFlags, info: VizElementInfo) -> VizElement {
        VizElement {
            shape: shape.into(),
            flags,
            info,
        }
    }
}

impl VizRotate for VizElement {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizElement {
        VizElement {
            shape: self.shape.rotate(rot),
            flags: self.flags,
            info:  self.info,
        }
    }
}

/// Convert a tuple of two [VizArc] objects into a [VizSector].
impl From<(f32, f32, VizArc, VizArc)> for VizSector {
    #[inline]
    fn from((start, end, outer, inner): (f32, f32, VizArc, VizArc)) -> VizSector {
        VizSector {
            start,
            end,
            outer,
            inner,
        }
    }
}

#[cfg(feature = "tiny_skia")]
impl From<VizPoint2d<f32>> for tiny_skia::Point {
    #[inline]
    fn from(p: VizPoint2d<f32>) -> tiny_skia::Point {
        tiny_skia::Point { x: p.x, y: p.y }
    }
}

#[cfg(feature = "tiny_skia")]
impl From<tiny_skia::Point> for VizPoint2d<f32> {
    #[inline]
    fn from(p: tiny_skia::Point) -> VizPoint2d<f32> {
        VizPoint2d { x: p.x, y: p.y }
    }
}

/// A slice of a track used in vector based data layer visualization.
#[derive(Clone, Debug)]
pub struct VizDataSlice {
    pub density: f32,         // The ratio of 1 bits set to the total number of bits in the slice
    pub decoded_density: f32, // The ratio of 1 bits set to the total number of bits in the slice after decoding
    pub arc: VizQuadraticArc, // The slice arc
}

impl VizRotate for VizDataSlice {
    #[inline]
    fn rotate(self, rot: &VizRotation) -> VizDataSlice {
        VizDataSlice {
            density: self.density,
            decoded_density: self.decoded_density,
            arc: self.arc.rotate(rot),
        }
    }
}
