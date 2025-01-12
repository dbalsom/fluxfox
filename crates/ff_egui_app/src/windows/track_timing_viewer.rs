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
use crate::windows::track_viewer::TrackViewer;
use fluxfox::{flux::pll::PllMarkerEntry, prelude::DiskCh};
use fluxfox_egui::widgets::{data_table::DataTableWidget, track_timing_chart::TrackTimingChart};

#[derive(Default)]
pub struct TrackTimingViewer {
    chart:   TrackTimingChart,
    phys_ch: DiskCh,
    open:    bool,
}

impl TrackTimingViewer {
    #[allow(dead_code)]
    pub fn new(phys_ch: DiskCh, fts: &[f64], markers: Option<&[PllMarkerEntry]>) -> Self {
        Self {
            chart: TrackTimingChart::new(fts, markers),
            phys_ch,
            open: false,
        }
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn update(&mut self, phys_ch: DiskCh, fts: &[f64], markers: Option<&[PllMarkerEntry]>) {
        self.phys_ch = phys_ch;
        self.chart = TrackTimingChart::new(fts, markers);
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Track Timings").open(&mut self.open).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(format!("Physical Track: {}", self.phys_ch));
                ui.checkbox(self.chart.marker_enable_mut(), "Show Markers");
                ui.separator();
                self.chart.show(ui);
            });
        });
    }
}
