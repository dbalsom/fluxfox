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
use std::path::PathBuf;
use std::sync::Arc;
use crate::App;

impl App {
    pub(crate) fn load_dropped_files(&mut self) {
        for file in &mut self.dropped_files {
            // Check if the file needs to be loaded
            if file.bytes.is_none() && file.path.is_some() && file.path.as_ref().unwrap().is_file() {

                let path = file.path.as_ref().unwrap();

                // Load the file
                file.bytes = match std::fs::read(path) {
                    Ok(bytes) => Some(Arc::from(bytes.into_boxed_slice())),
                    Err(e) => {
                        log::error!("Failed to read file {}: {:?}", path.display(), e);
                        None
                    }
                };
            }
        }
    }
}