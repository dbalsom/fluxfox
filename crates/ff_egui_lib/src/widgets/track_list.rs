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
use fluxfox::{track::TrackInfo, DiskCh, DiskImage};

struct TrackListItem {
    ch:   DiskCh,
    info: TrackInfo,
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
                ch:   track.ch(),
                info: track.info(),
            });
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Track List");
            ui.separator();
            ui.vertical(|ui| {
                for track in &self.track_list {
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.heading(format!("Track {}", track.ch));
                            ui.label(format!("Encoding: {}", track.info.encoding));
                            ui.label(format!("Bitcells: {}", track.info.bit_length));
                        });
                    });
                }
            });
        });
    }
}
