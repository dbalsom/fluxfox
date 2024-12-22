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

    Implement the hello widget
*/
#[cfg(not(target_arch = "wasm32"))]
use crate::native::util;
use fluxfox::prelude::Platform;
use strum::IntoEnumIterator;

#[cfg(target_arch = "wasm32")]
use crate::wasm::util;

#[derive(Default)]
pub struct HelloWidget {
    small: bool,
}

impl HelloWidget {
    pub fn set_small(&mut self, state: bool) {
        self.small = state;
    }

    pub fn show(&self, ui: &mut egui::Ui, app_name: &str, supported_extensions: &[String]) {
        let scale = if self.small { 0.5 } else { 1.0 };
        ui.add(util::get_logo_image().fit_to_original_size(scale));

        if !self.small {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new(format!("Welcome to {}!", app_name)).color(ui.visuals().strong_text_color()),
                );
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.hyperlink_to("GitHub", "https://github.com/dbalsom/fluxfox");
            });
        }

        ui.vertical(|ui| {
            ui.label(
                "Drag disk image files to this window to load. Kryoflux sets should be in single-disk ZIP archives.",
            );

            ui.label(format!("Image types supported: {}", supported_extensions.join(", ")));
            ui.label(format!(
                "Platform features enabled: {}",
                Platform::iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        });
    }
}
