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

use fluxfox::{track_schema::GenericTrackElement, visualization::prelude::VizColor, FoxHashMap};
use std::fmt::{self, Display, Formatter};

pub fn vizcolor_to_color(color: VizColor) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.r(), color.g(), color.b(), color.a())
}

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

/// All supported SVG `style` tag blending modes.
/// https://developer.mozilla.org/en-US/docs/Web/CSS/mix-blend-mode
///
/// If the `serde` feature is enabled these will be available for deserialization,
/// such as from a config file (see the `imgviz` example for an example of this).
///
/// The blending mode is applied to the metadata layer when metadata visualization
/// is enabled in addition to data visualization.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[derive(Copy, Clone, Debug, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Display for BlendMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BlendMode::Normal => write!(f, "normal"),
            BlendMode::Multiply => write!(f, "multiply"),
            BlendMode::Screen => write!(f, "screen"),
            BlendMode::Overlay => write!(f, "overlay"),
            BlendMode::Darken => write!(f, "darken"),
            BlendMode::Lighten => write!(f, "lighten"),
            BlendMode::ColorDodge => write!(f, "color-dodge"),
            BlendMode::ColorBurn => write!(f, "color-burn"),
            BlendMode::HardLight => write!(f, "hard-light"),
            BlendMode::SoftLight => write!(f, "soft-light"),
            BlendMode::Difference => write!(f, "difference"),
            BlendMode::Exclusion => write!(f, "exclusion"),
            BlendMode::Hue => write!(f, "hue"),
            BlendMode::Saturation => write!(f, "saturation"),
            BlendMode::Color => write!(f, "color"),
            BlendMode::Luminosity => write!(f, "luminosity"),
        }
    }
}

/// Define style attributes for SVG elements.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default)]
pub struct ElementStyle {
    /// The color to fill the element with. If no fill is desired, use `VizColor::TRANSPARENT`.
    pub fill: VizColor,
    /// The color to use to stroke the element path. If no stroke is desired, use `VizColor::TRANSPARENT`.
    /// or set stroke_width to 0.0 (probably do both).
    pub stroke: VizColor,
    /// The width of the stroke. If no stroke is desired, set to 0.0.
    pub stroke_width: f32,
}

/// Return a default mapping between [GenericTrackElement]s and [VizColor]s.
fn default_element_palette() -> FoxHashMap<GenericTrackElement, VizColor> {
    // Defined colors
    let viz_light_red: VizColor = VizColor::from_rgba8(180, 0, 0, 255);
    let vis_purple: VizColor = VizColor::from_rgba8(180, 0, 180, 255);
    let pal_medium_green = VizColor::from_rgba8(0x38, 0xb7, 0x64, 0xff);
    let pal_dark_green = VizColor::from_rgba8(0x25, 0x71, 0x79, 0xff);
    let pal_medium_blue = VizColor::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
    let pal_light_blue = VizColor::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
    let pal_orange = VizColor::from_rgba8(0xef, 0x7d, 0x57, 0xff);

    #[rustfmt::skip]
    let palette = FoxHashMap::from([
        (GenericTrackElement::SectorData, pal_medium_green),
        (GenericTrackElement::SectorBadData, pal_orange),
        (GenericTrackElement::SectorDeletedData, pal_dark_green),
        (GenericTrackElement::SectorBadDeletedData, viz_light_red),
        (GenericTrackElement::SectorHeader, pal_light_blue),
        (GenericTrackElement::SectorBadHeader, pal_medium_blue),
        (GenericTrackElement::Marker, vis_purple),
    ]);
    palette
}

/// Return a default mapping between [GenericTrackElement]s and [SkiaStyle]s.
pub fn default_skia_styles() -> FoxHashMap<GenericTrackElement, SkiaStyle> {
    let palette = default_element_palette();
    let mut styles = FoxHashMap::new();
    for (element, color) in palette.iter() {
        styles.insert(
            *element,
            SkiaStyle {
                fill: *color,
                stroke: VizColor::TRANSPARENT,
                stroke_width: 0.0,
            },
        );
    }
    styles
}
