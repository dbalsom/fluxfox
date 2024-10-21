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

    src/track/metasector.rs

    Implements the MetaSector track type and the Track trait for same.

*/
use super::{Track, TrackConsistency, TrackInfo, TrackSectorScanResult};
use crate::bitstream::mfm::MFM_BYTE_LEN;
use crate::bitstream::{EncodingVariant, TrackDataStream};
use crate::diskimage::{
    ReadSectorResult, ReadTrackResult, RwSectorScope, ScanSectorResult, SectorDescriptor, TrackSectorIndex,
    WriteSectorResult,
};
use crate::io::SeekFrom;
use crate::structure_parsers::system34::{
    System34Element, System34Marker, System34Parser, System34Standard, DAM_MARKER_BYTES, DDAM_MARKER_BYTES,
};
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem, DiskStructureParser,
};
use crate::util::crc_ibm_3740;
use crate::{DiskCh, DiskChs, DiskChsn, DiskDataEncoding, DiskDataRate, DiskImageError, FoxHashSet, SectorMapEntry};
use sha1_smol::Digest;
use std::any::Any;

pub struct MetaSectorTrack {
    pub(crate) encoding: DiskDataEncoding,
    pub(crate) data_rate: DiskDataRate,
    pub(crate) ch: DiskCh,
    pub(crate) sectors: Vec<TrackSectorIndex>,
    pub(crate) data: Vec<u8>,
    pub(crate) weak_mask: Vec<u8>,
}

impl Track for MetaSectorTrack {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn ch(&self) -> DiskCh {
        self.ch
    }

    fn set_ch(&mut self, new_ch: DiskCh) {
        self.ch = new_ch;
    }

    fn info(&self) -> TrackInfo {
        TrackInfo {
            encoding: self.encoding,
            data_rate: self.data_rate,
            bit_length: 0,
            sector_ct: self.sectors.len(),
        }
    }

    fn metadata(&self) -> Option<&DiskStructureMetadata> {
        None
    }

    fn get_sector_ct(&self) -> usize {
        self.sectors.len()
    }

    fn has_sector_id(&self, id: u8, id_chsn: Option<DiskChsn>) -> bool {
        for sector in &self.sectors {
            if id_chsn.is_none() && sector.id_chsn.s() == id {
                return true;
            } else if let Some(chsn) = id_chsn {
                if sector.id_chsn == chsn {
                    return true;
                }
            }
        }
        false
    }

    fn get_sector_list(&self) -> Vec<SectorMapEntry> {
        self.sectors
            .iter()
            .map(|s| SectorMapEntry {
                chsn: s.id_chsn,
                address_crc_valid: !s.address_crc_error,
                data_crc_valid: !s.data_crc_error,
                deleted_mark: s.deleted_mark,
                no_dam: false,
            })
            .collect()
    }

    fn read_exact_at(&mut self, _offset: usize, _buf: &mut [u8]) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn add_sector(&mut self, sd: &SectorDescriptor) -> Result<(), DiskImageError> {
        // Create an empty weak bit mask if none is provided.
        let weak_buf_vec = match &sd.weak {
            Some(weak_buf) => weak_buf.to_vec(),
            None => vec![0; sd.data.len()],
        };

        let id_chsn = DiskChsn::from((
            sd.cylinder_id.unwrap_or(self.ch.c()),
            sd.head_id.unwrap_or(self.ch.h()),
            sd.id,
            sd.n,
        ));

        self.sectors.push(TrackSectorIndex {
            id_chsn,
            t_idx: self.data.len(),
            len: sd.data.len(),
            address_crc_error: sd.address_crc_error,
            data_crc_error: sd.data_crc_error,
            deleted_mark: sd.deleted_mark,
        });

        self.data.extend(&sd.data);
        self.weak_mask.extend(weak_buf_vec);

        Ok(())
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

        let mut id_chsn = None;

        // No address mark for ByteStream data, so data starts immediately.
        data_idx = 0;
        data_len = 0;

        match scope {
            // Add 4 bytes for address mark and 2 bytes for CRC.
            RwSectorScope::DataBlock => unimplemented!("DataBlock scope not supported for ByteStream"),
            RwSectorScope::DataOnly => {}
        };

        let mut last_idam_matched = false;

        for si in &self.sectors {
            if si.id_chsn.s() == chs.s() {
                log::trace!("read_sector(): Found sector_id: {} at t_idx: {}", si.id_chsn, si.t_idx);

                let mut matched_n = false;
                if n.is_none() || si.id_chsn.n() == n.unwrap() {
                    matched_n = true;
                }

                if si.data_crc_error {
                    data_crc_error = true;
                }
                if si.id_chsn.c() != chs.c() {
                    wrong_cylinder = true;
                }
                let matched_c = !wrong_cylinder;
                if si.id_chsn.c() == 0xFF {
                    bad_cylinder = true;
                }
                if si.id_chsn.h() != chs.h() {
                    wrong_head = true;
                }
                let matched_h = !wrong_head;
                if si.deleted_mark {
                    deleted_mark = true;
                }

                if debug || (matched_c && matched_h && matched_n) {
                    last_idam_matched = true;
                }
            }

            if last_idam_matched {
                id_chsn = Some(si.id_chsn);
                data_len = std::cmp::min(si.t_idx + si.len, self.data.len()) - si.t_idx;
                read_vec.extend(self.data[si.t_idx..si.t_idx + data_len].to_vec());
                break;
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
        let mut data_crc_error = false;
        let mut address_crc_error = false;
        let mut deleted_mark = false;
        let mut wrong_cylinder = false;
        let bad_cylinder = false;
        let wrong_head = false;

        for si in &self.sectors {
            if si.id_chsn.s() == chs.s() {
                log::trace!(
                    "scan_sector(): Found sector_id: {} at t_idx: {}",
                    si.id_chsn.s(),
                    si.t_idx
                );

                if si.address_crc_error {
                    address_crc_error = true;
                }

                if si.data_crc_error {
                    data_crc_error = true;
                }

                if si.id_chsn.c() != chs.c() {
                    wrong_cylinder = true;
                }

                if si.deleted_mark {
                    deleted_mark = true;
                }
            }
        }

        Ok(ScanSectorResult {
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

    fn write_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        write_data: &[u8],
        _scope: RwSectorScope,
        write_deleted: bool,
        _debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let mut wrong_cylinder = false;
        let bad_cylinder = false;
        let mut wrong_head = false;

        for si in &self.sectors {
            let mut sector_match;

            sector_match = si.id_chsn.s() == chs.s();

            // Validate n too if provided.
            if let Some(n) = n {
                sector_match = sector_match && si.id_chsn.n() == n;
            }

            if sector_match {
                // Validate provided data size.
                let write_data_len = write_data.len();
                if DiskChsn::n_to_bytes(si.id_chsn.n()) != write_data_len {
                    // Caller didn't provide correct buffer size.
                    log::error!(
                        "write_sector(): Data buffer size mismatch, expected: {} got: {}",
                        DiskChsn::n_to_bytes(si.id_chsn.n()),
                        write_data_len
                    );
                    return Err(DiskImageError::ParameterError);
                }

                if si.id_chsn.c() != chs.c() {
                    wrong_cylinder = true;
                }

                if si.id_chsn.h() != chs.h() {
                    wrong_head = true;
                }

                self.data[si.t_idx..si.t_idx + write_data_len].copy_from_slice(write_data);
                break;
            }
        }

        Ok(WriteSectorResult {
            not_found: false,
            no_dam: false,
            address_crc_error: false,
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
        })
    }

    fn get_hash(&self) -> Digest {
        let mut hasher = sha1_smol::Sha1::new();
        hasher.update(&self.data);
        hasher.digest()
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        let eot = eot as u16;
        let mut track_read_vec = Vec::with_capacity(512 * 9);
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut address_crc_error = false;
        let mut data_crc_error = false;
        let mut deleted_mark = false;
        let mut last_data_end = 0;

        let mut not_found = true;
        let mut sectors_read = 0;

        for si in &self.sectors {
            log::trace!(
                "read_all_sectors_bytestream(): Found sector_id: {} at t_idx: {}",
                si.id_chsn.s(),
                si.t_idx
            );
            not_found = false;

            if sectors_read >= eot {
                log::trace!(
                    "\
                        read_all_sectors_bytestream(): Reached EOT at sector: {} \
                        sectors_read: {}, eot: {}",
                    si.id_chsn.s(),
                    sectors_read,
                    eot
                );
                break;
            }

            if si.t_idx < last_data_end {
                log::trace!(
                    "read_all_sectors_bytestream(): Skipping overlapped sector {} at t_idx: {}",
                    si.id_chsn.s(),
                    si.t_idx
                );
                continue;
            }

            sectors_read = sectors_read.saturating_add(1);

            let data_len = std::cmp::min(sector_data_len, self.data.len() - si.t_idx);
            track_read_vec.extend(self.data[si.t_idx..si.t_idx + data_len].to_vec());
            last_data_end = si.t_idx + data_len;

            if si.address_crc_error {
                address_crc_error |= true;
            }

            if si.data_crc_error {
                data_crc_error |= true;
            }

            if si.deleted_mark {
                deleted_mark |= true;
            }
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
        let first_sector = self.sectors.first()?;
        let mut sector_matched = false;
        for si in self.sectors.iter() {
            if sector_matched {
                return Some(DiskChsn::new(chs.c(), chs.h(), si.id_chsn.s(), si.id_chsn.n()));
            }
            if si.id_chsn.s() == chs.s() {
                // Have matching sector id
                sector_matched = true;
            }
        }
        // If we reached here, we matched the last sector in the list, so return the first
        // sector as we wrap around the track.
        if sector_matched {
            Some(DiskChsn::new(
                chs.c(),
                chs.h(),
                first_sector.id_chsn.s(),
                first_sector.id_chsn.n(),
            ))
        } else {
            None
        }
    }

    fn read_track(&mut self, _overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn has_weak_bits(&self) -> bool {
        !self.weak_mask.is_empty() && self.weak_mask.iter().any(|&x| x != 0)
    }

    fn format(
        &mut self,
        standard: System34Standard,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<(), DiskImageError> {
        // TODO: Implement format for MetaSectorTrack
        Err(DiskImageError::UnsupportedFormat)
    }

    fn get_track_consistency(&self) -> TrackConsistency {
        let sector_ct;

        let mut consistency = TrackConsistency::default();

        sector_ct = self.sectors.len();

        let mut n_set: FoxHashSet<u8> = FoxHashSet::new();
        let mut last_n = 0;
        for (si, sector) in self.sectors.iter().enumerate() {
            if sector.id_chsn.s() != si as u8 + 1 {
                consistency.nonconsecutive_sectors = true;
            }
            if sector.data_crc_error {
                consistency.bad_data_crc = true;
            }
            if sector.address_crc_error {
                consistency.bad_address_crc = true;
            }
            if sector.deleted_mark {
                consistency.deleted_data = true;
            }
            last_n = sector.id_chsn.n();
            n_set.insert(sector.id_chsn.n());
        }

        if n_set.len() > 1 {
            consistency.consistent_sector_size = None;
        } else {
            consistency.consistent_sector_size = Some(last_n);
        }

        consistency.sector_ct = sector_ct;
        consistency
    }
}
