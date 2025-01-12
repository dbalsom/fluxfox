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
use egui::Grid;
use fluxfox::file_system::fat::fat_fs::FatFileSystem;
use fluxfox_egui::widgets::data_table::DataTableWidget;

#[derive(Default)]
pub struct FileViewer {
    path: String,
    table: DataTableWidget,
    open: bool,
    error_string: Option<String>,
}

impl FileViewer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            path: String::new(),
            table: DataTableWidget::default(),
            open: false,
            error_string: None,
        }
    }

    pub fn update(&mut self, fs: &FatFileSystem, path: String) {
        self.path = path;

        let data = match fs.read_file(&self.path) {
            Ok(data) => data,
            Err(e) => {
                self.error_string = Some(format!("Error reading file: {}", e));
                return;
            }
        };

        self.table.set_data(&data);
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("File Viewer").open(&mut self.open).show(ctx, |ui| {
            Grid::new("file_viewer_grid").striped(true).show(ui, |ui| {
                ui.label("Path:");
                ui.label(self.path.to_string());
                ui.end_row();

                ui.label("Size:");
                ui.label(self.table.data_len().to_string());
                ui.end_row();
            });
            ui.separator();
            self.table.show(ui);
        });
    }
}
