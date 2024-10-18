/*
    ffedit
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

use crate::app::{App, AppEvent, ApplicationState};
use crate::history::HistoryEntry;
use crate::logger::LogEntry;
use crate::modal::ModalState;

impl App {
    pub(crate) fn handle_app_events(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            let mut history = self.history.borrow_mut();
            match msg {
                AppEvent::Log(entry) => match entry {
                    LogEntry::Trace(msg) => {
                        history.push(HistoryEntry::Trace(msg));
                    }
                    LogEntry::Info(msg) => {
                        history.push(HistoryEntry::Info(msg));
                    }
                    LogEntry::Debug(msg) => {
                        history.push(HistoryEntry::Debug(msg));
                    }
                    LogEntry::Warning(msg) => {
                        history.push(HistoryEntry::Warning(msg));
                    }
                    LogEntry::Error(msg) => {
                        history.push(HistoryEntry::Error(msg));
                    }
                },
                AppEvent::LoadingStatus(progress) => {
                    self.ctx.state =
                        ApplicationState::Modal(ModalState::ProgressBar("Loading Disk Image".to_string(), progress));
                }
                AppEvent::DiskImageLoaded(di, di_name) => {
                    self.ctx.di = Some(di);
                    self.ctx.di_name = Some(di_name.file_name().unwrap().into());
                    self.ctx.state = ApplicationState::Normal;

                    // Reset the selection.
                    self.ctx.selection = Default::default();
                    // Load the data block.
                    match self
                        .ctx
                        .db
                        .borrow_mut()
                        .load(self.ctx.di.as_mut().unwrap(), &self.ctx.selection)
                    {
                        Ok(_) => {
                            history.push(HistoryEntry::CommandResponse(format!(
                                "Loaded disk image: {}",
                                di_name.display()
                            )));
                        }
                        Err(e) => {
                            history.push(HistoryEntry::Error(format!("Failed to load disk image: {}", e)));
                        }
                    }
                }
                AppEvent::DiskImageLoadingFailed(msg) => {
                    self.ctx.state = ApplicationState::Normal;
                    history.push(HistoryEntry::CommandResponse(msg));
                }
                AppEvent::DiskSelectionChanged => {
                    // Depending on selection, we need to read the current track or sector,
                    // and update the data displayed in the data viewer.
                    if let Some(di) = &mut self.ctx.di {
                        _ = self.ctx.db.borrow_mut().load(di, &self.ctx.selection);
                    }
                }
            }
        }
    }
}
