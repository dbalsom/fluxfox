/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

//! The `track_schema` module defines a [TrackSchema] enum that represents a track schema used to
//! interpret the layout of a track, and a [TrackSchemaParser] trait that defines the interface for
//! track schema parsers.
//!
//! A track schema parser is responsible for interpreting the layout of syncs, gaps, and address
//! markers on a track, relying on a track's [TrackCodec] to decode the actual underlying data
//! representation.
//! However, a [TrackSchemaParser] implementation need not be fully encoding agnostic - a certain
//! schema may only ever have been paired with specific encoding types.
//!
//! A [TrackSchema] also defines the layout of a track for formatting operations, and defines any
//! applicable CRC algorithm.
//!
//! A track schema parser typically maintains no state. Since this is not object-compatible, the
//! [TrackSchemaParser] trait is implemented on the [TrackSchema] enum directly.
//!
//! A disk image may contain tracks with varying [TrackSchema] values, such as dual-format disks
//! (Amiga/PC), (Atari ST/Amiga).

use std::{
    fmt::{self, Display, Formatter},
    ops::Range,
};

#[cfg(feature = "amiga")]
pub mod amiga;
mod dispatch;
mod meta_encoding;
pub mod system34;

use crate::{
    bitstream_codec::{mfm::MFM_BYTE_LEN, TrackDataStream},
    track::{TrackAnalysis, TrackSectorScanResult},
    track_schema::system34::{System34Element, System34Marker, System34Variant},
    types::{chs::DiskChsn, IntegrityCheck, Platform, RwScope, SectorAttributes},
    SectorId,
    SectorIdQuery,
    SectorMapEntry,
};

#[cfg(feature = "amiga")]
use crate::track_schema::amiga::{AmigaElement, AmigaMarker, AmigaVariant};

use crate::source_map::SourceMap;
use bit_vec::BitVec;

pub enum TrackSchemaVariant {
    System34(System34Variant),
    #[cfg(feature = "amiga")]
    Amiga(AmigaVariant),
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, strum::EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackSchema {
    #[default]
    System34,
    #[cfg(feature = "amiga")]
    Amiga,
}

impl Display for TrackSchema {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TrackSchema::System34 => write!(f, "IBM System34"),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => write!(f, "Amiga"),
        }
    }
}

impl TryFrom<Platform> for TrackSchema {
    type Error = ();
    /// Convert a `Platform` to a `TrackSchema`. This provides a sensible default, but is not
    /// exhaustive as a platform may use multiple track schemas.
    fn try_from(platform: Platform) -> Result<TrackSchema, Self::Error> {
        match platform {
            Platform::IbmPc => Ok(TrackSchema::System34),
            #[cfg(feature = "amiga")]
            Platform::Amiga => Ok(TrackSchema::Amiga),
            #[cfg(not(feature = "amiga"))]
            Platform::Amiga => Err(()),
            #[cfg(feature = "macintosh")]
            Platform::Macintosh => Err(()),
            #[cfg(not(feature = "macintosh"))]
            Platform::Macintosh => Err(()),
            #[cfg(feature = "atari_st")]
            Platform::AtariSt => Ok(TrackSchema::System34),
            #[cfg(not(feature = "atari_st"))]
            Platform::AtariSt => Err(()),
            #[cfg(feature = "apple_ii")]
            Platform::AppleII => Err(()),
            #[cfg(not(feature = "apple_ii"))]
            Platform::AppleII => Err(()),
        }
    }
}

/// A `TrackMetadata` structure represents a collection of metadata items found in a track,
/// represented as `TrackElementInstance`s.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackMetadata {
    pub(crate) items: Vec<TrackElementInstance>,
    pub(crate) sector_ids: Vec<SectorId>,
    pub(crate) valid_sector_ids: Vec<SectorId>,
    pub(crate) element_map: SourceMap,
}

impl TrackMetadata {
    /// Create a new `DiskStructureMetadata` instance from the specified items.
    pub(crate) fn new(items: Vec<TrackElementInstance>, schema: TrackSchema) -> Self {
        TrackMetadata {
            sector_ids: Self::find_sector_ids(&items),
            valid_sector_ids: Self::find_valid_sector_ids(&items),
            element_map: schema.build_element_map(&items),
            items,
        }
    }

    /// Clear all metadata items from the collection.
    pub(crate) fn clear(&mut self) {
        self.items.clear();
        self.sector_ids.clear();
        self.valid_sector_ids.clear();
    }

    /// Return a vector of metadata items contained in the collection as `TrackElementInstance`s.
    pub fn elements(&self) -> &[TrackElementInstance] {
        &self.items
    }

    /// Add a new `TrackElementInstance` to the collection.
    /// This method is not currently public as it does not make sense for the user to add to
    /// the metadata collection directly.
    #[allow(dead_code)]
    pub(crate) fn add_element(&mut self, item: TrackElementInstance) {
        self.items.push(item);
    }

    /// Get the `TrackElementInstance` at the specified element index, or `None` if the index is
    /// out of bounds.
    pub fn item(&self, index: usize) -> Option<&TrackElementInstance> {
        self.items.get(index)
    }

    /// Return a reference to the innermost metadata item that contains the specified index,
    /// along with a count of the total number of matching items (to handle overlapping items).
    /// # Arguments
    /// * `index` - The bit index to match.
    /// # Returns
    /// A tuple containing a reference to the metadata item and the count of matching items, or
    /// `None` if no match was found.
    pub fn item_at(&self, index: usize) -> Option<(&TrackElementInstance, u32)> {
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

    /// Attempt a fast hit test via binary search that returns the smallest element containing
    /// the specified bit index.
    pub(crate) fn hit_test(&self, bit_index: usize) -> Option<(&TrackElementInstance, usize)> {
        if self.items.is_empty() {
            //og::warn!("hit_test() called on empty metadata collection");
            return None;
        }

        // Find the first element where `start` is greater than `bit_index` using binary search
        let pos = self
            .items
            .binary_search_by_key(&bit_index, |e| e.start)
            .unwrap_or_else(|x| x);

        //log::debug!("pos: {}", pos);

        // Search backward and forward from `pos` for candidates containing `bit_index`
        let mut result: Option<(&TrackElementInstance, usize)> = None;
        let mut smallest_length = usize::MAX;

        // Check elements before and including `pos`
        for i in (0..=pos.min(self.items.len() - 1)).rev() {
            let elem = &self.items[i];
            if elem.contains(bit_index) && elem.len() < smallest_length {
                smallest_length = elem.len();
                result = Some((elem, i));
            }
        }

        // Check elements after `pos`
        for i in pos..self.items.len() {
            let elem = &self.items[i];
            if elem.start > bit_index {
                break; // Later elements can't contain `bit_index`
            }
            if elem.contains(bit_index) && elem.len() < smallest_length {
                smallest_length = elem.len();
                result = Some((elem, i));
            }
        }

        result
    }

    /// Return the number of sectors represented in the metadata collection.
    /// To be counted, a sector must have a corresponding, valid sector header.
    pub fn sector_ct(&self) -> u8 {
        let mut sector_ct = 0;
        for item in &self.items {
            if item.element.is_sector_data_marker() {
                sector_ct += 1;
            }
        }
        sector_ct
    }

    pub fn markers(&self) -> Vec<TrackElementInstance> {
        let mut markers = Vec::new();
        for item in &self.items {
            if item.element.is_sector_data_marker() {
                markers.push(*item);
            }
        }
        markers
    }

    /// Return a vector of [SectorMapEntry]s representing the sectors contained in the metadata
    pub fn sector_list(&self) -> Vec<SectorMapEntry> {
        let mut sector_list = Vec::new();

        for item in &self.items {
            #[allow(clippy::unreachable)]
            match item.element {
                TrackElement::System34(System34Element::SectorData {
                    chsn,
                    address_error,
                    data_error,
                    deleted,
                }) => {
                    sector_list.push(SectorMapEntry {
                        chsn,
                        attributes: SectorAttributes {
                            address_error,
                            data_error,
                            deleted_mark: deleted,
                            no_dam: false,
                        },
                    });
                }
                #[cfg(feature = "amiga")]
                TrackElement::Amiga(AmigaElement::SectorData {
                    chsn,
                    address_error,
                    data_error,
                }) => {
                    sector_list.push(SectorMapEntry {
                        chsn,
                        attributes: SectorAttributes {
                            address_error,
                            data_error,
                            deleted_mark: false, // Amiga sectors can't be deleted
                            no_dam: false, // Can Amiga sectors be missing data? There is no DAM marker to check for.
                        },
                    });
                }
                _ => {}
            }
        }

        sector_list
    }

    /// Return a reference to a slice of the [SectorId]s represented in the metadata collection.
    /// Note that the number of Sector IDs may not match the number of sectors returned by
    /// sector_list(), as not all sector headers may correspond to valid sector data, especially
    /// on copy-protected disks.
    pub fn sector_ids(&self) -> &[SectorId] {
        &self.sector_ids
    }

    /// Return a reference to a slice of the [SectorId]s represented in the metadata collection
    /// that have valid sector headers (i.e. no address errors).
    /// Note that the number of Sector IDs may not match the number of sectors returned by
    /// sector_list(), as not all sector headers may correspond to valid sector data, especially
    /// on copy-protected disks.
    pub fn valid_sector_ids(&self) -> &[SectorId] {
        &self.valid_sector_ids
    }

    /// Return a vector of Sector IDs as [SectorId] represented in the metadata collection.
    /// Note that the number of Sector IDs may not match the number of sectors returned by
    /// sector_list(), as not all sector headers may correspond to valid sector data, especially
    /// on copy-protected disks.
    fn find_valid_sector_ids(items: &[TrackElementInstance]) -> Vec<SectorId> {
        let mut sector_ids: Vec<SectorId> = Vec::new();

        for item in items {
            #[allow(clippy::unreachable)]
            match item.element {
                TrackElement::System34(System34Element::SectorHeader {
                    chsn, address_error, ..
                }) if address_error == false => {
                    sector_ids.push(chsn);
                }
                #[cfg(feature = "amiga")]
                TrackElement::Amiga(AmigaElement::SectorHeader {
                    chsn, address_error, ..
                }) if address_error == false => {
                    sector_ids.push(chsn);
                }
                _ => {}
            }
        }

        sector_ids
    }

    /// Return a vector of Sector IDs as [SectorId] represented in the metadata collection.
    /// Note that the number of Sector IDs may not match the number of sectors returned by
    /// sector_list(), as not all sector headers may correspond to valid sector data, especially
    /// on copy-protected disks.
    fn find_sector_ids(items: &[TrackElementInstance]) -> Vec<SectorId> {
        let mut sector_ids: Vec<SectorId> = Vec::new();

        for item in items {
            #[allow(clippy::unreachable)]
            match item.element {
                TrackElement::System34(System34Element::SectorHeader { chsn, .. }) => {
                    sector_ids.push(chsn);
                }
                #[cfg(feature = "amiga")]
                TrackElement::Amiga(AmigaElement::SectorHeader { chsn, .. }) => {
                    sector_ids.push(chsn);
                }
                _ => {}
            }
        }

        sector_ids
    }

    /// Return a vector of data ranges representing the start and end bit indices of sector data.
    /// Primarily used as helper for disk visualization.
    /// # Returns
    /// A vector of tuples containing the start and end bit indices of sector data.
    pub fn data_ranges(&self) -> Vec<Range<usize>> {
        let mut data_ranges = Vec::new();

        for instance in &self.items {
            match instance.element {
                TrackElement::System34(System34Element::SectorData { .. }) => {
                    // Should the data range for a sector include the address mark?
                    // For now we will exclude it.
                    data_ranges.push(Range::from(instance.start + (4 * MFM_BYTE_LEN)..instance.end));
                }
                #[cfg(feature = "amiga")]
                TrackElement::Amiga(AmigaElement::SectorData { .. }) => {
                    data_ranges.push(Range::from(instance.start..instance.end));
                }
                _ => {}
            }
        }

        data_ranges
    }

    pub fn header_ranges(&self) -> Vec<Range<usize>> {
        let mut header_ranges: Vec<Range<usize>> = Vec::new();

        for item in &self.items {
            if item.element.is_sector_header() {
                header_ranges.push(Range::from(item.start..item.end));
            }
        }

        header_ranges
    }

    pub fn marker_ranges(&self) -> Vec<Range<usize>> {
        let mut marker_ranges: Vec<Range<usize>> = Vec::new();

        for item in &self.items {
            if let TrackElement::System34(System34Element::Marker { .. }) = item.element {
                marker_ranges.push(Range::from(item.start..item.end));
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

/// A `TrackElementInstance` represents a single element of a track schema, such as an address marker
/// or data marker. It encodes the start and end of the element (as raw bitstream offsets),
/// and optionally the status of any CRC field (valid for IDAM and DAM marks)
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackElementInstance {
    pub(crate) element: TrackElement,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) chsn: Option<DiskChsn>,
    // A flag indicating that this element belongs to the last sector on the track
    pub(crate) last_sector: bool,
}

impl TrackElementInstance {
    pub fn contains(&self, bit_index: usize) -> bool {
        self.start <= bit_index && self.end >= bit_index
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }
}

/// A [TrackMarker] represents an encoding marker found in a track, such as an address marker or
/// data marker. Markers are used by FM and MFM encodings, utilizing unique clock bit patterns to
/// create an out-of-band signal for synchronization.
///
/// When parsing a track, [TrackMarker]s are discovered first, effectively dividing a track into
/// regions, which are then used to discover [TrackElement]s to populate a [TrackMetadata]
/// collection.
///
/// In the event that FM/MFM markers are not applicable to a track schema, synthetic markers can
/// be created to divide tracks into regions for parsing metadata.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackMarker {
    System34(System34Marker),
    #[cfg(feature = "amiga")]
    Amiga(AmigaMarker),
    Placeholder,
}

/// A [GenericTrackElement] represents track elements in a generic fashion, not specific to a
/// particular track schema. This is useful for operations that do not require schema-specific
/// knowledge, such as disk visualization, which maps [GenericTrackElement]s to colors.
///
/// Elements defined by [TrackSchemaParser] implementations should implement `From<T>` to provide
/// a conversion to [GenericTrackElement]. Not all track schemas may use all generic elements -
/// this is fine!
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GenericTrackElement {
    NullElement,
    Marker,
    SectorHeader,
    SectorBadHeader,
    SectorData,
    SectorDeletedData,
    SectorBadData,
    SectorBadDeletedData,
}

impl Display for GenericTrackElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use GenericTrackElement::*;
        match self {
            NullElement => write!(f, "Null"),
            Marker => write!(f, "Marker"),
            SectorHeader => write!(f, "Sector Header"),
            SectorBadHeader => write!(f, "Sector Header (Bad)"),
            SectorData => write!(f, "Sector Data"),
            SectorDeletedData => write!(f, "Deleted Sector Data"),
            SectorBadData => write!(f, "Sector Data (Bad)"),
            SectorBadDeletedData => write!(f, "Deleted Sector Data (Bad)"),
        }
    }
}

/// A [TrackElement] encompasses the concept of a track 'element', representing any notable region
/// of the track such as markers, headers, sector data, syncs and gaps. [TrackElement]s may overlap
/// and be nested within each other.
/// [TrackMarker]s are used to discover and classify [TrackElement]s, and some [TrackElements]
/// represent markers.
/// A [TrackElement] contains only metadata. Its position and size are represented in by a
/// [TrackElementInstance].
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackElement {
    System34(System34Element),
    #[cfg(feature = "amiga")]
    Amiga(AmigaElement),
    Placeholder,
}

/// Convert a `TrackElement` to a `TrackGenericElement`.
impl From<TrackElement> for GenericTrackElement {
    fn from(elem: TrackElement) -> Self {
        match elem {
            TrackElement::System34(sys34elem) => sys34elem.into(),
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(ami_elem) => ami_elem.into(),
            _ => GenericTrackElement::NullElement,
        }
    }
}

impl TrackElement {
    pub fn is_marker(&self) -> bool {
        match self {
            TrackElement::System34(System34Element::Marker { .. }) => true,
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(AmigaElement::Marker { .. }) => true,
            _ => false,
        }
    }

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
            TrackElement::System34(System34Element::SectorData { chsn, .. }) => Some(*chsn),
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(AmigaElement::SectorHeader { chsn, .. }) => Some(*chsn),
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(AmigaElement::SectorData { chsn, .. }) => Some(*chsn),
            _ => None,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            TrackElement::System34(elem) => elem.size(),
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(elem) => elem.size(),
            _ => 0,
        }
    }

    pub fn range(&self, scope: RwScope) -> Option<Range<usize>> {
        match self {
            TrackElement::System34(element) => Some(element.range(scope)),
            #[cfg(feature = "amiga")]
            TrackElement::Amiga(element) => Some(element.range(scope)),
            _ => None,
        }
    }
}

/// The `TrackSchemaParser` trait defines the interface that must be implemented by any track schema
/// parser.
/// These methods are responsible for finding patterns of bytes within a bitstream, locating
/// markers and elements, and scanning a track for metadata.
pub(crate) trait TrackSchemaParser: Send + Sync {
    /*
        /// Find the provided pattern of decoded data bytes within the specified bitstream, starting at
        /// `offset` bits into the track.
        /// The pattern length is limited to 8 characters.
        /// # Arguments
        /// * `track` - The bitstream to search for the pattern.
        /// * `pattern` - The pattern to search for as a slice of bytes.
        /// * `index` - The bit index to start searching at.
        /// # Returns
        /// The bit offset of the pattern if found, otherwise `None`.
        fn find_data_pattern(&self, track: &TrackDataStream, pattern: &[u8], index: usize) -> Option<usize>;
    */

    /// Analyze the elements in the specified track and return a [TrackAnalysis] structure containing
    /// the results of the analysis. This method is responsible for identifying any 'nonstandard'
    /// conditions on the track that may affect the ability to represent the track in any given
    /// disk image format.
    fn analyze_elements(&self, elements: &TrackMetadata) -> TrackAnalysis;

    /// TODO: we could combine find_next_marker and find_marker if the latter took an Option<TrackMarker>
    /// Find the next marker (of any kind) within the specified bitstream, starting at `index` bits
    /// into the track.
    /// # Arguments
    /// * `track` - The [TrackDataStream] to search for the marker.
    /// * `index` - The bit index to start searching at.
    /// # Returns
    /// A tuple containing the marker type and the bit offset of the marker if found, otherwise `None`.
    fn find_next_marker(&self, track: &TrackDataStream, index: usize) -> Option<(TrackMarker, usize)>;

    /// Find a specific marker within the specified [TrackDataStream], starting at `index` bits
    /// into the track.
    /// # Arguments
    /// * `stream` - The [TrackDataStream] to search for the marker.
    /// * `marker` - The [TrackMarker] to search for.
    /// * `index`  - The bit index to start searching at.
    /// * `limit`  - An optional bit index to terminate the search at.
    /// # Returns
    /// A tuple containing the bit offset of the marker and the marker value if found, otherwise `None`.
    fn find_marker(
        &self,
        stream: &TrackDataStream,
        marker: TrackMarker,
        index: usize,
        limit: Option<usize>,
    ) -> Option<(usize, u16)>;

    /// Match the element in `elements` that corresponds to sector data specified by `id` within the
    /// list of track element instances. This function does not directly read the stream - so
    /// valid metadata must have been previously scanned before it can be used.
    ///
    /// A track schema may not have a concept of sectors, in which case this method should simply
    /// return `None`.
    ///
    /// # Arguments
    /// * `stream` - The [TrackDataStream] to search for the marker.
    /// * `id`     - The [SectorIdQuery] to use as matching criteria.
    /// * `index`  - The bit index within the track to start searching at.
    /// * `limit`  - An optional bit index to terminate the search at.
    /// # Returns
    /// A [TrackSectorScanResult] containing the result of the sector search.
    fn match_sector_element(
        &self,
        id: impl Into<SectorIdQuery>,
        elements: &[TrackElementInstance],
        index: usize,
        limit: Option<usize>,
    ) -> TrackSectorScanResult;

    /// Decode the element specified by `TrackElementInstance` from the track data stream into the
    /// provided buffer. The data may be transformed or decoded as necessary depending on the
    /// schema implementation - for example, Amiga sector data elements will be reconstructed from
    /// odd/even bit pairs.
    ///
    /// Not all schemas will support decoding all elements. In this case, the method should return
    /// 0.
    /// # Arguments
    /// * `stream` - The [TrackDataStream] to read the element from.
    /// * `item`   - The [TrackElementInstance] specifying the element to read.
    /// * `buf`    - A mutable reference to a byte slice to store the element data.
    ///              This buffer should be at least `TrackElement::size()` bytes long.
    /// * `scope`  - The read/write scope of the operation. An element may be partially decoded
    ///              by limiting the scope. This is useful, for example, when reading only the
    ///              sector data of a sector data element.
    /// # Returns
    /// * A [Range] representing the start and end byte indices into the buffer corresponding to
    ///   the requested `scope`.
    /// * An optional [IntegrityCheck] value representing the integrity of the data read.
    ///   Different track schemas may have different ways of verifying data integrity.
    fn decode_element(
        &self,
        stream: &TrackDataStream,
        element: &TrackElementInstance,
        scope: RwScope,
        buf: &mut [u8],
    ) -> (Range<usize>, Option<IntegrityCheck>);

    /// Encode the element specified by `TrackElementInstance` from the track data stream from the
    /// provided buffer. The data may be transformed or encoded as necessary depending on the
    /// schema implementation - for example, marker elements will receive appropriate clock patterns
    /// and Amiga sector data elements will be separated into odd/even bit pairs.
    ///
    /// Not all schemas will support encoding all elements. In this case, the method should return
    /// 0.
    /// # Arguments
    /// * `stream` - The [TrackDataStream] to write the element to.
    /// * `item`   - The [TrackElementInstance] specifying the element to write.
    /// * `buf`    - A reference to a byte slice that represents the element data.
    /// * `scope`  - The read/write scope of the operation. An element may be partially updated
    ///              by limiting the scope.
    /// # Returns
    /// The number of bytes written to the track.
    fn encode_element(
        &self,
        stream: &mut TrackDataStream,
        item: &mut TrackElementInstance,
        offset: usize,
        scope: RwScope,
        buf: &[u8],
    ) -> usize;

    /*
        /// Find the specified `TrackElement` within the specified bitstream, starting at `offset` bits
        /// into the track.
        /// # Arguments
        /// * `stream` - The [TrackDataStream] to search for the element.
        /// * `element` - The element to search for as a `TrackElement` enum.
        /// * `index`  - The bit index to start searching at.
        /// # Returns
        /// The bit offset of the element if found, otherwise `None`.
        fn find_element(&self, track: &TrackDataStream, element: TrackElement, index: usize) -> Option<usize>;
    */

    /// Scan the specified track for markers.
    /// # Arguments
    /// * `stream` - The [TrackDataStream] to scan for markers
    /// # Returns
    /// A vector of [TrackMarkerItem]s representing the markers found in the track. If no markers
    /// are found, an empty vector is returned.
    fn scan_for_markers(&self, track: &TrackDataStream) -> Vec<TrackMarkerItem>;

    /// Scan the specified track for [TrackElements].
    /// # Arguments
    /// * `track` - The [TrackDataStream] to scan for metadata.
    /// * `markers` - A vector of [TrackMarkerItem]s representing the markers found in the track.
    /// # Returns
    /// A vector of [TrackElementInstance] instances representing the metadata found in the track.
    /// If no metadata is found, an empty vector is returned.
    fn scan_for_elements(
        &self,
        track: &mut TrackDataStream,
        markers: Vec<TrackMarkerItem>,
    ) -> Vec<TrackElementInstance>;

    /// Create a clock map from the specified markers. A clock map enables random access into an encoded
    /// bitstream containing both clock and data bits.
    /// # Arguments
    /// * `markers` - A vector of [TrackMarkerItem]s representing the markers found in the track.
    /// * `clock_map` - A mutable reference to a [BitVec] to store the clock map.
    fn create_clock_map(&self, markers: &[TrackMarkerItem], clock_map: &mut BitVec);

    /// Calculate a 16-bit CRC for a region of the specified track. The region is assumed to end with
    /// a CRC value.
    /// # Arguments
    /// * `track` - The [TrackDataStream] to calculate the CRC for.
    /// * `index` - The bit index to start calculating the CRC from.
    /// * `index_end` - The bit index to stop calculating the CRC at.
    /// # Returns
    /// A tuple containing the CRC value as specified by the track data and the calculated CRC
    /// value.
    fn crc_u16(&self, track: &mut TrackDataStream, index: usize, index_end: usize) -> (u16, u16);

    /// Calculate a 16-bit CRC for the specified byte slice. The end of the slice should contain the
    /// encoded CRC.
    /// # Arguments
    /// * `buf` - A byte slice over which to calculate the CRC.
    /// # Returns
    /// A tuple containing the CRC value contained in the byte slice, and the calculated CRC value.
    fn crc_u16_buf(&self, buf: &[u8]) -> (u16, u16);

    fn build_element_map(&self, elements: &[TrackElementInstance]) -> SourceMap;
}
