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
use crate::styles::ElementStyle;
use fluxfox::visualization::prelude::VizColor;

pub const SVG_OVERLAY_5_25_FLOPPY_SIDE0: &str = include_str!("5_25_side0_03.svg");

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
pub enum Overlay {
    Overlay8,
    Overlay5_25,
    Overlay3_5,
}

impl Overlay {
    pub fn svg(&self, h: u8) -> &'static str {
        match self {
            Overlay::Overlay8 => match h {
                0 => "",
                _ => "",
            },
            Overlay::Overlay5_25 => match h {
                0 => SVG_OVERLAY_5_25_FLOPPY_SIDE0,
                _ => "",
            },
            Overlay::Overlay3_5 => match h {
                0 => "",
                _ => "",
            },
        }
    }

    pub fn default_style() -> ElementStyle {
        ElementStyle {
            fill: Default::default(),
            stroke: VizColor::BLACK,
            stroke_width: 0.5,
        }
    }
}
