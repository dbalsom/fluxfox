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

    src/widgets/boot_sector.rs

    Disk Info widget for displaying boot sector information, including the
    BIOS Parameter Block and the boot sector marker.
*/
use crate::{widgets::error_banner::ErrorBanner, UiEvent};
use fluxfox::{
    boot_sector::{BiosParameterBlock2, BiosParameterBlock3, BootSignature},
    prelude::*,
};

pub struct BootSectorWidget {
    pub loaded: bool,
    pub format: Option<StandardFormat>,
    pub pb2: BiosParameterBlock2,
    pub pb2_valid: bool,
    pub pb3: BiosParameterBlock3,
    pub sig: BootSignature,
}

impl Default for BootSectorWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BootSectorWidget {
    pub fn new() -> Self {
        Self {
            loaded: false,
            format: None,
            pb2: BiosParameterBlock2::default(),
            pb2_valid: false,
            pb3: BiosParameterBlock3::default(),
            sig: BootSignature::default(),
        }
    }

    pub fn update(&mut self, disk: &DiskImage) {
        if let Some(bs) = disk.boot_sector() {
            self.format = bs.standard_format();
            self.pb2 = bs.bpb2();
            self.pb2_valid = self.pb2.is_valid();
            self.pb3 = bs.bpb3();
            self.sig = bs.boot_signature();

            self.loaded = true;
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<UiEvent> {
        let new_event = None;

        ui.vertical(|ui| {
            if !self.loaded {
                ErrorBanner::new("No disk loaded").small().show(ui);
            }

            if let Some(format) = self.format {
                ui.label(format!("Disk format: {}", format));
            }
            else {
                ui.label("Disk format: Unknown");
                ui.label("Possible Booter disk");
            }

            egui::CollapsingHeader::new("BIOS Parameter Block v2").show(ui, |ui| {
                if !self.pb2_valid {
                    ErrorBanner::new("Invalid BPB v2!").small().show(ui);
                }

                egui::Grid::new("disk_bpb_grid").striped(true).show(ui, |ui| {
                    ui.label("Bytes per sector:");
                    ui.label(self.pb2.bytes_per_sector.to_string());
                    ui.end_row();

                    ui.label("Sectors per cluster:");
                    ui.label(self.pb2.sectors_per_cluster.to_string());
                    ui.end_row();

                    ui.label("Reserved sectors:");
                    ui.label(self.pb2.reserved_sectors.to_string());
                    ui.end_row();

                    ui.label("Number of FATs:");
                    ui.label(self.pb2.number_of_fats.to_string());
                    ui.end_row();

                    ui.label("Root Entries:");
                    ui.label(self.pb2.root_entries.to_string());
                    ui.end_row();

                    ui.label("Total Sectors:");
                    ui.label(self.pb2.total_sectors.to_string());
                    ui.end_row();

                    ui.label("Media descriptor:");
                    ui.label(format!("{:02X}", self.pb2.media_descriptor));
                    ui.end_row();

                    ui.label("Sectors per FAT:");
                    ui.label(self.pb2.sectors_per_fat.to_string());
                    ui.end_row();
                });
            });

            egui::CollapsingHeader::new("BIOS Parameter Block v3").show(ui, |ui| {
                egui::Grid::new("disk_bpb_grid").striped(true).show(ui, |ui| {
                    ui.label("Sectors per Track:");
                    ui.label(self.pb3.sectors_per_track.to_string());
                    ui.end_row();

                    ui.label("Number of heads:");
                    ui.label(self.pb3.number_of_heads.to_string());
                    ui.end_row();

                    ui.label("Hidden sectors:");
                    ui.label(self.pb3.hidden_sectors.to_string());
                    ui.end_row();
                });
            });

            egui::CollapsingHeader::new("Boot Sector Signature").show(ui, |ui| {
                let marker_text =
                    egui::RichText::new(format!("{:02X} {:02X}", self.sig.bytes()[0], self.sig.bytes()[1])).monospace();

                if !self.sig.is_valid() {
                    ErrorBanner::new("Invalid boot signature!").small().show(ui);
                }

                ui.label(marker_text);
            });
        });
        new_event
    }
}
