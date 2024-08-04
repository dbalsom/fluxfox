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

use crate::chs::DiskChsn;
use crate::diskimage::TrackDataStream;
use crate::structure_parsers::system34::{System34Element, System34Marker};
use bit_vec::BitVec;

#[derive(Default)]
pub struct DiskStructureMetadata {
    pub items: Vec<DiskStructureMetadataItem>,
}

impl DiskStructureMetadata {
    pub fn new(items: Vec<DiskStructureMetadataItem>) -> Self {
        DiskStructureMetadata { items }
    }

    pub fn add_item(&mut self, item: DiskStructureMetadataItem) {
        self.items.push(item);
    }

    /// Return a reference to the innermost metadata item that contains the specified index,
    /// along with a count of the total number of matching items (to handle overlapping items).
    /// Returns None if no match.
    pub fn item_at(&self, index: usize) -> Option<(&DiskStructureMetadataItem, u32)> {
        let mut ref_stack = Vec::new();
        let mut match_ct = 0;
        for item in &self.items {
            if item.start <= index && item.end >= index {
                ref_stack.push(item);
                match_ct += 1;
            }
        }

        if ref_stack.is_empty() {
            None
        } else {
            // Sort by smallest element to allow address markers to have highest
            // priority.
            ref_stack.sort_by(|a, b| a.start.cmp(&b.start));
            Some((ref_stack.pop().unwrap(), match_ct))
        }
    }

    pub fn sector_ct(&self) -> u8 {
        let mut sector_ct = 0;
        for item in &self.items {
            if item.elem_type.is_sector() {
                sector_ct += 1;
            }
        }
        sector_ct
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DiskStructureMarkerItem {
    pub(crate) elem_type: DiskStructureMarker,
    start: usize,
}

/// A DiskStructureMetadataItem represents a single element of a disk structure, such as an
/// address mark or data mark. It encodes the start and end of the element (as raw bitstream
/// addresses) as well as optionally the status of any CRC field (valid for IDAM and DAM marks)
#[derive(Copy, Clone, Debug)]
pub struct DiskStructureMetadataItem {
    pub(crate) elem_type: DiskStructureElement,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) chsn: Option<DiskChsn>,
    pub(crate) crc: Option<DiskStructureCrc>,
}

#[derive(Copy, Clone, Debug)]
pub struct DiskStructureCrc {
    stored: u16,
    calculated: u16,
}

impl DiskStructureCrc {
    pub fn valid(&self) -> bool {
        self.stored == self.calculated
    }
}

#[derive(Copy, Clone, Debug)]
pub enum DiskStructureMarker {
    System34(System34Marker),
    Placeholder,
}

#[derive(Copy, Clone, Debug)]
pub enum DiskStructureElement {
    System34(System34Element),
    Placeholder,
}

impl DiskStructureElement {
    pub fn is_sector(&self) -> bool {
        match self {
            DiskStructureElement::System34(elem) => elem.is_sector(),
            _ => false,
        }
    }
}

pub trait DiskStructureParser {
    /// Find the provided pattern of bytes within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// The bit offset of the pattern is returned if found, otherwise None.
    /// The pattern length is limited to 8 characters.
    fn find_data_pattern(track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize>;
    fn find_next_marker(track: &TrackDataStream, offset: usize) -> Option<(DiskStructureMarker, usize)>;

    fn find_marker(track: &TrackDataStream, marker: DiskStructureMarker, offset: usize) -> Option<usize>;
    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize>;

    fn scan_track_markers(track: &mut TrackDataStream) -> Vec<DiskStructureMarkerItem>;
    fn scan_track_metadata(
        track: &mut TrackDataStream,
        markers: Vec<DiskStructureMarkerItem>,
    ) -> Vec<DiskStructureMetadataItem>;

    fn create_clock_map(markers: &[DiskStructureMarkerItem], clock_map: &mut BitVec);

    /// Read `length` bytes from the sector containing the specified sector_id from a
    /// TrackBitStream. If Some value of sector_n is provided, the value of n must match as well
    /// for data to be returned. The `length` parameter allows data to be returned after the end
    /// of the sector, allowing reading into inter-sector gaps.
    fn read_sector(track: &TrackDataStream, sector_id: u8, sector_n: Option<u8>, length: usize) -> Option<Vec<u8>>;

    fn crc16(track: &mut TrackDataStream, start: usize, end: usize) -> u16;
}
