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
use crate::chs::DiskChsn;
use crate::diskimage::{ReadSectorResult, RwSectorScope, SectorMapEntry, TrackDataStream, TrackSectorIndex};
use crate::structure_parsers::system34::{System34Element, System34Marker};
use crate::structure_parsers::{DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem};
use crate::{DiskChs, DiskImageError};
use std::io::{Read, Seek, SeekFrom};

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
                    if let DiskStructureElement::System34(System34Element::Data { crc, deleted }) = item.elem_type {
                        if let Some(chsn) = item.chsn {
                            sector_list.push(SectorMapEntry {
                                chsn,
                                address_crc_valid: true,
                                data_crc_valid: crc,
                                deleted_mark: deleted,
                            });
                        }
                    }
                }
                sector_list
            }
        }
    }

    pub(crate) fn get_sector_bit_index(&self, seek_chs: DiskChs) -> Option<(usize, DiskChsn, bool)> {
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
                            if let Some(idam_chs) = chsn {
                                if DiskChs::from(*idam_chs) == seek_chs {
                                    log::warn!("Found matching IDAM at CHS: {:?}", idam_chs);
                                    last_idam_matched = true;
                                }
                            }
                            idam_chsn = *chsn;
                        }
                        DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::Data { crc, .. }),
                            ..
                        } => {
                            log::trace!(
                                "Found DAM at CHS: {:?}, index: {} last idam matched? {}",
                                idam_chsn,
                                mdi.start,
                                last_idam_matched
                            );
                            if last_idam_matched {
                                return Some((mdi.start, idam_chsn.unwrap(), *crc));
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

    pub(crate) fn read_sector(
        &mut self,
        chs: DiskChs,
        scope: RwSectorScope,
    ) -> Result<ReadSectorResult, DiskImageError> {
        let data_idx;
        let mut data_len;

        let mut read_vec = Vec::new();

        let mut crc_error = false;
        let mut deleted_mark = false;
        let mut wrong_cylinder = false;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = match self {
            TrackData::BitStream { .. } => self.get_sector_bit_index(chs),
            TrackData::ByteStream { .. } => None,
        };

        match self {
            TrackData::BitStream {
                data: TrackDataStream::Mfm(mfm_decoder),
                ..
            } => {
                let (sector_offset, chsn, crc_good) = match bit_index {
                    Some(idx) => idx,
                    None => {
                        log::error!("Sector marker not found reading sector!");
                        return Err(DiskImageError::SeekError);
                    }
                };

                if !crc_good {
                    crc_error = true;
                }

                // The caller can request the scope of the read to be the entire data block
                // including address mark and crc bytes, or just the data. Handle offsets accordingly.
                let (scope_read_off, scope_data_off, scope_data_adj) = match scope {
                    // Add 4 bytes for address mark and 2 bytes for CRC.
                    RwSectorScope::DataBlock => (0, 4, 6),
                    RwSectorScope::DataOnly => (32, 0, 0),
                };

                data_len = chsn.n_size();
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
                            crc_error = true;
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
            address_crc_error: false,
            data_crc_error: crc_error,
            wrong_cylinder,
            wrong_head: false,
        })
    }
}
