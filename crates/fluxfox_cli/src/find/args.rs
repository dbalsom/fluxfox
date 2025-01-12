/*
    fluxfox - fftool
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
use crate::args::{
    cylinder_parser,
    dump_format_parser,
    head_parser,
    in_file_parser,
    phys_cylinder_parser,
    phys_head_parser,
    row_size_parser,
    sector_parser,
    DumpFormat,
};

use bpaf::{construct, long, Parser};
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct FindParams {
    pub(crate) in_file: PathBuf,
    pub(crate) ascii: Option<String>,
    pub(crate) hex: Option<String>,
    pub(crate) head: Option<u8>,
    pub(crate) cylinder: Option<u16>,
    pub(crate) sector: Option<u8>,
    pub(crate) phys_head: Option<u8>,
    pub(crate) phys_cylinder: Option<u16>,
    pub(crate) dump: bool,
    pub(crate) format: Option<DumpFormat>,
    pub(crate) row_size: Option<u8>,
}

fn dump_parser() -> impl Parser<bool> {
    long("dump")
        .short('d')
        .help("Dump the sector where the match is found")
        .switch()
}

fn ascii_parser() -> impl Parser<String> {
    long("ascii")
        .argument::<String>("ASCII_STRING")
        .help("Search for ASCII string")
}

fn hex_parser() -> impl Parser<String> {
    long("hex")
        .argument::<String>("HEX_STRING")
        .help("Search for hexadecimal string")
}

pub(crate) fn find_parser() -> impl Parser<FindParams> {
    //let path = positional::<String>("PATH").help("Path to the file to dump");

    let in_file = in_file_parser();

    let head = head_parser().optional();
    let ascii = ascii_parser().optional();
    let hex = hex_parser().optional();

    let cylinder = cylinder_parser().optional();
    let phys_cylinder = phys_cylinder_parser().optional();
    let phys_head = phys_head_parser().optional();
    let sector = sector_parser().optional();
    let dump = dump_parser();
    let format = dump_format_parser().optional();
    let row_size = row_size_parser().optional();

    construct!(FindParams {
        in_file,
        ascii,
        hex,
        head,
        cylinder,
        sector,
        phys_head,
        phys_cylinder,
        dump,
        format,
        row_size
    })
    .guard(
        |params| params.ascii.is_some() || params.hex.is_some(),
        "Either --ascii or --hex must be specified",
    )
}
