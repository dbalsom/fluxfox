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

    src/trackdata.rs

    The TrackData enum that stores track data in either bitstream or bytestream format,
    and associated methods.

*/
use crate::bitstream::TrackDataStream;
use crate::chs::DiskChsn;
use crate::diskimage::{
    ReadSectorResult, ReadTrackResult, RwSectorScope, SectorMapEntry, TrackSectorIndex, WriteSectorResult,
};
use crate::structure_parsers::system34::{System34Element, System34Marker};
use crate::structure_parsers::{DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem};
use crate::{DiskCh, DiskChs, DiskImageError};
use sha1_smol::Digest;
use std::io::{Read, Seek, SeekFrom};

pub struct TrackDataIndexResult {
    element_start: usize,
    element_end: usize,
    sector_chsn: DiskChsn,
    address_crc_valid: bool,
    data_crc_valid: bool,
    deleted: bool,
}

/// A TrackData enum is one of two variants indicating the representational level of the disk image.
/// A BitStream variant contains an encoded bitstream of the disk data along with metadata describing
/// the structure of the data.
/// A ByteStream variant contains byte-level data organized by sector. A weak bit mask may be
/// present to indicate sectors with weak bits.
pub enum TrackData {
    BitStream {
        cylinder: u16,
        head: u8,
        data_clock: u32,
        data: TrackDataStream,
        metadata: DiskStructureMetadata,
    },
    ByteStream {
        cylinder: u16,
        head: u8,
        sectors: Vec<TrackSectorIndex>,
        data: Vec<u8>,
        weak_mask: Vec<u8>,
    },
}

impl TrackData {
    pub(crate) fn metadata(&self) -> Option<&DiskStructureMetadata> {
        match self {
            TrackData::BitStream { metadata, .. } => Some(metadata),
            TrackData::ByteStream { .. } => None,
        }
    }

    pub(crate) fn get_sector_ct(&self) -> usize {
        match self {
            TrackData::ByteStream { sectors, .. } => sectors.len(),
            TrackData::BitStream { metadata, .. } => {
                let mut sector_ct = 0;
                for item in &metadata.items {
                    if item.elem_type.is_sector() {
                        sector_ct += 1;
                    }
                }
                sector_ct
            }
        }
    }

    pub(crate) fn has_sector_id(&self, id: u8) -> bool {
        match self {
            TrackData::ByteStream { sectors, .. } => {
                for sector in sectors {
                    if sector.sector_id == id {
                        return true;
                    }
                }
            }
            TrackData::BitStream { metadata, .. } => {
                for item in &metadata.items {
                    if let DiskStructureElement::System34(System34Element::Marker(System34Marker::Idam, _)) =
                        item.elem_type
                    {
                        if let Some(chsn) = item.chsn {
                            if chsn.s() == id {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub(crate) fn get_sector_list(&self) -> Vec<SectorMapEntry> {
        match self {
            TrackData::ByteStream { sectors, .. } => sectors
                .iter()
                .map(|s| SectorMapEntry {
                    chsn: DiskChsn::from((s.cylinder_id, s.head_id, s.sector_id, s.n)),
                    address_crc_valid: !s.address_crc_error,
                    data_crc_valid: !s.data_crc_error,
                    deleted_mark: s.deleted_mark,
                })
                .collect(),
            TrackData::BitStream { metadata, .. } => {
                let mut sector_list = Vec::new();
                for item in &metadata.items {
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
                            });
                        }
                    }
                }
                sector_list
            }
        }
    }

    pub(crate) fn read_exact_at(&mut self, offset: usize, buf: &mut [u8]) -> Result<(), DiskImageError> {
        match self {
            TrackData::BitStream { data, .. } => match data {
                TrackDataStream::Mfm(mfm_decoder) => {
                    mfm_decoder
                        .seek(SeekFrom::Start((offset >> 1) as u64))
                        .map_err(|_| DiskImageError::SeekError)?;
                    mfm_decoder.read_exact(buf).map_err(|_| DiskImageError::IoError)?;
                }
                _ => {
                    return Err(DiskImageError::UnsupportedFormat);
                }
            },
            TrackData::ByteStream { data, .. } => {
                buf.copy_from_slice(&data[offset..offset + buf.len()]);
            }
        }
        Ok(())
    }

    /// Retrieves the bit index of the first sector in the track data.
    ///
    /// This function searches the metadata for the first IDAM (Index Address Mark) and returns
    /// the bit index of the corresponding sector data.
    ///
    /// # Returns
    /// - `Some(TrackDataIndexResult)` if the first sector is found, containing the start index,
    ///   sector CHSN, address CRC validity, data CRC validity, and deleted mark.
    /// - `None` if no sector is found.
    ///
    /// # Panics
    /// This function does not panic.
    pub(crate) fn get_first_sector_at_bit_index(&self, bit_index: usize) -> Option<TrackDataIndexResult> {
        match self {
            TrackData::BitStream { metadata, .. } => {
                let mut last_idam_matched = false;
                let mut idam_chsn: Option<DiskChsn> = None;
                for mdi in &metadata.items {
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
                            last_idam_matched = true;
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
                            if last_idam_matched {
                                return Some(TrackDataIndexResult {
                                    element_start: mdi.start,
                                    element_end: mdi.end,
                                    sector_chsn: idam_chsn?,
                                    address_crc_valid: *address_crc,
                                    data_crc_valid: *data_crc,
                                    deleted: *deleted,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            TrackData::ByteStream { .. } => {}
        }

        None
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
    pub(crate) fn get_sector_bit_index(
        &self,
        seek_chs: DiskChs,
        n: Option<u8>,
    ) -> Option<(usize, DiskChsn, bool, bool, bool)> {
        match self {
            TrackData::BitStream { metadata, .. } => {
                let mut last_idam_matched = false;
                let mut idam_chsn: Option<DiskChsn> = None;
                for mdi in &metadata.items {
                    match mdi {
                        DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::Marker(System34Marker::Idam, _)),
                            chsn,
                            ..
                        } => {
                            if let Some(metadata_chsn) = chsn {
                                if DiskChs::from(*metadata_chsn) == seek_chs && (n.is_none() || metadata_chsn.n() == n?)
                                {
                                    last_idam_matched = true;
                                }
                            }
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
                            // log::trace!(
                            //     "get_sector_bit_index(): Found DAM at CHS: {:?}, index: {} last idam matched? {}",
                            //     idam_chsn,
                            //     mdi.start,
                            //     last_idam_matched
                            // );
                            if last_idam_matched {
                                return Some((mdi.start, idam_chsn.unwrap(), *address_crc, *data_crc, *deleted));
                            }
                        }
                        _ => {}
                    }
                }
            }
            TrackData::ByteStream { .. } => {}
        }

        None
    }

    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags where are needed
    /// when handling ByteStream images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    pub(crate) fn read_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        let data_idx;
        let mut data_len;

        let mut read_vec = Vec::new();

        let mut data_crc_error = false;
        let mut address_crc_error = false;
        let mut deleted_mark = false;
        let mut wrong_cylinder = false;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = match self {
            TrackData::BitStream { .. } => self.get_sector_bit_index(chs, n),
            TrackData::ByteStream { .. } => None,
        };

        match self {
            TrackData::BitStream {
                data: TrackDataStream::Mfm(mfm_decoder),
                ..
            } => {
                let (sector_offset, chsn, address_crc_valid, data_crc_valid, deleted) = match bit_index {
                    Some(idx) => idx,
                    None => {
                        log::warn!("Sector marker not found reading sector!");
                        return Err(DiskImageError::DataError);
                    }
                };
                address_crc_error = !address_crc_valid;
                // If there's a bad address mark, we not proceed to read the data, unless we're requesting
                // it anyway for debugging purposes.
                if address_crc_error && !debug {
                    return Ok(ReadSectorResult {
                        data_idx: 0,
                        data_len: 0,
                        read_buf: Vec::new(),
                        deleted_mark: false,
                        not_found: false,
                        address_crc_error: true,
                        data_crc_error: false,
                        wrong_cylinder,
                        wrong_head: false,
                    });
                }

                deleted_mark = deleted;
                data_crc_error = !data_crc_valid;

                // The caller can request the scope of the read to be the entire data block
                // including address mark and crc bytes, or just the data. Handle offsets accordingly.
                let (scope_read_off, scope_data_off, scope_data_adj) = match scope {
                    // Add 4 bytes for address mark and 2 bytes for CRC.
                    RwSectorScope::DataBlock => (0, 4, 6),
                    RwSectorScope::DataOnly => (32, 0, 0),
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
                    } else {
                        if chsn.n() != n_value {
                            log::error!(
                                "read_sector(): Sector size mismatch, expected: {} got: {}",
                                chsn.n(),
                                n_value
                            );
                            return Err(DiskImageError::DataError);
                        }
                        data_len = chsn.n_size();
                    }
                } else {
                    data_len = chsn.n_size();
                }
                data_idx = scope_data_off;

                read_vec = vec![0u8; data_len + scope_data_adj];

                log::trace!(
                    "read_sector(): Found sector_id: {} at offset: {} read length: {}",
                    chs.s(),
                    sector_offset,
                    read_vec.len()
                );

                mfm_decoder
                    .seek(SeekFrom::Start(((sector_offset >> 1) + scope_read_off) as u64))
                    .map_err(|_| DiskImageError::SeekError)?;
                mfm_decoder
                    .read_exact(&mut read_vec)
                    .map_err(|_| DiskImageError::IoError)?;
            }
            TrackData::ByteStream { sectors, data, .. } => {
                // No address mark for ByteStream data, so data starts immediately.
                data_idx = 0;
                data_len = 0;

                match scope {
                    // Add 4 bytes for address mark and 2 bytes for CRC.
                    RwSectorScope::DataBlock => unimplemented!("DataBlock scope not supported for ByteStream"),
                    RwSectorScope::DataOnly => {}
                };

                for si in sectors {
                    if si.sector_id == chs.s() {
                        log::trace!(
                            "read_sector(): Found sector_id: {} at t_idx: {}",
                            si.sector_id,
                            si.t_idx
                        );

                        data_len = std::cmp::min(si.t_idx + si.len, data.len()) - si.t_idx;
                        read_vec.extend(data[si.t_idx..si.t_idx + data_len].to_vec());

                        if si.data_crc_error {
                            data_crc_error = true;
                        }

                        if si.cylinder_id != chs.c() {
                            wrong_cylinder = true;
                        }

                        if si.deleted_mark {
                            deleted_mark = true;
                        }
                    }
                }
            }
            _ => {
                return Err(DiskImageError::UnsupportedFormat);
            }
        }

        Ok(ReadSectorResult {
            data_idx,
            data_len,
            read_buf: read_vec,
            deleted_mark,
            not_found: false,
            address_crc_error,
            data_crc_error,
            wrong_cylinder,
            wrong_head: false,
        })
    }

    pub(crate) fn write_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        write_data: &[u8],
        _scope: RwSectorScope,
        _debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let mut wrong_cylinder = false;
        let mut wrong_head = false;
        match self {
            TrackData::BitStream { .. } => {
                log::error!("write_sector(): BitStream write not supported");
                return Err(DiskImageError::UnsupportedFormat);
            }
            TrackData::ByteStream { sectors, data, .. } => {
                for si in sectors {
                    let mut sector_match;

                    sector_match = si.sector_id == chs.s();

                    // Validate n too if provided.
                    if let Some(n) = n {
                        sector_match = sector_match && si.n == n;
                    }

                    if sector_match {
                        // Validate provided data size.
                        let write_data_len = write_data.len();
                        if DiskChsn::n_to_bytes(si.n) != write_data_len {
                            // Caller didn't provide correct buffer size.
                            log::error!(
                                "write_sector(): Data buffer size mismatch, expected: {} got: {}",
                                DiskChsn::n_to_bytes(si.n),
                                write_data_len
                            );
                            return Err(DiskImageError::ParameterError);
                        }

                        if si.cylinder_id != chs.c() {
                            wrong_cylinder = true;
                        }

                        if si.head_id != chs.h() {
                            wrong_head = true;
                        }

                        data[si.t_idx..si.t_idx + write_data_len].copy_from_slice(write_data);
                        break;
                    }
                }
            }
        }

        Ok(WriteSectorResult {
            not_found: false,
            address_crc_error: false,
            wrong_cylinder,
            wrong_head,
        })
    }

    pub(crate) fn get_hash(&self) -> Digest {
        let mut hasher = sha1_smol::Sha1::new();
        match self {
            TrackData::BitStream { data, .. } => {
                hasher.update(&data.data());
                hasher.digest()
            }
            TrackData::ByteStream { data, .. } => {
                hasher.update(data);
                hasher.digest()
            }
        }
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    pub(crate) fn read_all_sectors(&mut self, ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        match self {
            TrackData::BitStream { .. } => self.read_all_sectors_bitstream(ch, n, eot),
            TrackData::ByteStream { .. } => self.read_all_sectors_bytestream(ch, n, eot),
        }
    }

    fn read_all_sectors_bitstream(&mut self, _ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        let eot = eot as u16;
        let mut track_read_vec = Vec::with_capacity(512 * 9);
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut sector_read_vec = vec![0u8; sector_data_len];

        let mut data_crc_error = false;
        let mut address_crc_error = false;
        let mut deleted_mark = false;
        let mut not_found = true;
        let mut sectors_read: u16 = 0;

        // Read index first to avoid borrowing issues in next match.
        let mut bit_index = match self.get_first_sector_at_bit_index(0) {
            Some(tdir) => Some(tdir),
            None => return Err(DiskImageError::DataError),
        };

        while bit_index.is_some() {
            if let Some(TrackDataIndexResult {
                element_start,
                element_end,
                sector_chsn,
                address_crc_valid,
                data_crc_valid,
                deleted,
            }) = bit_index
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
                    .map_err(|_| DiskImageError::IoError)?;

                track_read_vec.extend(sector_read_vec.clone());
                sectors_read = sectors_read.saturating_add(1);

                if sectors_read >= eot {
                    log::trace!(
                        "\
                        read_all_sectors_bitstream(): Reached EOT at sector: {} \
                        sectors_read: {}, eot: {}",
                        sector_chsn.s(),
                        sectors_read,
                        eot
                    );
                    break;
                }

                bit_index = self.get_first_sector_at_bit_index(element_end);
            };
        }

        Ok(ReadTrackResult {
            not_found,
            sectors_read,
            read_buf: track_read_vec,
            deleted_mark,
            address_crc_error,
            data_crc_error,
        })
    }

    fn read_all_sectors_bytestream(&mut self, _ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        let eot = eot as u16;
        let mut track_read_vec = Vec::with_capacity(512 * 9);
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut address_crc_error = false;
        let mut data_crc_error = false;
        let mut deleted_mark = false;
        let mut last_data_end = 0;

        let mut not_found = true;
        let mut sectors_read = 0;

        if let TrackData::ByteStream { sectors, data, .. } = self {
            for si in sectors {
                log::trace!(
                    "read_all_sectors_bytestream(): Found sector_id: {} at t_idx: {}",
                    si.sector_id,
                    si.t_idx
                );
                not_found = false;

                if sectors_read >= eot {
                    log::trace!(
                        "\
                        read_all_sectors_bytestream(): Reached EOT at sector: {} \
                        sectors_read: {}, eot: {}",
                        si.sector_id,
                        sectors_read,
                        eot
                    );
                    break;
                }

                if si.t_idx < last_data_end {
                    log::trace!(
                        "read_all_sectors_bytestream(): Skipping overlapped sector {} at t_idx: {}",
                        si.sector_id,
                        si.t_idx
                    );
                    continue;
                }

                sectors_read = sectors_read.saturating_add(1);

                let data_len = std::cmp::min(sector_data_len, data.len() - si.t_idx);
                track_read_vec.extend(data[si.t_idx..si.t_idx + data_len].to_vec());
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
        }

        Ok(ReadTrackResult {
            not_found,
            sectors_read,
            read_buf: track_read_vec,
            deleted_mark,
            address_crc_error,
            data_crc_error,
        })
    }

    pub(crate) fn read_track(&mut self, ch: DiskCh) -> Result<ReadTrackResult, DiskImageError> {
        match self {
            TrackData::BitStream { .. } => self.read_track_bitstream(ch),
            TrackData::ByteStream { .. } => self.read_track_bytestream(ch),
        }
    }

    fn read_track_bitstream(&mut self, _ch: DiskCh) -> Result<ReadTrackResult, DiskImageError> {
        if let TrackData::BitStream {
            data: TrackDataStream::Mfm(mfm_decoder),
            ..
        } = self
        {
            let data_size = mfm_decoder.len() / 16 + if mfm_decoder.len() % 16 > 0 { 1 } else { 0 };
            let mut track_read_vec = vec![0u8; data_size];

            mfm_decoder
                .seek(SeekFrom::Start(0))
                .map_err(|_| DiskImageError::SeekError)?;
            mfm_decoder
                .read_exact(&mut track_read_vec)
                .map_err(|_| DiskImageError::IoError)?;

            Ok(ReadTrackResult {
                not_found: false,
                sectors_read: 0,
                read_buf: track_read_vec,
                deleted_mark: false,
                address_crc_error: false,
                data_crc_error: false,
            })
        } else {
            Err(DiskImageError::UnsupportedFormat)
        }
    }

    fn read_track_bytestream(&mut self, _ch: DiskCh) -> Result<ReadTrackResult, DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
