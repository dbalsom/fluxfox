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

//! A widget for displaying pills representing the Cylinder-Head-Sector (CHS) addressing mode.

use crate::{widgets::pill::PillWidget, WidgetSize};

use fluxfox::prelude::{DiskCh, DiskChs, DiskChsn};

use egui::{Response, Widget};

#[derive(Clone, Default)]
pub struct ChsWidget {
    pub size: WidgetSize,
    pub head: u8,
    pub cylinder: u16,
    pub size_n: Option<u8>,
    pub size_bytes: Option<usize>,
    pub sector: Option<u8>,
}

impl ChsWidget {
    pub fn from_ch(ch: DiskCh) -> Self {
        Self {
            head: ch.h(),
            cylinder: ch.c(),
            ..Self::default()
        }
    }

    pub fn from_chs(chs: DiskChs) -> Self {
        Self {
            head: chs.h(),
            cylinder: chs.c(),
            sector: Some(chs.s()),
            ..Self::default()
        }
    }

    pub fn from_chsn(chsn: DiskChsn) -> Self {
        Self {
            head: chsn.h(),
            cylinder: chsn.c(),
            sector: Some(chsn.s()),
            size_n: Some(chsn.n()),
            size_bytes: Some(chsn.n_size()),
            ..Self::default()
        }
    }

    pub fn with_size(mut self, size: WidgetSize) -> Self {
        self.size = size;
        self
    }

    pub fn with_sector(mut self, sector: u8) -> Self {
        self.sector = Some(sector);
        self
    }

    pub fn with_n(mut self, n: u8) -> Self {
        self.size_n = Some(n);
        self.size_bytes = Some(DiskChsn::n_to_bytes(n));
        self
    }

    fn show(&self, ui: &mut egui::Ui) -> Response {
        // Get color from ui visuals
        let fill_color = ui.visuals().widgets.inactive.bg_fill;
        let text_color = ui.visuals().widgets.inactive.fg_stroke.color;

        // Attempt to center the pills vertically in whatever containing ui they are in?
        ui.horizontal(|ui| {
            //ui.allocate_ui_with_layout(ui.available_size(), Layout::left_to_right(Align::TOP), |ui| {

            // Cylinder pill
            ui.add(
                PillWidget::new(&format!("c: {}", self.cylinder))
                    .with_size(self.size)
                    .with_color(text_color)
                    .with_fill(fill_color),
            );

            // Head pill
            ui.add(
                PillWidget::new(&format!("h: {}", self.head))
                    .with_size(self.size)
                    .with_color(text_color)
                    .with_fill(fill_color),
            );

            // Sector pill (if present)
            if let Some(sector) = self.sector {
                ui.add(
                    PillWidget::new(&format!("s: {}", sector))
                        .with_size(self.size)
                        .with_color(text_color)
                        .with_fill(fill_color),
                );
            }

            // Size pill (if present)
            if let Some(sector) = self.size_n {
                ui.add(
                    PillWidget::new(&format!("n: {}", sector))
                        .with_size(self.size)
                        .with_color(text_color)
                        .with_fill(fill_color),
                );
            }
        })
        .response
    }
}

impl Widget for ChsWidget {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        self.show(ui)
    }
}
