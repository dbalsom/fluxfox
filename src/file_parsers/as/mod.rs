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

//! Module for Applesauce disk image formats developed by John K. Morris.  
//! These include A2R, WOZ and MOOF.
//! A2R is a low level flux image format for Apple II disks, designed for use
//! with the Applesauce FDC hardware.
//!
//! WOZ is a disk image format intended for Apple II software preservation.
//! MOOF is similar to WOZ, but designed to contain Macintosh disk images.
//! These formats share a similar chunk structure and CRC algorithm, quite
//! similar to Hampa Hug's various PCE disk formats.

pub(crate) mod crc;
pub(crate) mod flux;
#[cfg(feature = "moof")]
pub(crate) mod moof;
