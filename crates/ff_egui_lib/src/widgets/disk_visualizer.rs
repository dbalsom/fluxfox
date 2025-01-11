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
use crate::visualization::{
    palette::default_palette,
    viz_elements::{paint_data, paint_elements},
    VizPalette,
};
use egui::{Pos2, Rect, Vec2};
use fluxfox::visualization::prelude::*;

pub struct DiskVisualizerWidget {
    pub open: bool,
    pub show_data_layer: bool,
    pub show_metadata_layer: bool,
    pub show_error_layer: bool,
    pub show_weak_layer: bool,
    pub resolution: Vec2,
    pub angle: f32,
    pub track_width: f32,
    pub palette: VizPalette,
    pub data_display_list: VizDataSliceDisplayList,
    pub metadata_display_list: VizElementDisplayList,
}

impl DiskVisualizerWidget {
    pub fn new(resolution: u32, turning_direction: TurningDirection, cylinders: usize) -> Self {
        Self {
            open: false,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
            resolution: Vec2::new(resolution as f32, resolution as f32),
            angle: 0.0,
            track_width: 1.0,
            palette: default_palette(),
            data_display_list: VizDataSliceDisplayList::new(turning_direction, cylinders, 0.0),
            metadata_display_list: VizElementDisplayList::new(turning_direction, 0, cylinders as u16),
        }
    }

    pub fn update_data(&mut self, display_list: VizDataSliceDisplayList) {
        self.data_display_list = display_list;
    }

    pub fn update_metadata(&mut self, display_list: VizElementDisplayList) {
        self.metadata_display_list = display_list;
    }

    pub fn angle_mut(&mut self) -> &mut f32 {
        &mut self.angle
    }

    pub fn show_metadata_layer_mut(&mut self) -> &mut bool {
        &mut self.show_metadata_layer
    }

    pub fn show_data_layer_mut(&mut self) -> &mut bool {
        &mut self.show_data_layer
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let (rect, _response) = ui.allocate_exact_size(self.resolution, egui::Sense::hover());
        let mut meta_painter = ui.painter().with_clip_rect(rect);

        // log::debug!(
        //     "DiskVisualizerWidget::show: rect: {:?} res: {:?}",
        //     rect,
        //     self.resolution
        // );

        let zoom = 1.0;

        // Create a transform to map from local (0,0) to rect.min
        let viz_rect = Rect::from_min_size(Pos2::ZERO, self.resolution / zoom);

        let to_screen = egui::emath::RectTransform::from_to(
            viz_rect, // Local space
            rect,     // Screen space
        );

        let rotation = VizRotation::new(
            self.angle,
            VizPoint2d::new(self.resolution.x / 2.0, self.resolution.y / 2.0),
        );

        let mut data_painter = meta_painter.with_clip_rect(rect);

        if self.show_metadata_layer {
            for track in &self.metadata_display_list.tracks {
                paint_elements(
                    &meta_painter,
                    &to_screen,
                    &rotation,
                    &self.palette,
                    track,
                    self.show_data_layer,
                );
            }
        }

        if self.show_data_layer {
            for (ti, track) in self.data_display_list.tracks.iter().enumerate() {
                // log::debug!(
                //     "DiskVisualizerWidget::show: painting data on track: {}, elements: {}",
                //     ti,
                //     track.len()
                // );
                paint_data(
                    &data_painter,
                    &to_screen,
                    &rotation,
                    track,
                    self.data_display_list.track_width,
                    self.show_metadata_layer,
                );
            }
        }
    }
}
