/*
    ffedit
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
use crate::{
    app::{AppEvent, ApplicationState},
    components::data_block::DataBlock,
    disk_selection::DiskSelection,
};
use crossbeam_channel::Sender;
use fluxfox::DiskImage;
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

// Contain mutable data for App
// This avoids borrowing issues when passing the mutable context to the command processor
pub struct AppContext {
    pub selection: DiskSelection,
    pub state: ApplicationState,
    pub di: Option<DiskImage>,
    pub di_name: Option<PathBuf>,
    pub sender: Sender<AppEvent>,
    pub db: Rc<RefCell<DataBlock>>,
}

impl AppContext {
    pub(crate) fn load_disk_image(&mut self, filename: PathBuf) {
        let outer_sender = self.sender.clone();
        let inner_filename = filename.clone();
        std::thread::spawn(move || {
            let inner_sender = outer_sender.clone();

            match DiskImage::load_from_file(
                &inner_filename,
                None,
                Some(Arc::new(move |status| match status {
                    fluxfox::LoadingStatus::Progress(progress) => {
                        inner_sender.send(AppEvent::LoadingStatus(progress)).unwrap();
                    }
                    fluxfox::LoadingStatus::Error => {
                        log::error!("load_disk_image()... Error loading disk image");
                        inner_sender
                            .send(AppEvent::DiskImageLoadingFailed("Unknown error".to_string()))
                            .unwrap();
                    }
                    _ => {}
                })),
            ) {
                Ok(di) => {
                    log::debug!("load_disk_image()... Successfully loaded disk image");
                    outer_sender
                        .send(AppEvent::DiskImageLoaded(di, filename.clone()))
                        .unwrap();
                }
                Err(e) => {
                    log::error!("load_disk_image()... Error loading disk image");
                    outer_sender
                        .send(AppEvent::DiskImageLoadingFailed(format!("Error: {}", e)))
                        .unwrap();
                }
            }
        });
    }
}
