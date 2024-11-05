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

    src/track/bitstream.rs

    Implements the Bitstream track type and the Track trait for same.

*/
use super::{Track, TrackConsistency, TrackInfo, TrackSectorScanResult};
use crate::bitstream::fm::FmCodec;
use crate::bitstream::mfm::{MfmCodec, MFM_BYTE_LEN};
use crate::bitstream::{EncodingVariant, TrackDataStream};
use crate::diskimage::{
    BitStreamTrackParams, ReadSectorResult, ReadTrackResult, RwSectorScope, ScanSectorResult, SectorDescriptor,
    SharedDiskContext, WriteSectorResult,
};
use crate::io::SeekFrom;
use crate::structure_parsers::system34::{
    System34Element, System34Marker, System34Parser, System34Standard, DAM_MARKER_BYTES, DDAM_MARKER_BYTES,
};
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem, DiskStructureParser,
};
use crate::track::fluxstream::FluxStreamTrack;
use crate::track::metasector::MetaSectorTrack;
use crate::util::crc_ibm_3740;
use crate::{
    DiskCh, DiskChs, DiskChsn, DiskDataEncoding, DiskDataRate, DiskDataResolution, DiskDensity, DiskImageError,
    DiskRpm, FoxHashSet, SectorMapEntry,
};
use bit_vec::BitVec;
use sha1_smol::Digest;
use std::any::Any;
use std::sync::{Arc, Mutex};

pub struct BitStreamTrack {
    pub(crate) encoding: DiskDataEncoding,
    pub(crate) data_rate: DiskDataRate,
    pub(crate) rpm: Option<DiskRpm>,
    pub(crate) ch: DiskCh,
    pub(crate) data: TrackDataStream,
    pub(crate) metadata: DiskStructureMetadata,
    pub(crate) sector_ids: Vec<DiskChsn>,
    pub(crate) shared: Arc<Mutex<SharedDiskContext>>,
}

impl Track for BitStreamTrack {
    fn resolution(&self) -> DiskDataResolution {
        DiskDataResolution::BitStream
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_metasector_track(&self) -> Option<&MetaSectorTrack> {
        None
    }

    fn as_bitstream_track(&self) -> Option<&BitStreamTrack> {
        self.as_any().downcast_ref::<BitStreamTrack>()
    }

    fn as_fluxstream_track(&self) -> Option<&FluxStreamTrack> {
        None
    }

    fn as_fluxstream_track_mut(&mut self) -> Option<&mut FluxStreamTrack> {
        None
    }

    fn ch(&self) -> DiskCh {
        self.ch
    }

    fn set_ch(&mut self, new_ch: DiskCh) {
        self.ch = new_ch;
    }

    fn encoding(&self) -> DiskDataEncoding {
        self.encoding
    }

    fn info(&self) -> TrackInfo {
        TrackInfo {
            encoding: self.encoding,
            data_rate: self.data_rate,
            density: Some(DiskDensity::from(self.data_rate)),
            rpm: self.rpm,
            bit_length: self.data.len(),
            sector_ct: self.sector_ids.len(),
        }
    }

    fn metadata(&self) -> Option<&DiskStructureMetadata> {
        Some(&self.metadata)
    }

    fn get_sector_ct(&self) -> usize {
        let mut sector_ct = 0;
        for item in &self.metadata.items {
            if item.elem_type.is_sector() {
                sector_ct += 1;
            }
        }
        sector_ct
    }

    fn has_sector_id(&self, id: u8, _id_chsn: Option<DiskChsn>) -> bool {
        for item in &self.metadata.items {
            if let DiskStructureElement::System34(System34Element::Marker(System34Marker::Idam, _)) = item.elem_type {
                if let Some(chsn) = item.chsn {
                    if chsn.s() == id {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn get_sector_list(&self) -> Vec<SectorMapEntry> {
        let mut sector_list = Vec::new();
        for item in &self.metadata.items {
            if let DiskStructureElement::System34(System34Element::Data {
                address_crc,
                data_crc,
                deleted,
            }) = item.elem_type
            {
                if let Some(chsn) = item.chsn {
                    sector_list.push(SectorMapEntry {
                        chsn,
                        address_crc_valid: address_crc,
                        data_crc_valid: data_crc,
                        deleted_mark: deleted,
                        no_dam: false,
                    });
                }
            }
        }
        sector_list
    }

    fn add_sector(&mut self, _sd: &SectorDescriptor, _alternate: bool) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

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
    ) -> Result<ReadSectorResult, DiskImageError> {
        let mut data_idx = 0;
        let mut data_len = 0;

        let mut read_vec = Vec::new();

        let mut data_crc_error = false;
        let mut address_crc_error = false;
        let mut deleted_mark = false;
        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = self.get_sector_bit_index(chs, n, debug);

        let mut id_chsn = None;

        match bit_index {
            TrackSectorScanResult::Found {
                address_crc_valid,
                no_dam,
                sector_chsn,
                ..
            } if no_dam => {
                // No DAM found. Return an empty buffer.
                address_crc_error = !address_crc_valid;
                return Ok(ReadSectorResult {
                    id_chsn: Some(sector_chsn),
                    data_idx: 0,
                    data_len: 0,
                    read_buf: Vec::new(),
                    deleted_mark: false,
                    not_found: false,
                    no_dam: true,
                    address_crc_error,
                    data_crc_error: false,
                    wrong_cylinder: false,
                    bad_cylinder: false,
                    wrong_head: false,
                });
            }
            TrackSectorScanResult::Found {
                element_start,
                sector_chsn,
                address_crc_valid,
                data_crc_valid,
                deleted,
                ..
            } => {
                address_crc_error = !address_crc_valid;
                id_chsn = Some(sector_chsn);
                // If there is a bad address mark we do not read the data unless the debug flag is set.
                // This allows dumping of sectors with bad address marks for debugging purposes.
                // So if the debug flag is not set, return our 'failure' now.
                if address_crc_error && !debug {
                    return Ok(ReadSectorResult {
                        id_chsn,
                        not_found: false,
                        no_dam: false,
                        deleted_mark: false,
                        address_crc_error: true,
                        data_crc_error: false,
                        wrong_cylinder: false,
                        bad_cylinder: false,
                        wrong_head: false,
                        data_idx: 0,
                        data_len: 0,
                        read_buf: Vec::new(),
                    });
                }

                deleted_mark = deleted;
                data_crc_error = !data_crc_valid;

                // The caller can request the scope of the read to be the entire data block
                // including address mark and crc bytes, or just the data. Handle offsets accordingly.
                let (scope_read_off, scope_data_off, scope_data_adj) = match scope {
                    // Add 4 bytes for address mark and 2 bytes for CRC.
                    RwSectorScope::DataBlock => (0, 4, 6),
                    RwSectorScope::DataOnly => (64, 0, 0),
                };

                // Normally we read the contents of the sector determined by N in the sector header.
                // The read operation however can override the value of N if 'debug' is true.
                // If the 'n' parameter is Some, then we use the provided value instead of the sector
                // header value.
                // If 'debug' is false, 'n' must be matched or the read operation will fail as
                // sector id not found.
                if let Some(n_value) = n {
                    if debug {
                        data_len = DiskChsn::n_to_bytes(n_value);
                    }
                    else {
                        if sector_chsn.n() != n_value {
                            log::error!(
                                "read_sector(): Sector size mismatch, expected: {} got: {}",
                                sector_chsn.n(),
                                n_value
                            );
                            return Err(DiskImageError::DataError);
                        }
                        data_len = sector_chsn.n_size();
                    }
                }
                else {
                    data_len = sector_chsn.n_size();
                }
                data_idx = scope_data_off;

                read_vec = vec![0u8; data_len + scope_data_adj];

                log::trace!(
                    "read_sector(): Found DAM for Sector ID: {} at offset: {:?} read length: {}",
                    chs.s(),
                    element_start,
                    read_vec.len()
                );

                log::debug!("read_sector(): Seeking to offset: {}", element_start + scope_read_off);
                self.data
                    .seek(SeekFrom::Start((element_start + scope_read_off) as u64))
                    .map_err(|_| DiskImageError::BitstreamError)?;
                log::debug!("read_sector(): Reading {} bytes.", read_vec.len());
                self.data
                    .read_exact(&mut read_vec)
                    .map_err(|_| DiskImageError::BitstreamError)?;
            }
            TrackSectorScanResult::NotFound {
                wrong_cylinder: wc,
                bad_cylinder: bc,
                wrong_head: wh,
            } => {
                log::trace!(
                    "read_sector(): Sector ID not matched reading track. wc: {} bc: {} wh: {}",
                    wc,
                    bc,
                    wh
                );

                wrong_cylinder = wc;
                bad_cylinder = bc;
                wrong_head = wh;
            }
            _ => {
                unreachable!()
            }
        }

        Ok(ReadSectorResult {
            id_chsn,
            data_idx,
            data_len,
            read_buf: read_vec,
            deleted_mark,
            not_found: false,
            no_dam: false,
            address_crc_error,
            data_crc_error,
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
        })
    }

    fn scan_sector(&self, chs: DiskChs, n: Option<u8>) -> Result<ScanSectorResult, DiskImageError> {
        let data_crc_error = false;
        let mut address_crc_error = false;
        let deleted_mark = false;
        let wrong_cylinder = false;
        let bad_cylinder = false;
        let wrong_head = false;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = self.get_sector_bit_index(chs, n, false);

        match bit_index {
            TrackSectorScanResult::Found {
                address_crc_valid,
                no_dam,
                ..
            } if no_dam => {
                // No DAM found. Return an empty buffer.
                address_crc_error = !address_crc_valid;
                Ok(ScanSectorResult {
                    deleted_mark: false,
                    not_found: false,
                    no_dam: true,
                    address_crc_error,
                    data_crc_error: false,
                    wrong_cylinder: false,
                    bad_cylinder: false,
                    wrong_head: false,
                })
            }
            TrackSectorScanResult::Found {
                address_crc_valid,
                data_crc_valid,
                deleted,
                ..
            } => {
                if !address_crc_valid {
                    // Bad address CRC, return status.
                    Ok(ScanSectorResult {
                        deleted_mark: false,
                        not_found: false,
                        no_dam: false,
                        address_crc_error: true,
                        data_crc_error: false,
                        wrong_cylinder,
                        bad_cylinder,
                        wrong_head,
                    })
                }
                else {
                    Ok(ScanSectorResult {
                        deleted_mark: deleted,
                        not_found: false,
                        no_dam: false,
                        address_crc_error,
                        data_crc_error: !data_crc_valid,
                        wrong_cylinder,
                        bad_cylinder,
                        wrong_head,
                    })
                }
            }
            TrackSectorScanResult::NotFound {
                wrong_cylinder: wc,
                bad_cylinder: bc,
                wrong_head: wh,
            } => {
                log::trace!(
                    "scan_sector: Sector ID not matched reading track. wc: {} bc: {} wh: {}",
                    wc,
                    bc,
                    wh
                );
                Ok(ScanSectorResult {
                    not_found: true,
                    no_dam: false,
                    deleted_mark,
                    address_crc_error,
                    data_crc_error,
                    wrong_cylinder: wc,
                    bad_cylinder: bc,
                    wrong_head: wc,
                })
            }
            _ => {
                unreachable!()
            }
        }
    }

    fn write_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        write_data: &[u8],
        _scope: RwSectorScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let data_len;
        let address_crc_error;
        let mut wrong_cylinder = false;
        let bad_cylinder = false;
        let mut wrong_head = false;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = self.get_sector_bit_index(chs, n, debug);

        match bit_index {
            TrackSectorScanResult::Found {
                address_crc_valid,
                no_dam,
                ..
            } if no_dam => {
                // No DAM found. Return an empty buffer.
                Ok(WriteSectorResult {
                    not_found: false,
                    no_dam: true,
                    address_crc_error: !address_crc_valid,
                    wrong_cylinder,
                    bad_cylinder,
                    wrong_head,
                })
            }
            TrackSectorScanResult::Found {
                element_start: sector_offset,
                sector_chsn,
                address_crc_valid,
                deleted,
                ..
            } => {
                wrong_cylinder = sector_chsn.c() != chs.c();
                wrong_head = sector_chsn.h() != chs.h();
                address_crc_error = !address_crc_valid;
                // If there's a bad address mark, we do not proceed to write the data, unless we're
                // requesting it anyway for debugging purposes.
                if address_crc_error && !debug {
                    return Ok(WriteSectorResult {
                        not_found: false,
                        no_dam: false,
                        address_crc_error,
                        wrong_cylinder,
                        bad_cylinder,
                        wrong_head,
                    });
                }

                let mark_bytes = match deleted {
                    true => DDAM_MARKER_BYTES,
                    false => DAM_MARKER_BYTES,
                };

                if write_deleted != deleted {
                    log::warn!(
                                "write_sector(): Deleted mark mismatch, expected: {} got: {}. Changing sector data type not implemented",
                                write_deleted,
                                deleted
                            );
                }

                // Normally we write the contents of the sector determined by N in the sector header.
                // The write operation however can override the value of N if 'debug' is true.
                // If the 'n' parameter is Some, then we use the provided value instead of the sector
                // header value.
                // If 'debug' is false, 'n' must be matched or the write operation will fail as
                // sector id not found.
                if let Some(n_value) = n {
                    if debug {
                        // Try to use provided n, but limit to the size of the write buffer.
                        data_len = std::cmp::min(write_data.len(), DiskChsn::n_to_bytes(n_value));
                    }
                    else {
                        if sector_chsn.n() != n_value {
                            log::error!(
                                "write_sector(): Sector size mismatch, expected: {} got: {}",
                                sector_chsn.n(),
                                n_value
                            );
                            return Err(DiskImageError::DataError);
                        }
                        data_len = sector_chsn.n_size();

                        if data_len > write_data.len() {
                            log::error!(
                                "write_sector(): Data buffer underflow, expected: {} got: {}",
                                data_len,
                                write_data.len()
                            );
                            return Err(DiskImageError::ParameterError);
                        }
                    }
                }
                else {
                    if DiskChsn::n_to_bytes(sector_chsn.n()) != write_data.len() {
                        log::error!(
                            "write_sector(): Data buffer size mismatch, expected: {} got: {}",
                            sector_chsn.n(),
                            write_data.len()
                        );
                        return Err(DiskImageError::ParameterError);
                    }
                    data_len = sector_chsn.n_size();
                }

                self.data
                    .seek(SeekFrom::Start(((sector_offset >> 1) + 32) as u64))
                    .map_err(|_| DiskImageError::SeekError)?;

                log::trace!(
                    "write_sector(): Writing {} bytes to sector_id: {} at offset: {}",
                    data_len,
                    chs.s(),
                    sector_offset + 4 * MFM_BYTE_LEN
                );

                self.data
                    .write_buf(&write_data[0..data_len], sector_offset + 4 * MFM_BYTE_LEN);

                // Calculate the CRC of the data address mark + data.
                let mut crc = crc_ibm_3740(&mark_bytes, None);
                crc = crc_ibm_3740(&write_data[0..data_len], Some(crc));

                // Write the CRC after the data.
                self.data
                    .write_buf(&crc.to_be_bytes(), sector_offset + (4 + data_len) * MFM_BYTE_LEN);

                self.add_write(data_len);

                Ok(WriteSectorResult {
                    not_found: false,
                    no_dam: false,
                    address_crc_error: false,
                    wrong_cylinder,
                    bad_cylinder,
                    wrong_head,
                })
            }
            TrackSectorScanResult::NotFound {
                wrong_cylinder: wc,
                bad_cylinder: bc,
                wrong_head: wh,
            } => {
                log::warn!(
                    "write_sector(): Sector ID not found writing sector: {} wc: {} bc: {} wh: {}",
                    chs,
                    wc,
                    bc,
                    wh
                );
                Ok(WriteSectorResult {
                    not_found: true,
                    no_dam: false,
                    address_crc_error: false,
                    wrong_cylinder: wc,
                    bad_cylinder: bc,
                    wrong_head: wh,
                })
            }
            _ => {
                unreachable!()
            }
        }
    }

    fn get_hash(&mut self) -> Digest {
        let mut hasher = sha1_smol::Sha1::new();

        hasher.update(&self.data.data());
        hasher.digest()
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, _ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        let mut track_read_vec = Vec::with_capacity(512 * 9);
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut sector_read_vec = vec![0u8; sector_data_len];

        let mut data_crc_error = false;
        let mut address_crc_error = false;
        let mut deleted_mark = false;
        let mut not_found = true;
        let mut sectors_read: u16 = 0;

        // Read index first to avoid borrowing issues in next match.
        let mut bit_index = self.get_first_sector_at_bit_index(0);

        while let TrackSectorScanResult::Found {
            element_start,
            element_end,
            sector_chsn,
            address_crc_valid,
            data_crc_valid,
            deleted,
            no_dam: _no_dam,
        } = bit_index
        {
            // We've found at least one sector.
            not_found = false;

            // Note the bad address mark CRC and data CRC, however ignore them and keep reading.
            address_crc_error |= !address_crc_valid;
            data_crc_error |= !data_crc_valid;
            deleted_mark |= deleted;

            // In a normal Read Sector operation, we'd check the value of N in the sector header.
            // When reading all sectors in a track, we specify the value of N for all sectors in
            // the entire track. The value of N in the sector header is ignored. This allows us
            // to read data outside a sector in the case of an 'N' mismatch.
            log::trace!(
                "read_all_sectors_bitstream(): Found sector_id: {} at offset: {} read length: {}",
                sector_chsn.s(),
                element_start,
                sector_read_vec.len()
            );

            self.read_exact_at(element_start + 64, &mut sector_read_vec)
                .map_err(|_| DiskImageError::BitstreamError)?;

            track_read_vec.extend(sector_read_vec.clone());
            sectors_read = sectors_read.saturating_add(1);

            if sector_chsn.s() == eot {
                println!(
                    "read_all_sectors_bitstream(): Reached EOT at sector: {} sectors_read: {}, eot: {}",
                    sector_chsn.s(),
                    sectors_read,
                    eot
                );
                break;
            }

            bit_index = self.get_first_sector_at_bit_index(element_end);
        }

        let read_len = track_read_vec.len();
        Ok(ReadTrackResult {
            not_found,
            sectors_read,
            read_buf: track_read_vec,
            deleted_mark,
            address_crc_error,
            data_crc_error,
            read_len_bits: read_len * 16,
            read_len_bytes: read_len,
        })
    }

    fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
        if self.sector_ids.is_empty() {
            log::warn!("get_next_id(): No sector_id vector for track!");
        }
        let first_sector = *self.sector_ids.first()?;
        let mut sector_matched = false;
        for sid in &self.sector_ids {
            if sector_matched {
                return Some(*sid);
            }
            if sid.s() == chs.s() {
                // Have matching sector id
                sector_matched = true;
            }
        }
        // If we reached here, we matched the last sector in the list, so return the first
        // sector as we wrap around the track.
        if sector_matched {
            Some(first_sector)
        }
        else {
            log::warn!("get_next_id(): Sector not found: {:?}", chs);
            None
        }
    }

    fn read_track(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        let extra_bytes = overdump.unwrap_or(0);

        let data_size = self.data.len() / 16 + if self.data.len() % 16 > 0 { 1 } else { 0 };
        let dump_size = data_size + extra_bytes;

        let mut track_read_vec = vec![0u8; dump_size];

        self.data
            .seek(SeekFrom::Start(0))
            .map_err(|_| DiskImageError::SeekError)?;
        self.data
            .read_exact(&mut track_read_vec)
            .map_err(|_| DiskImageError::BitstreamError)?;

        Ok(ReadTrackResult {
            not_found: false,
            sectors_read: 0,
            read_buf: track_read_vec,
            deleted_mark: false,
            address_crc_error: false,
            data_crc_error: false,
            read_len_bits: self.data.len(),
            read_len_bytes: data_size,
        })
    }

    fn read_track_raw(&mut self, _overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        //let extra_bytes = overdump.unwrap_or(0);

        let data_size = self.data.len() / 8 + if self.data.len() % 8 > 0 { 1 } else { 0 };
        //let dump_size = data_size + extra_bytes;

        let track_read_vec = self.data.data();

        Ok(ReadTrackResult {
            not_found: false,
            sectors_read: 0,
            read_buf: track_read_vec,
            deleted_mark: false,
            address_crc_error: false,
            data_crc_error: false,
            read_len_bits: self.data.len(),
            read_len_bytes: data_size,
        })
    }

    fn has_weak_bits(&self) -> bool {
        self.data.has_weak_bits()
    }

    fn format(
        &mut self,
        standard: System34Standard,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<(), DiskImageError> {
        let bitcell_ct = self.data.len();
        let format_result =
            System34Parser::format_track_as_bytes(standard, bitcell_ct, format_buffer, fill_pattern, gap3)?;

        let new_bit_vec = self
            .data
            .encode(&format_result.track_bytes, false, EncodingVariant::Data);
        log::trace!(
            "New bitstream size: {} from {} bytes",
            new_bit_vec.len(),
            format_result.track_bytes.len()
        );
        self.data.replace(new_bit_vec);

        System34Parser::set_track_markers(&mut self.data, format_result.markers)?;

        // Scan the new track data for markers and create a clock map.
        let markers = System34Parser::scan_track_markers(&self.data);
        if markers.is_empty() {
            log::error!("TrackData::format(): No markers found in track data post-format.");
        }
        else {
            log::trace!("TrackData::format(): Found {} markers in track data.", markers.len());
        }
        System34Parser::create_clock_map(&markers, self.data.clock_map_mut());

        let new_metadata = DiskStructureMetadata::new(System34Parser::scan_track_metadata(&mut self.data, markers));

        let data_ranges = new_metadata.data_ranges();
        if !data_ranges.is_empty() {
            self.data.set_data_ranges(data_ranges);
        }

        // log::trace!(
        //     "TrackData::format(): Found {} metadata items in track data.",
        //     new_metadata.items.len()
        // );

        let new_sector_ids = new_metadata.get_sector_ids();
        if new_sector_ids.is_empty() {
            log::warn!("TrackData::format(): No sectors ids found in track metadata post-format");
        }

        self.metadata = new_metadata;
        self.sector_ids = new_sector_ids;

        Ok(())
    }

    fn get_track_consistency(&self) -> Result<TrackConsistency, DiskImageError> {
        let sector_ct = self.sector_ids.len();
        let mut consistency = TrackConsistency::default();
        let mut n_set: FoxHashSet<u8> = FoxHashSet::new();
        let mut last_n = 0;

        for (si, sector_id) in self.sector_ids.iter().enumerate() {
            if sector_id.s() != si as u8 + 1 {
                consistency.nonconsecutive_sectors = true;
            }
            last_n = sector_id.n();
            n_set.insert(sector_id.n());
        }

        if n_set.len() > 1 {
            //log::warn!("get_track_consistency(): Variable sector sizes detected: {:?}", n_set);
            consistency.consistent_sector_size = None;
        }
        else {
            //log::warn!("get_track_consistency(): Consistent sector size: {}", last_n);
            consistency.consistent_sector_size = Some(last_n);
        }

        for item in &self.metadata.items {
            if let DiskStructureElement::System34(System34Element::Data {
                address_crc,
                data_crc,
                deleted,
            }) = item.elem_type
            {
                if !address_crc {
                    consistency.bad_address_crc = true;
                }
                if !data_crc {
                    //log::warn!("reporting bad CRC for sector: {:?}", item.chsn);
                    consistency.bad_data_crc = true;
                }
                if deleted {
                    consistency.deleted_data = true;
                }
            }
        }

        consistency.sector_ct = sector_ct;
        Ok(consistency)
    }

    fn get_track_stream(&self) -> Option<&TrackDataStream> {
        Some(&self.data)
    }
}

impl BitStreamTrack {
    pub fn new(
        params: BitStreamTrackParams,
        shared: Arc<Mutex<SharedDiskContext>>,
    ) -> Result<BitStreamTrack, DiskImageError> {
        if params.data.is_empty() {
            log::error!("add_track_bitstream(): Data is empty.");
            return Err(DiskImageError::ParameterError);
        }
        if params.weak.is_some() && (params.data.len() != params.weak.unwrap().len()) {
            log::error!("add_track_bitstream(): Data and weak bit mask lengths do not match.");
            return Err(DiskImageError::ParameterError);
        }

        log::debug!(
            "BitStreamTrack::new(): {} track {}, {} bits",
            params.encoding,
            params.ch,
            params.bitcell_ct.unwrap_or(params.data.len() * 8)
        );

        let data = BitVec::from_bytes(params.data);
        let weak_bitvec_opt = params.weak.map(BitVec::from_bytes);

        let (mut data_stream, markers) = match params.encoding {
            DiskDataEncoding::Mfm => {
                let mut codec;
                // If a weak bit mask was provided by the file format, we will honor it.
                // Otherwise, if 'detect_weak' is set we will try to detect weak bits from the MFM stream.
                if weak_bitvec_opt.is_some() {
                    codec = MfmCodec::new(data, params.bitcell_ct, weak_bitvec_opt);
                }
                else {
                    codec = MfmCodec::new(data, params.bitcell_ct, None);
                    if params.detect_weak {
                        log::debug!("add_track_bitstream(): detecting weak bits...");
                        let weak_bitvec = codec.create_weak_bit_mask(MfmCodec::WEAK_BIT_RUN);
                        if weak_bitvec.any() {
                            log::debug!(
                                "add_track_bitstream(): Detected {} weak bits in MFM bitstream.",
                                weak_bitvec.count_ones()
                            );
                        }
                        _ = codec.set_weak_mask(weak_bitvec);
                    }
                }

                //log::debug!("add_track_bitstream(): Scanning for markers...");
                let mut data_stream: TrackDataStream = Box::new(codec);
                let markers = System34Parser::scan_track_markers(&data_stream);
                if !markers.is_empty() {
                    log::debug!("First marker found at {}", markers[0].start);
                }

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut());

                data_stream.set_track_padding();

                (data_stream, markers)
            }
            DiskDataEncoding::Fm => {
                let mut codec;

                // If a weak bit mask was provided by the file format, we will honor it.
                // Otherwise, we will try to detect weak bits from the MFM stream.
                if weak_bitvec_opt.is_some() {
                    codec = FmCodec::new(data, params.bitcell_ct, weak_bitvec_opt);
                }
                else {
                    codec = FmCodec::new(data, params.bitcell_ct, None);
                    // let weak_regions = codec.detect_weak_bits(9);
                    // log::trace!(
                    //     "add_track_bitstream(): Detected {} weak bit regions",
                    //     weak_regions.len()
                    // );
                    let weak_bitvec = codec.create_weak_bit_mask(FmCodec::WEAK_BIT_RUN);
                    if weak_bitvec.any() {
                        log::trace!(
                            "add_track_bitstream(): Detected {} weak bits in FM bitstream.",
                            weak_bitvec.count_ones()
                        );
                    }
                    _ = codec.set_weak_mask(weak_bitvec);
                }

                let mut data_stream: TrackDataStream = Box::new(codec);
                let markers = System34Parser::scan_track_markers(&data_stream);

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut());

                data_stream.set_track_padding();

                (data_stream, markers)
            }
            _ => {
                log::error!(
                    "add_track_bitstream(): Unsupported data encoding: {:?}",
                    params.encoding
                );
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        // let format = TrackFormat {
        //     data_encoding,
        //     data_sync: data_stream.get_sync(),
        //     data_rate,
        // };

        let metadata = DiskStructureMetadata::new(System34Parser::scan_track_metadata(&mut data_stream, markers));
        let sector_ids = metadata.get_sector_ids();
        if sector_ids.is_empty() {
            log::warn!(
                "add_track_bitstream(): No sector ids found in track {} metadata.",
                params.ch.c()
            );
        }
        let data_ranges = metadata.data_ranges();
        if !data_ranges.is_empty() {
            data_stream.set_data_ranges(data_ranges);
        }

        let sector_offsets = metadata
            .items
            .iter()
            .filter_map(|i| {
                if let DiskStructureElement::System34(System34Element::Data { .. }) = i.elem_type {
                    //log::trace!("Got Data element, returning start address: {}", i.start);
                    Some(i.start)
                }
                else {
                    None
                }
            })
            .collect::<Vec<_>>();

        log::debug!(
            "add_track_bitstream(): Retrieved {} sector bitstream offsets from {} metadata entries.",
            sector_offsets.len(),
            metadata.items.len()
        );

        Ok(BitStreamTrack {
            encoding: params.encoding,
            data_rate: params.data_rate,
            rpm: None,
            ch: params.ch,
            data: data_stream,
            metadata,
            sector_ids,
            shared,
        })
    }

    pub(crate) fn add_write(&mut self, _bytes: usize) {
        let mut write_count = self.shared.lock().unwrap().writes;
        write_count += 1;
        self.shared.lock().unwrap().writes = write_count;
    }

    fn read_exact_at(&mut self, offset: usize, buf: &mut [u8]) -> Result<(), DiskImageError> {
        self.data
            .seek(SeekFrom::Start(offset as u64))
            .map_err(|_| DiskImageError::SeekError)?;
        self.data.read_exact(buf)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Retrieves the bit index of the first sector in the track data after the specified bit index.
    ///
    /// This function searches the metadata for the first IDAM (Index Address Mark) starting from
    /// the specified bit index and returns the bit index of the corresponding sector data.
    ///
    /// # Returns
    /// - `TrackSectorScanResult::Found` if a sector is found.
    /// - `TrackSectorScanResult::NotFound` if no sector is found. The fields returned are meaningless.
    ///
    /// # Panics
    /// This function does not panic.
    pub(crate) fn get_first_sector_at_bit_index(&self, bit_index: usize) -> TrackSectorScanResult {
        let mut idam_chsn: Option<DiskChsn> = None;
        for mdi in &self.metadata.items {
            // Skip items until ww reach the specified bit index.
            if mdi.start < bit_index {
                continue;
            }

            match mdi {
                DiskStructureMetadataItem {
                    elem_type: DiskStructureElement::System34(System34Element::Marker(System34Marker::Idam, _)),
                    chsn,
                    ..
                } => {
                    // Match the first IDAM seen as we are returning the first sector.
                    idam_chsn = *chsn;
                }
                DiskStructureMetadataItem {
                    elem_type:
                        DiskStructureElement::System34(System34Element::Data {
                            address_crc,
                            data_crc,
                            deleted,
                        }),
                    ..
                } => {
                    if let Some(sector_chsn) = idam_chsn {
                        return TrackSectorScanResult::Found {
                            element_start: mdi.start,
                            element_end: mdi.end,
                            sector_chsn,
                            address_crc_valid: *address_crc,
                            data_crc_valid: *data_crc,
                            deleted: *deleted,
                            no_dam: false,
                        };
                    }
                }
                _ => {}
            }
        }

        TrackSectorScanResult::NotFound {
            wrong_cylinder: false,
            bad_cylinder: false,
            wrong_head: false,
        }
    }

    /// Retrieves the bit index of the sector requested by the `seek_chs` parameter in the track data.
    ///
    /// This function searches the metadata for the first matching IDAM (Index Address Mark) and returns
    /// the bit index of the corresponding sector data.
    ///
    /// # Arguments
    /// - `seek_chs` - The CHS address of the sector to find.
    /// - `n` - The sector size to match. If `None`, the sector size is not checked.
    ///
    /// # Returns
    /// - `Some(TrackDataIndexResult)` if the first sector is found, containing the start index,
    ///   sector CHSN, address CRC validity, data CRC validity, and deleted mark.
    /// - `None` if no sector is found.
    ///
    /// # Panics
    /// This function does not panic.
    pub(crate) fn get_sector_bit_index(&self, seek_chs: DiskChs, n: Option<u8>, debug: bool) -> TrackSectorScanResult {
        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;

        let mut last_idam_matched = false;
        let mut idam_chsn: Option<DiskChsn> = None;
        for mdi in &self.metadata.items {
            match mdi {
                DiskStructureMetadataItem {
                    elem_type:
                        DiskStructureElement::System34(System34Element::SectorHeader {
                            chsn,
                            address_crc,
                            data_missing,
                        }),
                    ..
                } => {
                    if *data_missing {
                        // If this sector header has no DAM, we will return right away
                        // and set no_dam to true.
                        return TrackSectorScanResult::Found {
                            element_start: 0,
                            element_end: 0,
                            sector_chsn: *chsn,
                            address_crc_valid: *address_crc,
                            data_crc_valid: false,
                            deleted: false,
                            no_dam: true,
                        };
                    }

                    // Sector header should have a corresponding DAM marker which we will
                    // match in the next iteration, if this sector header matches.

                    // We match in two stages - first we match sector id if provided.
                    if chsn.s() == seek_chs.s() {
                        let mut matched_c = false;
                        let mut matched_h = false;
                        let matched_n = n.is_none() || chsn.n() == n.unwrap();

                        // if c is 0xFF, we set the flag for bad cylinder.
                        if chsn.c() == 0xFF {
                            bad_cylinder = true;
                        }
                        // If c differs, we set the flag for wrong cylinder.
                        if chsn.c() != seek_chs.c() {
                            wrong_cylinder = true;
                        }
                        else {
                            matched_c = true;
                        }
                        // If h differs, we set the flag for wrong head.
                        if chsn.h() != seek_chs.h() {
                            wrong_head = true;
                        }
                        else {
                            matched_h = true;
                        }

                        // Second stage match
                        // If 'debug' is set, we only match on sector.
                        // If 'debug' is clear, if we matched c, h and n, we set the flag for last idam matched.
                        if debug || (matched_c && matched_h && matched_n) {
                            last_idam_matched = true;
                        }
                    }
                    idam_chsn = Some(*chsn);
                }
                DiskStructureMetadataItem {
                    elem_type:
                        DiskStructureElement::System34(System34Element::Data {
                            address_crc,
                            data_crc,
                            deleted,
                        }),
                    ..
                } => {
                    // log::trace!(
                    //     "get_sector_bit_index(): Found DAM at CHS: {:?}, index: {} last idam matched? {}",
                    //     idam_chsn,
                    //     mdi.start,
                    //     last_idam_matched
                    // );

                    // If we matched the last sector header, then this is the sector data
                    // we are looking for. Return the info.
                    if last_idam_matched {
                        return TrackSectorScanResult::Found {
                            element_start: mdi.start,
                            element_end: mdi.end,
                            sector_chsn: idam_chsn.unwrap(),
                            address_crc_valid: *address_crc,
                            data_crc_valid: *data_crc,
                            deleted: *deleted,
                            no_dam: false,
                        };
                    }
                }
                _ => {}
            }
        }

        TrackSectorScanResult::NotFound {
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
        }
    }

    pub fn calc_quality_score(&self) -> i32 {
        let mut score = 0;
        for s in self.get_sector_list() {
            // Weight having a sector heavily, so that missing sectors are heavily penalized.
            score += 5;
            if !s.address_crc_valid {
                // Bad address CRC is unusual, most likely track error.
                score -= 5;
            }
            if !s.data_crc_valid {
                // Bad data CRC is more common. Weight it less relative to other issues.
                score -= 1;
            }
        }
        score
    }
}
