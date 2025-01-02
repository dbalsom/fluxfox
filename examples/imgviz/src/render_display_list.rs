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

use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        tiny_skia::{BlendMode, Color, Paint, Pixmap},
        tiny_skia_util::skia_render_element,
        RenderTrackMetadataParams,
        VizDisplayList,
    },
};
use std::collections::HashMap;
use tiny_skia::Transform;

pub fn render_display_list(
    pixmap: &mut Pixmap,
    params: &RenderTrackMetadataParams,
    display_list: &VizDisplayList,
    palette: &HashMap<GenericTrackElement, Color>,
) {
    let mut paint = Paint {
        blend_mode: BlendMode::SourceOver,
        anti_alias: true,
        ..Default::default()
    };

    let mut transform = Transform::identity();

    if params.index_angle != 0.0 {
        log::warn!("Rotating display list by {}", params.index_angle);
        transform = Transform::from_rotate_at(
            params.direction.adjust_angle(params.index_angle.to_degrees()),
            pixmap.width() as f32 / 2.0,
            pixmap.height() as f32 / 2.0,
        );
    }

    for element in display_list.iter() {
        skia_render_element(pixmap, &mut paint, element, &transform, palette);
    }
}
