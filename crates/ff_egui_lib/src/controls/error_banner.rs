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

    src/widgets/error_banner.rs

    Displays a dismissible error banner.
*/

#[allow(dead_code)]
pub enum ErrorBannerSize {
    Small,
    Medium,
    Large,
}

#[allow(dead_code)]
pub struct ErrorBannerLayout {
    rounding: f32,
    margin: f32,
    icon: f32,
    text: f32,
}

impl ErrorBannerSize {
    pub fn layout(&self) -> ErrorBannerLayout {
        match self {
            ErrorBannerSize::Small => ErrorBannerLayout {
                rounding: 4.0,
                margin: 4.0,
                icon: 16.0,
                text: 14.0,
            },
            ErrorBannerSize::Medium => ErrorBannerLayout {
                rounding: 6.0,
                margin: 6.0,
                icon: 24.0,
                text: 18.0,
            },
            ErrorBannerSize::Large => ErrorBannerLayout {
                rounding: 8.0,
                margin: 8.0,
                icon: 32.0,
                text: 24.0,
            },
        }
    }
}

pub struct ErrorBanner {
    message0: Option<String>,
    message1: Option<String>,
    size: ErrorBannerSize,
    dismiss_button: bool,
}

impl ErrorBanner {
    pub fn new(text: &str) -> Self {
        let messages = Self::split_message(text);
        Self {
            message0: Some(messages.0),
            message1: messages.1,
            size: ErrorBannerSize::Medium,
            dismiss_button: false,
        }
    }

    pub fn dismissable(mut self) -> Self {
        self.dismiss_button = true;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = ErrorBannerSize::Small;
        self
    }

    pub fn medium(mut self) -> Self {
        self.size = ErrorBannerSize::Medium;
        self
    }

    pub fn large(mut self) -> Self {
        self.size = ErrorBannerSize::Large;
        self
    }

    fn split_message(text: &str) -> (String, Option<String>) {
        if let Some((message0, message1)) = text.split_once(':').map(|(a, b)| (a.trim(), b.trim())) {
            //log::debug!("ErrorBanner: message0: {}, message1: {}", message0, message1);
            (message0.to_string(), Some(message1.to_string()))
        }
        else {
            //log::debug!("ErrorBanner: message0: {}", text);
            (text.to_string(), None)
        }
    }

    pub fn set_message(&mut self, text: &str) {
        let messages = Self::split_message(text);
        self.message0 = Some(messages.0);
        self.message1 = messages.1;
    }

    pub fn dismiss(&mut self) {
        self.message0 = None;
        self.message1 = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(message) = self.message0.clone() {
            let ErrorBannerLayout {
                rounding,
                margin,
                icon,
                text,
            } = self.size.layout();

            egui::Frame::none()
                .fill(egui::Color32::DARK_RED)
                .rounding(rounding)
                .inner_margin(margin)
                .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🗙").color(egui::Color32::WHITE).size(icon));
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(
                                egui::RichText::new(message).color(egui::Color32::WHITE).size(text),
                            ));
                            // Show the sub-error message if it exists
                            if let Some(message1) = &self.message1 {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(message1).color(egui::Color32::WHITE),
                                ));
                            }
                        });
                        if self.dismiss_button {
                            ui.button("Dismiss")
                                .on_hover_ui(|ui| {
                                    ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                })
                                .clicked()
                                .then(|| {
                                    self.dismiss();
                                });
                        }
                    });
                });
        }
    }
}
