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

//! The `track_schema` module defines a `TrackSchema` enum that represents a track schema used to
//! interpret the layout of a track.
//!
//! A track schema is responsible for interpreting the layout of syncs, gaps, and address markers on
//! a track, relying on a track's [TrackCodec] to decode the actual underlying data representation,
//! however a `TrackSchema` implementation need not be fully encoding agnostic - a certain schema
//! may only ever have been paired with a specific encoding type.
//!
//! A `TrackSchema` also defines the layout of a track for formatting operations, and defines any
//! applicable CRC algorithm.
//!
//! A `TrackSchema` typically contains no state.
//!
//! A disk image may contain tracks with varying `TrackSchema` values, such as dual-format disks
//! (Amiga/PC), (Atari ST/Amiga).
//!
//! For the time being, only the IBM System 34 schema used by IBM PC floppy disks is implemented.
//! This format is also used by 1.44MB HD MFM Macintosh diskettes.

mod dispatch;
pub mod system34;

use crate::{
    bitstream::{mfm::MFM_BYTE_LEN, TrackDataStream},
    track_schema::system34::{System34Element, System34Marker},
    types::chs::DiskChsn,
};
use bit_vec::BitVec;
use std::fmt::{self, Display, Formatter};

pub use TrackSchemaTrait as Schema;

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackSchema {
    System34,
    Amiga,
}

impl Display for TrackSchema {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TrackSchema::System34 => write!(f, "IBM System34"),
            TrackSchema::Amiga => write!(f, "Amiga"),
        }
    }
}

/// A `TrackMetadata` structure represents a collection of metadata items found in a track,
/// represented as `TrackMetadataItem`s.
#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackMetadata {
    pub(crate) items: Vec<TrackMetadataItem>,
}

impl TrackMetadata {
    /// Create a new `DiskStructureMetadata` instance from the specified items.
    pub(crate) fn new(items: Vec<TrackMetadataItem>) -> Self {
        TrackMetadata { items }
    }

    /// Return a vector of metadata items contained in the collection as `TrackMetadataItem`s.
    pub fn items(&self) -> Vec<TrackMetadataItem> {
        self.items.clone()
    }

    /// Add a new `TrackMetadataItem` to the collection.
    /// This method is not currently public as it does not make sense for the user to add to
    /// the metadata collection directly.
    #[allow(dead_code)]
    pub(crate) fn add_item(&mut self, item: TrackMetadataItem) {
        self.items.push(item);
    }

    /// Return a reference to the innermost metadata item that contains the specified index,
    /// along with a count of the total number of matching items (to handle overlapping items).
    /// # Arguments
    /// * `index` - The bit index to match.
    /// # Returns
    /// A tuple containing a reference to the metadata item and the count of matching items, or
    /// `None` if no match was found.
    pub fn item_at(&self, index: usize) -> Option<(&TrackMetadataItem, u32)> {
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
    pub fn sector_ids(&self) -> Vec<DiskChsn> {
        let mut sector_ids = Vec::new();

        for item in &self.items {
            if let TrackElement::System34(System34Element::SectorHeader { chsn, .. }) = item.elem_type {
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
            if let TrackElement::System34(System34Element::Data { .. }) = item.elem_type {
                // Should the data range for a sector include the address mark?
                // For now we will exclude it.
                data_ranges.push((item.start + (4 * MFM_BYTE_LEN), item.end));
            }
        }

        data_ranges
    }

    pub fn marker_ranges(&self) -> Vec<(usize, usize)> {
        let mut marker_ranges = Vec::new();

        for item in &self.items {
            if let TrackElement::System34(System34Element::Marker { .. }) = item.elem_type {
                marker_ranges.push((item.start, item.end));
            }
        }

        marker_ranges
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TrackMarkerItem {
    pub(crate) elem_type: TrackMarker,
    pub(crate) start: usize,
}

/// A `TrackMetadataItem` represents a single element of a track schema, such as an  address marker
/// or data marker. It encodes the start and end of the element (as raw bitstream offsets),
/// and optionally the status of any CRC field (valid for IDAM and DAM marks)
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackMetadataItem {
    pub(crate) elem_type: TrackElement,
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

/// A `TrackMarker` represents an encoding marker found in a track, such as an address marker or
/// data marker. Markers are used by FM and MFM encodings, utilizing unique clock bit patterns to
/// create an out-of-band signal for synchronization.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackMarker {
    System34(System34Marker),
    Placeholder,
}

/// A `TrackGenericElement` represents track elements in a generic fashion not specific to a
/// particular track schema.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackGenericElement {
    NoElement,
    Marker,
    SectorHeader,
    SectorBadHeader,
    SectorData,
    SectorDeletedData,
    SectorBadData,
    SectorBadDeletedData,
}

/// A `TrackElement` represents any element found in a track, representing any notable region
/// of the track such as markers, headers, sector data, syncs and gaps. `TrackElements` may overlap
/// and be nested within each other. All `TrackMarker`s are also `TrackElements`.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackElement {
    System34(System34Element),
    Placeholder,
}

/// Convert a `TrackElement` to a `TrackGenericElement`.
impl From<TrackElement> for TrackGenericElement {
    fn from(elem: TrackElement) -> Self {
        match elem {
            TrackElement::System34(sys34elem) => sys34elem.into(),
            _ => TrackGenericElement::NoElement,
        }
    }
}

impl TrackElement {
    pub fn is_sector_header(&self) -> bool {
        matches!(self, TrackElement::System34(System34Element::SectorHeader { .. }))
    }

    pub fn is_sector_data_marker(&self) -> bool {
        match self {
            TrackElement::System34(elem) => elem.is_sector_data_marker(),
            _ => false,
        }
    }

    pub fn is_sector_data(&self) -> bool {
        match self {
            TrackElement::System34(elem) => elem.is_sector_data(),
            _ => false,
        }
    }

    pub fn chsn(&self) -> Option<DiskChsn> {
        match self {
            TrackElement::System34(System34Element::SectorHeader { chsn, .. }) => Some(*chsn),
            TrackElement::System34(System34Element::Data { chsn, .. }) => Some(*chsn),
            _ => None,
        }
    }
}

/// The `TrackSchemaTrait` trait defines the interface that must be implemented by any track schema
/// parser.
/// These methods are responsible for finding patterns of bytes within a bitstream, locating
/// markers and elements, and scanning a track for metadata.
pub trait TrackSchemaTrait: Send + Sync {
    /// Find the provided pattern of decoded data bytes within the specified bitstream, starting at
    /// `offset` bits into the track.
    /// The pattern length is limited to 8 characters.
    /// # Arguments
    /// * `track` - The bitstream to search for the pattern.
    /// * `pattern` - The pattern to search for as a slice of bytes.
    /// * `offset` - The bit offset into the track to start searching.
    /// # Returns
    /// The bit offset of the pattern if found, otherwise `None`.
    fn find_data_pattern(&self, track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize>;

    /// Find the next marker within the specified bitstream, starting at `offset` bits into the track.
    /// # Arguments
    /// * `track` - The bitstream to search for the marker.
    /// * `offset` - The bit offset into the track to start searching.
    /// # Returns
    /// A tuple containing the marker value and the bit offset of the marker if found, otherwise `None`.
    fn find_next_marker(&self, track: &TrackDataStream, offset: usize) -> Option<(TrackMarker, usize)>;

    /// Find a specific marker within the specified bitstream, starting at `offset` bits into the track.
    /// # Arguments
    /// * `track` - The bitstream to search for the marker.
    /// * `marker` - The marker to search for as a `DiskStructureMarker` enum.
    /// * `offset` - The bit offset into the track to start searching.
    /// * `limit` - An optional limit to the number of bits to search.
    /// # Returns
    /// A tuple containing the bit offset of the marker and the marker value if found, otherwise `None`.
    fn find_marker(
        &self,
        track: &TrackDataStream,
        marker: TrackMarker,
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
    fn find_element(&self, track: &TrackDataStream, element: TrackElement, offset: usize) -> Option<usize>;

    /// Scan the specified track for markers.
    /// # Arguments
    /// * `track` - The bitstream to scan for markers.
    /// # Returns
    /// A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    fn scan_track_markers(&self, track: &TrackDataStream) -> Vec<TrackMarkerItem>;

    /// Scan the specified track for metadata.
    /// # Arguments
    /// * `track` - The bitstream to scan for metadata.
    /// * `markers` - A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    /// # Returns
    /// A vector of `DiskStructureMetadataItem` instances representing the metadata found in the track.
    fn scan_track_metadata(&self, track: &mut TrackDataStream, markers: Vec<TrackMarkerItem>)
        -> Vec<TrackMetadataItem>;

    /// Create a clock map from the specified markers. A clock map enables random access into an encoded
    /// bitstream containing both clock and data bits.
    /// # Arguments
    /// * `markers` - A vector of `DiskStructureMarkerItem` instances representing the markers found in the track.
    /// * `clock_map` - A mutable reference to a `BitVec` instance to store the clock map.
    fn create_clock_map(&self, markers: &[TrackMarkerItem], clock_map: &mut BitVec);

    /// Calculate a 16-bit CRC for a region of the specified track. The region is assumed to end with
    /// a CRC value.
    /// # Arguments
    /// * `track` - The bitstream to calculate the CRC for.
    /// * `bit_index` - The bit index to start calculating the CRC from.
    /// * `end` - The bit index to stop calculating the CRC at.
    /// # Returns
    /// A tuple containing the CRC value as specified by the track data and the calculated CRC
    /// value.
    fn crc16(&self, track: &mut TrackDataStream, bit_index: usize, end: usize) -> (u16, u16);

    /// Calculate a 16-bit CRC for the specified byte slice. The end of the slice should contain the
    /// encoded CRC.
    /// # Arguments
    /// * `data` - A byte slice representing the data to calculate a CRC for.
    /// # Returns
    /// A tuple containing the CRC value contained in the byte slice, and the calculated CRC
    /// value.
    fn crc16_bytes(&self, data: &[u8]) -> (u16, u16);
}
