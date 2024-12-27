/*
    fluxfox
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

    Implements a custom control that displays the image source map
*/
use egui::{CollapsingHeader, Ui};
use fluxfox::{source_map::SourceMap, DiskImage, DiskImageFileFormat};

#[derive(Default)]
pub struct SourceMapWidget {
    pub source_map:    Option<SourceMap>,
    pub source_format: Option<DiskImageFileFormat>,
}

impl SourceMapWidget {
    pub fn new() -> Self {
        Self {
            source_map:    None,
            source_format: None,
        }
    }

    pub fn update(&mut self, disk: &DiskImage) {
        self.source_map = disk.source_map().as_some().cloned();
        self.source_format = disk.source_format();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        if self.source_map.is_none() {
            ui.label("No source map available");
        }
        else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_width(500.00);
                self.render_source_map(ui);
            });
        }
    }

    /// Recursively renders a node and its children.
    pub fn render_source_map(&mut self, ui: &mut Ui) {
        let tree = self.source_map.as_ref().unwrap();
        // Display the file format as the root note, if there is one
        if let Some(format) = self.source_format {
            CollapsingHeader::new(format!("{:?}", format)).show(ui, |ui| {
                Self::render_source_map_recursive(ui, tree, 0);
            });
        }
        else {
            // No file format, so just start at the root's children
            Self::render_source_map_recursive(ui, tree, 0);
        }
    }

    pub fn render_source_map_recursive(ui: &mut Ui, map: &SourceMap, idx: usize) {
        for &child_index in map.children(idx) {
            let (name, value) = map.node(child_index);

            if map.children(child_index).is_empty() {
                //log::debug!("Rendering leaf node: {}", name);
                // Node has no children, so just show a label
                //ui.label(format! {"{}: {}", name, value});

                ui.horizontal(|ui| {
                    ui.horizontal(|ui| {
                        // Set a minimum width for the key field
                        ui.set_min_width(120.0);
                        ui.label(name);
                    });

                    let value_text = egui::RichText::new(value.to_string());

                    // If value is bad, display it as error color
                    if value.is_bad() {
                        ui.label(value_text.color(ui.visuals().error_fg_color));
                    }
                    else {
                        ui.label(value_text);
                    }

                    // Display any comment in the third grid column, as weak fg and italics
                    if let Some(comment) = value.comment_ref() {
                        ui.label(
                            egui::RichText::new(comment)
                                .italics()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
            }
            else {
                //log::debug!("Rendering new parent node: {}", name);
                // Render children under collapsing header
                CollapsingHeader::new(name)
                    .id_salt(format!("ch_node{}", child_index))
                    .show(ui, |ui| {
                        // Recursively render children
                        Self::render_source_map_recursive(ui, map, child_index);
                    });
            }
        }
    }
}
