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

//! A [VizElementDisplayList] is a list of [VizElement] objects to be rendered.
//! Operations can be implemented on this list, such as scaling and rotation.

use crate::visualization::{
    types::shapes::{VizDataSlice, VizElement},
    TurningDirection,
};

/// A [VizElementDisplayList] is a list of [VizElement] objects to be rendered.
/// Operations can be implemented on this list, such as scaling and rotation.
#[derive(Clone)]
pub struct VizElementDisplayList {
    pub turning: TurningDirection,
    pub side:    u8,
    pub tracks:  Vec<Vec<VizElement>>,
}

impl VizElementDisplayList {
    pub fn new(turning: TurningDirection, side: u8, cylinders: u16) -> VizElementDisplayList {
        VizElementDisplayList {
            turning,
            side,
            tracks: vec![Vec::new(); cylinders as usize],
        }
    }

    /// Push a [VizElement] onto the display list at the specified track.
    /// If the track does not exist, nothing will happen.
    pub fn push(&mut self, c: usize, element: VizElement) {
        if c < self.tracks.len() {
            self.tracks[c].push(element);
        }
    }

    /// Return the total number of [VizElement]s in the display list.
    pub fn len(&self) -> usize {
        let mut total = 0;
        for track in &self.tracks {
            log::debug!("track.len() = {}", track.len());
            total += track.len()
        }
        total
    }

    /// Rotate all items in the display list by the specified `angle` in radians.
    /// ## Warning: This is a lossy operation. Multiple rotations will accumulate errors.
    /// This feature is mostly designed for debugging and testing.
    /// To properly rotate a visualization you should use a transformation matrix in your rendering
    /// engine. See the `imgviz` example crate for an example of how to do this with `svg` and
    /// `tiny_skia`, or the `ff_egui_lib` crate for an example of how to do this with `egui`.
    // pub fn rotate(&mut self, angle: f32) {
    //     for track in &mut self.tracks {
    //         for element in track {
    //             element.rotate(angle);
    //         }
    //     }
    // }

    /// Return an Iterator that yields all the [VizElement]s in the display list,
    /// in order, by track.
    pub fn iter(&self) -> VizDisplayListIter {
        let mut outer = self.tracks.iter();
        // Initialize inner iterator with the first track
        let inner = outer.next().map(|v| v.iter());
        VizDisplayListIter { outer, inner }
    }

    /// Return a slice of the items in the display list at the specified track.
    pub fn items(&self, c: usize) -> Option<&[VizElement]> {
        self.tracks.get(c).map(|v| v.as_slice())
    }
}

// Iterator struct
pub struct VizDisplayListIter<'a> {
    outer: std::slice::Iter<'a, Vec<VizElement>>,
    inner: Option<std::slice::Iter<'a, VizElement>>,
}

impl<'a> Iterator for VizDisplayListIter<'a> {
    type Item = &'a VizElement;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(inner) = &mut self.inner {
                if let Some(next_item) = inner.next() {
                    return Some(next_item);
                }
            }

            // Move to the next outer track if the current inner is exhausted
            self.inner = self.outer.next().map(|v| v.iter());

            // If there are no more tracks, break out
            if self.inner.is_none() {
                return None;
            }
        }
    }
}

/// A [VizDataSliceDisplayList] is a list of [VizDataSlice] objects to be rendered.
/// Operations can be implemented on this list, such as scaling and rotation.
pub struct VizDataSliceDisplayList {
    pub min_density: f32,
    pub max_density: f32,
    pub track_width: f32,
    pub turning: TurningDirection,
    pub tracks: Vec<Vec<VizDataSlice>>,
}

// Iterator struct
pub struct VizDataDisplayListIter<'a> {
    outer: std::slice::Iter<'a, Vec<VizDataSlice>>,
    inner: Option<std::slice::Iter<'a, VizDataSlice>>,
}

impl<'a> Iterator for VizDataDisplayListIter<'a> {
    type Item = &'a VizDataSlice;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(inner) = &mut self.inner {
                if let Some(next_item) = inner.next() {
                    return Some(next_item);
                }
            }

            // Move to the next outer track if the current inner is exhausted
            self.inner = self.outer.next().map(|v| v.iter());

            // If there are no more tracks, break out
            if self.inner.is_none() {
                return None;
            }
        }
    }
}

impl VizDataSliceDisplayList {
    pub fn new(turning: TurningDirection, cylinders: usize, track_width: f32) -> VizDataSliceDisplayList {
        VizDataSliceDisplayList {
            min_density: 0.0,
            max_density: 1.0,
            track_width,
            turning,
            tracks: vec![Vec::new(); cylinders],
        }
    }

    pub fn set_track_width(&mut self, track_width: f32) {
        self.track_width = track_width;
    }

    pub fn push(&mut self, c: usize, element: VizDataSlice) {
        if c < self.tracks.len() {
            self.tracks[c].push(element);
        }
    }

    pub fn len(&self) -> usize {
        let mut total = 0;
        for track in &self.tracks {
            total += track.len()
        }
        total
    }

    /// Rotate all items in the display list by the specified `angle` in radians.
    /// ## Warning: This is a lossy operation. Multiple rotations will accumulate errors.
    /// This feature is mostly designed for debugging and testing.
    /// To properly rotate a visualization you should use a transformation matrix in your rendering engine.
    // pub fn rotate(&mut self, angle: f32) {
    //     for track in &mut self.tracks {
    //         for element in track {
    //             element.rotate(angle);
    //         }
    //     }
    // }

    /// Produce an iterator that yields all the [VizDataSlice]s in the display list,
    pub fn iter(&self) -> VizDataDisplayListIter {
        let mut outer = self.tracks.iter();
        // Initialize inner iterator with the first track
        let inner = outer.next().map(|v| v.iter());
        VizDataDisplayListIter { outer, inner }
    }
}
