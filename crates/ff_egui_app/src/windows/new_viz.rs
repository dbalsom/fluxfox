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
use crate::{
    app::Tool,
    lock::TrackingLock,
    widgets::viz::{VisualizationState, VizEvent},
};
use anyhow::Result;
use fluxfox::{prelude::TrackDataResolution, visualization::prelude::*, DiskImage};
use fluxfox_egui::widgets::{disk_visualizer::DiskVisualizerWidget, error_banner::ErrorBanner};
use std::{
    collections::HashMap,
    f32::consts::TAU,
    sync::{Arc, RwLock},
};

pub const VIZ_RESOLUTION: u32 = 768;

pub struct NewVizViewer {
    disk: Option<TrackingLock<DiskImage>>,
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
            disk: None,
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

    pub fn set_disk(&mut self, disk: TrackingLock<DiskImage>) {
        self.disk = Some(disk);
    }

    pub fn render(&mut self) -> Result<()> {
        if self.disk.is_none() {
            return Ok(());
        }
        let disk = self.disk.as_ref().unwrap().read(Tool::NewViz).unwrap();

        self.compatible = !disk.resolution().contains(&TrackDataResolution::MetaSector);

        let common_viz_params = CommonVizParams {
            radius: Some(VIZ_RESOLUTION as f32 / 2.0),
            max_radius_ratio: 1.0,
            min_radius_ratio: 0.3,
            pos_offset: None,
            index_angle: 0.0,
            track_limit: None,
            pin_last_standard_track: true,
            track_gap: 0.0,
            direction: TurningDirection::Clockwise,
        };

        let metadata_params = RenderTrackMetadataParams {
            quadrant: None,
            geometry: RenderGeometry::Arc,
            winding: Default::default(),
            side: 0,
            draw_empty_tracks: false,
            draw_sector_lookup: false,
        };

        let display_list = vectorize_disk_elements_by_quadrants(&disk, &common_viz_params, &metadata_params)?;

        log::debug!("Updating visualization with {} elements", display_list.len());
        self.viz.update_metadata(display_list);

        let data_params = RenderTrackDataParams {
            side: 0,
            decode: false,
            sector_mask: false,
            resolution: Default::default(),
            slices: 360,
            overlap: -0.10,
        };

        let vector_params = RenderVectorizationParams {
            view_box: Default::default(),
            image_bg_color: None,
            disk_bg_color: None,
            mask_color: None,
            pos_offset: None,
        };

        let data_display_list = vectorize_disk_data(&disk, &common_viz_params, &data_params, &vector_params)?;

        self.viz.update_data(data_display_list);

        Ok(())
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if self.open {
            egui::Window::new("New Disk Visualization")
                .open(&mut self.open)
                .show(ctx, |ui| {
                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("Layers", |ui| {
                            if ui.checkbox(self.viz.show_data_layer_mut(), "Data Layer").changed() {
                                //self.viz.enable_data_layer(self.show_data_layer);
                            }
                            if ui
                                .checkbox(self.viz.show_metadata_layer_mut(), "Metadata Layer")
                                .changed()
                            {
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

                    ui.horizontal(|ui| {
                        ui.set_min_width(200.0);
                        ui.add(
                            egui::Slider::new(self.viz.angle_mut(), 0.0..=TAU)
                                .text("Angle")
                                .step_by((TAU / 360.0) as f64),
                        );
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
