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
use fluxfox::file_system::{fat::fat::FatFileSystem, FileTreeNode};
use fluxfox_egui::widgets::{dir_tree::DirTreeWidget, file_list::FileListWidget, path_selection::PathSelectionWidget};
use std::cell::Cell;

pub struct FileSystemWidget {
    tree_widget: DirTreeWidget,
    list_widget: FileListWidget,
    path_selection_widget: PathSelectionWidget,
    path_selection: Option<String>,
    file_selection: Option<String>,
    new_file_selection: Cell<bool>,
    tree: FileTreeNode,
}

impl Default for FileSystemWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemWidget {
    pub fn new() -> Self {
        Self {
            tree_widget: DirTreeWidget::new(),
            list_widget: FileListWidget::new(),
            path_selection_widget: PathSelectionWidget::default(),
            path_selection: None,
            file_selection: None,
            new_file_selection: Cell::new(false),
            tree: FileTreeNode::default(),
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        *self = FileSystemWidget { ..Default::default() }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("File System");
            self.path_selection_widget
                .set(self.path_selection.as_ref().map(|x| x.as_str()));
            if let Some(selected_path) = self.path_selection_widget.show(ui) {
                self.update_selection(Some(selected_path));
            }
            ui.separator();
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                if let Some(selected_path) = self.tree_widget.show(ui) {
                    self.update_selection(Some(selected_path));
                }
                ui.separator();
                if let Some(selected_file) = self.list_widget.show(ui) {
                    log::debug!("Selected file: {:?}", selected_file);

                    self.file_selection = Some(selected_file.path.clone());
                    self.new_file_selection.set(true);
                }
            });
        });
    }

    pub fn new_file_selection(&self) -> Option<String> {
        if self.new_file_selection.get() {
            self.new_file_selection.set(false);
            self.file_selection.clone()
        }
        else {
            None
        }
    }

    fn update_selection(&mut self, new_selection: Option<String>) {
        log::debug!("Updating selection: {:?}", new_selection);
        self.list_widget.reset();
        if let Some(path) = new_selection.clone() {
            // Read the directory
            let files = self.tree.dir(&path).unwrap_or_else(|| {
                log::error!("Failed to read directory: {}", path);
                Vec::new()
            });

            self.list_widget.update(&files);
            self.tree_widget.set_selection(new_selection.clone());
        }
        self.path_selection = new_selection;
    }

    pub fn update(&mut self, fs: &FatFileSystem) {
        self.tree = fs.build_file_tree_from_root().unwrap_or_else(|| {
            log::error!("Failed to build filesystem tree!");
            FileTreeNode::default()
        });

        self.tree_widget.update(self.tree.clone());
        self.update_selection(Some("/".to_string()));
    }
}
