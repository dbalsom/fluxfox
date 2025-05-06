/*
    fftool
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
use crate::args::in_file_parser;
use bpaf::{construct, Parser};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct InfoParams {
    // Define specific parameters for `info`
    pub(crate) in_file: PathBuf,
    pub(crate) sector_list: bool,
    pub(crate) track_list: bool,
    pub(crate) rev_list: bool,
}

fn sector_list_parser() -> impl Parser<bool> {
    bpaf::long("sector-list")
        .help("List all sectors in the disk image")
        .switch()
}

fn track_list_parser() -> impl Parser<bool> {
    bpaf::long("track-list")
        .help("List all tracks in the disk image")
        .switch()
}

fn rev_list_parser() -> impl Parser<bool> {
    bpaf::long("rev-list")
        .help("List all revolutions in the disk image")
        .switch()
}

pub(crate) fn info_parser() -> impl Parser<InfoParams> {
    let in_file = in_file_parser();
    let sector_list = sector_list_parser();
    let track_list = track_list_parser();
    let rev_list = rev_list_parser();

    construct!(InfoParams {
        in_file,
        sector_list,
        track_list,
        rev_list,
    })
}
