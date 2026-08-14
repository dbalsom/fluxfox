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
use fluxfox::prelude::*;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct CreateParams {
    pub(crate) out_file: PathBuf,
    pub(crate) disk_format: StandardFormatParam,
    pub(crate) formatted: bool,
    pub(crate) sector_test: bool,
    pub(crate) from_dir: Option<PathBuf>,
    pub(crate) from_archive: Option<PathBuf>,
    pub(crate) bootsector: Option<PathBuf>,
    pub(crate) no_recursive: bool,
    pub(crate) must_fit: bool,
}

pub(crate) fn create_parser() -> impl Parser<CreateParams> {
    let out_file = out_file_parser();
    let disk_format = standard_format_parser();
    let formatted = long("formatted").switch().help("Format the new disk image.");
    let sector_test = long("sector_test")
        .switch()
        .help("Create a sector test image [internal use].");
    let from_dir = from_dir_parser().optional();
    let from_archive = from_archive_parser().optional();
    let bootsector = bootsector_parser().optional();
    let no_recursive = long("no_recursive")
        .switch()
        .help("Only copy files from the source root");
    let must_fit = long("must_fit")
        .switch()
        .help("Fail instead of creating a partial image when the source is too large");

    construct!(CreateParams {
        out_file,
        disk_format,
        formatted,
        sector_test,
        from_dir,
        from_archive,
        bootsector,
        no_recursive,
        must_fit
    })
}
