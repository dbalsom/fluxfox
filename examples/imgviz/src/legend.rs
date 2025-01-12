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

use crate::text::{calculate_scaled_font_size, measure_text, render_text, Justification};
use rusttype::Font;
use tiny_skia::{Color, Pixmap};

pub struct VizLegend {
    pub title_string: String,
    pub title_font: Option<Font<'static>>,
    pub title_font_size: f32,
    pub title_font_color: Color,
    pub image_size: u32,
    pub height: Option<i32>,
}

impl VizLegend {
    pub fn new(title: &str, image_size: u32) -> Self {
        log::debug!("Using title: {}", title);

        Self {
            title_string: title.to_string(),
            title_font: None,
            title_font_size: 0.0,
            title_font_color: Color::WHITE,
            image_size,
            height: None,
        }
    }

    pub fn set_title_font(&mut self, font: Font<'static>, size: f32) {
        self.title_font = Some(font);
        self.title_font_size = size;
        self.calculate_height();
    }

    fn calculate_height(&mut self) {
        if let Some(font) = &self.title_font {
            let font_size = calculate_scaled_font_size(40.0, self.image_size, 1024);
            let (_, font_h) = measure_text(&font, &self.title_string, font_size);
            self.height = Some(font_h * 3); // 3 lines of text. Title will be centered within.
        }
    }

    pub fn height(&self) -> Option<i32> {
        self.height
    }

    pub fn render(&mut self, pixmap: &mut Pixmap) {
        if let Some(title_font) = &self.title_font {
            let (_, font_h) = measure_text(title_font, &self.title_string, self.title_font_size);

            let legend_height = self.height.unwrap_or(0);
            let x = (pixmap.width() / 2) as i32;
            let y = pixmap.height() as i32 - legend_height - font_h; // Draw text one 'line' up from bottom of image.

            log::debug!("Rendering text at ({}, {})", x, y);
            render_text(
                pixmap,
                title_font,
                self.title_font_size,
                &self.title_string,
                x,
                y,
                Justification::Center,
                self.title_font_color,
            );
        }
    }
}
