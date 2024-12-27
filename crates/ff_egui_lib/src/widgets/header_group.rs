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
use egui::{Pos2, Rect, RichText, Rounding, Stroke, TextStyle, Vec2};

pub struct HeaderGroup {
    heading: String,
    strong:  bool,
    expand:  bool,
}

impl HeaderGroup {
    pub fn new(str: &str) -> Self {
        Self {
            heading: str.to_string(),
            strong:  false,
            expand:  false,
        }
    }

    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    pub fn expand(mut self) -> Self {
        self.expand = true;
        self
    }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        body_content: impl FnOnce(&mut egui::Ui),
        header_content: impl FnOnce(&mut egui::Ui),
    ) {
        // Add some margin space for the group
        let margin = ui.style().spacing.window_margin;

        // Begin the group
        let response = ui.scope(|ui| {
            if self.expand {
                ui.set_width(ui.available_width()); // Use the full width
            }

            ui.horizontal(|ui| {
                ui.add_space(margin.left); // Left margin
                ui.vertical(|ui| {
                    // Paint the heading
                    ui.add_space(margin.top); // Top margin

                    ui.horizontal(|ui| {
                        let mut text = RichText::new(&self.heading);
                        if self.strong {
                            text = text.strong();
                        }

                        ui.heading(text);
                        header_content(ui);
                    });

                    ui.add_space(margin.top); // Top margin

                    // Draw the custom content
                    ui.horizontal(|ui| {
                        body_content(ui);
                        //ui.add_space(ui.available_width());
                    });

                    ui.add_space(margin.bottom); // Bottom margin
                });
            });
        });

        // Get the rect for the entire group we just created
        let group_rect = response.response.rect;

        // Paint the header background
        if ui.is_rect_visible(group_rect) {
            let header_height = ui.fonts(|fonts| fonts.row_height(&TextStyle::Heading.resolve(ui.style())));
            let header_rect = Rect::from_min_size(
                group_rect.min,
                Vec2::new(group_rect.width(), header_height + margin.top * 2.0), // Include padding for margins
            );
            let painter = ui.painter();
            let visuals = ui.visuals();
            let bg_color = visuals.faint_bg_color.gamma_multiply(1.2); // Slightly brighter than the default background
            let rounding = Rounding {
                nw: 4.0, // Top-left corner
                ne: 4.0, // Top-right corner
                sw: 0.0, // Bottom-left corner
                se: 0.0, // Bottom-right corner
            };
            painter.rect_filled(header_rect, rounding, bg_color);

            // Draw a light line at the bottom of the header
            let line_color = visuals.widgets.inactive.bg_fill; // Use a lighter color for the line
            let line_stroke = Stroke::new(1.0, line_color);
            let bottom_line_start = Pos2::new(header_rect.min.x, header_rect.max.y);
            let bottom_line_end = Pos2::new(header_rect.max.x, header_rect.max.y);
            painter.line_segment([bottom_line_start, bottom_line_end], line_stroke);
        }

        // Draw the overall border for the group
        if ui.is_rect_visible(group_rect) {
            let painter = ui.painter();
            let visuals = ui.visuals();
            let border_color = visuals.widgets.noninteractive.bg_stroke.color;
            let stroke = Stroke::new(1.0, border_color);
            let rounding = Rounding::same(6.0); // Overall group rounding
            painter.rect_stroke(group_rect, rounding, stroke);
        }
    }
}
