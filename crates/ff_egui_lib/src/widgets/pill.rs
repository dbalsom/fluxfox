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

//! A [Pill] widget for egui. This creates a label with a rounded background.

use crate::WidgetSize;
use egui::{Color32, Response, Ui, Widget};

pub struct PillWidget {
    label: String,
    size:  WidgetSize,
    color: Color32,
    fill:  Color32,
}

impl PillWidget {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            size:  WidgetSize::default(),
            color: Color32::WHITE,
            fill:  Color32::TRANSPARENT,
        }
    }

    pub fn with_size(mut self, size: WidgetSize) -> Self {
        self.size = size;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn with_fill(mut self, color: Color32) -> Self {
        self.fill = color;
        self
    }

    pub fn show(&self, ui: &mut Ui) -> Response {
        let frame = egui::Frame::none()
            .fill(self.fill)
            .rounding(self.size.rounding())
            .inner_margin(self.size.padding())
            .outer_margin(egui::Margin::from(0.0));

        frame
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&self.label).color(self.color));
            })
            .response
    }
}

impl Widget for PillWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
