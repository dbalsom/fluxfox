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
#![allow(dead_code)]
//! Parsing IPF files requires certain domain knowledge about the format being
//! parsed. For example, Amiga disks encoded as CAPS IPF tracks use certain
//! conventions that may not be obvious.
//! The 'Sync' data stream element, for example, is in encoded MFM format,
//! likely to capture the clock sync pattern of the marker.
//! The `Data` stream element includes the sector header and checksum fields
//! in Amiga odd/even LONG format, sector labels in odd/even 4xLONG format,
//! and the sector data in odd/even 512xBYTE format, and there's no indication
//! of the actual format of the sector or the differences in encoding.
//!
//! This module provides the necessary structures and tools to parse Amiga
//! track data from IPF files.

/// An Amiga sector as represented by an IPF CAPS Data Stream Element.
pub(crate) struct IpfAmigaSector {
    /// MFM decoded sync bytes, usually `[0x00, 0x00]`
    pub(crate) sync:   [u8; 2],
    pub(crate) header: IpfAmigaSectorHeader,
    pub(crate) data:   IpfAmigaSectorData,
}

pub(crate) struct IpfAmigaSectorHeader {
    /// MFM `encoded` address marker. It is encoded to preserve the MFM clock sync pattern.
    pub(crate) marker: [u8; 4],
    /// MFM decoded sector ID.
    pub(crate) id: u32,
    /// Decoded 4 LONGs representing the sector label.
    pub(crate) label: [u32; 4],
    /// The header checksum.
    pub(crate) header_checksum: u32,
}

pub(crate) struct IpfAmigaSectorData {
    /// The data checksum
    pub(crate) checksum: u32,
    /// MFM decoded sector data.
    pub(crate) data: [u8; 512],
}
