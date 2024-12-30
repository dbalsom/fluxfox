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

    src/widgets/disk_info.rs

    Disk Info widget for displaying basic disk information.
*/

use fluxfox::prelude::*;

#[derive(Default)]
pub struct DiskInfoWidget {
    pub filename: Option<String>,
    pub platforms: Option<Vec<Platform>>,
    pub resolution: Vec<TrackDataResolution>,
    pub geometry: DiskCh,
    pub rate: TrackDataRate,
    pub encoding: TrackDataEncoding,
    pub density: TrackDensity,
}

impl DiskInfoWidget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, disk: &DiskImage, filename: Option<String>) {
        self.filename = filename;
        self.platforms = disk.image_format().platforms.clone();
        self.resolution = disk.resolution();
        self.geometry = disk.geometry();
        self.rate = disk.data_rate();
        self.encoding = disk.data_encoding();
        self.density = disk.image_format().density;
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            egui::Grid::new("disk_info_grid").striped(true).show(ui, |ui| {
                // Filename can be very long (think TDC title names) - maybe there is a better place to
                // display the filename.

                // ui.label("Filename:");
                // ui.label(self.filename.as_ref().unwrap_or(&"None".to_string()));
                // ui.end_row();

                if let Some(platforms) = &self.platforms {
                    if platforms.len() > 1 {
                        ui.label("Multi-platform:");
                        ui.end_row();
                        for (i, platform) in platforms.iter().enumerate() {
                            ui.label(format!("[{}]", i));
                            ui.label(format!("{}", platform));
                            ui.end_row();
                        }
                    }
                    else if !platforms.is_empty() {
                        ui.label("Platform:");
                        ui.label(format!("{}", platforms[0]));
                        ui.end_row();
                    }
                }

                if self.resolution.len() > 1 {
                    ui.label("Multi-resolution:");
                    ui.end_row();
                    for (i, resolution) in self.resolution.iter().enumerate() {
                        ui.label(format!("[{}]", i));
                        ui.label(format!("{:?}", resolution));
                        ui.end_row();
                    }
                }
                else {
                    ui.label("Resolution:");
                    ui.label(format!("{:?}", self.resolution[0]));
                    ui.end_row();
                }

                ui.label("Geometry:");
                ui.horizontal(|ui| {
                    ui.label(format!("Heads: {}", self.geometry.h()));
                    ui.label(format!("Cylinders: {}", self.geometry.c()));
                });
                ui.end_row();

                ui.label("Data Rate:");
                ui.label(format!("{}", self.rate));
                ui.end_row();

                ui.label("Data Encoding:");
                ui.label(format!("{:?}", self.encoding).to_uppercase());
                ui.end_row();

                ui.label("Density:");
                ui.label(format!("{:?}", self.density));
                ui.end_row();
            });
        });
    }
}
