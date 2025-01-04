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

use std::collections::HashMap;

use fluxfox::{track_schema::GenericTrackElement, FoxHashMap};

use egui::Color32;

pub fn default_palette() -> FoxHashMap<GenericTrackElement, Color32> {
    let viz_light_red: Color32 = Color32::from_rgba_premultiplied(180, 0, 0, 255);

    //let viz_orange: Color = Color::from_rgba8(255, 100, 0, 255);
    let vis_purple: Color32 = Color32::from_rgba_premultiplied(180, 0, 180, 255);
    //let viz_cyan: Color = Color::from_rgba8(70, 200, 200, 255);
    //let vis_light_purple: Color = Color::from_rgba8(185, 0, 255, 255);

    let pal_medium_green = Color32::from_rgba_premultiplied(0x38, 0xb7, 0x64, 0xff);
    let pal_dark_green = Color32::from_rgba_premultiplied(0x25, 0x71, 0x79, 0xff);
    //let pal_dark_blue = Color::from_rgba8(0x29, 0x36, 0x6f, 0xff);
    let pal_medium_blue = Color32::from_rgba_premultiplied(0x3b, 0x5d, 0xc9, 0xff);
    let pal_light_blue = Color32::from_rgba_premultiplied(0x41, 0xa6, 0xf6, 0xff);
    //let pal_dark_purple = Color::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
    let pal_orange = Color32::from_rgba_premultiplied(0xef, 0x7d, 0x57, 0xff);
    //let pal_dark_red = Color::from_rgba8(0xb1, 0x3e, 0x53, 0xff);
    //let pal_weak_bits = Color32::from_rgba8(70, 200, 200, 255);
    //let pal_error_bits = Color32::from_rgba8(255, 0, 0, 255);

    #[rustfmt::skip]
    let palette = HashMap::from([
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
