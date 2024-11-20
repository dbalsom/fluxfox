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
use crate::{widgets::track_list::TrackListWidget, SectorSelection, TrackListSelection, TrackSelection};
use egui::ScrollArea;
use fluxfox::{file_system::fat::fat::FatFileSystem, track::TrackInfo, DiskCh, DiskImage, SectorMapEntry};

#[derive(Clone)]
pub struct FileListItem {
    pub name: String,
    pub size: u64,
}

#[derive(Default)]
pub struct FileListWidget {
    file_list: Vec<FileListItem>,
}

impl FileListWidget {
    pub fn new() -> Self {
        Self { file_list: Vec::new() }
    }

    pub fn reset(&mut self) {
        self.file_list.clear();
    }

    pub fn update(&mut self, fs: &FatFileSystem) {
        self.file_list.clear();
        let files = fs.list_all_files();

        for file in files {
            log::debug!("FileListWidget::update(): file: {}", file);
            self.file_list.push(FileListItem {
                name: file.clone(),
                size: 0,
            });
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<FileListItem> {
        if self.file_list.is_empty() {
            return None;
        }

        let mut selected_file = None;

        ui.vertical(|ui| {
            ui.heading(egui::RichText::new("Filesystem").strong());

            let scroll_area = ScrollArea::vertical()
                .id_salt("file_list_scrollarea")
                .auto_shrink([false; 2]);

            scroll_area.show(ui, |ui| {
                ui.vertical(|ui| {
                    for file in self.file_list.iter() {
                        if ui.button(file.name.clone()).clicked() {
                            selected_file = Some(file.clone());
                        }
                    }
                });
            });
        });

        selected_file
    }
}
