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
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use fluxfox::{
    prelude::TrackDataResolution,
    visualization::{vectorize_disk_elements, CommonVizParams, RenderTrackMetadataParams, TurningDirection},
};
use fluxfox_egui::widgets::{disk_visualizer::DiskVisualizerWidget, error_banner::ErrorBanner};

pub const VIZ_RESOLUTION: u32 = 1024;

pub struct NewVizViewer {
    compatible: bool,
    viz: DiskVisualizerWidget,
    resolution: u32,

    show_data_layer: bool,
    show_metadata_layer: bool,
    show_error_layer: bool,
    show_weak_layer: bool,
    open: bool,
}

impl Default for NewVizViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl NewVizViewer {
    pub fn new() -> Self {
        Self {
            compatible: false,
            viz: DiskVisualizerWidget::new(VIZ_RESOLUTION, TurningDirection::Clockwise, 80),
            resolution: VIZ_RESOLUTION,
            open: false,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
        }
    }

    /// Reset, but don't destroy the visualization state
    pub fn reset(&mut self) {
        self.open = false;
    }

    pub fn init(&mut self, ctx: egui::Context, resolution: u32) {
        //self.viz = VisualizationState::new(ctx, resolution);
    }

    pub fn set_open(&mut self, state: bool) {
        self.open = state;
    }

    pub fn open_mut(&mut self) -> &mut bool {
        &mut self.open
    }

    pub fn render(&mut self, disk_lock: Arc<RwLock<DiskImage>>) -> Result<()> {
        {
            let disk = disk_lock.read().unwrap();

            self.compatible = !disk.resolution().contains(&TrackDataResolution::MetaSector);

            let common_viz_params = CommonVizParams {
                radius: Some(VIZ_RESOLUTION as f32 / 2.0),
                max_radius_ratio: 1.0,
                min_radius_ratio: 0.3,
                pos_offset: None,
                index_angle: 0.0,
                track_limit: None,
                pin_last_standard_track: true,
                track_gap: 0.1,
                absolute_gap: false,
                direction: TurningDirection::Clockwise,
            };

            let metadata_params = RenderTrackMetadataParams {
                quadrant: None,
                head: 0,
                draw_empty_tracks: false,
                draw_sector_lookup: false,
            };

            let display_list = match vectorize_disk_elements(&disk, &common_viz_params, &metadata_params) {
                Ok(display_list) => display_list,
                Err(e) => {
                    eprintln!("Error rendering metadata: {}", e);
                    std::process::exit(1);
                }
            };

            log::debug!("Updating visualization with {} elements", display_list.len());
            self.viz.update(display_list);
        }
        Ok(())
    }

    pub fn show(&mut self, ctx: &egui::Context, disk_lock: Arc<RwLock<DiskImage>>) {
        if self.open {
            egui::Window::new("New Disk Visualization")
                .open(&mut self.open)
                .show(ctx, |ui| {
                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("Layers", |ui| {
                            if ui.checkbox(&mut self.show_data_layer, "Data Layer").changed() {
                                //self.viz.enable_data_layer(self.show_data_layer);
                            }
                            if ui.checkbox(&mut self.show_metadata_layer, "Metadata Layer").changed() {
                                //self.viz.enable_metadata_layer(self.show_metadata_layer);
                            }
                            if ui.checkbox(&mut self.show_error_layer, "Error Layer").changed() {
                                //self.viz.set_error_layer(self.show_error_layer);
                            }
                            if ui.checkbox(&mut self.show_weak_layer, "Weak Layer").changed() {
                                //self.viz.set_weak_layer(self.show_weak_layer);
                            }
                        });

                        // ui.menu_button("Save", |ui| {
                        //     for side in 0..self.viz.sides {
                        //         if ui.button(format!("Save Side {} as PNG", side).as_str()).clicked() {
                        //             self.viz.save_side_as(&format!("fluxfox_viz_side{}.png", side), side);
                        //         }
                        //     }
                        // });
                    });

                    if self.compatible {
                        // if let Some(new_event) = self.viz.show(ui) {
                        //     match new_event {
                        //         VizEvent::NewSectorSelected { c, h, s_idx } => {
                        //             log::debug!("New sector selected: c:{} h:{}, s:{}", c, h, s_idx);
                        //
                        //             //self.viz.update_selection(disk_lock, c, h, s_idx);
                        //         }
                        //         _ => {}
                        //     }
                        // }

                        self.viz.show(ui);
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
