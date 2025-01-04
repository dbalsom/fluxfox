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
use crate::visualization::{palette::default_palette, viz_elements::paint_elements, VizPalette};
use egui::{Pos2, Rect, Vec2};
use fluxfox::visualization::{TurningDirection, VizElementDisplayList};

pub struct DiskVisualizerWidget {
    pub open: bool,
    pub show_data_layer: bool,
    pub show_metadata_layer: bool,
    pub show_error_layer: bool,
    pub show_weak_layer: bool,
    pub resolution: Vec2,
    pub palette: VizPalette,
    pub metadata_display_list: VizElementDisplayList,
}

impl DiskVisualizerWidget {
    pub fn new(resolution: u32, turning_direction: TurningDirection) -> Self {
        Self {
            open: false,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
            resolution: Vec2::new(resolution as f32, resolution as f32),
            palette: default_palette(),
            metadata_display_list: VizElementDisplayList::new(turning_direction),
        }
    }

    pub fn update(&mut self, display_list: VizElementDisplayList) {
        self.metadata_display_list = display_list;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let (rect, _response) = ui.allocate_exact_size(self.resolution, egui::Sense::hover());
        let painter = ui.painter();

        log::debug!(
            "DiskVisualizerWidget::show: rect: {:?} res: {:?}",
            rect,
            self.resolution
        );

        // Create a transform to map from local (0,0) to rect.min
        let viz_rect = Rect::from_min_size(Pos2::ZERO, self.resolution);
        let to_screen = egui::emath::RectTransform::from_to(
            viz_rect, // Local space
            rect,     // Screen space
        );

        if self.show_metadata_layer {
            for track in &self.metadata_display_list.tracks {
                paint_elements(painter, &to_screen, &self.palette, track);
            }
        }
    }
}
