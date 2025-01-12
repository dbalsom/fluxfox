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
use std::{fs, path::Path};

use crate::style::Style;

use anyhow::Error;
use fluxfox::{track_schema::GenericTrackElement, visualization::prelude::*, FoxHashMap};
use fluxfox_svg::prelude::BlendMode;
use serde::Deserialize;

// Deserialize colors as either RGBA tuple or u32
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ConfigColor {
    Rgba(u8, u8, u8, u8),
    U32(u32),
}

#[derive(Copy, Clone, Debug, Default, Deserialize)]
pub enum ConfigBlendMode {
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

#[cfg(feature = "use_svg")]
impl From<ConfigBlendMode> for BlendMode {
    fn from(value: ConfigBlendMode) -> Self {
        match value {
            ConfigBlendMode::Normal => BlendMode::Normal,
            ConfigBlendMode::Multiply => BlendMode::Multiply,
            ConfigBlendMode::Screen => BlendMode::Screen,
            ConfigBlendMode::Overlay => BlendMode::Overlay,
            ConfigBlendMode::Darken => BlendMode::Darken,
            ConfigBlendMode::Lighten => BlendMode::Lighten,
            ConfigBlendMode::ColorDodge => BlendMode::ColorDodge,
            ConfigBlendMode::ColorBurn => BlendMode::ColorBurn,
            ConfigBlendMode::HardLight => BlendMode::HardLight,
            ConfigBlendMode::SoftLight => BlendMode::SoftLight,
            ConfigBlendMode::Difference => BlendMode::Difference,
            ConfigBlendMode::Exclusion => BlendMode::Exclusion,
            ConfigBlendMode::Hue => BlendMode::Hue,
            ConfigBlendMode::Saturation => BlendMode::Saturation,
            ConfigBlendMode::Color => BlendMode::Color,
            ConfigBlendMode::Luminosity => BlendMode::Luminosity,
        }
    }
}

// Optional style fields
#[derive(Debug, Deserialize)]
struct PartialStyleConfig {
    fill: Option<ConfigColor>,
    stroke: Option<ConfigColor>,
    stroke_width: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct MaskConfigInput {
    weak:  ConfigColor,
    error: ConfigColor,
}

pub(crate) struct MaskConfig {
    pub(crate) weak:  VizColor,
    pub(crate) error: VizColor,
}

// Complete palette configuration
#[derive(Debug, Deserialize)]
struct StyleConfigFileInput {
    track_gap: f32,
    default_style: PartialStyleConfig,
    masks: MaskConfigInput,
    track_style: PartialStyleConfig,
    element_styles: FoxHashMap<String, PartialStyleConfig>,
    blend_mode: ConfigBlendMode,
}

// Translated and merged configuration
pub(crate) struct StyleConfig {
    pub(crate) track_gap: f32,
    pub(crate) masks: MaskConfig,
    pub(crate) track_style: Style,
    pub(crate) element_styles: FoxHashMap<GenericTrackElement, Style>,
    pub(crate) blend_mode: ConfigBlendMode,
}

// Conversion for ConfigColor to VizColor
impl ConfigColor {
    fn to_viz_color(&self) -> VizColor {
        match self {
            ConfigColor::Rgba(r, g, b, a) => VizColor::from_rgba8(*r, *g, *b, *a),
            ConfigColor::U32(val) => {
                let r = ((val >> 24) & 0xFF) as u8;
                let g = ((val >> 16) & 0xFF) as u8;
                let b = ((val >> 8) & 0xFF) as u8;
                let a = (val & 0xFF) as u8;
                VizColor::from_rgba8(r, g, b, a)
            }
        }
    }
}

// Merge optional styles with defaults
fn merge_style(default: &PartialStyleConfig, custom: &PartialStyleConfig) -> Style {
    let fill = custom
        .fill
        .as_ref()
        .or(default.fill.as_ref())
        .expect("Fill color must be defined")
        .to_viz_color();

    let stroke = custom
        .stroke
        .as_ref()
        .or(default.stroke.as_ref())
        .expect("Stroke color must be defined")
        .to_viz_color();

    let stroke_width = custom.stroke_width.unwrap_or(default.stroke_width.unwrap_or(1.0));

    Style {
        fill,
        stroke,
        stroke_width,
    }
}

pub fn load_style_config(path: impl AsRef<Path>) -> Result<StyleConfig, Error> {
    let config_str = fs::read_to_string(path)?;
    let config: StyleConfigFileInput = toml::from_str(&config_str)?;

    let mut styles = FoxHashMap::new();

    for (key, partial_style) in config.element_styles {
        let element = match key.as_str() {
            "NullElement" => GenericTrackElement::NullElement,
            "Marker" => GenericTrackElement::Marker,
            "SectorHeader" => GenericTrackElement::SectorHeader,
            "SectorBadHeader" => GenericTrackElement::SectorBadHeader,
            "SectorData" => GenericTrackElement::SectorData,
            "SectorDeletedData" => GenericTrackElement::SectorDeletedData,
            "SectorBadData" => GenericTrackElement::SectorBadData,
            "SectorBadDeletedData" => GenericTrackElement::SectorBadDeletedData,
            _ => continue,
        };

        let style = merge_style(&config.default_style, &partial_style);
        styles.insert(element, style);
    }

    let track_style = merge_style(&config.default_style, &config.track_style);

    Ok(StyleConfig {
        track_gap: config.track_gap,
        masks: MaskConfig {
            weak:  config.masks.weak.to_viz_color(),
            error: config.masks.error.to_viz_color(),
        },
        track_style,
        element_styles: styles,
        blend_mode: config.blend_mode,
    })
}
