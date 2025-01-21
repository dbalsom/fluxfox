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

use fluxfox::{
    prelude::*,
    track::{DiskTrack, TrackInfo},
    track_schema::{TrackElementInstance, TrackSchema},
};
use fluxfox_egui::{
    controls::data_table::{DataRange, DataTableWidget},
    tracking_lock::TrackingLock,
    widgets::chs::ChsWidget,
    TrackSelection,
    UiLockContext,
};
use std::ops::Range;

#[derive(Default)]
pub struct TrackViewer {
    disk: Option<TrackingLock<DiskImage>>,
    phys_ch: DiskCh,
    track: Option<DiskTrack>,
    track_info: TrackInfo,
    markers: Vec<TrackElementInstance>,
    marker_sync: usize,
    table: DataTableWidget,
    open: bool,
    valid: bool,
    error_string: Option<String>,
}

impl TrackViewer {
    #[allow(dead_code)]
    pub fn new(phys_ch: DiskCh) -> Self {
        Self {
            disk: None,
            phys_ch,
            track: None,
            track_info: TrackInfo::default(),
            // Capacity of 37 markers (18 sectors * 2 (IDAM + DAM) + IAM)
            markers: Vec::with_capacity(38),
            marker_sync: 0,
            table: DataTableWidget::default(),
            open: false,
            valid: false,
            error_string: None,
        }
    }

    pub fn update_disk(&mut self, disk: TrackingLock<DiskImage>) {
        self.disk = Some(disk);
        self.phys_ch = DiskCh::default();
        self.track = None;
        self.markers = Vec::new();
        self.track_info = TrackInfo::default();
    }

    pub fn update_selection(&mut self, selection: TrackSelection) {
        self.marker_sync = 0;
        self.error_string = None;

        if let Some(disk_lock) = &self.disk {
            let disk = disk_lock.read(UiLockContext::TrackViewer).unwrap();

            self.phys_ch = selection.phys_ch;

            let track_ref = match disk.track(selection.phys_ch) {
                Some(tr) => tr,
                None => {
                    self.error_string = Some("Invalid track index".to_string());
                    self.valid = false;
                    return;
                }
            };

            self.track_info = track_ref.info();
            // Take a clone of the track reference so we can re-read the track at different offsets
            // without having to re-acquire the lock.
            self.track = Some(track_ref.clone());
        }

        self.scan_track();
        self.read_track(0);
    }

    // Scan the track for markers.
    fn scan_track(&mut self) {
        if let Some(track) = &self.track {
            if let Some(metadata) = track.metadata() {
                self.markers = metadata.markers();
                log::debug!("scan_track(): Found {} markers", self.markers.len());
            }
        }
    }

    fn read_track(&mut self, offset: isize) {
        if let Some(track) = &self.track {
            let rtr = match track.read(Some(offset), None) {
                Ok(rtr) => rtr,
                Err(e) => {
                    log::error!("Error reading sector: {:?}", e);
                    self.error_string = Some(e.to_string());
                    self.valid = false;
                    return;
                }
            };

            if rtr.not_found {
                self.error_string = Some("Unexpected error: Track not found(?)".to_string());
                self.valid = false;
                return;
            }

            self.table.set_data(&rtr.read_buf);
            self.valid = true;

            if let Some(metadata) = track.metadata() {
                for item in metadata.header_ranges() {
                    //let offset_range = (item.start + offset as usize)..(item.end + offset as usize);

                    let range = DataRange {
                        name: "Sector Header".to_string(),
                        range: (item.start / 16)..(item.end / 16).saturating_sub(1),
                        fg_color: egui::Color32::from_rgb(0xff, 0x53, 0x53),
                    };
                    self.table.add_range(range);
                }

                for item in metadata.marker_ranges() {
                    //let offset_range = (item.start + offset as usize)..(item.end + offset as usize);

                    let range = DataRange {
                        name: "Marker".to_string(),
                        range: (item.start / 16)..(item.end / 16).saturating_sub(1),
                        fg_color: egui::Color32::from_rgb(0x53, 0xdd, 0xff),
                    };
                    self.table.add_range(range);
                }
            };
        }
    }

    fn decompose_header_range(&self, range: Range<usize>) {}

    fn sync_to(&mut self, marker_start: usize) {
        // Marker offset is modulo 16 for FM and MFM.
        match self.track_info.encoding {
            TrackDataEncoding::Fm | TrackDataEncoding::Mfm => {
                let offset = marker_start % 16;
                self.read_track(offset as isize);
            }
            _ => {
                self.error_string = Some("Unsupported encoding".to_string());
                self.valid = false;
            }
        }
    }

    pub fn open_mut(&mut self) -> &mut bool {
        &mut self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    // Show the window
    pub fn show(&mut self, ctx: &egui::Context) {
        // Separate open state to avoid borrowing self
        let mut open = self.open;
        // Show the window
        egui::Window::new("Track Viewer").open(&mut open).show(ctx, |ui| {
            self.ui(ui);
        });
        // Sync open state
        self.set_open(open);
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            egui::Grid::new("track_viewer_grid").num_columns(4).show(ui, |ui| {
                ui.label("Physical Track:");
                ui.add(ChsWidget::from_ch(self.phys_ch));
                ui.end_row();

                ui.label("Sync to bitcell:");
                egui::ComboBox::new("track_viewer_combo", "")
                    .selected_text(format!("{}", self.marker_sync))
                    .show_ui(ui, |ui| {
                        let mut sync_to_opt = None;
                        if ui.selectable_value(&mut self.marker_sync, 0, "Track Start").clicked() {
                            sync_to_opt = Some(0);
                        }
                        for marker in &self.markers {
                            if ui
                                .selectable_value(
                                    &mut self.marker_sync,
                                    marker.range().start,
                                    format!("Marker @ {}", marker.range().start),
                                )
                                .clicked()
                            {
                                sync_to_opt = Some(marker.range().start);
                            }
                        }

                        if let Some(marker_start) = sync_to_opt {
                            self.sync_to(marker_start);
                        }
                    });

                ui.label("Offset:");
                ui.label(format!("{}", self.marker_sync % 16));
                ui.end_row();
            });

            self.table.show(ui);
        });
    }
}
