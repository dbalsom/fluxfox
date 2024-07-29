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

    src/structure_parser/mod.rs

    Main module for a disk structure parser module.

    After the bitstream has been decoded, a structure parser is responsible for
    interpreting the layout of syncs, gaps, address marks and data. It is also
    responsible for encoding data to be written back into a compatible layout.

    A DiskStructureParser trait is defined here that can be implemented by
    different parser types. For the time being, only the IBM System 32 (standard
    PC floppy) type will be implemented.
*/

pub mod system34;

use crate::bitstream::mfm::MfmDecoder;
use crate::diskimage::TrackDataStream;
use crate::structure_parsers::system34::System34Element;
use std::ops::Index;

pub struct DiskStructureMetadata {
    pub items: Vec<DiskStructureMetadataItem>,
}

impl Default for DiskStructureMetadata {
    fn default() -> Self {
        DiskStructureMetadata { items: Vec::new() }
    }
}

impl DiskStructureMetadata {
    pub fn new(items: Vec<DiskStructureMetadataItem>) -> Self {
        DiskStructureMetadata { items }
    }

    pub fn add_item(&mut self, item: DiskStructureMetadataItem) {
        self.items.push(item);
    }

    pub fn item_at(&self, index: usize) -> Option<&DiskStructureMetadataItem> {
        for item in &self.items {
            if item.start <= index && item.end >= index {
                return Some(&item);
            }
        }
        None
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DiskStructureMetadataItem {
    pub(crate) elem_type: DiskStructureElement,
    start: usize,
    end: usize,
    crc: Option<DiskStructureCrc>,
}

#[derive(Copy, Clone, Debug)]
pub enum DiskStructureCrc {
    System34Crc8(u8),
    System34Crc16(u16),
}

#[derive(Copy, Clone, Debug)]
pub enum DiskStructureElement {
    System34(System34Element),
}

pub trait DiskStructureParser {
    /// Find the provided pattern of bytes within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// The bit offset of the pattern is returned if found, otherwise None.
    /// The pattern length is limited to 8 characters.
    fn find_pattern(track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize>;

    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize>;

    fn scan_track_elements(track: &TrackDataStream) -> Vec<DiskStructureMetadataItem>;

    /// Read `length` bytes from the sector containing the specified sector_id from a
    /// TrackBitStream. If Some value of sector_n is provided, the value of n must match as well
    /// for data to be returned. The `length` parameter allows data to be returned after the end
    /// of the sector, allowing reading into inter-sector gaps.
    fn read_sector(track: &TrackDataStream, sector_id: u8, sector_n: Option<u8>, length: usize) -> Option<Vec<u8>>;
}
