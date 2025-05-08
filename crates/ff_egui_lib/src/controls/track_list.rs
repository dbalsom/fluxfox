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

//! Module providing the [TrackListControl] for displaying a list of tracks on a disk image.
//!
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

pub const TRACK_ENTRY_WIDTH_DEFAULT: f32 = 480.0;
pub const SECTOR_STATUS_WRAP_DEFAULT: usize = 18;

#[derive(Copy, Clone, Debug, PartialEq, Default)]
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

#[derive(Clone)]
struct TrackListItem {
    ch: DiskCh,
    info: TrackInfo,
    sectors: Vec<SectorMapEntry>,
}

/// The [TrackListControlBuilder] should be used to construct a [TrackListControl] with the
/// desired options.
pub struct TrackListControlBuilder {
    draw_header_text: bool,
    track_menu: bool,
    view_sectors: bool,
    bitstream_track_color: Option<egui::Color32>,
    fluxstream_track_color: Option<egui::Color32>,
    metasector_track_color: Option<egui::Color32>,
    fixed_width: Option<f32>,
    sector_wrap: usize,
}

impl Default for TrackListControlBuilder {
    fn default() -> Self {
        Self {
            draw_header_text: true,
            track_menu: false,
            view_sectors: false,
            bitstream_track_color: None,
            fluxstream_track_color: None,
            metasector_track_color: None,
            fixed_width: Some(TRACK_ENTRY_WIDTH_DEFAULT),
            sector_wrap: SECTOR_STATUS_WRAP_DEFAULT,
        }
    }
}

impl TrackListControlBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify whether to draw the header text "Track List" at the top of the control.
    /// If you are embedding this in your own UI it might be redundant.
    pub fn with_header_text(mut self, draw: bool) -> Self {
        self.draw_header_text = draw;
        self
    }

    /// Specify whether to show the track menu dropdown in the header of each track.
    pub fn with_track_menu(mut self, show: bool) -> Self {
        self.track_menu = show;
        self
    }

    /// Specify whether the user can click the sector status icons to view the sector.
    /// This will also enable the "click me" hint text in the sector status popup.
    pub fn with_view_sectors(mut self, view: bool) -> Self {
        self.view_sectors = view;
        self
    }

    /// Specify the colors used for the various track type indicators.
    pub fn with_track_type_colors(
        mut self,
        bitstream: egui::Color32,
        fluxstream: egui::Color32,
        metasector: egui::Color32,
    ) -> Self {
        self.bitstream_track_color = Some(bitstream);
        self.fluxstream_track_color = Some(fluxstream);
        self.metasector_track_color = Some(metasector);
        self
    }

    /// Specify the fixed width of the track list entries. If None is specified, the track list
    /// entries will use all available horizontal space.
    pub fn with_fixed_width(mut self, width: Option<f32>) -> Self {
        self.fixed_width = width;
        self
    }

    /// Specify the number of sectors to show in each row of the sector status icon grid.
    /// The default wrapping value is 18 which corresponds to a high density 3.5" IBM floppy disk.
    pub fn with_sector_wrap(mut self, wrap: usize) -> Self {
        if self.sector_wrap > 0 {
            self.sector_wrap = wrap;
        }
        self
    }

    /// Build the [TrackListControl].
    pub fn build(self) -> TrackListControl {
        TrackListControl {
            heads: 2,
            head_filter: HeadFilter::default(),
            track_list: Vec::new(),
            draw_header_text: self.draw_header_text,
            track_menu: self.track_menu,
            view_sectors: self.view_sectors,
            bitstream_track_color: self.bitstream_track_color,
            fluxstream_track_color: self.fluxstream_track_color,
            metasector_track_color: self.metasector_track_color,
            fixed_width: self.fixed_width,
            sector_wrap: self.sector_wrap,
            scroll_to: None,
        }
    }
}

/// The [TrackListControl] is a vertically scrollable list of tracks on a disk image. It can be
/// filtered by head. Each track display shows the track type, number of sectors, and a grid of
/// sector status icons. The track type is color coded and the sector status icons are color coded.
/// The user can optionally click on a sector status icon to generate a sector selection event.
///
/// A [TrackListControl] must be built with [TrackListControlBuilder].
#[derive(Clone)]
pub struct TrackListControl {
    heads: u8,
    head_filter: HeadFilter,
    track_list: Vec<TrackListItem>,
    draw_header_text: bool,
    track_menu: bool,
    view_sectors: bool,
    bitstream_track_color: Option<egui::Color32>,
    fluxstream_track_color: Option<egui::Color32>,
    metasector_track_color: Option<egui::Color32>,
    fixed_width: Option<f32>,
    sector_wrap: usize,
    scroll_to: Option<DiskCh>,
}

impl TrackListControl {
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

    /// Request a scroll to the specified head/track. This will be processed on the next
    /// show() call. If the head is currently filtered, this method will have no effect.
    pub fn scroll_to(&mut self, ch: DiskCh) {
        match self.head_filter {
            HeadFilter::Zero if ch.h() == 1 => {
                return;
            }
            HeadFilter::One if ch.h() == 0 => {
                return;
            }
            _ => {}
        }
        self.scroll_to = Some(ch);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<TrackListSelection> {
        let mut new_selection = None;
        let mut new_selection2 = None;

        let scroll_area = ScrollArea::vertical()
            .id_salt("track_list_scrollarea")
            .auto_shrink([true, false]);

        ui.vertical(|ui| {
            if self.draw_header_text {
                ui.heading(egui::RichText::new("Track List").color(ui.visuals().strong_text_color()));
            }

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
                        let hg_response = HeaderGroup::new(&format!("{} Track", track.info.encoding))
                            .strong()
                            .show(
                                ui,
                                |ui| {
                                    ui.vertical(|ui| {
                                        if let Some(fixed_width) = self.fixed_width {
                                            ui.set_width(fixed_width);
                                        }
                                        egui::Grid::new(format!("track_list_grid_{}", ti)).striped(true).show(
                                            ui,
                                            |ui| match track.info.resolution {
                                                TrackDataResolution::FluxStream => {
                                                    egui::CollapsingHeader::new(
                                                        egui::RichText::new(format!(
                                                            "FluxStream Track: {} Bitcells",
                                                            track.info.bit_length
                                                        ))
                                                        .color(
                                                            self.fluxstream_track_color
                                                                .unwrap_or(ui.visuals().hyperlink_color),
                                                        ),
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
                                                        .color(
                                                            self.bitstream_track_color
                                                                .unwrap_or(ui.visuals().warn_fg_color),
                                                        ),
                                                    );
                                                    ui.end_row();
                                                }
                                                TrackDataResolution::MetaSector => {
                                                    ui.label(
                                                        egui::RichText::new("MetaSector Track").color(
                                                            self.metasector_track_color
                                                                .unwrap_or(ui.visuals().text_color()),
                                                        ),
                                                    );
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

                                                        if sector_status(ui, sector, true, self.view_sectors).clicked()
                                                        {
                                                            log::debug!("Sector clicked!");
                                                            new_selection =
                                                                Some(TrackListSelection::Sector(SectorSelection {
                                                                    phys_ch:    track.ch,
                                                                    sector_id:  sector.chsn,
                                                                    bit_offset: None,
                                                                }));
                                                        }
                                                    });

                                                    if si % self.sector_wrap == self.sector_wrap - 1 {
                                                        ui.end_row();
                                                    }
                                                }
                                            });
                                    });
                                },
                                Some(|ui: &mut egui::Ui, text| {
                                    ui.horizontal(|ui| {
                                        if let Some(fixed_width) = self.fixed_width {
                                            ui.set_width(fixed_width);
                                        }
                                        ui.heading(text);
                                        ui.add(ChsWidget::from_ch(track.ch));

                                        if self.track_menu {
                                            ui.menu_button("⏷", |ui| match track.info.resolution {
                                                TrackDataResolution::FluxStream => {
                                                    if ui.button("View Track Elements").clicked() {
                                                        new_selection2 =
                                                            Some(TrackListSelection::Track(TrackSelection {
                                                                sel_scope: TrackSelectionScope::Elements,
                                                                phys_ch:   track.ch,
                                                            }));
                                                        ui.close_menu();
                                                    }

                                                    if ui.button("View Track Data Stream").clicked() {
                                                        new_selection2 =
                                                            Some(TrackListSelection::Track(TrackSelection {
                                                                sel_scope: TrackSelectionScope::DecodedDataStream,
                                                                phys_ch:   track.ch,
                                                            }));
                                                        ui.close_menu();
                                                    }

                                                    if ui.button("View Track Flux Timings").clicked() {
                                                        new_selection2 =
                                                            Some(TrackListSelection::Track(TrackSelection {
                                                                sel_scope: TrackSelectionScope::Timings,
                                                                phys_ch:   track.ch,
                                                            }));
                                                        ui.close_menu();
                                                    }
                                                }
                                                TrackDataResolution::BitStream => {
                                                    if ui.button("View Track Elements").clicked() {
                                                        new_selection2 =
                                                            Some(TrackListSelection::Track(TrackSelection {
                                                                sel_scope: TrackSelectionScope::Elements,
                                                                phys_ch:   track.ch,
                                                            }));
                                                    }

                                                    if ui.button("View Track Data Stream").clicked() {
                                                        new_selection2 =
                                                            Some(TrackListSelection::Track(TrackSelection {
                                                                sel_scope: TrackSelectionScope::DecodedDataStream,
                                                                phys_ch:   track.ch,
                                                            }));
                                                    }
                                                }
                                                TrackDataResolution::MetaSector => {}
                                            });
                                        }
                                    });
                                }),
                            );
                        ui.add_space(8.0);

                        // if this was the group for the scroll target, scroll to it
                        if let Some(scroll_ch) = self.scroll_to {
                            if track.ch == scroll_ch {
                                hg_response.scroll_to_me(Some(egui::Align::TOP));
                                self.scroll_to = None;
                            }
                        }
                    }
                });
            });
        });

        new_selection.or(new_selection2)
    }
}
