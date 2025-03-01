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

use fluxfox::DiskImage;
use fluxfox_egui::controls::source_map::SourceMapWidget;

#[derive(Default)]
pub struct SourceMapViewer {
    pub open:   bool,
    pub widget: SourceMapWidget,
}

impl SourceMapViewer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            open:   false,
            widget: SourceMapWidget::new(),
        }
    }

    pub fn update(&mut self, disk: &DiskImage) {
        self.widget.update(disk);
    }

    #[allow(dead_code)]
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn open_mut(&mut self) -> &mut bool {
        &mut self.open
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Source Map")
            .open(&mut self.open)
            .resizable(egui::Vec2b::new(true, true))
            .show(ctx, |ui| self.widget.show(ui));
    }
}
