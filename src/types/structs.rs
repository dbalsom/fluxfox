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

    structs.rs

    Defines common structs
*/

use crate::{
    file_parsers::FormatCaps,
    prelude::{DiskCh, DiskChsn},
    track::TrackConsistency,
    types::{DiskDataEncoding, DiskDataRate, DiskDensity, DiskRpm},
};

/// A structure that defines several flags that can apply to a sector.
#[derive(Copy, Clone, Debug, Default)]
pub struct SectorAttributes {
    pub address_crc_valid: bool,
    pub data_crc_valid: bool,
    pub deleted_mark: bool,
    pub no_dam: bool,
}

/// A structure used to describe the parameters of a sector to be created on a `MetaSector`
/// resolution track.
#[derive(Default)]
pub struct SectorDescriptor {
    pub id_chsn: DiskChsn,
    pub data: Vec<u8>,
    pub weak_mask: Option<Vec<u8>>,
    pub hole_mask: Option<Vec<u8>>,
    pub attributes: SectorAttributes,
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
#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskConsistency {
    // A field to hold image format capability flags that this image requires in order to be represented.
    pub image_caps: FormatCaps,
    /// Whether the disk image contains weak bits.
    pub weak: bool,
    /// Whether the disk image contains deleted sectors.
    pub deleted_data: bool,
    /// Whether the disk image contains sector IDAMs with no corresponding DAMS.
    pub no_dam: bool,
    /// Whether the disk image contains sectors with bad address mark CRCs
    pub bad_address_crc: bool,
    /// Whether the disk image contains sectors with bad data CRCs
    pub bad_data_crc: bool,
    /// Whether the disk image contains overlapped sectors
    pub overlapped: bool,
    /// The sector size if the disk image has consistent sector sizes, otherwise None.
    pub consistent_sector_size: Option<u8>,
    /// The track length in sectors if the disk image has consistent track lengths, otherwise None.
    pub consistent_track_length: Option<u32>,
}

impl DiskConsistency {
    pub fn set_track_consistency(&mut self, track_consistency: &TrackConsistency) {
        self.deleted_data = track_consistency.deleted_data;
        self.bad_address_crc = track_consistency.bad_address_crc;
        self.bad_data_crc = track_consistency.bad_data_crc;
        self.no_dam = track_consistency.no_dam;

        if track_consistency.consistent_sector_size.is_none() {
            self.consistent_sector_size = None;
        }
    }
}

/// A `DiskDescriptor` structure describes the basic geometry and parameters of a disk image.
#[derive(Copy, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskDescriptor {
    /// The basic geometry of the disk. Not all tracks present need to conform to the specified sector count (s).
    pub geometry: DiskCh,
    /// The "default" sector size of the disk. Larger or smaller sectors may still be present in the disk image.
    pub default_sector_size: usize,
    /// The default data encoding used. The disk may still contain tracks in different encodings.
    pub data_encoding: DiskDataEncoding,
    /// The density of the disk
    pub density: DiskDensity,
    /// The data rate of the disk
    pub data_rate: DiskDataRate,
    /// The rotation rate of the disk. If not provided, this can be determined from other parameters.
    pub rpm: Option<DiskRpm>,
    /// Whether the disk image should be considered read-only (None if image did not define this flag)
    pub write_protect: Option<bool>,
}

/// A `ScanSectorResult` structure contains the results of a scan sector operation.
#[derive(Debug, Default, Clone)]
pub struct ScanSectorResult {
    /// Whether the specified Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector was marked deleted.
    pub deleted_mark: bool,
    /// Whether the specified sector had a CRC error with the sector header.
    pub address_crc_error: bool,
    /// Whether the specified sector had a CRC error with the sector data.
    pub data_crc_error: bool,
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
    /// Whether the specified sector had a CRC error with the sector data.
    pub data_crc_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
    /// The index of the start of sector data within `read_buf`.
    pub data_idx: usize,
    /// The length of sector data, starting from `data_idx`, within `read_buf`.
    pub data_len: usize,
    /// The data read for the sector, potentially including address mark and CRC bytes.
    /// Use the `data_idx` and `data_len` fields to isolate the sector data within this vector.
    pub read_buf: Vec<u8>,
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

pub struct BitStreamTrackParams<'a> {
    pub encoding: DiskDataEncoding,
    pub data_rate: DiskDataRate,
    pub rpm: Option<DiskRpm>,
    pub ch: DiskCh,
    pub bitcell_ct: Option<usize>,
    pub data: &'a [u8],
    pub weak: Option<&'a [u8]>,
    pub hole: Option<&'a [u8]>,
    pub detect_weak: bool,
}

#[derive(Default)]
pub(crate) struct SharedDiskContext {
    /// The number of write operations (WriteData or FormatTrack) operations performed on the disk image.
    /// This can be used to determine if the disk image has been modified since the last save.
    pub(crate) writes: u64,
}
