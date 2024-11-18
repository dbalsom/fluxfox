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
use crate::widgets::sector_status::sector_status;
use egui::ScrollArea;
use fluxfox::{track::TrackInfo, DiskCh, DiskImage, SectorMapEntry};

struct TrackListItem {
    ch: DiskCh,
    info: TrackInfo,
    sectors: Vec<SectorMapEntry>,
}

#[derive(Default)]
pub struct TrackListWidget {
    track_list: Vec<TrackListItem>,
}

impl TrackListWidget {
    pub fn new() -> Self {
        Self { track_list: Vec::new() }
    }

    pub fn update(&mut self, disk: &DiskImage) {
        for track in disk.track_iter() {
            self.track_list.push(TrackListItem {
                ch: track.ch(),
                info: track.info(),
                sectors: track.get_sector_list(),
            });
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let scroll_area = ScrollArea::vertical()
            .id_salt("track_list_scrollarea")
            .auto_shrink([false; 2]);

        ui.vertical(|ui| {
            ui.heading(egui::RichText::new("Track List").color(ui.visuals().strong_text_color()));

            scroll_area.show(ui, |ui| {
                ui.vertical(|ui| {
                    for (ti, track) in self.track_list.iter().enumerate() {
                        ui.group(|ui| {
                            ui.set_min_width(150.0);
                            ui.vertical(|ui| {
                                ui.heading(format!("Track {}", track.ch));
                                egui::Grid::new(format!("track_list_grid_{}", ti))
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.label("Encoding:");
                                        ui.label(format!("{}", track.info.encoding));
                                        ui.end_row();

                                        ui.label("Bitcells:");
                                        ui.label(format!("{}", track.info.bit_length));
                                        ui.end_row();
                                    });

                                ui.label("Sectors:");
                                egui::Grid::new(format!("track_list_sector_grid_{}", ti))
                                    .min_col_width(0.0)
                                    .show(ui, |ui| {
                                        for sector in &track.sectors {
                                            ui.vertical_centered(|ui| {
                                                ui.label(format!("{}", sector.chsn.s()));
                                                sector_status(ui, sector, true);
                                            });
                                        }
                                    });
                            });
                        });
                    }
                });
            });
        });
    }
}
