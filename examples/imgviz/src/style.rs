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

use fluxfox::{track_schema::GenericTrackElement, visualization::prelude::*, FoxHashMap};

use fluxfox_svg::prelude::ElementStyle;
use fluxfox_tiny_skia::styles::SkiaStyle;

// Style struct for storing visual properties
#[derive(Copy, Clone, Debug, Default)]
pub struct Style {
    pub fill: VizColor,
    pub stroke: VizColor,
    pub stroke_width: f32,
}

impl Style {
    pub fn fill_only(fill: VizColor) -> Style {
        Style {
            fill,
            stroke: VizColor::from_rgba8(0, 0, 0, 0),
            stroke_width: 0.0,
        }
    }
}

#[cfg(feature = "use_svg")]
pub fn style_map_to_fluxfox_svg(
    style_map: &FoxHashMap<GenericTrackElement, Style>,
) -> FoxHashMap<GenericTrackElement, ElementStyle> {
    style_map
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                ElementStyle {
                    fill: v.fill,
                    stroke: v.stroke,
                    stroke_width: v.stroke_width,
                },
            )
        })
        .collect()
}

#[cfg(feature = "use_tiny_skia")]
pub fn style_map_to_skia(
    style_map: &FoxHashMap<GenericTrackElement, Style>,
) -> FoxHashMap<GenericTrackElement, SkiaStyle> {
    style_map
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                SkiaStyle {
                    fill: v.fill,
                    stroke: v.stroke,
                    stroke_width: v.stroke_width,
                },
            )
        })
        .collect()
}

#[cfg(feature = "use_tiny_skia")]
impl From<Style> for SkiaStyle {
    fn from(style: Style) -> Self {
        SkiaStyle {
            fill: style.fill,
            stroke: style.stroke,
            stroke_width: style.stroke_width,
        }
    }
}

#[cfg(feature = "use_tiny_skia")]
impl From<&Style> for SkiaStyle {
    fn from(style: &Style) -> Self {
        SkiaStyle {
            fill: style.fill,
            stroke: style.stroke,
            stroke_width: style.stroke_width,
        }
    }
}
