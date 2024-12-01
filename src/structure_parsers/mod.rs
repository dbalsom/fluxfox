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

//! The `structure_parsers` module defines a `DiskStructureParser` trait that can be implemented
//! by different parser types.
//!
//! A disk structure parser is responsible for interpreting the layout of syncs, gaps, address marks
//! after the basic decoding of a bitstream has been completed. It is also responsible for encoding
//! data to be written back into a compatible layout.
//!
//! For the time being, only the IBM System 34 (standard PC floppy) format is implemented.

pub mod system34;

use crate::{
    bitstream::{mfm::MFM_BYTE_LEN, TrackDataStream},
    chs::DiskChsn,
    structure_parsers::system34::{System34Element, System34Marker},
};
use bit_vec::BitVec;

/// A `DiskStructureMetadata` structure represents a collection of metadata items found in a track.
#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskStructureMetadata {
    pub items: Vec<DiskStructureMetadataItem>,
}

impl DiskStructureMetadata {
    /// Create a new `DiskStructureMetadata` instance from the specified items.
    pub fn new(items: Vec<DiskStructureMetadataItem>) -> Self {
        DiskStructureMetadata { items }
    }

    /// Add a new metadata item to the collection.
    pub fn add_item(&mut self, item: DiskStructureMetadataItem) {
        self.items.push(item);
    }

    /// Return a reference to the innermost metadata item that contains the specified index,
    /// along with a count of the total number of matching items (to handle overlapping items).
    /// # Arguments
    /// * `index` - The bit index to match.
    /// # Returns
    /// A tuple containing a reference to the metadata item and the count of matching items, or `None`
    /// if no match was found.
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
        }
        else {
            // Sort by smallest element to allow address markers to have highest
            // priority.
            ref_stack.sort_by(|a, b| a.start.cmp(&b.start));
            Some((ref_stack.pop()?, match_ct))
        }
    }

    /// Return the number of sectors represented in the metadata collection.
    /// To be counted, a sector must have a corresponding, valid sector header.
    pub fn sector_ct(&self) -> u8 {
        let mut sector_ct = 0;
        for item in &self.items {
            if item.elem_type.is_sector_data_marker() {
                sector_ct += 1;
            }
        }
        sector_ct
    }

    /// Return a vector of sector IDs as `DiskChsn` represented in the metadata collection.
    pub fn get_sector_ids(&self) -> Vec<DiskChsn> {
        let mut sector_ids = Vec::new();

        for item in &self.items {
            if let DiskStructureElement::System34(System34Element::SectorHeader { chsn, .. }) = item.elem_type {
                sector_ids.push(chsn);
            }
        }

        sector_ids
    }

    /// Return a vector of data ranges representing the start and end bit indices of sector data.
    /// Primarily used as helper for disk visualization.
    /// # Returns
    /// A vector of tuples containing the start and end bit indices of sector data.
    pub fn data_ranges(&self) -> Vec<(usize, usize)> {
        let mut data_ranges = Vec::new();

        for item in &self.items {
            if let DiskStructureElement::System34(System34Element::Data { .. }) = item.elem_type {
                // Should the data range for a sector include the address mark?
                // For now we will exclude it.
                data_ranges.push((item.start + (4 * MFM_BYTE_LEN), item.end));
            }
        }

        data_ranges
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DiskStructureMarkerItem {
    pub(crate) elem_type: DiskStructureMarker,
    pub(crate) start: usize,
}

/// A `DiskStructureMetadataItem` represents a single element of a disk structure, such as an
/// address marker or data marker. It encodes the start and end of the element (as raw bitstream
/// offsets) as well as optionally the status of any CRC field (valid for IDAM and DAM marks)
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskStructureMetadataItem {
    pub(crate) elem_type: DiskStructureElement,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) chsn: Option<DiskChsn>,
    pub(crate) _crc: Option<DiskStructureCrc>,
}

/// A `DiskStructureCrc` represents a 16-bit CRC value related to a region of a track. It contains
/// both the stored CRC value read from the disk and the calculated CRC value.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskStructureCrc {
    stored: u16,
    calculated: u16,
}

impl DiskStructureCrc {
    /// Return true if the stored CRC value matches the calculated CRC value.
    pub fn valid(&self) -> bool {
        self.stored == self.calculated
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskStructureMarker {
    System34(System34Marker),
    Placeholder,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskStructureGenericElement {
    NoElement,
    Marker,
    SectorHeader,
    SectorBadHeader,
    SectorData,
    SectorDeletedData,
    SectorBadData,
    SectorBadDeletedData,
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiskStructureElement {
    System34(System34Element),
    Placeholder,
}

impl From<DiskStructureElement> for DiskStructureGenericElement {
    fn from(elem: DiskStructureElement) -> Self {
        match elem {
            DiskStructureElement::System34(sys34elem) => sys34elem.into(),
            _ => DiskStructureGenericElement::NoElement,
        }
    }
}

impl DiskStructureElement {
    pub fn is_sector_header(&self) -> bool {
        matches!(
            self,
            DiskStructureElement::System34(System34Element::SectorHeader { .. })
        )
    }

    pub fn is_sector_data_marker(&self) -> bool {
        match self {
            DiskStructureElement::System34(elem) => elem.is_sector_data_marker(),
            _ => false,
        }
    }

    pub fn is_sector_data(&self) -> bool {
        match self {
            DiskStructureElement::System34(elem) => elem.is_sector_data(),
            _ => false,
        }
    }

    pub fn chsn(&self) -> Option<DiskChsn> {
        match self {
            DiskStructureElement::System34(System34Element::SectorHeader { chsn, .. }) => Some(*chsn),
            DiskStructureElement::System34(System34Element::Data { chsn, .. }) => Some(*chsn),
            _ => None,
        }
    }
}

/// The `DiskStructureParser` trait defines methods that must be implemented by a disk structure
/// parser. These methods are responsible for finding patterns of bytes within a bitstream, locating
/// markers and elements, and scanning a track for metadata.
pub trait DiskStructureParser {
    /// Find the provided pattern of decoded data bytes within the specified bitstream, starting at
    /// `offset` bits into the track.
    /// The pattern length is limited to 8 characters.
    /// # Arguments
    /// * `track` - The bitstream to search for the pattern.
    /// * `pattern` - The pattern to search for as a slice of bytes.
    /// * `offset` - The bit offset into the track to start searching.
    /// # Returns
    /// The bit offset of the pattern if found, otherwise `None`.
    fn find_data_pattern(track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize>;

    /// Find the next marker within the specified bitstream, starting at `offset` bits into the track.
    /// # Arguments
    /// * `track` - The bitstream to search for the marker.
    /// * `offset` - The bit offset into the track to start searching.
    /// # Returns
    /// A tuple containing the marker value and the bit offset of the marker if found, otherwise `None`.
    fn find_next_marker(track: &TrackDataStream, offset: usize) -> Option<(DiskStructureMarker, usize)>;

    /// Find a specific marker within the specified bitstream, starting at `offset` bits into the track.
    /// # Arguments
    /// * `track` - The bitstream to search for the marker.
    /// * `marker` - The marker to search for as a `DiskStructureMarker` enum.
    /// * `offset` - The bit offset into the track to start searching.
    /// * `limit` - An optional limit to the number of bits to search.
    /// # Returns
    /// A tuple containing the bit offset of the marker and the marker value if found, otherwise `None`.
    fn find_marker(
        track: &TrackDataStream,
        marker: DiskStructureMarker,
        offset: usize,
        limit: Option<usize>,
    ) -> Option<(usize, u16)>;

    /// Find the specified `DiskStructureElement` within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// # Arguments
    /// * `track` - The bitstream to search for the element.
    /// * `element` - The element to search for as a `DiskStructureElement` enum.
    /// * `offset` - The bit offset into the track to start searching.
    /// # Returns
    /// The bit offset of the element if found, otherwise `None`.
    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize>;

    /// Scan the specified track for markers.
    /// # Arguments
    /// * `track` - The bitstream to scan for markers.
    /// # Returns
    /// A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    fn scan_track_markers(track: &TrackDataStream) -> Vec<DiskStructureMarkerItem>;

    /// Scan the specified track for metadata.
    /// # Arguments
    /// * `track` - The bitstream to scan for metadata.
    /// * `markers` - A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    /// # Returns
    /// A vector of `DiskStructureMetadataItem` instances representing the metadata found in the track.
    fn scan_track_metadata(
        track: &mut TrackDataStream,
        markers: Vec<DiskStructureMarkerItem>,
    ) -> Vec<DiskStructureMetadataItem>;

    /// Create a clock map from the specified markers. A clock map enables random access into an encoded
    /// bitstream containing both clock and data bits.
    /// # Arguments
    /// * `markers` - A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    /// * `clock_map` - A mutable reference to a `BitVec` instance to store the clock map.
    fn create_clock_map(markers: &[DiskStructureMarkerItem], clock_map: &mut BitVec);

    /// Calculate a 16-bit CRC for a region of the specified track. The region is assumed to end with
    /// a CRC value.
    /// # Arguments
    /// * `track` - The bitstream to calculate the CRC for.
    /// * `bit_index` - The bit index to start calculating the CRC from.
    /// * `end` - The bit index to stop calculating the CRC at.
    /// # Returns
    /// A tuple containing the CRC value as specified by the track and the CRC value calculated from the
    /// data read.
    fn crc16(track: &mut TrackDataStream, bit_index: usize, end: usize) -> (u16, u16);
}
