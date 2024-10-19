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
mod app;
mod app_context;
mod app_events;
mod cmd_interpreter;
mod components;
mod disk_selection;
mod layout;
mod logger;
mod modal;
mod util;
mod widget;

use std::fmt::Display;
use std::io;
use std::io::Write;
use std::path::PathBuf;

use bpaf::{construct, short, OptionParser, Parser};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;

use app::App;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct CmdParams {
    in_filename: Option<PathBuf>,
    mouse: bool,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<CmdParams> {
    let in_filename = short('i')
        .long("in_filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("IN_FILE")
        .optional();

    let mouse = short('m').long("switch").help("Enable mouse support").switch();

    construct!(CmdParams { in_filename, mouse }).to_options()
}

fn main() -> io::Result<()> {
    let opts = opts().run();
    let mut terminal = ratatui::init();

    if opts.mouse {
        std::io::stdout().execute(crossterm::event::EnableMouseCapture).unwrap();
    }

    let mut app = App::new(opts);
    let app_result = app.run(&mut terminal);

    ratatui::restore();
    app_result
}
