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

    structs.rs

    Defines common structs
*/
use crate::{
    file_parsers::FormatCaps,
    platform::Platform,
    prelude::{DiskCh, DiskChsn},
    track::TrackAnalysis,
    track_schema::TrackSchema,
    types::{DiskRpm, IntegrityCheck, TrackDataEncoding, TrackDataRate, TrackDensity},
};
use std::{
    fmt,
    fmt::{Display, Formatter},
    ops::Range,
};

/// A structure that defines several flags that can apply to a sector.
#[derive(Copy, Clone, Debug, Default)]
pub struct SectorAttributes {
    pub address_error: bool,
    pub data_error: bool,
    pub deleted_mark: bool,
    pub no_dam: bool,
}

/// A structure used to describe the parameters of a sector to be created on a `MetaSector`
/// resolution track.
#[derive(Default)]
pub struct AddSectorParams<'a> {
    pub id_chsn: DiskChsn,
    pub data: &'a [u8],
    pub weak_mask: Option<&'a [u8]>,
    pub hole_mask: Option<&'a [u8]>,
    pub attributes: SectorAttributes,
    pub alternate: bool,
    pub bit_index: Option<usize>,
}

/// A structure to uniquely identify a specific sector on a track.
#[derive(Copy, Clone, Debug, Default)]
pub struct SectorCursor {
    /// The sector id. Either a `sector_idx` or `bit_offset` is required to discriminate between
    /// sectors with the same ID.
    pub id_chsn: DiskChsn,
    /// The physical sector index within the track, starting at 0.
    pub sector_idx: Option<usize>,
    /// The bit offset of the start of the sector header element.
    pub header_offset: Option<usize>,
    /// The bit offset of the start of the sector data element.
    pub data_offset: Option<usize>,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct SectorMapEntry {
    pub chsn: DiskChsn,
    pub attributes: SectorAttributes,
}

/// A DiskConsistency structure maintains information about the consistency of a disk image.
#[derive(Default, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskAnalysis {
    // A field to hold image format capability flags that this image requires in order to be represented.
    pub image_caps: FormatCaps,
    /// Whether the disk image contains weak bits.
    pub weak: bool,
    /// Whether the disk image contains deleted sectors.
    pub deleted_data: bool,
    /// Whether the disk image contains sector IDAMs with no corresponding DAMS.
    pub no_dam: bool,
    /// Whether the disk image contains sectors with bad address mark CRCs
    pub address_error: bool,
    /// Whether the disk image contains sectors with bad data CRCs
    pub data_error: bool,
    /// Whether the disk image contains overlapped sectors
    pub overlapped: bool,
    /// The sector size if the disk image has consistent sector sizes, otherwise None.
    pub consistent_sector_size: Option<u8>,
    /// The track length in sectors if the disk image has consistent track lengths, otherwise None.
    pub consistent_track_length: Option<u32>,
}

impl DiskAnalysis {
    pub fn set_track_analysis(&mut self, ta: &TrackAnalysis) {
        self.deleted_data = ta.deleted_data;
        self.address_error = ta.address_error;
        self.data_error = ta.data_error;
        self.no_dam = ta.no_dam;

        if ta.consistent_sector_size.is_none() {
            self.consistent_sector_size = None;
        }
    }
}

/// A `DiskDescriptor` structure describes the basic geometry and parameters of a disk image.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskDescriptor {
    /// The platform(s) that the disk image is intended for, if determined
    /// Multiple platforms can be specified for dual and triple-format disks.
    pub platforms: Option<Vec<Platform>>,
    /// The basic geometry of the disk. Not all tracks present need to conform to the specified sector count (s).
    pub geometry: DiskCh,
    /// The overall data encoding of the disk. (one or more tracks may have different encodings).
    pub data_encoding: TrackDataEncoding,
    /// The overall density of the disk (one or more tracks may have different densities).
    pub density: TrackDensity,
    /// The overall data rate of the disk (one or more tracks may have different data rates).
    pub data_rate: TrackDataRate,
    /// The rotation rate of the disk. If not provided, this can be determined from other parameters.
    pub rpm: Option<DiskRpm>,
    /// Whether the disk image should be considered read-only (None if image did not define this flag)
    pub write_protect: Option<bool>,
}

/// A `ScanSectorResult` structure contains the results of a scan sector operation.
#[derive(Debug, Clone)]
pub struct ScanSectorResult {
    /// Whether the specified Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector has a "deleted data" address mark.
    pub deleted_mark: bool,
    /// Whether the specified sector failed a header data integrity check.
    pub address_error: bool,
    /// Whether the specified sector failed a data integrity check.
    pub data_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
}

impl Default for ScanSectorResult {
    fn default() -> Self {
        Self {
            not_found: true,
            no_dam: false,
            deleted_mark: false,
            address_error: false,
            data_error: false,
            wrong_cylinder: false,
            bad_cylinder: false,
            wrong_head: false,
        }
    }
}

/// A structure containing the (optional) recorded and calculated CRC values for a region of data.
/// This can represent the result of a CRC or checksum calculation resulting in the specified type,
/// but does not specify the exact algorithm used.
///
/// An [IntegrityField] is usually stored within a [DataIntegrity] enum that specifies the type of
/// check performed.
#[derive(Copy, Clone, Debug)]
pub struct IntegrityField<T> {
    pub valid: bool,
    pub recorded: Option<T>,
    pub calculated: T,
}

impl<T: PartialEq> From<(T, T)> for IntegrityField<T> {
    fn from((recorded, calculated): (T, T)) -> Self {
        IntegrityField::new(recorded, calculated)
    }
}

impl<T> Display for IntegrityField<T>
where
    T: Display + PartialEq + fmt::UpperHex,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let hex_width = size_of::<T>() * 2; // Determine the width for hex formatting
        match &self.recorded {
            Some(recorded) => write!(
                f,
                "[Recorded: {:#0hex_width$X}, Calculated: {:#0hex_width$X} {}]",
                recorded,
                self.calculated,
                if self.is_valid() { "Valid" } else { "*Invalid*" },
                hex_width = hex_width
            ),
            None => write!(
                f,
                "[No CRC recorded, Calculated: {:#0hex_width$X}]",
                self.calculated,
                hex_width = hex_width
            ),
        }
    }
}

impl<T> IntegrityField<T>
where
    T: PartialEq,
{
    pub fn new(recorded: T, calculated: T) -> Self {
        Self {
            valid: recorded == calculated,
            recorded: Some(recorded),
            calculated,
        }
    }

    /// Create a new DataCheckResult without a recorded value (ie, from a MetaSector resolution
    /// image file that only stores boolean flags for CRC validity).
    pub fn unrecorded(valid: bool, calculated: T) -> Self {
        Self {
            valid,
            recorded: None,
            calculated,
        }
    }

    /// Check whether the recorded value matches the calculated value.
    pub fn is_valid(&self) -> bool {
        self.recorded
            .as_ref()
            .map(|recorded| recorded == &self.calculated)
            .unwrap_or(self.valid)
    }

    pub fn is_error(&self) -> bool {
        !self.is_valid()
    }
}

/// A `ReadSectorResult` structure contains the results of a read sector operation.
#[derive(Clone)]
pub struct ReadSectorResult {
    /// The matching Sector ID as `DiskChsn`, or `None`.
    pub id_chsn: Option<DiskChsn>,
    /// Whether the specified Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector was marked deleted.
    pub deleted_mark: bool,
    /// Whether the specified sector had a CRC error with the sector header.
    pub address_crc_error: bool,
    /// The CRC values for the sector header, if available.
    pub address_crc: Option<IntegrityCheck>,
    /// Whether the specified sector had a CRC error with the sector data.
    pub data_crc_error: bool,
    /// The CRC values for the sector data, if available.
    pub data_crc: Option<IntegrityCheck>,
    /// Set when a sector ID with an unexpected specifier was encountered when searching for the
    /// specified sector ID.
    pub wrong_cylinder: bool,
    /// Set when a sector ID with a bad cylinder specifier was encountered when searching for the
    /// specified sector ID.
    pub bad_cylinder: bool,
    /// Set when a sector ID with an unexpected head specifier was encountered when searching for
    /// the specified sector ID.
    pub wrong_head: bool,
    /// Whether the sector read was the last sector on the track.
    pub last_sector: bool,
    /// The index of the start of sector data within `read_buf`.
    pub data_range: Range<usize>,
    /// The data read for the sector, potentially including address mark and CRC bytes.
    /// Use the `data_idx` and `data_len` fields to isolate the sector data within this vector.
    pub read_buf: Vec<u8>,
}

impl Default for ReadSectorResult {
    fn default() -> Self {
        Self {
            id_chsn: None,
            not_found: true,
            no_dam: false,
            deleted_mark: false,
            address_crc_error: false,
            address_crc: None,
            data_crc_error: false,
            data_crc: None,
            wrong_cylinder: false,
            bad_cylinder: false,
            wrong_head: false,
            last_sector: false,
            data_range: 0..0,
            read_buf: Vec::new(),
        }
    }
}

impl ReadSectorResult {
    pub fn data(&self) -> &[u8] {
        &self.read_buf[self.data_range.clone()]
    }
}

/// A `ReadTrackResult` structure contains the results of a read track operation.
#[derive(Clone)]
pub struct ReadTrackResult {
    /// Whether no sectors were found reading the track.
    pub not_found: bool,
    /// Whether the track contained at least one sector with a deleted data mark.
    pub deleted_mark: bool,
    /// Whether the track contained at least one sector with a CRC error in the address mark.
    pub address_crc_error: bool,
    /// Whether the track contained at least one sector with a CRC error in the data.
    pub data_crc_error: bool,
    /// The total number of sectors read from the track.
    pub sectors_read: u16,
    /// The data read for the track.
    pub read_buf: Vec<u8>,
    /// The total number of bits read.
    pub read_len_bits: usize,
    /// The total number of bytes read.
    pub read_len_bytes: usize,
}

/// A `WriteSectorResult` structure contains the results of a write sector operation.
#[derive(Clone)]
pub struct WriteSectorResult {
    /// Whether a matching Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector header matching the Sector ID had a bad CRC.
    /// In this case, the write operation will have failed.
    pub address_crc_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
}

pub struct TrackRegion {
    pub start: usize,
    pub end:   usize,
}

/// `BitStreamTrackParams` structure contains parameters required to create a `BitStream`
/// resolution track.
///
/// `add_track_bitstream()` takes a `BitStreamTrackParams` structure as an argument.
pub struct BitStreamTrackParams<'a> {
    /// The track schema to use, if known. If not known, use `None` then the schema will be
    /// detected - however supplying the schema can improve performance by avoiding wasted decoding
    /// attempts.
    pub schema: Option<TrackSchema>,
    /// The physical cylinder and head of the track to add.
    pub ch: DiskCh,
    pub encoding: TrackDataEncoding,
    pub data_rate: TrackDataRate,
    pub rpm: Option<DiskRpm>,
    pub bitcell_ct: Option<usize>,
    pub data: &'a [u8],
    pub weak: Option<&'a [u8]>,
    pub hole: Option<&'a [u8]>,
    pub detect_weak: bool,
}

/// `FluxStreamTrackParams` contains parameters required to create a `FluxStream` resolution track.
///
/// `add_track_fluxstream()` takes a `FluxStreamTrackParams` structure as an argument.
pub struct FluxStreamTrackParams {
    /// The physical cylinder and head of the track to add.
    pub ch: DiskCh,
    /// The track schema to use for the track, if known. If not known, use `None` and the schema
    /// will be inferred from the track data.
    pub schema: Option<TrackSchema>,
    /// The data encoding used in the track. If not known, use `None` and the encoding will be
    /// inferred from the track data.
    pub encoding: Option<TrackDataEncoding>,
    /// A hint for the base PLL clock frequency to use decoding the track. If not known, use `None`
    /// and the clock frequency will be inferred from the track data.
    pub clock: Option<f64>,
    /// A hint for the disk rotation rate to use decoding the track. If not known, use `None`
    /// and the rotation rate will be inferred from the track data.
    pub rpm: Option<DiskRpm>,
}

/// `MetaSectorTrackParams` contains parameters required to create a `MetaSector` resolution track.
///
/// `add_track_metasector()` takes a `MetaSectorTrackParams` structure as an argument.
pub struct MetaSectorTrackParams {
    /// The physical cylinder and head of the track to add.
    /// This should be the next available track in the disk image.
    pub ch: DiskCh,
    /// The track data encoding used in the track. This may not be specified by a `MetaSector`
    /// disk image, but can be inferred from the `Platform` or `StandardFormat`.
    /// It does not really affect the operation of a `MetaSector` track, but incorrect values may
    /// persist in exported disk images.
    pub encoding: TrackDataEncoding,
    /// The track data rate. Similar caveats to the ones discussed for `encoding` apply.
    pub data_rate: TrackDataRate,
}

#[derive(Default)]
pub(crate) struct SharedDiskContext {
    /// The number of write operations (WriteData or FormatTrack) operations performed on the disk image.
    /// This can be used to determine if the disk image has been modified since the last save.
    pub(crate) writes: u64,
}
