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
use fluxfox::prelude::*;
use fluxfox_egui::{
    widgets::data_table::{DataRange, DataTableWidget},
    TrackSelection,
};
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct TrackViewer {
    phys_ch: DiskCh,

    table: DataTableWidget,
    open: bool,
    valid: bool,
    error_string: Option<String>,
}

impl TrackViewer {
    #[allow(dead_code)]
    pub fn new(phys_ch: DiskCh) -> Self {
        Self {
            phys_ch,
            table: DataTableWidget::default(),
            open: false,
            valid: false,
            error_string: None,
        }
    }

    pub fn update(&mut self, disk_lock: TrackingLock<DiskImage>, selection: TrackSelection) {
        let disk = &mut disk_lock.write(Tool::TrackViewer).unwrap();

        self.phys_ch = selection.phys_ch;

        let track_ref = match disk.track_mut(selection.phys_ch) {
            Some(tr) => tr,
            None => {
                self.error_string = Some("Invalid track index".to_string());
                self.valid = false;
                return;
            }
        };

        let rtr = match track_ref.read(None) {
            Ok(rtr) => rtr,
            Err(e) => {
                log::error!("Error reading sector: {:?}", e);
                self.error_string = Some(e.to_string());
                self.valid = false;
                return;
            }
        };

        if rtr.not_found {
            self.error_string = Some("Track not found".to_string());
            self.valid = false;
            return;
        }

        self.table.set_data(&rtr.read_buf);
        self.valid = true;

        if let Some(metadata) = track_ref.metadata() {
            for item in metadata.marker_ranges() {
                let range = DataRange {
                    name: "Marker".to_string(),
                    range: (item.0 / 16)..(item.1 / 16).saturating_sub(1),
                    fg_color: egui::Color32::from_rgb(0x53, 0xdd, 0xff),
                };
                self.table.add_range(range);
            }
        };

        // test setting a range
        // let range = DataRange {
        //     name: "Test".to_string(),
        //     start: 3,
        //     end: 7,
        //     fg_color: egui::Color32::from_rgb(0, 0, 255),
        // };

        //self.table.add_range(range);
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Track Viewer").open(&mut self.open).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(format!("Physical Track: {}", self.phys_ch));

                self.table.show(ui);
            });
        });
    }
}
