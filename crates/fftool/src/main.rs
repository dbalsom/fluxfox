/*
    fftool
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

pub mod args;
pub mod convert;
pub mod create;
pub mod dump;
mod find;
pub mod info;
mod prompt;

use anyhow::Error;
use bpaf::Parser;
use std::{io::Cursor, path::Path};

use crate::args::Command;
use args::command_parser;

fn main() -> Result<(), Error> {
    env_logger::init();

    let app_params = command_parser().run();

    let command_result = match &app_params.command {
        Command::Version => {
            println!("fftool v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Find(params) => find::run(&app_params.global, params),
        Command::Convert(params) => convert::run(&app_params.global, params),
        Command::Create(params) => create::run(&app_params.global, params),
        Command::Dump(params) => dump::run(&app_params.global, params),
        Command::Info(params) => info::run(&app_params.global, params),
    };

    match command_result {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Command '{}' failed: {}", app_params.command, e);
            for cause in e.chain().skip(1) {
                eprintln!("Caused by: {}", cause);
            }
            std::process::exit(1);
        }
    }
}

pub(crate) fn read_file(path: &Path) -> Result<Cursor<Vec<u8>>, Error> {
    let buffer = std::fs::read(path)?;
    Ok(Cursor::new(buffer))
}
