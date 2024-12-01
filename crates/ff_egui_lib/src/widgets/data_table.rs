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

    src/widgets/disk_info.rs

    Disk Info widget for displaying basic disk information.
*/
use crate::widgets::{data_visualizer::DataVisualizerWidget, tab_group::TabGroup};
use egui_extras::{Column, TableBuilder};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct DataTableWidget {
    num_columns: usize,
    num_rows: usize,
    scroll_to_row_slider: usize,
    scroll_to_row: Option<usize>,
    selection: std::collections::HashSet<usize>,
    checked: bool,
    reversed: bool,
    data: Vec<u8>,
    row_string_width: usize,
    tabs: TabGroup,
    viz_widget: Option<DataVisualizerWidget>,
}

impl Default for DataTableWidget {
    fn default() -> Self {
        Self {
            num_columns: 16,
            num_rows: 512 / 16,
            scroll_to_row_slider: 0,
            scroll_to_row: None,
            selection: Default::default(),
            checked: false,
            reversed: false,
            data: vec![0xFF; 512],
            row_string_width: 3,
            tabs: TabGroup::new().with_tab("hex").with_tab("text").with_tab("viz"),
            viz_widget: None,
        }
    }
}

impl DataTableWidget {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.tabs.show(ui);
        ui.separator();

        match self.tabs.selected_tab() {
            0 => {
                self.table_ui(ui, false);
            }
            1 => {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    self.text_ui(ui);
                });
            }
            2 => {
                if self.viz_widget.is_some() {
                    self.viz_ui(ui);
                }
                else {
                    let id = format!(
                        "data_table_{},{}",
                        ui.next_widget_position().x as u32,
                        ui.next_widget_position().y as u32
                    );
                    self.viz_widget = Some(DataVisualizerWidget::new(ui.ctx(), &id));
                    self.viz_ui(ui);
                }
            }
            _ => {}
        }
    }

    fn viz_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(viz_widget) = &mut self.viz_widget {
            let (_, start) = viz_widget.get_address();
            let start = start.min(self.data.len());

            let end = start + viz_widget.get_required_data_size();
            let end = end.min(self.data.len());

            let slice = &self.data[start..end];
            viz_widget.update_data(slice);

            viz_widget.show(ui);
        }
    }

    fn text_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            let strings = self.data_to_string();
            let available_height = ui.available_height();

            let text_height = egui::TextStyle::Body
                .resolve(ui.style())
                .size
                .max(ui.spacing().interact_size.y);

            let table = TableBuilder::new(ui)
                .striped(false)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
                .column(Column::initial(40.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height);

            table
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Line");
                    });
                    header.col(|ui| {
                        ui.strong("Text");
                    });
                })
                .body(|body| {
                    body.rows(text_height, strings.len(), |mut row| {
                        let row_index = row.index();
                        row.col(|ui| {
                            let formatted = format!("{}", row_index);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(formatted).monospace().strong())
                                        .selectable(false),
                                );
                            });
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&strings[row_index]).monospace());
                        });
                    });
                });
        });
    }

    fn table_ui(&mut self, ui: &mut egui::Ui, reset: bool) {
        ui.vertical(|ui| {
            use egui_extras::{Column, TableBuilder};

            let text_height = egui::TextStyle::Body
                .resolve(ui.style())
                .size
                .max(ui.spacing().interact_size.y);

            let available_height = ui.available_height();

            let mut table = TableBuilder::new(ui)
                .striped(false)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
                .column(Column::auto())
                .column(Column::auto())
                .column(Column::auto())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height);

            // if self.clickable {
            //     table = table.sense(egui::Sense::click());
            // }

            if let Some(row_index) = self.scroll_to_row.take() {
                table = table.scroll_to_row(row_index, None);
            }

            if reset {
                table.reset();
            }

            table
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Addr");
                    });
                    header.col(|ui| {
                        ui.strong("Hex View");
                    });
                    header.col(|ui| {
                        ui.strong("ASCII View");
                    });
                })
                .body(|body| {
                    body.rows(text_height, self.num_rows, |mut row| {
                        let row_index = row.index();
                        row.set_selected(self.selection.contains(&row_index));

                        row.col(|ui| {
                            let formatted = format!(
                                "{:0width$X}",
                                row_index * self.num_columns,
                                width = self.row_string_width
                            );
                            ui.label(egui::RichText::new(formatted).monospace());
                        });
                        row.col(|ui| {
                            ui.label(self.row_string_hex(row_index));
                        });
                        row.col(|ui| {
                            ui.label(self.row_string_ascii(row_index));
                        });

                        self.toggle_row_selection(row_index, &row.response());
                    });
                });
        });
    }

    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = data;
        self.calc_layout();
    }

    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    fn row_string_hex(&mut self, row_index: usize) -> egui::RichText {
        let data_index = row_index * self.num_columns;
        if data_index >= self.data.len() {
            return egui::RichText::new("");
        }
        let data_slice = &self.data[data_index..std::cmp::min(data_index + self.num_columns, self.data.len())];

        let mut row_string = String::new();
        for byte in data_slice {
            row_string.push_str(&format!("{:02X} ", byte));
        }

        egui::RichText::new(row_string).monospace()
    }

    fn row_string_ascii(&mut self, row_index: usize) -> egui::RichText {
        let data_index = row_index * self.num_columns;
        if data_index >= self.data.len() {
            return egui::RichText::new("");
        }
        let data_slice = &self.data[data_index..std::cmp::min(data_index + self.num_columns, self.data.len())];

        let mut row_string = String::new();
        for byte in data_slice {
            if *byte >= 0x20 && *byte <= 0x7E {
                row_string.push(*byte as char);
            }
            else {
                row_string.push('.');
            }
        }

        egui::RichText::new(row_string).monospace()
    }

    fn calc_layout(&mut self) {
        assert!(self.num_columns > 0, "num_columns must be greater than 0");

        // Calculate the number of rows, including a partial row
        let num_rows = self.data.len().div_ceil(self.num_columns);

        // Determine the required number of hex digits for row numbers
        let max_row_index = num_rows.saturating_sub(1);
        let required_hex_digits = ((max_row_index * self.num_columns) as f64).log(16.0).ceil() as usize;

        self.num_rows = num_rows;
        self.row_string_width = required_hex_digits;
    }

    fn toggle_row_selection(&mut self, row_index: usize, row_response: &egui::Response) {
        if row_response.clicked() {
            if self.selection.contains(&row_index) {
                self.selection.remove(&row_index);
            }
            else {
                self.selection.insert(row_index);
            }
        }
    }

    fn data_to_string(&self) -> Vec<String> {
        let converted_data = self
            .data
            .iter()
            .map(|byte| match byte {
                0x0A | 0x0D => *byte,
                0x20..0x7E => *byte,
                _ => 0x20,
            })
            .collect::<Vec<u8>>();

        let converted_string = String::from_utf8_lossy(&converted_data);

        // Split by Unix newlines (`\n`) and trim DOS carriage returns (`\r`)
        let strings: Vec<String> = converted_string
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect();

        strings
    }
}
