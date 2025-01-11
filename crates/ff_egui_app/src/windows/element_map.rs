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
use crate::{app::Tool, lock::TrackingLock};
use fluxfox::{source_map::SourceMap, DiskImage};
use fluxfox_egui::{widgets::source_map::SourceMapWidget, TrackSelection};
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct ElementMapViewer {
    pub open:   bool,
    pub widget: SourceMapWidget,
}

impl ElementMapViewer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            open:   false,
            widget: SourceMapWidget::new(),
        }
    }

    pub fn update(&mut self, disk_lock: TrackingLock<DiskImage>, selection: TrackSelection) {
        match disk_lock.read(Tool::TrackElementMap) {
            Ok(disk) => {
                if let Some(map) = disk.track(selection.phys_ch).and_then(|track| track.element_map()) {
                    self.widget.update_direct(map, None);
                }
            }
            Err(_) => {
                log::error!("Failed to lock disk image");
            }
        }
    }

    #[allow(dead_code)]
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn open_mut(&mut self) -> &mut bool {
        &mut self.open
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Track Element Map")
            .open(&mut self.open)
            .resizable(egui::Vec2b::new(true, true))
            .show(ctx, |ui| self.widget.show(ui));
    }
}
