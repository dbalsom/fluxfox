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
use egui::{Ui, Vec2b};
use egui_plot::{Line, MarkerShape, Plot, PlotPoints, Points};
use fluxfox::flux::pll::PllMarkerEntry;

#[derive(Default)]
pub struct TrackTimingChart {
    flux_times: Vec<f64>,
    markers: Vec<PllMarkerEntry>,
    draw_markers: bool,
}

impl TrackTimingChart {
    /// Create a new FluxTimingDiagram
    pub fn new(flux_times: &[f64], markers: Option<&[PllMarkerEntry]>) -> Self {
        Self {
            flux_times: flux_times.to_vec(),
            markers: markers.unwrap_or_default().to_vec(),
            draw_markers: true,
        }
    }

    pub fn marker_enable_mut(&mut self) -> &mut bool {
        &mut self.draw_markers
    }

    /// Draw the widget
    pub fn show(&self, ui: &mut Ui) {
        let mut points = Vec::new();
        let mut running_total = 0.0;

        for &flux_time in &self.flux_times {
            let ms = flux_time * 1e3; // Convert to milliseconds
            let us = flux_time * 1e6; // Convert to microseconds
            running_total += ms;
            points.push([running_total, us]);
        }

        let scatter = Points::new(PlotPoints::from(points))
            .color(egui::Color32::YELLOW) // LIGHT_YELLOW without transparency
            .shape(MarkerShape::Circle)
            .radius(1.5); // Set circle radius

        let mut markers = Vec::new();
        for marker in &self.markers {
            let x = marker.time * 1_000.0;
            markers.push(Line::new(PlotPoints::from(vec![[x, 0.0], [x, 10.0]])).color(egui::Color32::LIGHT_BLUE));
        }

        Plot::new("flux_timing_diagram")
            .x_axis_label("Time (ms)")
            .y_axis_label("Transition (µs)")
            .include_x(0.0)
            .include_y(0.0) // Pin y-axis min to 0
            .include_x(running_total)
            .include_y(10.0) // Pin y-axis max to 10
            .allow_scroll(Vec2b::new(false, false))
            .allow_zoom(Vec2b::new(true, false))
            .allow_drag(Vec2b::new(true, false))
            .allow_boxed_zoom(false)
            .auto_bounds(Vec2b::new(true, false))
            .show(ui, |plot_ui| {
                if self.draw_markers {
                    for marker_line in markers {
                        plot_ui.line(marker_line);
                    }
                }
                plot_ui.points(scatter)
            });
    }
}
