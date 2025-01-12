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

    src/track/bitstream.rs

    Implements the Bitstream track type and the Track trait for same.

*/
use super::{Track, TrackAnalysis, TrackInfo, TrackSectorScanResult};
use crate::{
    bitstream_codec::{fm::FmCodec, gcr::GcrCodec, mfm::MfmCodec, EncodingVariant, TrackCodec, TrackDataStream},
    io::SeekFrom,
    source_map::SourceMap,
    track_schema::{
        system34::{System34Element, System34Marker, System34Schema, System34Standard},
        TrackElement,
        TrackElementInstance,
        TrackMetadata,
        TrackSchema,
        TrackSchemaParser,
    },
    types::{
        chs::DiskChsnQuery,
        AddSectorParams,
        BitStreamTrackParams,
        DiskCh,
        DiskChs,
        DiskChsn,
        DiskRpm,
        ReadSectorResult,
        ReadTrackResult,
        RwScope,
        ScanSectorResult,
        SharedDiskContext,
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
use bit_vec::BitVec;
use sha1_smol::Digest;
use std::{
    any::Any,
    sync::{Arc, Mutex},
};
use strum::IntoEnumIterator;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct BitStreamTrack {
    pub(crate) encoding: TrackDataEncoding,
    pub(crate) data_rate: TrackDataRate,
    pub(crate) rpm: Option<DiskRpm>,
    pub(crate) ch: DiskCh,
    pub(crate) data: TrackDataStream,
    pub(crate) metadata: TrackMetadata,
    pub(crate) schema: Option<TrackSchema>,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) shared: Option<Arc<Mutex<SharedDiskContext>>>,
}

#[cfg_attr(feature = "serde", typetag::serde)]
impl Track for BitStreamTrack {
    fn resolution(&self) -> TrackDataResolution {
        TrackDataResolution::BitStream
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_bitstream_track(&self) -> Option<&BitStreamTrack> {
        self.as_any().downcast_ref::<BitStreamTrack>()
    }

    fn as_bitstream_track_mut(&mut self) -> Option<&mut BitStreamTrack> {
        self.as_any_mut().downcast_mut::<BitStreamTrack>()
    }

    fn ch(&self) -> DiskCh {
        self.ch
    }

    fn set_ch(&mut self, new_ch: DiskCh) {
        self.ch = new_ch;
    }

    fn encoding(&self) -> TrackDataEncoding {
        self.encoding
    }

    fn info(&self) -> TrackInfo {
        TrackInfo {
            resolution: self.resolution(),
            encoding: self.encoding,
            schema: self.schema,
            data_rate: self.data_rate,
            density: Some(TrackDensity::from(self.data_rate)),
            rpm: self.rpm,
            bit_length: self.data.len(),
            sector_ct: self.metadata.sector_ids().len(),
            flux_info: None,
        }
    }

    fn metadata(&self) -> Option<&TrackMetadata> {
        Some(&self.metadata)
    }

    fn sector_ct(&self) -> usize {
        let mut sector_ct = 0;
        for item in &self.metadata.items {
            if item.element.is_sector_header() {
                sector_ct += 1;
            }
        }
        sector_ct
    }

    fn has_sector_id(&self, id: u8, _id_chsn: Option<DiskChsn>) -> bool {
        for item in &self.metadata.items {
            if let TrackElement::System34(System34Element::Marker(System34Marker::Idam, _)) = item.element {
                if let Some(chsn) = item.chsn {
                    if chsn.s() == id {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn sector_list(&self) -> Vec<SectorMapEntry> {
        self.metadata.sector_list()

        // if self.schema.is_none() {
        //     log::debug!("sector_list(): No schema found for track!");
        //     return Vec::new();
        // }
        //
        // let mut sector_list = Vec::new();
        // if let Some(schema) = self.schema {
        //     for item in &self.metadata.items {
        //         if let TrackElement::System34(System34Element::Data {
        //             address_crc,
        //             data_crc,
        //             deleted,
        //             ..
        //         }) = item.elem_type
        //         {
        //             if let Some(chsn) = item.chsn {
        //                 sector_list.push(SectorMapEntry {
        //                     chsn,
        //                     attributes: SectorAttributes {
        //                         address_crc_valid: address_crc,
        //                         data_crc_valid: data_crc,
        //                         deleted_mark: deleted,
        //                         no_dam: false,
        //                     },
        //                 });
        //             }
        //         }
        //     }
        // }
        // sector_list
    }

    fn add_sector(&mut self, _sd: &AddSectorParams) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// [ReadSectorResult] struct which also sets some convenience metadata flags which are needed
    /// when handling `MetaSector` resolution images.
    /// When reading a `BitStream` resolution image, the sector data can optionally include any
    /// applicable metadata such as the address mark and CRC bytes, depending on the value of
    /// [RwScope].
    /// Offsets are provided within [ReadSectorResult] so these can be skipped when processing the
    /// read operation.
    fn read_sector(
        &self,
        id: SectorIdQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        let mut read_vec = Vec::new();
        let mut result_data_error = false;
        let mut result_address_error = false;
        let mut result_deleted_mark = false;
        let mut result_data_range = 0..0;
        let mut result_chsn = None;

        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;
        let mut data_crc = None;

        let schema = self.schema.ok_or(DiskImageError::SchemaError)?;

        // Read index first to avoid borrowing issues in next match.
        let bit_index = self.scan_sector_element(id, offset.unwrap_or(0))?;
        log::debug!("read_sector(): Bit index: {:?}", bit_index);

        match bit_index {
            TrackSectorScanResult::Found {
                address_error,
                no_dam,
                sector_chsn,
                ..
            } if no_dam => {
                // Sector id was matched, but has no associated data.
                // Return an empty buffer with the `no_dam` flag set.
                return Ok(ReadSectorResult {
                    id_chsn: Some(sector_chsn),
                    no_dam,
                    address_crc_error: address_error,
                    ..ReadSectorResult::default()
                });
            }
            TrackSectorScanResult::Found {
                ei,
                sector_chsn,
                address_error,
                data_error,
                deleted_mark,
                ..
            } => {
                result_chsn = Some(sector_chsn);
                // If there is a bad address mark, we do not read the sector data, unless the debug
                // flag is set.
                // This allows dumping of sectors with bad address marks for debugging purposes.
                // So if the debug flag is not set, return our 'failure' now.
                if address_error && !debug {
                    return Ok(ReadSectorResult {
                        id_chsn: result_chsn,
                        address_crc_error: true,
                        ..ReadSectorResult::default()
                    });
                }

                // TODO: All this should be moved into TrackSchema logic - we shouldn't have to know
                //       about the formatting details in Track

                // Should be safe to the instance
                let instance = self.element(ei).unwrap();
                // Get the size and range of the sector data element.
                let element_size = instance.element.size();
                let scope_range = instance.element.range(scope).unwrap_or(0..element_size);
                let scope_overhead = element_size - scope_range.len();

                // Normally we read the contents of the sector determined by N in the sector header.
                // The read operation however can override the value of N if the `n` parameter
                // is Some.
                let data_len = if let Some(n_value) = n {
                    DiskChsn::n_to_bytes(n_value) + scope_overhead
                }
                else {
                    sector_chsn.n_size() + scope_overhead
                };
                log::debug!(
                    "read_sector(): Allocating {} bytes for sector {} data element of size {} at offset: {:05X}",
                    data_len,
                    sector_chsn,
                    element_size,
                    instance.start
                );
                read_vec = vec![0u8; data_len];

                let (_, crc_opt) = schema.decode_element(&self.data, instance, scope, &mut read_vec);
                let crc = crc_opt.unwrap();

                // Sanity check: Read CRC matches metadata?
                if crc.is_error() != data_error {
                    log::warn!(
                        "read_sector(): CRC data/metadata mismatch for sector {}: calculated: {} metadata: {}",
                        sector_chsn,
                        crc,
                        if data_error { "Invalid" } else { "Valid" }
                    );
                }
                result_address_error = address_error;
                result_data_error = data_error;
                result_deleted_mark = deleted_mark;
                result_data_range = scope_range;
                // Move crc into Option for return
                data_crc = Some(crc);

                // if read_vec.len() < data_len {
                //     log::error!(
                //         "read_sector(): Data buffer underrun, expected: {} got: {}",
                //         data_len,
                //         read_vec.len()
                //     );
                //     return Err(DiskImageError::DataError);
                // }

                // self.data
                //     .seek(SeekFrom::Start((element_start + scope_read_off) as u64))
                //     .map_err(|_| DiskImageError::BitstreamError)?;
                // log::trace!("read_sector(): Reading {} bytes.", read_vec.len());
                // self.data
                //     .read_exact(&mut read_vec)
                //     .map_err(|_| DiskImageError::BitstreamError)?;
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
            id_chsn: result_chsn,
            read_buf: read_vec,
            data_range: result_data_range,
            deleted_mark: result_deleted_mark,
            not_found: false,
            no_dam: false,
            address_crc_error: result_address_error,
            address_crc: None,
            data_crc_error: result_data_error,
            data_crc,
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
        })
    }

    fn scan_sector(&self, id: DiskChsnQuery, offset: Option<usize>) -> Result<ScanSectorResult, DiskImageError> {
        // let data_crc_error = false;
        // let mut address_crc_error = false;
        // let deleted_mark = false;
        // let wrong_cylinder = false;
        // let bad_cylinder = false;
        // let wrong_head = false;

        // Read index first to avoid borrowing issues in next match.
        let track_scan_result = self.scan_sector_element(id, offset.unwrap_or(0))?;
        Ok(track_scan_result.into())

        // match bit_index {
        //     TrackSectorScanResult::Found {
        //         address_error, no_dam, ..
        //     } if no_dam => {
        //         // No DAM found. Return an empty buffer.
        //         Ok(ScanSectorResult {
        //             deleted_mark: false,
        //             not_found: false,
        //             no_dam: true,
        //             address_error,
        //             data_error: false,
        //             wrong_cylinder: false,
        //             bad_cylinder: false,
        //             wrong_head: false,
        //         })
        //     }
        //     TrackSectorScanResult::Found {
        //         address_crc_valid,
        //         data_crc_valid,
        //         deleted,
        //         ..
        //     } => {
        //         if !address_crc_valid {
        //             // Bad address CRC, return status.
        //             Ok(ScanSectorResult {
        //                 deleted_mark: false,
        //                 not_found: false,
        //                 no_dam: false,
        //                 address_error: true,
        //                 data_error: false,
        //                 wrong_cylinder,
        //                 bad_cylinder,
        //                 wrong_head,
        //             })
        //         }
        //         else {
        //             Ok(ScanSectorResult {
        //                 deleted_mark: deleted,
        //                 not_found: false,
        //                 no_dam: false,
        //                 address_error: address_crc_error,
        //                 data_error: !data_crc_valid,
        //                 wrong_cylinder,
        //                 bad_cylinder,
        //                 wrong_head,
        //             })
        //         }
        //     }
        //     TrackSectorScanResult::NotFound {
        //         wrong_cylinder: wc,
        //         bad_cylinder: bc,
        //         wrong_head: wh,
        //     } => {
        //         log::trace!(
        //             "scan_sector: Sector ID not matched reading track. wc: {} bc: {} wh: {}",
        //             wc,
        //             bc,
        //             wh
        //         );
        //         Ok(ScanSectorResult {
        //             not_found: true,
        //             no_dam: false,
        //             deleted_mark,
        //             address_error: address_crc_error,
        //             data_error: data_crc_error,
        //             wrong_cylinder: wc,
        //             bad_cylinder: bc,
        //             wrong_head: wc,
        //         })
        //     }
        //     _ => {
        //         unreachable!()
        //     }
        // }
    }

    fn write_sector(
        &mut self,
        id: DiskChsnQuery,
        offset: Option<usize>,
        write_data: &[u8],
        _scope: RwScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let data_len;
        let mut wrong_cylinder = false;
        let bad_cylinder = false;
        let mut wrong_head = false;

        // Find the bit offset of the requested sector
        let bit_index = self.scan_sector_element(id, offset.unwrap_or(0))?;

        match bit_index {
            TrackSectorScanResult::Found {
                address_error, no_dam, ..
            } if no_dam => {
                // No DAM found. Return an empty buffer.
                Ok(WriteSectorResult {
                    not_found: false,
                    no_dam: true,
                    address_crc_error: address_error,
                    wrong_cylinder,
                    bad_cylinder,
                    wrong_head,
                })
            }
            TrackSectorScanResult::Found {
                sector_chsn,
                address_error,
                deleted_mark,
                ..
            } => {
                wrong_cylinder = id.c().is_some() && sector_chsn.c() != id.c().unwrap();
                wrong_head = id.h().is_some() && sector_chsn.h() != id.h().unwrap();

                // If there's a bad address mark, we do not proceed to write the data, unless we're
                // requesting it anyway for debugging purposes.
                if address_error && !debug {
                    return Ok(WriteSectorResult {
                        not_found: false,
                        no_dam: false,
                        address_crc_error: address_error,
                        wrong_cylinder,
                        bad_cylinder,
                        wrong_head,
                    });
                }

                if write_deleted != deleted_mark {
                    log::warn!(
                        "write_sector(): Deleted mark mismatch, expected: {} got: {}. Changing sector data type not implemented",
                        write_deleted,
                        deleted_mark
                    );
                    return Err(DiskImageError::ParameterError);
                }

                data_len = write_data.len();
                if sector_chsn.n_size() != data_len {
                    log::error!(
                        "write_sector(): Data buffer size mismatch, expected: {} got: {}",
                        sector_chsn.n_size(),
                        write_data.len()
                    );
                    return Err(DiskImageError::ParameterError);
                }

                /*                self.data
                    .seek(SeekFrom::Start(((ei.start >> 1) + 32) as u64))
                    .map_err(|_| DiskImageError::SeekError)?;

                log::trace!(
                    "write_sector(): Writing {} bytes to sector_id: {} at offset: {}",
                    write_data.len(),
                    id.s(),
                    ei.start + 4 * MFM_BYTE_LEN
                );

                // Write the sector data, if the write scope is the entire sector.
                if !matches!(scope, RwScope::CrcOnly) {
                    self.data
                        .write_encoded_buf(&write_data[0..data_len], ei.start + 4 * MFM_BYTE_LEN);
                }

                // Calculate the CRC of the data address mark + data.
                let mut crc = crc_ibm_3740(&mark_bytes, None);
                crc = crc_ibm_3740(&write_data[0..data_len], Some(crc));

                // Write the CRC after the data.
                self.data
                    .write_encoded_buf(&crc.to_be_bytes(), ei.start + (4 + data_len) * MFM_BYTE_LEN);

                self.add_write(data_len);*/

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
                    id,
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

    fn recalculate_sector_crc(&mut self, id: DiskChsnQuery, offset: Option<usize>) -> Result<(), DiskImageError> {
        // First, read the sector data.
        let rr = self.read_sector(id, None, offset, RwScope::DataOnly, false)?;

        // Write the data back to the sector, which will recalculate the CRC.
        // TODO: We may wish to optimize this in the future to just write the new CRC, but I don't expect
        //       this function to be called heavily.
        self.write_sector(
            id,
            offset,
            &rr.read_buf[rr.data_range],
            RwScope::CrcOnly,
            rr.deleted_mark,
            false,
        )?;

        Ok(())
    }

    fn hash(&mut self) -> Digest {
        let mut hasher = sha1_smol::Sha1::new();
        hasher.update(&self.data.data_copied());
        hasher.digest()
    }

    /// Read all sectors from the track. The data is returned within a [ReadTrackResult] struct
    /// which also sets some convenience metadata flags which are needed when handling `MetaSector`
    /// resolution images.
    /// The data returned is only the actual sector data. The address marks and CRCs are not included
    /// in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, _ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        let mut track_read_vec = Vec::with_capacity(512 * 9);
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut sector_read_vec = vec![0u8; sector_data_len];

        let mut result_data_error = false;
        let mut result_address_error = false;
        let mut result_deleted_mark = false;
        let mut result_not_found = true;
        let mut sectors_read: u16 = 0;

        // Read index first to avoid borrowing issues in next match.
        let mut bit_index = self.next_sector(0);

        while let TrackSectorScanResult::Found {
            ei,
            sector_chsn,
            address_error,
            data_error,
            deleted_mark,
            no_dam: _no_dam,
            ..
        } = bit_index
        {
            // We've found at least one sector.
            result_not_found = false;

            // Note any data and address integrity errors, however keep reading.
            result_address_error |= address_error;
            result_data_error |= data_error;
            result_deleted_mark |= deleted_mark;

            // Resolve the element instance offsets
            let TrackElementInstance { start, end, .. } = *self.element(ei).unwrap();

            // In a normal Read Sector operation, we'd check the value of N in the sector header.
            // When reading all sectors in a track, we specify the value of N for all sectors in
            // the entire track. The value of N in the sector header is ignored. This allows us
            // to read data outside a sector in the case of an 'N' mismatch.
            log::trace!(
                "read_all_sectors_bitstream(): Found sector_id: {} at offset: {} read length: {}",
                sector_chsn.s(),
                start,
                sector_read_vec.len()
            );

            self.read_exact_at(start + 64, &mut sector_read_vec)
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

            bit_index = self.next_sector(end);
        }

        let read_len = track_read_vec.len();
        Ok(ReadTrackResult {
            not_found: result_not_found,
            sectors_read,
            read_buf: track_read_vec,
            deleted_mark: result_deleted_mark,
            address_crc_error: result_address_error,
            data_crc_error: result_data_error,
            read_len_bits: read_len * 16,
            read_len_bytes: read_len,
        })
    }

    fn next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
        if self.metadata.sector_ids.is_empty() {
            log::warn!("get_next_id(): No sector_id vector for track!");
        }
        let first_sector = *self.metadata.sector_ids.first()?;
        let mut sector_matched = false;
        for sid in &self.metadata.sector_ids {
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

    fn read(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
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

    fn read_raw(&mut self, _overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        //let extra_bytes = overdump.unwrap_or(0);

        let data_size = self.data.len() / 8 + if self.data.len() % 8 > 0 { 1 } else { 0 };
        //let dump_size = data_size + extra_bytes;

        let track_read_vec = self.data.data_copied();

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
            System34Schema::format_track_as_bytes(standard, bitcell_ct, format_buffer, fill_pattern, gap3)?;

        let new_bit_vec = self
            .data
            .encode(&format_result.track_bytes, false, EncodingVariant::Data);
        log::trace!(
            "New bitstream size: {} from {} bytes",
            new_bit_vec.len(),
            format_result.track_bytes.len()
        );
        self.data.replace(new_bit_vec);

        System34Schema::set_track_markers(&mut self.data, format_result.markers)?;

        // Scan the new track data for markers and create a clock map.
        let markers = System34Schema::scan_markers(&self.data);
        if markers.is_empty() {
            log::error!("TrackData::format(): No markers found in track data post-format.");
        }
        else {
            log::trace!("TrackData::format(): Found {} markers in track data.", markers.len());
        }
        System34Schema::create_clock_map(&markers, self.data.clock_map_mut());

        let new_metadata = TrackMetadata::new(
            System34Schema::scan_metadata(&mut self.data, markers),
            TrackSchema::System34,
        );

        let data_ranges = new_metadata.data_ranges();
        if !data_ranges.is_empty() {
            self.data.set_data_ranges(data_ranges);
        }

        self.metadata = new_metadata;
        Ok(())
    }

    fn analysis(&self) -> Result<TrackAnalysis, DiskImageError> {
        let schema = self.schema.ok_or(DiskImageError::SchemaError)?;
        Ok(schema.analyze_elements(&self.metadata))
    }

    fn stream(&self) -> Option<&TrackDataStream> {
        Some(&self.data)
    }

    fn stream_mut(&mut self) -> Option<&mut TrackDataStream> {
        Some(&mut self.data)
    }

    fn element_map(&self) -> Option<&SourceMap> {
        Some(&self.metadata.element_map)
    }
}

impl BitStreamTrack {
    pub(crate) fn new(
        params: &BitStreamTrackParams,
        shared: Arc<Mutex<SharedDiskContext>>,
    ) -> Result<BitStreamTrack, DiskImageError> {
        Self::new_optional_ctx(params, Some(shared))
    }

    pub(crate) fn new_optional_ctx(
        params: &BitStreamTrackParams,
        shared: Option<Arc<Mutex<SharedDiskContext>>>,
    ) -> Result<BitStreamTrack, DiskImageError> {
        // if params.data.is_empty() {
        //     log::error!("add_track_bitstream(): Data is empty.");
        //     return Err(DiskImageError::ParameterError);
        // }
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

        // The data vec is optional if we have a bitcell count and MFM/FM encoding.
        let data = if params.data.is_empty() {
            if let Some(bitcell_ct) = params.bitcell_ct {
                #[allow(unreachable_patterns)]
                match params.encoding {
                    TrackDataEncoding::Mfm | TrackDataEncoding::Fm => BitVec::from_fn(bitcell_ct, |i| i % 2 == 0),
                    TrackDataEncoding::Gcr => BitVec::from_elem(bitcell_ct, false),
                    _ => {
                        log::error!(
                            "add_track_bitstream(): Unsupported data encoding: {:?}",
                            params.encoding
                        );
                        return Err(DiskImageError::UnsupportedFormat);
                    }
                }
            }
            else {
                log::error!("add_track_bitstream(): Data or Bitcell count not provided.");
                return Err(DiskImageError::ParameterError);
            }
        }
        else {
            BitVec::from_bytes(params.data)
        };
        let weak_bitvec_opt = params.weak.map(BitVec::from_bytes);
        let default_schema = params.schema.unwrap_or_default();
        let mut track_schema = None;
        let mut track_metadata = TrackMetadata::default();

        // TODO: Let the schema handle encoding. We should not need to know the encoding here.
        #[allow(unreachable_patterns)]
        let mut data_stream: Box<dyn TrackCodec> = match params.encoding {
            TrackDataEncoding::Gcr => {
                let mut codec;
                if weak_bitvec_opt.is_some() {
                    codec = GcrCodec::new(data, params.bitcell_ct, weak_bitvec_opt);
                }
                else {
                    codec = GcrCodec::new(data, params.bitcell_ct, None);
                    if params.detect_weak {
                        log::debug!("add_track_bitstream(): detecting weak bits in GCR stream...");
                        let weak_bitvec = codec.create_weak_bit_mask(GcrCodec::WEAK_BIT_RUN);
                        if weak_bitvec.any() {
                            log::debug!(
                                "add_track_bitstream(): Detected {} weak bits in GCR bitstream.",
                                weak_bitvec.count_ones()
                            );
                        }
                        _ = codec.set_weak_mask(weak_bitvec);
                    }
                }
                Box::new(codec)
            }
            TrackDataEncoding::Mfm => {
                let mut codec;
                // If a weak bit mask was provided by the file format, we will honor it.
                // Otherwise, if 'detect_weak' is set we will try to detect weak bits from the MFM stream.
                if weak_bitvec_opt.is_some() {
                    codec = MfmCodec::new(data, params.bitcell_ct, weak_bitvec_opt);
                }
                else {
                    codec = MfmCodec::new(data, params.bitcell_ct, None);
                    if params.detect_weak {
                        log::debug!("add_track_bitstream(): detecting weak bits in MFM stream...");
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
                Box::new(codec)
            }
            TrackDataEncoding::Fm => {
                let mut codec;
                // If a weak bit mask was provided by the file format, we will honor it.
                // Otherwise, we will try to detect weak bits from the FM stream.
                if weak_bitvec_opt.is_some() {
                    codec = FmCodec::new(data, params.bitcell_ct, weak_bitvec_opt);
                }
                else {
                    codec = FmCodec::new(data, params.bitcell_ct, None);
                    if params.detect_weak {
                        log::debug!("add_track_bitstream(): detecting weak bits in FM stream...");
                        let weak_bitvec = codec.create_weak_bit_mask(FmCodec::WEAK_BIT_RUN);
                        if weak_bitvec.any() {
                            log::trace!(
                                "add_track_bitstream(): Detected {} weak bits in FM bitstream.",
                                weak_bitvec.count_ones()
                            );
                        }
                        _ = codec.set_weak_mask(weak_bitvec);
                    }
                }

                Box::new(codec)
            }
            _ => {
                log::error!(
                    "add_track_bitstream(): Unsupported data encoding: {:?}",
                    params.encoding
                );
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        // Now that we have a track, we can try different track schemas to see if we can decode
        // the track markers.

        // Create a list of schemas to try, filtering out the default schema.
        let mut schema_list: Vec<TrackSchema> = vec![default_schema];
        schema_list.extend(TrackSchema::iter().filter(|s| *s != default_schema));

        assert_ne!(schema_list.len(), 0);

        let mut track_markers = Vec::new();
        for schema in schema_list {
            log::trace!("Trying track schema: {:?}", schema);

            track_markers = schema.scan_for_markers(&data_stream);
            if !track_markers.is_empty() {
                log::trace!(
                    "Schema {:?} found {} markers, first marker at {}",
                    schema,
                    track_markers.len(),
                    track_markers[0].start
                );

                schema.create_clock_map(&track_markers, data_stream.clock_map_mut());
                // Set the schema and break out of the loop.
                track_schema = Some(schema);
                break;
            }
            else {
                log::trace!("Schema {:?} failed to detect track markers.", schema);
            }
        }

        if params.bitcell_ct.is_none() {
            log::debug!("Bitcell count not provided, attempting to detect track padding.");
            data_stream.set_track_padding();
        }

        if track_schema.is_none() {
            log::warn!("No compatible track schema was detected. Track will be treated as unformatted.");
        }

        if let Some(schema) = track_schema {
            track_metadata = TrackMetadata::new(schema.scan_for_elements(&mut data_stream, track_markers), schema);
        }

        let sector_ids = track_metadata.sector_ids();
        if sector_ids.is_empty() {
            log::warn!(
                "add_track_bitstream(): No sector ids found in track {} metadata.",
                params.ch.c()
            );
        }
        let data_ranges = track_metadata.data_ranges();
        if !data_ranges.is_empty() {
            log::debug!(
                "add_track_bitstream(): Adding {} data ranges to track stream",
                data_ranges.len(),
            );
            data_stream.set_data_ranges(data_ranges);
        }

        // let sector_offsets = track_metadata
        //     .items
        //     .iter()
        //     .filter_map(|i| {
        //         if let TrackElement::System34(System34Element::Data { .. }) = i.elem_type {
        //             //log::trace!("Got Data element, returning start address: {}", i.start);
        //             Some(i.start)
        //         }
        //         else {
        //             None
        //         }
        //     })
        //     .collect::<Vec<_>>();
        //
        // log::debug!(
        //     "add_track_bitstream(): Retrieved {} sector bitstream offsets from {} metadata entries.",
        //     sector_offsets.len(),
        //     track_metadata.items.len()
        // );

        Ok(BitStreamTrack {
            encoding: params.encoding,
            data_rate: params.data_rate,
            rpm: None,
            ch: params.ch,
            data: data_stream,
            metadata: track_metadata,
            schema: track_schema,
            shared,
        })
    }

    pub fn set_schema(&mut self, schema: TrackSchema) {
        self.schema = Some(schema);
    }

    /// Rescan the track for markers and metadata. This can be called if we have manually
    /// written the track and need to update the metadata.
    pub fn rescan(&mut self, schema_hint: Option<TrackSchema>) -> Result<(), DiskImageError> {
        let schemas = self.schema_list(schema_hint);

        for schema in schemas {
            // Just because we find markers doesn't mean that we have found the right schema.
            // The Amiga track schema will pick up PC sector markers, for example. So we need to
            // check how many valid sector headers we have - if all sector headers on a track
            // have a bad crc then we should try the next schema.
            let track_markers = schema.scan_for_markers(&self.data);
            if !track_markers.is_empty() {
                log::trace!(
                    "Schema {:?} found {} markers, first marker at {}",
                    schema,
                    track_markers.len(),
                    track_markers[0].start
                );

                schema.create_clock_map(&track_markers, self.data.clock_map_mut());
            }
            else {
                log::warn!("Schema {:?} failed to detect track markers.", schema);
            }

            self.metadata = TrackMetadata::new(schema.scan_for_elements(&mut self.data, track_markers), schema);
            let sector_ids = self.metadata.valid_sector_ids();
            if sector_ids.is_empty() {
                log::debug!(
                    "rescan(): No valid sector ids found in track {} metadata. Trying next schema...",
                    self.ch.c()
                );
                // Clear metadata and try the next schema.
                self.metadata.clear();
                continue;
            }
            let data_ranges = self.metadata.data_ranges();
            if !data_ranges.is_empty() {
                log::debug!("rescan(): Adding {} data ranges to track stream", data_ranges.len(),);
                self.data.set_data_ranges(data_ranges);
            }
            break;
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn elements(&self) -> &[TrackElementInstance] {
        &self.metadata.items
    }

    #[allow(dead_code)]
    fn elements_mut(&mut self) -> &mut [TrackElementInstance] {
        &mut self.metadata.items
    }

    #[allow(dead_code)]
    fn element(&self, idx: usize) -> Option<&TrackElementInstance> {
        self.metadata.items.get(idx)
    }

    #[allow(dead_code)]
    fn element_mut(&mut self, idx: usize) -> Option<&mut TrackElementInstance> {
        self.metadata.items.get_mut(idx)
    }

    pub(crate) fn add_write(&mut self, _bytes: usize) {
        if let Some(shared) = &self.shared {
            let mut write_count = shared.lock().unwrap().writes;
            write_count += 1;
            shared.lock().unwrap().writes = write_count;
        }
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

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Scan for the next sector on the track starting from the specified bit index.
    ///
    /// This function searches the metadata for the first IDAM (Index Address Mark) starting from
    /// the specified bit index and returns the bit index of the corresponding sector data.
    ///
    /// # Arguments
    /// * `index` - The bit index of the track to start searching from.
    ///
    /// # Returns
    /// * `TrackSectorScanResult::Found` if a sector is found.
    /// * `TrackSectorScanResult::NotFound` if no sector is found. The fields returned are meaningless.
    pub(crate) fn next_sector(&self, index: usize) -> TrackSectorScanResult {
        let mut idam_chsn: Option<DiskChsn> = None;
        for (ei, instance) in self.metadata.items.iter().enumerate() {
            // Skip items until ww reach the specified bit index.
            if instance.start < index {
                continue;
            }

            match instance {
                TrackElementInstance {
                    element: TrackElement::System34(System34Element::Marker(System34Marker::Idam, _)),
                    chsn,
                    ..
                } => {
                    // Match the first IDAM seen as we are returning the first sector.
                    idam_chsn = *chsn;
                }
                TrackElementInstance {
                    element:
                        TrackElement::System34(System34Element::SectorData {
                            address_error,
                            data_error,
                            deleted,
                            ..
                        }),
                    ..
                } => {
                    if let Some(sector_chsn) = idam_chsn {
                        return TrackSectorScanResult::Found {
                            ei,
                            sector_chsn,
                            address_error: *address_error,
                            data_error: *data_error,
                            deleted_mark: *deleted,
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

    /// Scan for the sector requested by the `id` parameter in the track data stream, starting
    /// at `index`.
    ///
    /// This function searches the metadata for the first matching sector element and returns
    /// the bit index of the corresponding sector data.
    ///
    /// # Arguments
    /// - `id` - A `SectorIdQuery` indicating the sector parameters to match when searching.
    /// - `index` - The bit index of the track to start searching from. If `None`, the search
    ///             will start from the beginning of the track.
    ///
    /// # Returns
    /// A [TrackSectorScanResult] containing the scan status, start index, sector id, address and
    /// data integrity result, and deleted status.
    pub(crate) fn scan_sector_element(
        &self,
        id: SectorIdQuery,
        index: usize,
    ) -> Result<TrackSectorScanResult, DiskImageError> {
        if let Some(schema) = self.schema {
            Ok(schema.match_sector_element(id, &self.metadata.items, index, None))
        }
        else {
            log::error!("scan_sector_element(): No track schema found for track.");
            Err(DiskImageError::UnsupportedFormat)
        }
    }

    pub fn set_weak_mask(&mut self, weak_mask: BitVec, offset: usize) {
        let mut new_mask = self.data.weak_mask().clone();
        for (i, bit) in weak_mask.iter().enumerate() {
            if i + offset >= new_mask.len() {
                break;
            }
            new_mask.set(i + offset, bit);
        }

        self.data.set_weak_mask(new_mask);
    }

    pub fn write_weak_mask_u32(&mut self, weak_mask: u32, offset: usize) {
        let mask_bits = self.data.weak_mask_mut();

        for i in 0..32 {
            if weak_mask & (0x8000_0000 >> i) != 0 {
                if i + offset >= mask_bits.len() {
                    break;
                }
                mask_bits.set(i + offset, true);
            }
        }
    }

    pub fn calc_quality_score(&self) -> i32 {
        let mut score = 0;
        for s in self.sector_list() {
            // Weight having a sector heavily, so that missing sectors are heavily penalized.
            score += 5;
            if s.attributes.address_error {
                // Bad address CRC is unusual, most likely track error.
                score -= 5;
            }
            if s.attributes.data_error {
                // Bad data CRC is more common. Weight it less relative to other issues.
                score -= 1;
            }
        }
        score
    }

    /// Return a list of schemas to try decoding, starting at the provided hint, the currently
    /// set track schema, or the default if neither are set.
    /// The list should encompass all schemas enabled by the current feature flags.
    fn schema_list(&self, hint: Option<TrackSchema>) -> Vec<TrackSchema> {
        let default_schema = hint.unwrap_or(self.schema.unwrap_or_default());
        let mut schema_list: Vec<TrackSchema> = vec![default_schema];
        schema_list.extend(TrackSchema::iter().filter(|s| *s != default_schema));
        schema_list
    }
}
