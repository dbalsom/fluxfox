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

use crate::{track_schema::GenericTrackElement, visualization::TurningDirection};
use bitflags::bitflags;

bitflags! {
    #[derive (Clone, Default)]
    pub struct VizSectorFlags: u32 {
        const NONE = 0b0000_0000;
        const OVERLAP = 0b0000_0001; // This sector crosses the index
        const OVERLAP_LONG = 0b0000_0010; // This sector crosses the index, and is sufficiently long that it should be faded out
        const EMPTY_TRACK = 0b0000_0100; // This sector represents an entire empty track
    }
}

/// A [VizPoint2d] represents a point in 2D space in the range `[(0,0), (1,1)]`.
pub struct VizPoint2d {
    pub x: f32,
    pub y: f32,
}

impl From<(f32, f32)> for VizPoint2d {
    #[inline]
    fn from(tuple: (f32, f32)) -> VizPoint2d {
        VizPoint2d { x: tuple.0, y: tuple.1 }
    }
}

impl VizPoint2d {
    pub fn new(x: f32, y: f32) -> VizPoint2d {
        VizPoint2d { x, y }
    }
}

/// A [VizArc] represents a cubic Bezier curve in 2D space.
pub struct VizArc {
    pub start: VizPoint2d, // Start point of arc
    pub end:   VizPoint2d, // End point of arc
    pub cp1:   VizPoint2d, // 1st control point
    pub cp2:   VizPoint2d, // 2nd control point
}

/// A [VizSector] represents an arc with thickness, or an 'annular sector'. This may be literally
/// be a sector on a disk, but may represent other track elements or regions as well.
pub struct VizSector {
    pub outer: VizArc,
    pub inner: VizArc,
}

/// A [VizElement] represents all the information needed to render a track element in a visualization
/// as well as resolve the element back to the track, useful for interactive visualizations (e.g.,
/// selecting sectors with the mouse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VizElement {
    /// The type of element as [GenericTrackElement]
    pub element_type: GenericTrackElement,
    /// The physical track containing the element
    pub c: u16,
    /// The bit index of the element within the track.
    pub bit_idx: Option<usize>,
    /// The index of the element within the track's element list.
    pub element_idx: Option<usize>,
    /// The physical index of the sector on the track, starting at 0 at the index.
    pub sector_idx: Option<usize>,
}

impl VizElement {
    pub fn new(
        element_type: GenericTrackElement,
        c: u16,
        bit_idx: Option<usize>,
        element_idx: Option<usize>,
        sector_idx: Option<usize>,
    ) -> VizElement {
        VizElement {
            element_type,
            c,
            bit_idx,
            element_idx,
            sector_idx,
        }
    }
}

impl Default for VizElement {
    fn default() -> VizElement {
        VizElement {
            element_type: GenericTrackElement::NullElement,
            c: 0,
            bit_idx: None,
            element_idx: None,
            sector_idx: None,
        }
    }
}

/// A [VizElementMetadata] represents a [VizSector] with additional metadata, such as color and
/// track location.
pub struct VizElementMetadata {
    pub sector:  VizSector,      // The sector definition
    pub flags:   VizSectorFlags, // Flags for the sector
    pub element: VizElement,     // The element represented by the sector
}

impl VizElementMetadata {
    pub fn new(sector: VizSector, flags: VizSectorFlags, element: VizElement) -> VizElementMetadata {
        VizElementMetadata { sector, flags, element }
    }
}

/// A [VizDisplayList] is a list of [VizElementMetadata] objects to be rendered.
/// Operations can be implemented on this list, such as scaling and rotation.
pub struct VizDisplayList {
    pub turning:  TurningDirection,
    pub elements: Vec<VizElementMetadata>,
}

impl VizDisplayList {
    pub fn new(turning: TurningDirection) -> VizDisplayList {
        VizDisplayList {
            turning,
            elements: Vec::new(),
        }
    }

    pub fn push(&mut self, element: VizElementMetadata) {
        self.elements.push(element);
    }
}

/// Convert a tuple of two [VizArc] objects into a [VizSector].
impl From<(VizArc, VizArc)> for VizSector {
    #[inline]
    fn from((outer, inner): (VizArc, VizArc)) -> VizSector {
        VizSector { outer, inner }
    }
}

#[cfg(feature = "tiny_skia")]
impl From<VizPoint2d> for tiny_skia::Point {
    #[inline]
    fn from(p: VizPoint2d) -> tiny_skia::Point {
        tiny_skia::Point { x: p.x, y: p.y }
    }
}

#[cfg(feature = "tiny_skia")]
impl From<tiny_skia::Point> for VizPoint2d {
    #[inline]
    fn from(p: tiny_skia::Point) -> VizPoint2d {
        VizPoint2d { x: p.x, y: p.y }
    }
}
