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
use bytemuck::{Pod, Zeroable};

/// A [VizColor] represents a color in 32-bit premultiplied RGBA format.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
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

    pub fn from_value(value: u8, alpha: u8) -> VizColor {
        VizColor {
            r: value,
            g: value,
            b: value,
            a: alpha,
        }
    }
    #[inline]
    pub fn r(&self) -> u8 {
        self.r
    }
    #[inline]
    pub fn set_r(&mut self, r: u8) {
        self.r = r;
    }
    #[inline]
    pub fn g(&self) -> u8 {
        self.g
    }
    #[inline]
    pub fn set_g(&mut self, g: u8) {
        self.g = g;
    }
    #[inline]
    pub fn b(&self) -> u8 {
        self.b
    }
    #[inline]
    pub fn set_b(&mut self, b: u8) {
        self.b = b;
    }
    #[inline]
    pub fn a(&self) -> u8 {
        self.a
    }
    #[inline]
    pub fn set_a(&mut self, a: u8) {
        self.a = a;
    }
}
