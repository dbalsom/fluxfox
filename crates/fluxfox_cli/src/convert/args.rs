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
use crate::args::*;
use bpaf::{construct, long, Parser};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct ConvertParams {
    pub(crate) in_file: PathBuf,
    pub(crate) out_file: PathBuf,
    #[allow(dead_code)]
    pub(crate) weak_to_holes: bool,
    pub(crate) prolok: bool,
}

fn weak_to_holes_parser() -> impl Parser<bool> {
    long("weak-to-holes").switch().help("Convert weak bits to holes")
}

fn prolok_parser() -> impl Parser<bool> {
    long("prolok")
        .switch()
        .help("Convert weak bits to holes on Prolok-protected tracks")
}

pub(crate) fn convert_parser() -> impl Parser<ConvertParams> {
    //let path = positional::<String>("PATH").help("Path to the file to dump");

    let in_file = in_file_parser();
    let out_file = out_file_parser();
    let weak_to_holes = weak_to_holes_parser();
    let prolok = prolok_parser();

    construct!(ConvertParams {
        in_file,
        out_file,
        weak_to_holes,
        prolok,
    })
}
