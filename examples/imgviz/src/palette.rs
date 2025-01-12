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

//! Palette module for defining the palette for imgviz to use.
//! Currently, this returns a single default palette, but eventually we should
//! be able to load a user-specified palette, probably from a TOML file...

use crate::{
    config::{ConfigBlendMode, MaskConfig, StyleConfig},
    style::Style,
};
use fluxfox::{track_schema::GenericTrackElement, visualization::prelude::*, FoxHashMap};

/// Return a default palette for visualization.
/// There's no need to modify this - override colors with a style.toml file and use the `--style`
/// flag to apply them.
pub fn default_palette() -> FoxHashMap<GenericTrackElement, VizColor> {
    // Defined colors
    let viz_light_red: VizColor = VizColor::from_rgba8(180, 0, 0, 255);
    let vis_purple: VizColor = VizColor::from_rgba8(180, 0, 180, 255);
    let pal_medium_green = VizColor::from_rgba8(0x38, 0xb7, 0x64, 0xff);
    let pal_dark_green = VizColor::from_rgba8(0x25, 0x71, 0x79, 0xff);
    let pal_medium_blue = VizColor::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
    let pal_light_blue = VizColor::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
    let pal_orange = VizColor::from_rgba8(0xef, 0x7d, 0x57, 0xff);

    // Here are some other colors you can use if you want to change the palette

    //let viz_orange = VizColor::from_rgba8(255, 100, 0, 255);
    //let viz_cyan = VizColor::from_rgba8(70, 200, 200, 255);
    //let vis_light_purple = VizColor::from_rgba8(185, 0, 255, 255);
    //let pal_dark_blue = VizColor::from_rgba8(0x29, 0x36, 0x6f, 0xff);
    //let pal_dark_purple = VizColor::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
    //let pal_dark_red = VizColor::from_rgba8(0xb1, 0x3e, 0x53, 0xff);
    //let pal_weak_bits = VizColor::from_rgba8(70, 200, 200, 255);
    //let pal_error_bits = VizColor::from_rgba8(255, 0, 0, 255);

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

/// Convert a simple color palette to a style map, using default stroke color and stroke width
pub fn palette_to_style_config(palette: &FoxHashMap<GenericTrackElement, VizColor>) -> StyleConfig {
    let mut style_map = FoxHashMap::new();
    for (element, color) in palette.iter() {
        style_map.insert(*element, Style::fill_only(*color));
    }

    StyleConfig {
        track_gap: 0.0,
        masks: MaskConfig {
            weak:  default_weak_bit_color(),
            error: default_error_bit_color(),
        },
        track_style: Style::fill_only(VizColor::from_rgba8(0, 0, 0, 0)),
        element_styles: style_map,
        blend_mode: ConfigBlendMode::Multiply,
    }
}

/// Return the default style configuration for visualization if a style file is not provided.
pub fn default_style_config() -> StyleConfig {
    let palette = default_palette();
    palette_to_style_config(&palette)
}

/// Return a default color for weak bit visualization.
/// There's no need to modify this - override colors with a style.toml file and use the `--style`
pub fn default_weak_bit_color() -> VizColor {
    VizColor::from_rgba8(70, 200, 200, 255)
}

/// Return a default color for error bit visualization.
/// There's no need to modify this - override colors with a style.toml file and use the `--style`
pub fn default_error_bit_color() -> VizColor {
    VizColor::from_rgba8(255, 0, 0, 255)
}
