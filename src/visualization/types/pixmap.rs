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

//! Defines a generic VizPixmap that is intended to be generally compatible
//! with tiny_skia's Pixmap without requiring a dependency on tiny_skia.
//! A [VizPixmap] represents a 32-bit RGBA image buffer with a width, height,
//! and u8 pixel buffer.

use crate::visualization::types::color::VizColor;
use bytemuck::cast_slice;

pub struct VizPixmap {
    pub width: u32,
    pub height: u32,
    pub pixel_data: Vec<u8>,
}

impl VizPixmap {
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_data = vec![0; (width * height) as usize * 4];
        Self {
            width,
            height,
            pixel_data,
        }
    }

    pub fn pixel_data(&self) -> &[u8] {
        &self.pixel_data
    }

    pub fn pixel_data_mut(&mut self) -> &mut [u8] {
        &mut self.pixel_data
    }

    pub fn as_vizcolor(&self) -> &[VizColor] {
        cast_slice(self.pixel_data())
    }
}
