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
//mod sector_iterator;

use crate::{
    bitstream::TrackDataStream,
    source_map::SourceMap,
    track::{bitstream::BitStreamTrack, fluxstream::FluxStreamTrack, metasector::MetaSectorTrack},
    track_schema::{system34::System34Standard, TrackMetadata, TrackSchema},
    types::{
        chs::DiskChsnQuery,
        AddSectorParams,
        DiskCh,
        DiskChs,
        DiskChsn,
        DiskRpm,
        ReadSectorResult,
        ReadTrackResult,
        RwScope,
        ScanSectorResult,
        TrackDataEncoding,
        TrackDataRate,
        TrackDataResolution,
        TrackDensity,
        WriteSectorResult,
    },
    DiskImageError,
    SectorIdQuery,
    SectorMapEntry,
};
use dyn_clone::{clone_trait_object, DynClone};
use sha1_smol::Digest;
use std::any::Any;

/// A struct containing information about a track's encoding, data rate, density, RPM, bit length,
/// and sector count.
#[derive(Debug)]
pub struct TrackInfo {
    /// The type of encoding used on the track as a `DiskDataEncoding` enum.
    pub encoding: TrackDataEncoding,
    /// The track data schema
    pub schema: Option<TrackSchema>,
    /// The data rate of the track as a `DiskDataRate` enum.
    pub data_rate: TrackDataRate,
    /// The density of the track as a `DiskDensity` enum, or `None` if density has not been determined.
    pub density: Option<TrackDensity>,
    /// The RPM of the track as an `DiskRpm`, or `None` if RPM has not been determined.
    pub rpm: Option<DiskRpm>,
    /// The bit length of the track.
    pub bit_length: usize,
    /// The number of sectors on the track.
    pub sector_ct: usize,
}

/// A struct representing the result of a sector scan operation on a track.
#[derive(Debug)]
pub(crate) enum TrackSectorScanResult {
    /// A variant indicating the specified sector ID was found on the track.
    Found {
        /// The index of the [TrackElementInstance] that was found.
        ei: usize,
        /// The matching sector ID found on the track.
        sector_chsn: DiskChsn,
        /// Whether the specified sector failed a header data integrity check.
        address_error: bool,
        /// Whether the specified sector failed a data integrity check.
        data_error: bool,
        /// Whether the specific sector has a "deleted data" address mark.
        deleted_mark: bool,
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

impl From<TrackSectorScanResult> for ScanSectorResult {
    fn from(other: TrackSectorScanResult) -> Self {
        match other {
            TrackSectorScanResult::Found {
                address_error,
                data_error,
                deleted_mark,
                no_dam,
                ..
            } => ScanSectorResult {
                not_found: false,
                no_dam,
                deleted_mark,
                address_error,
                data_error,
                ..Default::default()
            },
            TrackSectorScanResult::NotFound {
                wrong_cylinder,
                bad_cylinder,
                wrong_head,
            } => ScanSectorResult {
                wrong_cylinder,
                bad_cylinder,
                wrong_head,
                ..Default::default()
            },
            TrackSectorScanResult::Incompatible => Default::default(),
        }
    }
}

/// A structure containing information about a track's consistency vs a standard track.
#[derive(Debug, Default)]
pub struct TrackAnalysis {
    /// A boolean flag indicating whether the track contains sectors with bad data CRCs.
    pub data_error: bool,
    /// A boolean flag indicating whether the track contains sectors with bad address CRCs.
    pub address_error: bool,
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

impl TrackAnalysis {
    /// Merge a [TrackAnalysis] struct with another, by OR'ing together their boolean values.
    pub fn join(&mut self, other: &TrackAnalysis) {
        self.data_error |= other.data_error;
        self.address_error |= other.address_error;
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
pub trait Track: DynClone + Any + Send + Sync {
    /// Return the resolution of the track as a `DiskDataResolution`.
    /// This can be used to determine the track's underlying representation, especially if you wish
    /// to downcast the track to a specific type.
    fn resolution(&self) -> TrackDataResolution;

    /// Return a reference to the track as a `&dyn Any`, for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Return a mutable reference to the track as a `&mut dyn Any`, for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Downcast the track to a `MetaSectorTrack` reference, if possible.
    fn as_metasector_track(&self) -> Option<&MetaSectorTrack> {
        None
    }

    /// Downcast the track to a mutable `MetaSectorTrack` reference, if possible.
    fn as_metasector_track_mut(&mut self) -> Option<&mut MetaSectorTrack> {
        None
    }

    /// Downcast the track to a `BitStreamTrack` reference, if possible.
    fn as_bitstream_track(&self) -> Option<&BitStreamTrack> {
        None
    }

    /// Downcast the track to a `BitStreamTrack` reference, if possible.
    fn as_bitstream_track_mut(&mut self) -> Option<&mut BitStreamTrack> {
        None
    }

    /// Downcast the track to a `FluxStreamTrack` reference, if possible.
    fn as_fluxstream_track(&self) -> Option<&FluxStreamTrack> {
        None
    }

    /// Downcast the track to a mutable `FluxStreamTrack` reference, if possible.
    fn as_fluxstream_track_mut(&mut self) -> Option<&mut FluxStreamTrack> {
        None
    }

    /// Return the track's physical cylinder and head as a `DiskCh`.
    fn ch(&self) -> DiskCh;

    /// Set the track's physical cylinder and head.
    fn set_ch(&mut self, ch: DiskCh);

    /// Return the encoding of the track as `DiskDataEncoding`.
    fn encoding(&self) -> TrackDataEncoding;

    /// Return information about the track as a `TrackInfo` struct.
    fn info(&self) -> TrackInfo;

    /// Return the track's metadata as a reference to [TrackMetadata], or None if the track has not
    /// been scanned for metadata or no metadata was found.
    fn metadata(&self) -> Option<&TrackMetadata>;

    /// Return a count of the sectors on the track.
    fn sector_ct(&self) -> usize;

    /// Returns `true` if the track contains a sector with the specified ID.
    ///
    /// # Arguments
    /// - `id`: The sector ID to search for.
    /// - `id_chsn`: An optional `DiskChsn` value. If provided, the `id` parameter is ignored and
    ///              the entire `DiskChsn` value is used to search for the sector.

    fn has_sector_id(&self, id: u8, id_chsn: Option<DiskChsn>) -> bool;

    // Return a SectorIterator for the current track.
    // Warning: Reformatting the track will invalidate the iterator.
    //fn sector_iter(&self) -> SectorIterator<'a, T>;

    /// TODO: Rename SectorMapEntry - it's not a map, it's a list.
    /// Returns a vector of `SectorMapEntry` structs representing the sectors on the track.
    fn sector_list(&self) -> Vec<SectorMapEntry>;

    /// Adds a new sector to a track in the disk image, essentially 'formatting' a new sector,
    /// This function is only valid for tracks with `MetaSector` resolution.
    ///
    /// # Arguments
    /// - `sd`: A reference to a `SectorDescriptor` containing the sector data and metadata.
    /// - `alternate`: A boolean flag indicating whether the sector is an alternate sector.
    ///                Alternate sectors will calculate weak bit masks for the existing sector.
    ///                If the existing sector does not exist, the alternate flag is ignored.
    ///
    /// # Returns
    /// - `Ok(())` if the sector was successfully mastered.
    /// - `Err(DiskImageError::SeekError)` if the head value in `chs` is greater than 1 or the track map does not contain the specified cylinder.
    /// - `Err(DiskImageError::UnsupportedFormat)` if the track data is not of `MetaSector` resolution.
    fn add_sector(&mut self, sd: &AddSectorParams) -> Result<(), DiskImageError>;

    /// Attempts to read the sector data from the sector identified by `id`.
    ///
    /// # Arguments
    /// - `id`: The sector ID to read as a `SectorIdQuery`.
    /// - `n`: An optional override value for the sector's size parameter. If provided, the sector
    ///        will be read as a sector of this size.
    /// - `offset`: An optional bit offset to start reading the sector data from. If a track
    ///             contains multiple sectors with the same ID, the offset can be used to specify
    ///             which sector to read.
    /// - `scope`: The scope of the read operation as a `RwSectorScope` enum. This can be used to
    ///            specify whether to include the sector's address mark and CRC in the read data.
    /// - `debug`: A boolean flag controlling debug mode. When set to `true`, the read operation
    ///            return data even if the sector has an invalid address CRC or would otherwise
    ///            normally not be read.
    ///
    /// # Returns
    /// A Result containing either
    /// - [ReadSectorResult] struct which provides various result flags and the resulting data if
    ///   the sector was successfully read.
    /// - [DiskImageError] if an error occurred while reading the sector.
    fn read_sector(
        &self,
        id: SectorIdQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError>;

    fn scan_sector(&self, id: SectorIdQuery, offset: Option<usize>) -> Result<ScanSectorResult, DiskImageError>;

    fn write_sector(
        &mut self,
        id: DiskChsnQuery,
        offset: Option<usize>,
        write_data: &[u8],
        scope: RwScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError>;

    /// Recalculate the sector CRC for the first sector matching the query from the specified bit
    /// offset.
    fn recalculate_sector_crc(&mut self, id: DiskChsnQuery, offset: Option<usize>) -> Result<(), DiskImageError>;

    /// Return a hash that uniquely identifies the track data. Intended for use in identifying
    /// duplicate tracks.
    fn hash(&mut self) -> Digest;

    /// Read all sectors from the track. The data is returned within a `ReadSectorResult` struct
    /// which also sets some convenience metadata flags which are needed when handling `MetaSector`
    /// resolution images.
    /// Unlike `read_sector`, the data returned is only the actual sector data. The address marks
    /// and CRCs are not included in the data.
    /// This function is intended for use in implementing the µPD765 FDC's "Read Track" command.
    fn read_all_sectors(&mut self, ch: DiskCh, n: u8, track_len: u8) -> Result<ReadTrackResult, DiskImageError>;

    fn next_id(&self, chs: DiskChs) -> Option<DiskChsn>;

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
    fn read(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError>;

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
    fn read_raw(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError>;

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
    /// Returns a `TrackAnalysis` struct containing information about the track's formatting,
    /// such as bad CRCs, deleted data, and overlapping sectors.
    /// # Returns
    /// - `Ok(TrackAnalysis)` if the track was successfully analyzed
    /// - `Err(DiskImageError)` if an error occurred while checking the analyzing the track
    fn analysis(&self) -> Result<TrackAnalysis, DiskImageError>;

    /// Return a reference to the underlying `TrackDataStream`.
    fn stream(&self) -> Option<&TrackDataStream>;

    /// Return a mutable reference to the underlying `TrackDataStream`.
    fn stream_mut(&mut self) -> Option<&mut TrackDataStream>;

    /// Return a SourceMap containing info about the track's elements for display in a UI or
    /// debug output.
    fn element_map(&self) -> Option<&SourceMap> {
        None
    }
}

clone_trait_object!(Track);

pub type DiskTrack = Box<dyn Track>;
