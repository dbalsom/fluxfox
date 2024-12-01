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
use crate::{
    widgets::{dir_tree::DirTreeWidget, file_list::FileListWidget, path_selection::PathSelectionWidget},
    UiEvent,
};
use fluxfox::file_system::{fat::fat::FatFileSystem, FileTreeNode};
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

    /// Show the file system widget
    /// This widget comprises a header, a path selection widget, then a horizontal ui with
    /// a directory tree on the left and a file list on the right.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<UiEvent> {
        let mut new_event = None;
        ui.vertical(|ui| {
            // Show the header. We may eventually want to make this a separate widget...
            ui.heading(egui::RichText::new("File System").strong());
            ui.separator();

            // Make sure the path selection widget is set to the current path.
            self.path_selection_widget.set(self.path_selection.as_deref());

            // Next, show the path selection widget and process any resulting event.
            // Currently, it can only return a SelectPath event so we can unwrap it directly.
            if let Some(UiEvent::SelectPath(selected_path)) = self.path_selection_widget.show(ui) {
                self.update_selection(Some(selected_path));
            }
            ui.separator();

            // Start a horizontal UI layout to place the tree and file list widgets side-by-side
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                // Show the directory tree widget and process any resulting event from it.
                if let Some(event) = self.tree_widget.show(ui) {
                    log::debug!("Got event from tree widget: {:?}", event);
                    match event {
                        UiEvent::SelectPath(path) => {
                            log::debug!("Got selected path from tree widget: {:?}", path);
                            self.update_selection(Some(path));
                        }
                        _ => {
                            log::debug!("Got unhandled event from tree widget: {:?}", event);
                            new_event = Some(event);
                        }
                    }
                }

                // Draw vertical separator between tree and file list
                ui.separator();

                // Show the file list widget and process any resulting event from it.
                if let Some(event) = self.list_widget.show(ui) {
                    //log::debug!("Selected file: {:?}", event);
                    match &event {
                        UiEvent::SelectFile(entry) => {
                            self.file_selection = Some(entry.path.clone());
                            self.new_file_selection.set(true);
                        }
                        _ => {
                            // We shouldn't be able ot generate an event from the file list widget
                            // if the directory tree widget generated one, but just in case, show
                            // a warning.
                            if new_event.is_some() {
                                log::warn!("Multiple UI events in one frame! {:?}", new_event);
                            }
                        }
                    }
                    new_event = Some(event);
                }
            });
        });
        new_event
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
