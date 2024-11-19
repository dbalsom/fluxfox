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

use fluxfox::{diskimage::RwSectorScope, DiskCh, DiskImage, SectorId, SectorIdQuery};
use fluxfox_egui::{widgets::data_table::DataTableWidget, SectorSelection};

#[derive(Default)]
pub struct SectorViewer {
    phys_ch:   DiskCh,
    sector_id: SectorId,

    table: DataTableWidget,
    open: bool,
    valid: bool,
    error_string: Option<String>,
}

impl SectorViewer {
    #[allow(dead_code)]
    pub fn new(phys_ch: DiskCh, sector_id: SectorId) -> Self {
        Self {
            phys_ch,
            sector_id,

            table: DataTableWidget::default(),
            open: false,
            valid: false,
            error_string: None,
        }
    }

    pub fn update(&mut self, disk: &mut DiskImage, selection: SectorSelection) {
        self.phys_ch = selection.phys_ch;
        let query = SectorIdQuery::new(
            selection.sector_id.c(),
            selection.sector_id.h(),
            selection.sector_id.s(),
            selection.sector_id.n(),
        );
        let rsr = match disk.read_sector(self.phys_ch, query, None, None, RwSectorScope::DataOnly, false) {
            Ok(rsr) => rsr,
            Err(e) => {
                eprintln!("Error reading sector: {:?}", e);
                self.error_string = Some(e.to_string());
                self.valid = false;
                return;
            }
        };

        if rsr.not_found {
            self.error_string = Some("Sector not found".to_string());
            self.valid = false;
            return;
        }

        self.sector_id = rsr.id_chsn.unwrap();
        self.table.set_data(rsr.read_buf);
        self.valid = true;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Sector Viewer").open(&mut self.open).show(ctx, |ui| {
            ui.label(format!("Physical Track: {}", self.phys_ch));
            ui.label(format!("Sector ID: {}", self.sector_id));

            self.table.ui(ui);
        });
    }
}
