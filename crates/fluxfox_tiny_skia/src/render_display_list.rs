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
use crate::{render_elements::*, styles::SkiaStyle};

use fluxfox::{track_schema::GenericTrackElement, visualization::prelude::*, FoxHashMap};

use tiny_skia::{Paint, Pixmap, Stroke};

pub fn render_display_list(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    angle: f32,
    display_list: &VizElementDisplayList,
    track_style: &SkiaStyle,
    styles: &FoxHashMap<GenericTrackElement, SkiaStyle>,
) -> Result<(), String> {
    // Create a transform to rotate around the center of the pixmap
    let transform = tiny_skia::Transform::from_rotate_at(
        angle.to_degrees(),
        pixmap.width() as f32 / 2.0,
        pixmap.height() as f32 / 2.0,
    );

    for element in display_list.iter() {
        skia_render_element(pixmap, paint, &transform, track_style, &styles, element);
    }

    Ok(())
}

pub fn render_data_display_list(
    pixmap: &mut Pixmap,
    paint: &mut Paint,
    angle: f32,
    display_list: &VizDataSliceDisplayList,
) -> Result<(), String> {
    // Create a transform to rotate around the center of the pixmap
    let transform = tiny_skia::Transform::from_rotate_at(
        angle.to_degrees(),
        pixmap.width() as f32 / 2.0,
        pixmap.height() as f32 / 2.0,
    );

    // Disable antialiasing to reduce moiré. We do a similar thing with SVG rendering
    paint.anti_alias = false;

    let mut stroke = Stroke::default();
    if display_list.track_width == 0.0 {
        log::error!("Track width is 0, nothing will be rendered!");
    }
    stroke.width = display_list.track_width;

    for slice in display_list.iter() {
        skia_render_data_slice(pixmap, paint, &mut stroke, &transform, slice);
    }

    Ok(())
}
