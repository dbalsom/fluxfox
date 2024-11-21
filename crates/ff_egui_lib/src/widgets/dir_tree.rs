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
use egui::{CollapsingHeader, RichText, TextStyle};
use fluxfox::file_system::FileTreeNode;

pub const MIN_TREE_WIDTH: f32 = 120.0;

#[derive(Default)]
pub struct DirTreeWidget {
    pub tree: FileTreeNode,
    pub selected_path: Option<String>,
}

impl DirTreeWidget {
    pub fn new() -> Self {
        Self {
            tree: FileTreeNode::default(),
            selected_path: Some("/".to_string()),
        }
    }

    pub fn update(&mut self, tree: FileTreeNode) {
        self.tree = tree;
        self.selected_path = Some("/".to_string());
    }

    pub fn selection(&mut self) -> Option<String> {
        self.selected_path.clone()
    }

    pub fn set_selection(&mut self, selection: Option<String>) {
        self.selected_path = selection;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let mut selected_path = self.selected_path.clone();

        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_width(MIN_TREE_WIDTH);

            self.tree_ui(ui, &self.tree, &mut selected_path, true);
            ui.set_min_height(ui.available_height());
        });
        self.selected_path = selected_path;
    }

    pub fn tree_ui(&self, ui: &mut egui::Ui, node: &FileTreeNode, selected_path: &mut Option<String>, root: bool) {
        fn dir_icon(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
            let rect = response.rect;

            // Calculate the position for the icon
            let icon = if openness > 0.0 { "📂" } else { "📁" };
            let icon_pos = rect.min + egui::vec2(0.0, rect.height() / 2.0);
            let font = TextStyle::Button.resolve(&ui.style());

            // Draw the icon using the painter
            ui.painter().text(
                icon_pos,
                egui::Align2::LEFT_CENTER,
                icon,
                font,
                ui.visuals().text_color(),
            );
        }

        match node {
            FileTreeNode::File(_) => {
                return;
            }
            FileTreeNode::Directory { fs, children } => {
                //log::debug!("Drawing directory: {} with {:?} children", fs.name, children);
                let is_selected = Some(&fs.path) == self.selected_path.as_ref();

                //ui.visuals_mut().collapsing_header_frame = true;

                let mut text = RichText::new(format!("{}", if root { "root" } else { &fs.name }));
                if is_selected {
                    text = text.color(ui.visuals().strong_text_color())
                }

                // Prevent empty directories from being opened
                let open_control = children.is_empty().then_some(false);

                let header_response = CollapsingHeader::new(text)
                    .default_open(root)
                    .icon(dir_icon)
                    .show_background(is_selected)
                    .open(open_control)
                    .show(ui, |ui| {
                        // Draw children recursively
                        children.iter().for_each(|child| {
                            self.tree_ui(ui, child, selected_path, false);
                        });
                    })
                    .header_response;

                let mut visuals = ui.style_mut().interact_selectable(&header_response, is_selected);

                if is_selected {
                    visuals.weak_bg_fill = egui::Color32::from_rgb(200, 200, 255);
                }

                header_response.clicked().then(|| {
                    log::debug!("Selected path: {}", fs.path);
                    *selected_path = Some(fs.path.clone());
                });
            }
        }
    }
}
