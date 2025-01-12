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
use crate::widgets::viz::{VisualizationState, VizEvent};
use fluxfox::DiskImage;
use std::sync::{Arc, RwLock};

use crate::lock::TrackingLock;
use anyhow::Result;
use fluxfox_egui::widgets::error_banner::ErrorBanner;

pub struct VizViewer {
    viz: VisualizationState,

    show_data_layer: bool,
    show_metadata_layer: bool,
    show_error_layer: bool,
    show_weak_layer: bool,
    open: bool,
}

impl Default for VizViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl VizViewer {
    pub fn new() -> Self {
        Self {
            viz: VisualizationState::default(),
            open: false,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
        }
    }

    /// Reset, but don't destroy the visualization state
    pub fn reset(&mut self) {
        //self.viz.clear();

        self.open = false;
    }

    pub fn init(&mut self, ctx: egui::Context, resolution: u32) {
        self.viz = VisualizationState::new(ctx, resolution);
    }

    pub fn set_open(&mut self, state: bool) {
        self.open = state;
    }

    pub fn open_mut(&mut self) -> &mut bool {
        &mut self.open
    }

    pub fn update_disk(&mut self, disk_lock: TrackingLock<DiskImage>) {
        self.viz.update_disk(disk_lock);
        _ = self.render()
    }

    pub fn render(&mut self) -> Result<()> {
        self.viz.render_visualization(0)?;
        self.viz.render_visualization(1)?;
        Ok(())
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if self.open {
            egui::Window::new("Disk Visualization")
                .open(&mut self.open)
                .show(ctx, |ui| {
                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("Layers", |ui| {
                            if ui.checkbox(&mut self.show_data_layer, "Data Layer").changed() {
                                self.viz.enable_data_layer(self.show_data_layer);
                            }
                            if ui.checkbox(&mut self.show_metadata_layer, "Metadata Layer").changed() {
                                self.viz.enable_metadata_layer(self.show_metadata_layer);
                            }
                            if ui.checkbox(&mut self.show_error_layer, "Error Layer").changed() {
                                //self.viz.set_error_layer(self.show_error_layer);
                            }
                            if ui.checkbox(&mut self.show_weak_layer, "Weak Layer").changed() {
                                //self.viz.set_weak_layer(self.show_weak_layer);
                            }
                        });

                        ui.menu_button("Save", |ui| {
                            for side in 0..self.viz.sides {
                                #[cfg(not(feature = "svg"))]
                                if ui.button(format!("Save Side {} as PNG", side).as_str()).clicked() {
                                    self.viz
                                        .save_side_as_svg(&format!("fluxfox_viz_side{}.png", side), side);
                                }
                                #[cfg(feature = "svg")]
                                ui.menu_button(format!("Save Side {} as...", side).as_str(), |ui| {
                                    if ui.button("PNG").clicked() {
                                        self.viz
                                            .save_side_as_png(&format!("fluxfox_viz_side{}.png", side), side);
                                        ui.close_menu();
                                    }
                                    if ui.button("SVG").clicked() {
                                        match self
                                            .viz
                                            .save_side_as_svg(&format!("fluxfox_viz_side{}.svg", side), side)
                                        {
                                            Ok(_) => {
                                                log::info!("SVG saved successfully");
                                            }
                                            Err(e) => {
                                                log::error!("Error saving SVG: {}", e);
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            }
                        });

                        ui.menu_button("Zoom", |ui| {
                            for side in 0..self.viz.sides {
                                ui.label(format!("Side {} Zoom", side));
                                ui.group(|ui| {
                                    egui::Grid::new(format!("side{}_zoom_grid", side)).show(ui, |ui| {
                                        if ui.button("1").clicked() {
                                            self.viz.set_quadrant(side, Some(1));
                                        }
                                        if ui.button("0").clicked() {
                                            self.viz.set_quadrant(side, Some(0));
                                        }
                                        ui.end_row();
                                        if ui.button("2").clicked() {
                                            self.viz.set_quadrant(side, Some(2));
                                        }
                                        if ui.button("3").clicked() {
                                            self.viz.set_quadrant(side, Some(3));
                                        }
                                        ui.end_row();
                                    });
                                    if ui.button("Reset").on_hover_text("Reset zoom").clicked() {
                                        self.viz.set_quadrant(side, None);
                                    }
                                });
                            }
                        });
                    });

                    if self.viz.compatible {
                        if let Some(new_event) = self.viz.show(ui) {
                            match new_event {
                                VizEvent::NewSectorSelected { c, h, s_idx } => {
                                    log::debug!("New sector selected: c:{} h:{}, s:{}", c, h, s_idx);

                                    self.viz.update_selection(c, h, s_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    else {
                        ErrorBanner::new("Visualization not compatible with current disk image.")
                            .medium()
                            .show(ui);
                    }
                });
        }
    }
}
