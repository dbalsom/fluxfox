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

    src/parsers/scp.rs

    A parser for the SuperCardPro format.

    SCP format images encode raw flux information for each track of the disk.

*/

pub const MAX_TRACK_NUMBER: usize = 167;

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpFileHeader {
    pub id: [u8; 3],
    pub version: u8,
    pub disk_type: u8,
    pub revolutions: u8,
    pub start_track: u8,
    pub end_track: u8,
    pub flags: u8,
    pub bit_cell_width: u8,
    pub heads: u8,
    pub resolution: u8,
    pub checksum: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackOffsetTable {
    pub track_offsets: [u32; 168],
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackHeader {
    pub id: [u8; 3],
    pub track_number: u8,
}
