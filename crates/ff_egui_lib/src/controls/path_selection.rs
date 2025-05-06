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
use crate::UiEvent;

#[derive(Default)]
pub struct PathSelectionWidget {
    path: String,
}

impl PathSelectionWidget {
    pub fn set(&mut self, path: Option<&str>) {
        if let Some(path) = path {
            self.path = path.to_string();
        }
        else {
            self.path = "/".to_string();
        }
    }

    pub fn get(&self) -> String {
        let path = self.path.clone().strip_prefix("root").unwrap_or(&self.path).to_string();
        if path.is_empty() {
            "/".to_string()
        }
        else {
            path
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<UiEvent> {
        let path = self.path.clone();
        let mut changed = false;

        let parts = if path == "/" {
            vec!["root"]
        }
        else {
            let mut parts = path.split("/").collect::<Vec<&str>>();
            if parts.len() > 1 {
                parts[0] = "root"
            }
            parts
        };

        //log::debug!("split path: {} into parts: {:?}", path, parts);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut new_selection = Vec::new();
            for (pi, part) in parts.iter().enumerate() {
                new_selection.push(*part);

                if pi > 0 {
                    ui.label("⏵");
                }
                if ui.button(*part).clicked() {
                    changed = true;
                    self.path = new_selection.join("/");
                    log::debug!("Clicked: {}", self.path);
                }
            }
        });

        if changed {
            Some(UiEvent::SelectPath(self.get()))
        }
        else {
            None
        }
    }
}
