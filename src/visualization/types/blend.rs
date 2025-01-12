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

//! Generic blend mode definitions. These are a subset supported by both SVG (really CSS)
//! and the `tiny_skia` library.

use std::{
    fmt,
    fmt::{Display, Formatter},
};

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
pub enum VizBlendMode {
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

/// Implement the `Display` trait for `VizBlendMode`, in a fashion compatible with CSS names for
/// standard blend modes. This allows direct use of blend modes as CSS strings.
impl Display for VizBlendMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use VizBlendMode::*;
        match self {
            Normal => write!(f, "normal"),
            Multiply => write!(f, "multiply"),
            Screen => write!(f, "screen"),
            Overlay => write!(f, "overlay"),
            Darken => write!(f, "darken"),
            Lighten => write!(f, "lighten"),
            ColorDodge => write!(f, "color-dodge"),
            ColorBurn => write!(f, "color-burn"),
            HardLight => write!(f, "hard-light"),
            SoftLight => write!(f, "soft-light"),
            Difference => write!(f, "difference"),
            Exclusion => write!(f, "exclusion"),
            Hue => write!(f, "hue"),
            Saturation => write!(f, "saturation"),
            Color => write!(f, "color"),
            Luminosity => write!(f, "luminosity"),
        }
    }
}
