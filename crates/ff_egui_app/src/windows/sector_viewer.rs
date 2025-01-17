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
use crate::{app::Tool, lock::TrackingLock};
use fluxfox::{
    prelude::*,
    types::{IntegrityCheck, IntegrityField, ReadSectorResult},
};
use fluxfox_egui::{
    controls::{data_table::DataTableWidget, error_banner::ErrorBanner},
    widgets::{chs::ChsWidget, pill::PillWidget},
    SectorSelection,
};

#[derive(Default)]
pub struct SectorViewer {
    phys_ch:   DiskCh,
    sector_id: SectorId,

    table: DataTableWidget,
    open: bool,
    valid: bool,
    error_string: Option<String>,
    read_result: Option<ReadSectorResult>,
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
            read_result: None,
        }
    }

    pub fn update(&mut self, disk_lock: TrackingLock<DiskImage>, selection: SectorSelection) {
        match disk_lock.write(Tool::SectorViewer) {
            Ok(mut disk) => {
                self.phys_ch = selection.phys_ch;
                let query = SectorIdQuery::new(
                    selection.sector_id.c(),
                    selection.sector_id.h(),
                    selection.sector_id.s(),
                    selection.sector_id.n(),
                );

                log::debug!("Reading sector: {:?}", query);
                let rsr = match disk.read_sector(self.phys_ch, query, None, None, RwScope::DataOnly, true) {
                    Ok(rsr) => rsr,
                    Err(e) => {
                        log::error!("Error reading sector: {:?}", e);
                        self.error_string = Some(e.to_string());
                        self.valid = false;
                        return;
                    }
                };

                self.read_result = Some(rsr.clone());

                if rsr.not_found {
                    self.error_string = Some(format!("Sector {} not found", selection.sector_id));
                    self.table.set_data(&[0; 512]);
                    self.valid = false;
                    return;
                }

                // When is id_chsn None after a successful read?
                if let Some(chsn) = rsr.id_chsn {
                    self.sector_id = chsn;
                    self.table.set_data(&rsr.read_buf[rsr.data_range]);
                    self.error_string = None;
                    self.valid = true;
                }
                else {
                    self.error_string = Some("Sector ID not returned".to_string());
                    self.table.set_data(&[0; 512]);
                    self.valid = false;
                }
            }
            Err(e) => {
                for tool in e {
                    log::warn!("Failed to acquire write lock, locked by tool: {:?}", tool);
                }
                self.error_string = Some("Failed to acquire disk write lock.".to_string());
                self.valid = false;
            }
        }
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Sector Viewer").open(&mut self.open).show(ctx, |ui| {
            ui.vertical(|ui| {
                if let Some(error_string) = &self.error_string {
                    ErrorBanner::new(error_string).small().show(ui);
                }
                egui::Grid::new("sector_viewer_grid").show(ui, |ui| {
                    ui.label("Physical Track:");
                    ui.add(ChsWidget::from_ch(self.phys_ch));
                    ui.end_row();

                    ui.label("Sector ID:");
                    ui.add(ChsWidget::from_chsn(self.sector_id));
                    ui.end_row();

                    if let Some(rsr) = &self.read_result {
                        ui.label("Sector Size:");
                        ui.label(format!("{} bytes", rsr.data_range.len()));
                        ui.end_row();

                        if let Some(check) = rsr.data_crc {
                            let (valid, recorded, calculated) = match check {
                                IntegrityCheck::Crc16(IntegrityField {
                                    valid,
                                    recorded,
                                    calculated,
                                }) => {
                                    ui.label("CRC16:");
                                    (valid, recorded, calculated)
                                }
                                IntegrityCheck::Checksum16(IntegrityField {
                                    valid,
                                    recorded,
                                    calculated,
                                }) => {
                                    ui.label("Checksum16:");
                                    (valid, recorded, calculated)
                                }
                            };

                            if let Some(recorded_val) = recorded {
                                ui.label("Recorded:");
                                ui.add(PillWidget::new(&format!("{:04X}", recorded_val)).with_fill(if valid {
                                    egui::Color32::DARK_GREEN
                                }
                                else {
                                    egui::Color32::DARK_RED
                                }));
                            }
                            else {
                                ui.add(
                                    PillWidget::new(if valid { "Valid" } else { "Invalid" }).with_fill(if valid {
                                        egui::Color32::DARK_GREEN
                                    }
                                    else {
                                        egui::Color32::DARK_RED
                                    }),
                                );
                            }

                            ui.end_row();
                            ui.label("");
                            ui.label("Calculated:");
                            ui.label(format!("{:04X}", calculated));
                            ui.end_row();
                        }
                    }
                });

                ui.separator();
                self.table.show(ui);
            });
        });
    }
}
