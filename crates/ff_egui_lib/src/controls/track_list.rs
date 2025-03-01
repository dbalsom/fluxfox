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
    controls::{header_group::HeaderGroup, sector_status::sector_status},
    widgets::chs::ChsWidget,
    SectorSelection,
    TrackListSelection,
    TrackSelection,
    TrackSelectionScope,
};
use egui::{ScrollArea, TextStyle};
use fluxfox::{prelude::*, track::TrackInfo};

pub const TRACK_ENTRY_WIDTH: f32 = 480.0;
pub const SECTOR_STATUS_WRAP: usize = 18;

#[derive(PartialEq, Default)]
pub enum HeadFilter {
    Zero,
    One,
    #[default]
    Both,
}

impl HeadFilter {
    pub fn predicate(&self, ch: DiskCh) -> bool {
        match self {
            HeadFilter::Zero => ch.h() == 0,
            HeadFilter::One => ch.h() == 1,
            HeadFilter::Both => true,
        }
    }
}

struct TrackListItem {
    ch: DiskCh,
    info: TrackInfo,
    sectors: Vec<SectorMapEntry>,
}

#[derive(Default)]
pub struct TrackListWidget {
    heads: u8,
    head_filter: HeadFilter,
    track_list: Vec<TrackListItem>,
}

impl TrackListWidget {
    pub fn new() -> Self {
        Self {
            heads: 2,
            head_filter: HeadFilter::default(),
            track_list: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.track_list.clear();
    }

    pub fn update(&mut self, disk: &DiskImage) {
        self.track_list.clear();

        self.heads = disk.heads();
        if self.heads == 1 {
            self.head_filter = HeadFilter::Both;
        }

        for track in disk.track_iter() {
            self.track_list.push(TrackListItem {
                ch: track.ch(),
                info: track.info(),
                sectors: track.sector_list(),
            });
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<TrackListSelection> {
        let mut new_selection = None;
        let mut new_selection2 = None;

        let scroll_area = ScrollArea::vertical()
            .id_salt("track_list_scrollarea")
            .auto_shrink([true, false]);

        ui.vertical(|ui| {
            ui.heading(egui::RichText::new("Track List").color(ui.visuals().strong_text_color()));
            if self.heads > 1 {
                ui.horizontal(|ui| {
                    ui.label("Show heads:");
                    if ui
                        .add(egui::RadioButton::new(self.head_filter == HeadFilter::Both, "Both"))
                        .clicked()
                    {
                        self.head_filter = HeadFilter::Both;
                    }
                    if ui
                        .add(egui::RadioButton::new(self.head_filter == HeadFilter::Zero, "Head 0"))
                        .clicked()
                    {
                        self.head_filter = HeadFilter::Zero;
                    }
                    if ui
                        .add(egui::RadioButton::new(self.head_filter == HeadFilter::One, "Head 1"))
                        .clicked()
                    {
                        self.head_filter = HeadFilter::One;
                    }
                });
            }

            scroll_area.show(ui, |ui| {
                ui.vertical(|ui| {
                    for (ti, track) in self
                        .track_list
                        .iter()
                        .filter(|tli| self.head_filter.predicate(tli.ch))
                        .enumerate()
                    {
                        HeaderGroup::new(&format!("{} Track", track.info.encoding))
                            .strong()
                            .show(
                                ui,
                                |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(TRACK_ENTRY_WIDTH);
                                        egui::Grid::new(format!("track_list_grid_{}", ti)).striped(true).show(
                                            ui,
                                            |ui| match track.info.resolution {
                                                TrackDataResolution::FluxStream => {
                                                    egui::CollapsingHeader::new(
                                                        egui::RichText::new(format!(
                                                            "FluxStream Track: {} Bitcells",
                                                            track.info.bit_length
                                                        ))
                                                        .color(ui.visuals().hyperlink_color),
                                                    )
                                                    .id_salt(format!("fluxstream_trk{}", ti))
                                                    .default_open(false)
                                                    .show(
                                                        ui,
                                                        |ui| {
                                                            if let Some(flux_info) = &track.info.flux_info {
                                                                egui::Grid::new(format!("fluxstream_trk_grid_{}", ti))
                                                                    .striped(true)
                                                                    .show(ui, |ui| {
                                                                        ui.label("Revolutions:");
                                                                        ui.label(format!("{}", flux_info.revolutions));
                                                                        ui.end_row();
                                                                        ui.label("Flux transitions:");
                                                                        ui.label(format!(
                                                                            "{}",
                                                                            flux_info.transitions
                                                                                [flux_info.best_revolution]
                                                                        ));
                                                                        ui.end_row();
                                                                        ui.label("Bitcells:");
                                                                        ui.label(format!("{}", track.info.bit_length));
                                                                        ui.end_row();
                                                                    });
                                                            };
                                                        },
                                                    );
                                                }
                                                TrackDataResolution::BitStream => {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "BitStream Track: {} Bitcells",
                                                            track.info.bit_length
                                                        ))
                                                        .color(ui.visuals().warn_fg_color),
                                                    );
                                                    ui.end_row();
                                                }
                                                TrackDataResolution::MetaSector => {
                                                    ui.label("MetaSector Track");
                                                    ui.end_row();
                                                }
                                            },
                                        );

                                        ui.label(format!("{} Sectors:", track.sectors.len()));
                                        egui::Grid::new(format!("track_list_sector_grid_{}", ti))
                                            .min_col_width(0.0)
                                            .show(ui, |ui| {
                                                let mut previous_id: Option<u8> = None;
                                                for (si, sector) in track.sectors.iter().enumerate() {
                                                    ui.vertical_centered(|ui| {
                                                        let sid = sector.chsn.s();
                                                        let consecutive_sector = match previous_id {
                                                            Some(prev) => {
                                                                sid != u8::MAX && sid == prev.saturating_add(1)
                                                            }
                                                            None => sid == 1,
                                                        };
                                                        previous_id = Some(sid);

                                                        let label_height = TextStyle::Body.resolve(ui.style()).size; // Use the normal label height for consistency
                                                        let small_size = TextStyle::Small.resolve(ui.style()).size;
                                                        let padding = ui.spacing().item_spacing.y;

                                                        ui.vertical(|ui| {
                                                            ui.set_min_height(label_height + padding);
                                                            ui.allocate_ui_with_layout(
                                                                egui::Vec2::new(
                                                                    ui.available_width(),
                                                                    label_height + padding,
                                                                ),
                                                                egui::Layout::centered_and_justified(
                                                                    egui::Direction::TopDown,
                                                                ),
                                                                |ui| {
                                                                    if sid > 99 {
                                                                        let mut text =
                                                                            egui::RichText::new(format!("{:03}", sid))
                                                                                .size(small_size);
                                                                        if !consecutive_sector {
                                                                            text =
                                                                                text.color(ui.visuals().warn_fg_color);
                                                                        }
                                                                        ui.label(text);
                                                                    }
                                                                    else {
                                                                        let mut text =
                                                                            egui::RichText::new(format!("{}", sid));
                                                                        if !consecutive_sector {
                                                                            text =
                                                                                text.color(ui.visuals().warn_fg_color);
                                                                        }
                                                                        ui.label(text);
                                                                    }
                                                                },
                                                            );
                                                        });

                                                        if sector_status(ui, sector, true).clicked() {
                                                            log::debug!("Sector clicked!");
                                                            new_selection =
                                                                Some(TrackListSelection::Sector(SectorSelection {
                                                                    phys_ch:    track.ch,
                                                                    sector_id:  sector.chsn,
                                                                    bit_offset: None,
                                                                }));
                                                        }
                                                    });

                                                    if si % SECTOR_STATUS_WRAP == SECTOR_STATUS_WRAP - 1 {
                                                        ui.end_row();
                                                    }
                                                }
                                            });
                                    });
                                },
                                Some(|ui: &mut egui::Ui, text| {
                                    ui.horizontal(|ui| {
                                        ui.set_width(TRACK_ENTRY_WIDTH);
                                        ui.heading(text);
                                        ui.add(ChsWidget::from_ch(track.ch));
                                        ui.menu_button("⏷", |ui| match track.info.resolution {
                                            TrackDataResolution::FluxStream => {
                                                if ui.button("View Track Elements").clicked() {
                                                    new_selection2 = Some(TrackListSelection::Track(TrackSelection {
                                                        sel_scope: TrackSelectionScope::Elements,
                                                        phys_ch:   track.ch,
                                                    }));
                                                    ui.close_menu();
                                                }

                                                if ui.button("View Track Data Stream").clicked() {
                                                    new_selection2 = Some(TrackListSelection::Track(TrackSelection {
                                                        sel_scope: TrackSelectionScope::DecodedDataStream,
                                                        phys_ch:   track.ch,
                                                    }));
                                                    ui.close_menu();
                                                }

                                                if ui.button("View Track Flux Timings").clicked() {
                                                    new_selection2 = Some(TrackListSelection::Track(TrackSelection {
                                                        sel_scope: TrackSelectionScope::Timings,
                                                        phys_ch:   track.ch,
                                                    }));
                                                    ui.close_menu();
                                                }
                                            }
                                            TrackDataResolution::BitStream => {
                                                if ui.button("View Track Elements").clicked() {
                                                    new_selection2 = Some(TrackListSelection::Track(TrackSelection {
                                                        sel_scope: TrackSelectionScope::Elements,
                                                        phys_ch:   track.ch,
                                                    }));
                                                }

                                                if ui.button("View Track Data Stream").clicked() {
                                                    new_selection2 = Some(TrackListSelection::Track(TrackSelection {
                                                        sel_scope: TrackSelectionScope::DecodedDataStream,
                                                        phys_ch:   track.ch,
                                                    }));
                                                }
                                            }
                                            TrackDataResolution::MetaSector => {}
                                        });
                                    });

                                    // ui.set_max_width(TRACK_ENTRY_WIDTH - 8.0);
                                    // ui.allocate_ui_with_layout(
                                    //     egui::Vec2::new(ui.available_width(), ui.available_height()),
                                    //     egui::Layout::right_to_left(egui::Align::TOP),
                                    //     |ui| {
                                    //         ui.add(ChsWidget::from_ch(track.ch));
                                    //         ui.menu_button("⏷", |ui| match track.info.resolution {
                                    //             TrackDataResolution::FluxStream => {
                                    //                 if ui.button("View Track Elements").clicked() {
                                    //                     new_selection2 =
                                    //                         Some(TrackListSelection::Track(TrackSelection {
                                    //                             sel_scope: TrackSelectionScope::Elements,
                                    //                             phys_ch:   track.ch,
                                    //                         }));
                                    //                     ui.close_menu();
                                    //                 }
                                    //
                                    //                 if ui.button("View Track Data Stream").clicked() {
                                    //                     new_selection2 =
                                    //                         Some(TrackListSelection::Track(TrackSelection {
                                    //                             sel_scope: TrackSelectionScope::DecodedDataStream,
                                    //                             phys_ch:   track.ch,
                                    //                         }));
                                    //                     ui.close_menu();
                                    //                 }
                                    //
                                    //                 if ui.button("View Track Flux Timings").clicked() {
                                    //                     new_selection2 =
                                    //                         Some(TrackListSelection::Track(TrackSelection {
                                    //                             sel_scope: TrackSelectionScope::Timings,
                                    //                             phys_ch:   track.ch,
                                    //                         }));
                                    //                     ui.close_menu();
                                    //                 }
                                    //             }
                                    //             TrackDataResolution::BitStream => {
                                    //                 if ui.button("View Track Elements").clicked() {
                                    //                     new_selection2 =
                                    //                         Some(TrackListSelection::Track(TrackSelection {
                                    //                             sel_scope: TrackSelectionScope::Elements,
                                    //                             phys_ch:   track.ch,
                                    //                         }));
                                    //                 }
                                    //
                                    //                 if ui.button("View Track Data Stream").clicked() {
                                    //                     new_selection2 =
                                    //                         Some(TrackListSelection::Track(TrackSelection {
                                    //                             sel_scope: TrackSelectionScope::DecodedDataStream,
                                    //                             phys_ch:   track.ch,
                                    //                         }));
                                    //                 }
                                    //             }
                                    //             TrackDataResolution::MetaSector => {}
                                    //         });
                                    //     },
                                    // );
                                }),
                            );
                        ui.add_space(8.0);
                    }
                });
            });
        });

        new_selection.or(new_selection2)
    }
}
