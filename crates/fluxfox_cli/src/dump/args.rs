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
    pub(crate) raw: bool,
    pub(crate) rev: Option<u8>,
    pub(crate) clock_map: bool,
    pub(crate) bit_address: bool,
}

fn dupe_mark_parser() -> impl Parser<bool> {
    long("dupe_mark").help("Dump the duplication mark if present").switch()
}

fn raw_parser() -> impl Parser<bool> {
    long("raw")
        .help("Dump the raw sector data (only valid for bitstream images or higher)")
        .switch()
}

fn clock_map_parser() -> impl Parser<bool> {
    long("clock_map")
        .help("Dump the track clock map (only valid for bitstream images or higher)")
        .switch()
}

fn bit_address_parser() -> impl Parser<bool> {
    long("bit_address")
        .help("Show dump address as track bitcell offset")
        .switch()
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
    let raw = raw_parser();
    let rev = rev_parser().optional();
    let clock_map = clock_map_parser();
    let bit_address = bit_address_parser();

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
        row_size,
        raw,
        rev,
        clock_map,
        bit_address
    })
}
