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

//! Module providing default rasterization functions based on `tiny_skia`.
//! This module can be enabled via the `tiny_skia` feature flag, which means
//! if you are willing to pull in this dependency you do not have to write
//! your own rasterization functions.

pub mod helpers;
pub mod rasterize_disk;

use crate::visualization::prelude::VizColor;

pub use helpers::*;
pub use rasterize_disk::*;

#[derive(Copy, Clone, Debug, Default)]
pub struct SkiaStyle {
    pub fill: VizColor,
    pub stroke: VizColor,
    pub stroke_width: f32,
}

impl SkiaStyle {
    pub fn fill_only(fill: VizColor) -> SkiaStyle {
        SkiaStyle {
            fill,
            stroke: VizColor::from_rgba8(0, 0, 0, 0),
            stroke_width: 0.0,
        }
    }
}
