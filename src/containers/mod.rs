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

    src/containers/mod.rs

    A module supporting disk image container formats.
    Currently only ZIP is expected to be needed, but other container formats
    could be added in the future.

*/
use crate::DiskImageFormat;
use std::fmt::{Display, Formatter, Result};

#[cfg(feature = "zip")]
pub mod zip;

#[derive(Copy, Clone, Debug)]
pub enum DiskImageContainer {
    Raw(DiskImageFormat),
    Zip(DiskImageFormat),
}

impl Display for DiskImageContainer {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            DiskImageContainer::Raw(fmt) => write!(f, "{:?}", fmt),
            DiskImageContainer::Zip(fmt) => write!(f, "Zipped {:?}", fmt),
        }
    }
}
