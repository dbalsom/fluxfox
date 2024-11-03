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
use bpaf::*;
use std::path::PathBuf;
use std::str::FromStr;

use crate::dump::args::{dump_parser, DumpParams};
use crate::info::args::{info_parser, InfoParams};

#[derive(Debug, Clone, Copy)]
pub enum DumpFormat {
    Binary,
    Hex,
    Ascii,
}

impl FromStr for DumpFormat {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "binary" => Ok(DumpFormat::Binary),
            "hex" => Ok(DumpFormat::Hex),
            "ascii" => Ok(DumpFormat::Ascii),
            _ => Err("Invalid format; expected 'binary', 'hex', or 'ascii'"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    Version,
    Dump(DumpParams),
    Info(InfoParams),
}

#[derive(Debug)]
pub struct AppParams {
    pub global: GlobalOptions,
    pub command: Command,
}

#[derive(Debug)]
pub struct GlobalOptions {
    pub silent: bool,
}

pub fn global_options_parser() -> impl Parser<GlobalOptions> {
    let silent = long("silent")
        .help("Suppress all output except required output")
        .switch(); // Switch returns a bool, true if the flag is present

    construct!(GlobalOptions { silent })
}

pub(crate) fn in_file_parser() -> impl Parser<PathBuf> {
    long("in_file")
        .short('i')
        .argument::<PathBuf>("IN_FILE")
        .help("Path to input file")
}

pub(crate) fn command_parser() -> impl Parser<AppParams> {
    let global = global_options_parser();

    let version = pure(Command::Version)
        .to_options()
        .command("version")
        .help("Display version information and exit");

    let dump = construct!(Command::Dump(dump_parser()))
        .to_options()
        .command("dump")
        .help("Dump data from a disk image");
    let info = construct!(Command::Info(info_parser()))
        .to_options()
        .command("info")
        .help("Display information about a disk image");

    let command = construct!([version, dump, info]);

    construct!(AppParams { global, command })
}

pub(crate) fn sector_parser() -> impl Parser<u8> {
    long("sector")
        .short('s')
        .argument::<u8>("SECTOR")
        .help("Specify the sector number to dump")
}

pub(crate) fn cylinder_parser() -> impl Parser<u16> {
    long("cylinder")
        .short('c')
        .argument::<u16>("CYLINDER")
        .help("Specify the cylinder number to dump")
}

pub(crate) fn phys_cylinder_parser() -> impl Parser<u16> {
    long("p_cylinder")
        .argument::<u16>("PHYSICAL_CYLINDER")
        .help("Specify the physical cylinder number to dump")
}

pub(crate) fn phys_head_parser() -> impl Parser<u8> {
    long("p_head")
        .argument::<u8>("PHYSICAL_HEAD")
        .help("Specify the physical head number to dump")
}

pub(crate) fn head_parser() -> impl Parser<u8> {
    long("head")
        .short('h')
        .argument::<u8>("HEAD")
        .help("Specify the head number to dump")
        .guard(|&head| head == 0 || head == 1, "Head must be either 0 or 1")
}

pub(crate) fn n_parser() -> impl Parser<u8> {
    long("size")
        .short('n')
        .argument::<u8>("SECTOR_SIZE")
        .help("Specify the size of the sector to dump. 0=128 bytes, 1=256 bytes, 2=512 bytes ... 6=8192 bytes")
        .guard(|&size| size <= 6, "Size must be between 0 and 6")
}
