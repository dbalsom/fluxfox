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

use crate::bitstream::TrackDataStream;
use crate::diskimage::{
    ReadSectorResult, ReadTrackResult, RwSectorScope, ScanSectorResult, SectorDescriptor, WriteSectorResult,
};
use crate::structure_parsers::system34::System34Standard;
use crate::structure_parsers::DiskStructureMetadata;
use crate::{DiskCh, DiskChs, DiskChsn, DiskDataEncoding, DiskDataRate, DiskImageError, SectorMapEntry};
use sha1_smol::Digest;
use std::any::Any;

pub struct TrackInfo {
    pub encoding: DiskDataEncoding,
    pub data_rate: DiskDataRate,
    pub bit_length: usize,
    pub sector_ct: usize,
}

pub enum TrackSectorScanResult {
    Found {
        element_start: usize,
        element_end: usize,
        sector_chsn: DiskChsn,
        address_crc_valid: bool,
        data_crc_valid: bool,
        deleted: bool,
        no_dam: bool,
    },
    NotFound {
        wrong_cylinder: bool,
        bad_cylinder: bool,
        wrong_head: bool,
    },
    #[allow(dead_code)] // use this someday (wrong track encoding?)
    Incompatible,
}

#[derive(Debug, Default)]
pub struct TrackConsistency {
    pub bad_data_crc: bool,
    pub bad_address_crc: bool,
    pub deleted_data: bool,
    pub no_dam: bool,
    pub consistent_sector_size: Option<u8>,
    pub nonconsecutive_sectors: bool,
    pub sector_ct: usize,
}

impl TrackConsistency {
    pub fn join(&mut self, other: &TrackConsistency) {
        self.bad_data_crc |= other.bad_data_crc;
        self.bad_address_crc |= other.bad_address_crc;
        self.deleted_data |= other.deleted_data;
        self.no_dam |= other.no_dam;
        self.nonconsecutive_sectors |= other.nonconsecutive_sectors;

        if other.consistent_sector_size.is_none() {
            self.consistent_sector_size = None;
        }
    }
}

pub trait Track: Any {
    fn as_any(&self) -> &dyn Any;
    fn ch(&self) -> DiskCh;

    fn set_ch(&mut self, ch: DiskCh);

    fn encoding(&self) -> DiskDataEncoding;
    fn info(&self) -> TrackInfo;

    fn metadata(&self) -> Option<&DiskStructureMetadata>;

    fn get_sector_ct(&self) -> usize;

    fn has_sector_id(&self, id: u8, id_chsn: Option<DiskChsn>) -> bool;

    fn get_sector_list(&self) -> Vec<SectorMapEntry>;

    /// Masters a new sector to a track in the disk image, essentially 'formatting' a new sector,
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
        chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError>;

    fn scan_sector(&self, chs: DiskChs, n: Option<u8>) -> Result<ScanSectorResult, DiskImageError>;

    fn write_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        write_data: &[u8],
        scope: RwSectorScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError>;

    fn get_hash(&mut self) -> Digest;

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, ch: DiskCh, n: u8, track_len: u8) -> Result<ReadTrackResult, DiskImageError>;
    fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn>;
    fn read_track(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError>;
    fn has_weak_bits(&self) -> bool;
    fn format(
        &mut self,
        standard: System34Standard,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<(), DiskImageError>;

    fn get_track_consistency(&self) -> Result<TrackConsistency, DiskImageError>;

    fn get_track_stream(&self) -> Option<&TrackDataStream>;
}

pub type DiskTrack = Box<dyn Track + Send + Sync>;
