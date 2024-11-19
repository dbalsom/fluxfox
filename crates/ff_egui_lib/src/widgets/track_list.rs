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
use crate::{widgets::sector_status::sector_status, SectorSelection, TrackListSelection};
use egui::{Pos2, Rect, Rounding, ScrollArea, Stroke, TextStyle, Ui, Vec2};
use fluxfox::{track::TrackInfo, DiskCh, DiskImage, SectorMapEntry};

pub const TRACK_ENTRY_WIDTH: f32 = 420.0;
pub const SECTOR_STATUS_WRAP: usize = 16;

struct TrackListItem {
    ch: DiskCh,
    info: TrackInfo,
    sectors: Vec<SectorMapEntry>,
}

#[derive(Default)]
pub struct TrackListWidget {
    track_list: Vec<TrackListItem>,
}

impl TrackListWidget {
    pub fn new() -> Self {
        Self { track_list: Vec::new() }
    }

    pub fn reset(&mut self) {
        self.track_list.clear();
    }

    pub fn update(&mut self, disk: &DiskImage) {
        self.track_list.clear();
        for track in disk.track_iter() {
            self.track_list.push(TrackListItem {
                ch: track.ch(),
                info: track.info(),
                sectors: track.get_sector_list(),
            });
        }
    }

    fn custom_group(ui: &mut Ui, heading: &str, content: impl FnOnce(&mut Ui)) {
        // Add some margin space for the group
        let margin = ui.style().spacing.window_margin;

        // Begin the group
        let response = ui.scope(|ui| {
            ui.horizontal(|ui| {
                ui.add_space(margin.left); // Left margin
                ui.vertical(|ui| {
                    // Paint the heading
                    ui.add_space(margin.top); // Top margin
                    ui.heading(heading);
                    ui.add_space(margin.top); // Top margin

                    // Draw the custom content
                    content(ui);
                    ui.add_space(margin.bottom); // Bottom margin
                });
            });
        });

        // Get the rect for the entire group we just created
        let group_rect = response.response.rect;

        // Paint the header background
        if ui.is_rect_visible(group_rect) {
            let header_height = ui.fonts(|fonts| fonts.row_height(&TextStyle::Heading.resolve(&ui.style())));
            let header_rect = Rect::from_min_size(
                group_rect.min,
                Vec2::new(group_rect.width(), header_height + margin.top * 2.0), // Include padding for margins
            );
            let painter = ui.painter();
            let visuals = ui.visuals();
            let bg_color = visuals.faint_bg_color.gamma_multiply(1.2); // Slightly brighter than the default background
            let rounding = Rounding {
                nw: 4.0, // Top-left corner
                ne: 4.0, // Top-right corner
                sw: 0.0, // Bottom-left corner
                se: 0.0, // Bottom-right corner
            };
            painter.rect_filled(header_rect, rounding, bg_color);

            // Draw a light line at the bottom of the header
            let line_color = visuals.widgets.inactive.bg_fill; // Use a lighter color for the line
            let line_stroke = Stroke::new(1.0, line_color);
            let bottom_line_start = Pos2::new(header_rect.min.x, header_rect.max.y);
            let bottom_line_end = Pos2::new(header_rect.max.x, header_rect.max.y);
            painter.line_segment([bottom_line_start, bottom_line_end], line_stroke);
        }

        // Draw the overall border for the group
        if ui.is_rect_visible(group_rect) {
            let painter = ui.painter();
            let visuals = ui.visuals();
            let border_color = visuals.widgets.noninteractive.bg_stroke.color;
            let stroke = Stroke::new(1.0, border_color);
            let rounding = Rounding::same(6.0); // Overall group rounding
            painter.rect_stroke(group_rect, rounding, stroke);
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<TrackListSelection> {
        let mut new_selection = None;

        let scroll_area = ScrollArea::vertical()
            .id_salt("track_list_scrollarea")
            .auto_shrink([false; 2]);

        ui.vertical(|ui| {
            //ui.heading(egui::RichText::new("Track List").color(ui.visuals().strong_text_color()));
            ui.heading(egui::RichText::new("Track List").strong());

            scroll_area.show(ui, |ui| {
                ui.vertical(|ui| {
                    for (ti, track) in self.track_list.iter().enumerate() {
                        Self::custom_group(ui, &format!("{} Track {}", track.info.encoding, track.ch), |ui| {
                            ui.vertical(|ui| {
                                ui.set_min_width(TRACK_ENTRY_WIDTH);
                                egui::Grid::new(format!("track_list_grid_{}", ti))
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.label("Bitcells:");
                                        ui.label(format!("{}", track.info.bit_length));
                                        ui.end_row();
                                    });

                                ui.label(format!("Sectors ({}):", track.sectors.len()));
                                egui::Grid::new(format!("track_list_sector_grid_{}", ti))
                                    .min_col_width(0.0)
                                    .show(ui, |ui| {
                                        let mut previous_id: Option<u8> = None;
                                        for sector in track.sectors.iter() {
                                            ui.vertical_centered(|ui| {
                                                let sid = sector.chsn.s();
                                                let consecutive_sector = match previous_id {
                                                    Some(prev) => sid == prev + 1,
                                                    None => sid == 1,
                                                };
                                                previous_id = Some(sid);

                                                let label_height = TextStyle::Body.resolve(&ui.style()).size; // Use the normal label height for consistency
                                                let small_size = TextStyle::Small.resolve(&ui.style()).size;
                                                let padding = ui.spacing().item_spacing.y;

                                                ui.vertical(|ui| {
                                                    ui.set_min_height(label_height + padding);
                                                    ui.allocate_ui_with_layout(
                                                        egui::Vec2::new(ui.available_width(), label_height + padding),
                                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                                        |ui| {
                                                            if sid > 99 {
                                                                let mut text =
                                                                    egui::RichText::new(format!("{:03}", sid))
                                                                        .size(small_size);
                                                                if !consecutive_sector {
                                                                    text = text.color(ui.visuals().warn_fg_color);
                                                                }
                                                                ui.label(text);
                                                            }
                                                            else {
                                                                let mut text = egui::RichText::new(format!("{}", sid));
                                                                if !consecutive_sector {
                                                                    text = text.color(ui.visuals().warn_fg_color);
                                                                }
                                                                ui.label(text);
                                                            }
                                                        },
                                                    );
                                                });

                                                if sector_status(ui, sector, true).clicked() {
                                                    log::debug!("Sector clicked!");
                                                    new_selection = Some(TrackListSelection::Sector(SectorSelection {
                                                        phys_ch:    track.ch,
                                                        sector_id:  sector.chsn,
                                                        bit_offset: None,
                                                    }));
                                                }
                                            });
                                        }
                                    });
                            });
                        });
                        ui.add_space(8.0);
                    }
                });
            });
        });

        new_selection
    }
}
