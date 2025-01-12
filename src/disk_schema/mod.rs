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

//! A [DiskSchema] is a high level interpreter of a disk image's platform-specific data.
//! A [DiskSchema] is responsible for detecting platform type(s), reading information
//! such as the Bios Parameter Block (BPB).
//! A disk image may have multiple disk schemas, for example dual and triple-format
//! disk images. There should generally be one [DiskSchema] per [Platform] associated
//! with a disk image.
//! A [DiskSchema] is not strictly required (neither is a [Platform]), but operations
//! and information about the disk image will be limited.

// This module is in progress
#![allow(dead_code)]

use crate::DiskImage;

pub enum DiskSchema {
    Dos,
    MacintoshGcr,
    MacintoshMfm,
    AmigaGcr,
    AmigaMfm,
    AtariSt,
}

impl DiskSchema {
    pub fn detect(_disk: &DiskImage) -> Option<Vec<Self>> {
        None
    }
}
