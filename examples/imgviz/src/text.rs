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

    examples/imgviz/src/text.rs

    Text-handling routines using rusttype.


*/

use rusttype::{point, Font, Scale};
use tiny_skia::{BlendMode, Color, FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};

/// Enum to specify text justification
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum Justification {
    Left,
    Center,
    Right,
}

pub(crate) fn calculate_scaled_font_size(base_font_size: f32, resolution: u32, baseline_resolution: u32) -> f32 {
    // Calculate scaling factors for width and height relative to the baseline resolution
    let scale = resolution as f32 / baseline_resolution as f32;

    // Scale the base font size
    base_font_size * scale
}

pub(crate) fn create_font(font_data: &[u8]) -> Result<Font, String> {
    Font::try_from_bytes(font_data).ok_or("Failed to load font".to_string())
}

pub(crate) fn render_text(
    pixmap: &mut Pixmap,
    font: &Font,
    size: f32,
    text: &str,
    x: i32,
    y: i32,
    justification: Justification,
    base_color: Color,
) {
    let scale = Scale::uniform(size); // Set font size

    // Calculate text width to adjust the starting position based on justification
    let text_width = measure_text(font, text, size).0;
    let adjusted_x = match justification {
        Justification::Left => x,
        Justification::Center => x - text_width / 2,
        Justification::Right => x - text_width,
    };

    // Layout the text at origin (0, 0) and draw each glyph at the adjusted position
    for glyph in font.layout(text, scale, point(0.0, 0.0)) {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            // Create a pixmap for the glyph
            let mut glyph_pixmap = Pixmap::new(bounding_box.width() as u32, bounding_box.height() as u32)
                .expect("Failed to create glyph pixmap");

            // Render the glyph into its pixmap
            glyph.draw(|px, py, alpha| {
                let alpha = alpha.max(0.0).min(1.0); // Clamp alpha to [0.0, 1.0]
                let alpha_color = PremultipliedColorU8::from_rgba(
                    (base_color.red() * alpha * 255.0) as u8,
                    (base_color.green() * alpha * 255.0) as u8,
                    (base_color.blue() * alpha * 255.0) as u8,
                    (base_color.alpha() * alpha * 255.0) as u8,
                )
                .expect("Failed to create PremultipliedColorU8");

                let pixel_index = ((py * glyph_pixmap.width() + px) * 4) as usize;
                let data = glyph_pixmap.data_mut();
                data[pixel_index] = alpha_color.red();
                data[pixel_index + 1] = alpha_color.green();
                data[pixel_index + 2] = alpha_color.blue();
                data[pixel_index + 3] = alpha_color.alpha();
            });

            // Draw the glyph onto the main pixmap at the final position
            pixmap.draw_pixmap(
                bounding_box.min.x + adjusted_x,
                bounding_box.min.y + y,
                glyph_pixmap.as_ref(),
                &PixmapPaint {
                    opacity: 1.0,
                    blend_mode: BlendMode::SourceOver,
                    quality: FilterQuality::Bilinear,
                },
                Transform::identity(),
                None,
            );
        }
    }
}

/// Calculate the width and height of rendered text based on the font, text, and font size.
pub(crate) fn measure_text(font: &Font, text: &str, font_size: f32) -> (i32, i32) {
    let scale = Scale::uniform(font_size);

    let mut min_x = 0;
    let mut min_y = 0;
    let mut max_x = 0;
    let mut max_y = 0;

    for glyph in font.layout(text, scale, point(0.0, 0.0)) {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            min_x = min_x.min(bounding_box.min.x);
            max_x = max_x.max(bounding_box.max.x);
            min_y = min_y.min(bounding_box.min.y);
            max_y = max_y.max(bounding_box.max.y);
        }
    }

    (max_x - min_x, max_y - min_y)
}
