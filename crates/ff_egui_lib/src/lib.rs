/*
    FluxFox
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

use fluxfox::{file_system::FileEntry, prelude::*};
use std::fmt::{Debug, Formatter, Result};

pub mod widgets;

#[derive(Debug, Clone, Default)]
pub struct SectorSelection {
    pub phys_ch:    DiskCh,
    pub sector_id:  SectorId,
    pub bit_offset: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct TrackSelection {
    pub phys_ch: DiskCh,
}

#[derive(Debug, Clone)]
pub enum TrackListSelection {
    Track(TrackSelection),
    Sector(SectorSelection),
}

#[derive(Clone)]
pub enum UiEvent {
    ExportFile(String),
    SelectPath(String),
    SelectFile(FileEntry),
    ExportDir(String),
}

impl Debug for UiEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // Match on the enum to display only the variant name
        let variant_name = match self {
            UiEvent::ExportFile(_) => "ExportFile",
            UiEvent::SelectPath(_) => "SelectPath",
            UiEvent::SelectFile(_) => "SelectFile",
            UiEvent::ExportDir(_) => "ExportDir",
        };
        write!(f, "{}", variant_name)
    }
}
