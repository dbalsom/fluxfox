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
use egui::{CursorIcon, Label, Sense};
use egui_extras::{Column, TableBuilder};
use fluxfox::file_system::FileEntry;

pub const GENERIC_FILE_ICON: &str = "🗋";

pub struct FileListWidget {
    file_list: Vec<FileEntry>,
    icon_map:  fluxfox::FoxHashMap<&'static str, &'static str>,
}

impl Default for FileListWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl FileListWidget {
    pub fn new() -> Self {
        log::warn!("FileListWidget::new()");
        Self {
            file_list: Vec::new(),
            icon_map:  FileListWidget::icon_map(),
        }
    }

    fn icon_map() -> fluxfox::FoxHashMap<&'static str, &'static str> {
        let mut map = fluxfox::FoxHashMap::new();

        let exe_icon = "🖥";
        let doc_icon = "🖹";
        let image_icon = "🖻";
        let audio_icon = "🔉";
        let archive_icon = "📚";
        map.insert("exe", exe_icon);
        map.insert("com", exe_icon);
        map.insert("bat", exe_icon);
        map.insert("sys", exe_icon);
        map.insert("dll", exe_icon);
        map.insert("doc", doc_icon);
        map.insert("txt", doc_icon);
        map.insert("pcx", image_icon);
        map.insert("iff", image_icon);
        map.insert("tga", image_icon);
        map.insert("bmp", image_icon);
        map.insert("jpg", image_icon);
        map.insert("gif", image_icon);
        map.insert("png", image_icon);
        map.insert("wav", audio_icon);
        map.insert("mp3", audio_icon);
        map.insert("arj", archive_icon);
        map.insert("zip", archive_icon);
        map.insert("lha", archive_icon);
        map.insert("lzh", archive_icon);
        map.insert("arc", archive_icon);
        map
    }

    pub fn reset(&mut self) {
        self.file_list.clear();
    }

    pub fn update(&mut self, files: &[FileEntry]) {
        self.file_list = files.to_vec();
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<FileEntry> {
        self.show_dir_table(ui)
    }

    fn show_dir_table(&self, ui: &mut egui::Ui) -> Option<FileEntry> {
        // if self.dir_list.is_empty() {
        //     return None;
        // }

        let mut selected_file = None;
        let num_rows = self.file_list.len();

        let text_height = egui::TextStyle::Body
            .resolve(ui.style())
            .size
            .max(ui.spacing().interact_size.y);

        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            let available_height = ui.available_height();
            //ui.set_min_height(available_height);
            let table = TableBuilder::new(ui)
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
                .column(Column::exact(110.0))
                .column(Column::exact(80.0))
                .column(Column::exact(80.0))
                .column(Column::exact(80.0))
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height);

            table
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Filename");
                    });
                    header.col(|ui| {
                        ui.strong("Size");
                    });
                    header.col(|ui| {
                        ui.strong("Modified Date");
                    });
                    header.col(|ui| {
                        ui.strong("Attributes");
                    });
                })
                .body(|body| {
                    body.rows(text_height, num_rows, |mut row| {
                        let row_index = row.index();
                        //row.set_selected(self.selection.contains(&row_index));

                        let icon = self.get_icon(&self.file_list[row_index]);

                        row.col(|ui| {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.label(icon);
                                if ui
                                    .add(
                                        Label::new(egui::RichText::new(&self.file_list[row_index].name).monospace())
                                            .sense(Sense::click()),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    selected_file = Some(self.file_list[row_index].clone());
                                    //log::debug!("Selected file: {:?}", selected_file);
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(self.file_list[row_index].size.to_string());
                            });
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                    });
                });
        });
        selected_file
    }

    fn get_icon(&self, entry: &FileEntry) -> String {
        if entry.is_dir() {
            "📁".to_string()
        }
        else {
            self.icon_map
                .get(entry.ext().unwrap_or("").to_ascii_lowercase().as_str())
                .map_or_else(|| GENERIC_FILE_ICON.to_string(), ToString::to_string)
        }
    }
}
