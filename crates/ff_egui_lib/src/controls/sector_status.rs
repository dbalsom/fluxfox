/*
    fluxfox
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

    Implements a custom control that displays a sector status indicator.

*/

use crate::widgets::{chs::ChsWidget, pill::PillWidget};
use egui::*;
use fluxfox::SectorMapEntry;

//let pal_medium_green = Color::from_rgba8(0x38, 0xb7, 0x64, 0xff);
const COLOR_SECTOR_OK: Color32 = Color32::from_rgb(0x38, 0xb7, 0x64);
const COLOR_BAD_CRC: Color32 = Color32::from_rgb(0xef, 0x7d, 0x57);
const COLOR_DELETED_DATA: Color32 = Color32::from_rgb(0x25, 0x71, 0x79);
//const COLOR_BAD_DELETED_DATA: Color32 = Color32::from_rgb(0xb1, 0x3e, 0x53);
const COLOR_BAD_HEADER: Color32 = Color32::RED;
const COLOR_NO_DAM: Color32 = Color32::GRAY;

/// Simple color swatch widget. Used for palette register display.
pub fn sector_status(ui: &mut Ui, entry: &SectorMapEntry, open: bool) -> Response {
    let size = ui.spacing().interact_size;
    let size = Vec2 { x: size.y, y: size.y }; // Make square
    let (rect, response) = ui.allocate_exact_size(
        size,
        Sense {
            click: true,
            drag: false,
            focusable: false,
        },
    );
    //response.widget_info(|| WidgetInfo::new(WidgetType::ColorButton));

    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    ui.spacing_mut().button_padding = vec2(0.0, 0.0);

    if ui.is_rect_visible(rect) {
        let visuals = if open {
            &ui.visuals().widgets.open
        }
        else {
            ui.style().interact(&response)
        };
        //let rect = rect.expand(visuals.expansion);

        //painter.rect_filled(rect, 0.0, color);

        let rounding = visuals.rounding.at_most(2.0);

        // pub chsn: DiskChsn,
        // pub address_crc_valid: bool,
        // pub data_crc_valid: bool,
        // pub deleted_mark: bool,
        // pub no_dam: bool,
        let color = match (
            !entry.attributes.address_error,
            !entry.attributes.data_error,
            entry.attributes.deleted_mark,
            entry.attributes.no_dam,
        ) {
            (true, true, false, false) => COLOR_SECTOR_OK,
            (true, true, true, _) => COLOR_DELETED_DATA,
            (true, false, _, _) => COLOR_BAD_CRC,
            (true, true, false, true) => COLOR_NO_DAM,
            (false, _, _, _) => COLOR_BAD_HEADER,
        };

        ui.painter().rect_filled(rect, rounding, color);
        ui.painter().rect_stroke(rect, rounding, (2.0, visuals.bg_fill)); // fill is intentional, because default style has no border
    }

    // We don't use hovered_ui as it implements a delay.
    if response.hovered() {
        show_tooltip(
            &response.ctx,
            ui.layer_id(),
            response.id.with("sector_attributes_tooltip"),
            |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Click square to view sector").italics());
                    Grid::new("popup_sector_attributes_grid").show(ui, |ui| {
                        ui.label("ID");
                        ui.add(ChsWidget::from_chsn(entry.chsn));
                        ui.end_row();

                        ui.label("Size");
                        ui.label(entry.chsn.n_size().to_string());
                        ui.end_row();

                        let good_color = Color32::DARK_GREEN;
                        let bad_color = Color32::DARK_RED;

                        ui.label("Address integrity");
                        match entry.attributes.address_error {
                            true => ui.add(PillWidget::new("Bad").with_fill(bad_color)),
                            false => ui.add(PillWidget::new("Good").with_fill(good_color)),
                        };
                        ui.end_row();

                        ui.label("Data integrity");
                        match entry.attributes.data_error {
                            true => ui.add(PillWidget::new("Bad").with_fill(bad_color)),
                            false => ui.add(PillWidget::new("Good").with_fill(good_color)),
                        };
                        ui.end_row();

                        ui.label("Sector type");
                        match entry.attributes.deleted_mark {
                            true => ui.add(PillWidget::new("Deleted data").with_fill(bad_color)),
                            false => ui.label("Normal data"),
                        };
                        ui.end_row();
                    });
                });
            },
        );
    }
    response
}
