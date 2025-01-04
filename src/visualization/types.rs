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
use crate::{track_schema::GenericTrackElement, types::DiskCh, visualization::VizRotate};
use bitflags::bitflags;
use core::fmt;
use num_traits::Num;
use std::{
    fmt::{Display, Formatter},
    ops::{Add, Div, Range},
};

/// A [VizColor] represents a color in 32-bit premultiplied RGBA format.
#[derive(Copy, Clone, Debug)]
pub struct VizColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for VizColor {
    fn default() -> VizColor {
        VizColor::TRANSPARENT
    }
}

#[rustfmt::skip]
impl VizColor {
    pub const TRANSPARENT: VizColor = VizColor { r: 0, g: 0, b: 0, a: 0 };
    pub const WHITE: VizColor = VizColor { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: VizColor = VizColor { r: 0, g: 0, b: 0, a: 255 };
    pub const RED: VizColor = VizColor { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: VizColor = VizColor { r: 0, g: 255, b: 0, a: 255 };
    pub const BLUE: VizColor = VizColor { r: 0, g: 0, b: 255, a: 255 };
    
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> VizColor {
        VizColor { r, g, b, a }
    }
}

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

    }
}

/// A [VizDimensions] represents the width and height of a rectangular region, such as a pixmap.
pub type VizDimensions = VizPoint2d<u32>;

/// A [VizRect] represents a rectangle in 2D space. It is generic across numeric types, using
/// `num_traits`.
///
/// The rectangle is defined by two points, the top-left and bottom-right corners.
/// Methods are provided for calculating the width and height of the rectangle.
pub struct VizRect<T> {
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

impl VizRotate for VizPoint2d<f32> {
    #[inline]
    fn rotate(&mut self, angle: f32) {
        let cos_theta = angle.cos();
        let sin_theta = angle.sin();
        self.x = self.x * cos_theta - self.y * sin_theta;
        self.y = self.x * sin_theta + self.y * cos_theta;
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

impl VizRotate for VizArc {
    #[inline]
    fn rotate(&mut self, angle: f32) {
        self.start.rotate(angle);
        self.end.rotate(angle);
        self.cp1.rotate(angle);
        self.cp2.rotate(angle);
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

impl VizRotate for VizSector {
    #[inline]
    fn rotate(&mut self, angle: f32) {
        self.outer.rotate(angle);
        self.inner.rotate(angle);
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
    pub sector: VizSector,       // The sector definition
    pub flags:  VizElementFlags, // Flags for the sector
    pub info:   VizElementInfo,  // The element represented by the sector
}

impl VizElement {
    pub fn new(sector: VizSector, flags: VizElementFlags, info: VizElementInfo) -> VizElement {
        VizElement { sector, flags, info }
    }
}

impl VizRotate for VizElement {
    #[inline]
    fn rotate(&mut self, angle: f32) {
        self.sector.rotate(angle);
    }
}

// impl VizRotate for &mut VizElement {
//     #[inline]
//     fn rotate(&mut self, angle: f32) {
//         self.sector.rotate(angle);
//     }
// }

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
pub struct VizDataSlice {
    pub popcnt: u8,         // The count of `1` bits in the slice
    pub decoded_popcnt: u8, // The count of `1` bits in the slice after decoding
    pub sector: VizSector,  // The sector definition
}

impl VizRotate for VizDataSlice {
    #[inline]
    fn rotate(&mut self, angle: f32) {
        self.sector.rotate(angle);
    }
}
