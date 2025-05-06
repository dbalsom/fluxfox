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
use crate::app::AppEvent;
use crossbeam_channel::Sender;
use log::{Level, Metadata, Record, SetLoggerError};

pub enum LogEntry {
    Trace(String),
    Info(String),
    Debug(String),
    Warning(String),
    Error(String),
}

struct TuiLogger {
    sender: Sender<AppEvent>, // Add a sender for crossbeam channel.
}

impl TuiLogger {
    fn new(sender: Sender<AppEvent>) -> Self {
        TuiLogger { sender }
    }
}

impl log::Log for TuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug // Adjust the level as needed
    }

    fn log(&self, record: &Record) {
        let from_ffedit = record.target().starts_with("ffedit");
        let message = if from_ffedit {
            format!("{}", record.args().to_string())
        }
        else {
            format!("[{}] {}", record.target().to_string(), record.args().to_string())
        };
        let log = match record.level() {
            Level::Trace if from_ffedit => LogEntry::Trace(message),
            Level::Info if from_ffedit => LogEntry::Info(message),
            Level::Warn => LogEntry::Warning(message),
            Level::Error => LogEntry::Error(message),
            Level::Debug if from_ffedit => LogEntry::Debug(message),
            _ => {
                // Ignore other log levels from external libraries
                return;
            }
        };

        self.sender.send(AppEvent::Log(log)).unwrap();
    }

    fn flush(&self) {}
}

//static LOGGER: Lazy<TuiLogger> = Lazy::new(TuiLogger::new);

pub(crate) fn init_logger(sender: Sender<AppEvent>) -> Result<(), SetLoggerError> {
    //log::set_logger(&*LOGGER).map(|()| log::set_max_level(log::LevelFilter::Info))

    let logger = TuiLogger::new(sender);
    log::set_boxed_logger(Box::new(logger)) // Set the logger as a boxed trait object.
        .map(|()| log::set_max_level(log::LevelFilter::Debug))
}
