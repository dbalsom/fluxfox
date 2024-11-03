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
pub(crate) struct DumpParams {
    pub(crate) in_file: PathBuf,
    pub(crate) head: u8,
    pub(crate) cylinder: u16,
    pub(crate) sector: Option<u8>,
    pub(crate) n: Option<u8>,
    pub(crate) phys_head: Option<u8>,
    pub(crate) phys_cylinder: Option<u16>,
    #[allow(dead_code)]
    pub(crate) format: Option<DumpFormat>,
    pub(crate) dupe_mark: bool,
    pub(crate) row_size: Option<u8>,
}

fn dump_format_parser() -> impl Parser<DumpFormat> {
    long("format")
        .short('f')
        .argument::<DumpFormat>("FORMAT")
        .help("Specify the dump format: binary, hex, or ascii")
}

fn dupe_mark_parser() -> impl Parser<bool> {
    long("dupe-mark").help("Dump the duplication mark if present").switch()
}

fn row_size_parser() -> impl Parser<u8> {
    long("row-size")
        .argument::<u8>("HEAD")
        .help("Specify the number of elements per row to be dumped")
        .guard(|&size| size >= 8 && size <= 128, "Size must be between 8 and 128")
}

pub(crate) fn dump_parser() -> impl Parser<DumpParams> {
    //let path = positional::<String>("PATH").help("Path to the file to dump");

    let in_file = in_file_parser();

    let head = head_parser();
    let cylinder = cylinder_parser();
    let n = n_parser().optional();
    let phys_cylinder = phys_cylinder_parser().optional();
    let phys_head = phys_head_parser().optional();
    let sector = sector_parser().optional();
    let format = dump_format_parser().optional();
    let dupe_mark = dupe_mark_parser();
    let row_size = row_size_parser().optional();

    construct!(DumpParams {
        in_file,
        head,
        cylinder,
        sector,
        n,
        phys_head,
        phys_cylinder,
        format,
        dupe_mark,
        row_size
    })
}
