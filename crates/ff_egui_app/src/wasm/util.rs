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

use eframe::egui::Image;
use eframe::wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(module = "/assets/base_url.js")]
extern "C" {
    fn getBaseURL() -> String;
}

fn construct_full_url(relative_path: &str) -> String {
    //unsafe {
    let base_path = option_env!("URL_PATH");
    //log::debug!("construct_full_url(): base_path: {:?}", base_path);
    let base_url = getBaseURL();
    let url = format!(
        "{}{}/{}",
        base_url.trim_start_matches('/'),
        base_path.unwrap_or("").trim_start_matches('/'),
        relative_path.trim_start_matches('/')
    );
    //log::debug!("construct_full_url(): {}", url);
    url
}

pub(crate) fn get_logo_image<'a>() -> Image<'a> {
    let url = construct_full_url("./assets/fluxfox_logo.png");
    egui::Image::new(url).fit_to_original_size(1.0)
}
