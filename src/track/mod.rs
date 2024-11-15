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

    src/track/mod.rs

    Defines the Track trait

*/
pub mod bitstream;
pub mod fluxstream;
pub mod metasector;

use crate::{
    bitstream::TrackDataStream,
    chs::DiskChsnQuery,
    diskimage::{
        ReadSectorResult, ReadTrackResult, RwSectorScope, ScanSectorResult, SectorDescriptor, WriteSectorResult,
    },
    structure_parsers::{system34::System34Standard, DiskStructureMetadata},
    track::{bitstream::BitStreamTrack, fluxstream::FluxStreamTrack, metasector::MetaSectorTrack},
    DiskCh, DiskChs, DiskChsn, DiskDataEncoding, DiskDataRate, DiskDataResolution, DiskDensity, DiskImageError,
    DiskRpm, SectorMapEntry,
};
use sha1_smol::Digest;
use std::any::Any;

/// A struct containing information about a track's encoding, data rate, density, RPM, bit length,
/// and sector count.
#[derive(Debug)]
pub struct TrackInfo {
    /// The type of encoding used on the track as a `DiskDataEncoding` enum.
    pub encoding: DiskDataEncoding,
    /// The data rate of the track as a `DiskDataRate` enum.
    pub data_rate: DiskDataRate,
    /// The density of the track as a `DiskDensity` enum, or `None` if density has not been determined.
    pub density: Option<DiskDensity>,
    /// The RPM of the track as an `DiskRpm`, or `None` if RPM has not been determined.
    pub rpm: Option<DiskRpm>,
    /// The bit length of the track.
    pub bit_length: usize,
    /// The number of sectors on the track.
    pub sector_ct: usize,
}

/// A struct representing the result of a sector scan operation on a track.
pub enum TrackSectorScanResult {
    /// A variant indicating the specified sector ID was found on the track.
    Found {
        /// The starting bit offset of the sector element on the track.
        element_start: usize,
        /// The ending bit offset of the sector element on the track.
        element_end: usize,
        /// The matching sector ID found on the track.
        sector_chsn: DiskChsn,
        /// A boolean flag indicating whether the sector's address CRC is valid.
        address_crc_valid: bool,
        /// A boolean flag indicating whether the sector's data CRC is valid.
        data_crc_valid: bool,
        /// A boolean flag indicating whether the sector had a deleted data marker.
        deleted: bool,
        /// A boolean flag indicating whether the sector ID was matched, but no sector data was found.
        no_dam: bool,
    },
    /// A variant indicating the specified sector ID was not found on the track.
    NotFound {
        /// A sector ID with a different cylinder ID as the requested sector was found while scanning
        /// the track.
        wrong_cylinder: bool,
        /// A sector ID with a different head ID as the requested sector was found while scanning
        /// the track.
        bad_cylinder: bool,
        /// A sector ID with a different head ID as the requested sector was found while scanning
        /// the track.
        wrong_head: bool,
    },
    #[allow(dead_code)] // use this someday (wrong track encoding?)
    Incompatible,
}

/// A structure containing information about a track's consistency vs a standard track.
#[derive(Debug, Default)]
pub struct TrackConsistency {
    /// A boolean flag indicating whether the track contains sectors with bad data CRCs.
    pub bad_data_crc: bool,
    /// A boolean flag indicating whether the track contains sectors with bad address CRCs.
    pub bad_address_crc: bool,
    /// A boolean flag indicating whether the track contains sectors with deleted data.
    pub deleted_data: bool,
    /// A boolean flag indicating whether the track contains sectors with no DAM.
    pub no_dam: bool,
    /// An optional value indicating the consistent sector size of the track, or None if the track
    /// contains sectors of varying sizes.
    pub consistent_sector_size: Option<u8>,
    /// A boolean flag indicating whether the track contains nonconsecutive sectors.
    pub nonconsecutive_sectors: bool,
    /// A boolean flag indicating whether the track contains overlapping sectors.
    pub overlapping_sectors: bool,
    /// A boolean flag indicating whether the track contains sectors that cross the index.
    pub sector_crossing_index: bool,
    /// The number of sectors on the track.
    pub sector_ct: usize,
}

impl TrackConsistency {
    /// Merge a `TrackConsistency` struct with another, by OR'ing together their boolean values.
    pub fn join(&mut self, other: &TrackConsistency) {
        self.bad_data_crc |= other.bad_data_crc;
        self.bad_address_crc |= other.bad_address_crc;
        self.deleted_data |= other.deleted_data;
        self.no_dam |= other.no_dam;
        self.nonconsecutive_sectors |= other.nonconsecutive_sectors;
        self.overlapping_sectors |= other.overlapping_sectors;
        self.sector_crossing_index |= other.sector_crossing_index;

        if other.consistent_sector_size.is_none() {
            self.consistent_sector_size = None;
        }
    }
}

#[cfg_attr(feature = "serde", typetag::serde)]
pub trait Track: Any + Send + Sync {
    /// Return the resolution of the track as a `DiskDataResolution`.
    /// This can be used to determine the track's underlying representation, especially if you wish
    /// to downcast the track to a specific type.
    fn resolution(&self) -> DiskDataResolution;
    /// Return a reference to the track as a `&dyn Any`, for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Return a mutable reference to the track as a `&mut dyn Any`, for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Downcast the track to a `MetaSectorTrack` reference, if possible.
    fn as_metasector_track(&self) -> Option<&MetaSectorTrack>;
    /// Downcast the track to a `BitStreamTrack` reference, if possible.
    fn as_bitstream_track(&self) -> Option<&BitStreamTrack>;
    /// Downcast the track to a `FluxStreamTrack` reference, if possible.
    fn as_fluxstream_track(&self) -> Option<&FluxStreamTrack>;
    /// Downcast the track to a mutable `FluxStreamTrack` reference, if possible.
    fn as_fluxstream_track_mut(&mut self) -> Option<&mut FluxStreamTrack>;
    /// Return the track's physical cylinder and head as a `DiskCh`.
    fn ch(&self) -> DiskCh;
    /// Set the track's physical cylinder and head.
    fn set_ch(&mut self, ch: DiskCh);
    /// Return the encoding of the track as `DiskDataEncoding`.
    fn encoding(&self) -> DiskDataEncoding;
    /// Return information about the track as a `TrackInfo` struct.
    fn info(&self) -> TrackInfo;
    /// Return a list of the track's metadata, or None if the track has not been scanned for metadata.
    fn metadata(&self) -> Option<&DiskStructureMetadata>;
    /// Return a count of the sectors on the track.
    fn get_sector_ct(&self) -> usize;
    /// Returns `true` if the track contains a sector with the specified ID.
    ///
    /// # Parameters
    /// - `id`: The sector ID to search for.
    /// - `id_chsn`: An optional `DiskChsn` value. If provided, the `id` parameter is ignored and
    ///              the entire `DiskChsn` value is used to search for the sector.
    fn has_sector_id(&self, id: u8, id_chsn: Option<DiskChsn>) -> bool;
    /// Returns a vector of `SectorMapEntry` structs representing the sectors on the track.
    fn get_sector_list(&self) -> Vec<SectorMapEntry>;
    /// Adds a new sector to a track in the disk image, essentially 'formatting' a new sector,
    /// This function is only valid for tracks with `MetaSector` resolution.
    ///
    /// # Parameters
    /// - `sd`: A reference to a `SectorDescriptor` containing the sector data and metadata.
    /// - `alternate`: A boolean flag indicating whether the sector is an alternate sector.
    ///                Alternate sectors will calculate weak bit masks for the existing sector.
    ///                If the existing sector does not exist, the alternate flag is ignored.
    ///
    /// # Returns
    /// - `Ok(())` if the sector was successfully mastered.
    /// - `Err(DiskImageError::SeekError)` if the head value in `chs` is greater than 1 or the track map does not contain the specified cylinder.
    /// - `Err(DiskImageError::UnsupportedFormat)` if the track data is not of `MetaSector` resolution.
    fn add_sector(&mut self, sd: &SectorDescriptor, alternate: bool) -> Result<(), DiskImageError>;
    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags where are needed
    /// when handling ByteStream images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    fn read_sector(
        &mut self,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError>;

    fn scan_sector(
        &self,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
    ) -> Result<ScanSectorResult, DiskImageError>;

    fn write_sector(
        &mut self,
        id: DiskChsnQuery,
        offset: Option<usize>,
        write_data: &[u8],
        scope: RwSectorScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError>;

    /// Return a hash that uniquely identifies the track data. Intended for use in identifying
    /// duplicate tracks.
    fn get_hash(&mut self) -> Digest;
    /// Read all sectors from the track. The data is returned within a `ReadSectorResult` struct
    /// which also sets some convenience metadata flags which are needed when handling ByteStream
    /// images.
    /// Unlike `read_sectors`, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the µPD765 FDC's "Read Track" command.
    fn read_all_sectors(&mut self, ch: DiskCh, n: u8, track_len: u8) -> Result<ReadTrackResult, DiskImageError>;
    fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn>;

    /// Read the entire track, decoding the data within.
    /// Not valid for MetaSector resolution tracks, which will return `DiskImageError::UnsupportedFormat`.
    ///
    /// # Parameters
    /// - `ch`: The cylinder and head of the track to read.
    /// - `overdump`: An optional parameter to specify the number of bytes to read past the end of
    ///               the track. This is useful for examining track wrapping behavior.
    /// # Returns
    /// - `Ok(ReadTrackResult)` if the track was successfully read.
    /// - `Err(DiskImageError)` if an error occurred while reading the track.
    fn read_track(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError>;

    /// Read the entire track without decoding.
    /// Not valid for MetaSector resolution tracks, which will return `DiskImageError::UnsupportedFormat`.
    ///
    /// # Parameters
    /// - `ch`: The cylinder and head of the track to read.
    /// - `overdump`: An optional parameter to specify the number of bytes to read past the end of
    ///               the track. This is useful for examining track wrapping behavior.
    /// # Returns
    /// - `Ok(ReadTrackResult)` if the track was successfully read.
    /// - `Err(DiskImageError)` if an error occurred while reading the track.
    fn read_track_raw(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError>;
    /// Return a boolean value indicating whether the track has bits set in its weak bit mask.
    fn has_weak_bits(&self) -> bool;
    /// Format the track with the specified parameters.
    /// # Arguments
    /// - `standard`: The disk structure standard to use when formatting the track.
    /// - `format_buffer`: A vector of `DiskChsn` values representing the sectors to format.
    /// - `fill_pattern`: A slice of bytes to use as the fill pattern when formatting the track.
    /// - `gap3`: The GAP3 length in bytes to use when formatting the track.
    fn format(
        &mut self,
        standard: System34Standard,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<(), DiskImageError>;

    /// Retrieve information about a track's consistency vs a standard track.
    /// Returns a `TrackConsistency` struct containing information about the track's consistency,
    /// such as bad CRCs, deleted data, and overlapping sectors.
    /// # Returns
    /// - `Ok(TrackConsistency)` if the track was successfully checked for consistency.
    /// - `Err(DiskImageError)` if an error occurred while checking the track for consistency.
    fn get_track_consistency(&self) -> Result<TrackConsistency, DiskImageError>;
    /// Return a reference to the underlying `TrackDataStream`.
    fn get_track_stream(&self) -> Option<&TrackDataStream>;
}

pub type DiskTrack = Box<dyn Track>;
