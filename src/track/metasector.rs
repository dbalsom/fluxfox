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
use super::{Track, TrackAnalysis, TrackInfo};

use crate::types::{
    AddSectorParams,
    ReadSectorResult,
    ReadTrackResult,
    RwScope,
    ScanSectorResult,
    SectorAttributes,
    SharedDiskContext,
    WriteSectorResult,
};

use crate::track_schema::{system34::System34Standard, TrackMetadata, TrackSchema};

use crate::{
    bitstream_codec::TrackDataStream,
    types::{chs::DiskChsnQuery, DiskCh, DiskChs, DiskChsn, TrackDataEncoding, TrackDataRate, TrackDataResolution},
    DiskImageError,
    FoxHashSet,
    SectorMapEntry,
};
use sha1_smol::Digest;
use std::{
    any::Any,
    sync::{Arc, Mutex},
};

struct SectorMatch<'a> {
    pub(crate) sectors: Vec<&'a MetaSector>,
    pub(crate) sizes: Vec<u8>,
    pub(crate) wrong_cylinder: bool,
    pub(crate) bad_cylinder: bool,
    pub(crate) wrong_head: bool,
}

impl SectorMatch<'_> {
    fn len(&'_ self) -> usize {
        self.sectors.len()
    }
    #[allow(dead_code)]
    fn iter(&'_ self) -> std::slice::Iter<&MetaSector> {
        self.sectors.iter()
    }
}

struct SectorMatchMut<'a> {
    pub(crate) sectors: Vec<&'a mut MetaSector>,
    #[allow(dead_code)]
    pub(crate) sizes: Vec<u8>,
    pub(crate) wrong_cylinder: bool,
    pub(crate) bad_cylinder: bool,
    pub(crate) wrong_head: bool,
    pub(crate) shared: Arc<Mutex<SharedDiskContext>>,
}

impl<'a> SectorMatchMut<'a> {
    fn len(&'a self) -> usize {
        self.sectors.len()
    }
    #[allow(dead_code)]
    fn iter_mut(&'a mut self) -> std::slice::IterMut<'a, &'a mut MetaSector> {
        self.sectors.iter_mut()
    }
}

#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct MetaMask {
    has_bits: bool,
    mask: Vec<u8>,
}

impl MetaMask {
    fn empty(len: usize) -> MetaMask {
        MetaMask {
            has_bits: false,
            mask: vec![0; len],
        }
    }
    fn from(mask: &[u8]) -> MetaMask {
        let mut m = MetaMask::default();
        m.set_mask(mask);
        m
    }
    fn set_mask(&mut self, mask: &[u8]) {
        self.mask = mask.to_vec();
        self.has_bits = mask.iter().any(|&x| x != 0);
    }
    #[allow(dead_code)]
    fn or_mask(&mut self, source_mask: &MetaMask) {
        for (i, &m) in source_mask.iter().enumerate() {
            self.mask[i] |= m;
        }
        self.has_bits = self.mask.iter().any(|&x| x != 0);
    }
    fn or_slice(&mut self, source_mask: &[u8]) {
        for (i, &m) in source_mask.iter().enumerate() {
            self.mask[i] |= m;
        }
        self.has_bits = self.mask.iter().any(|&x| x != 0);
    }
    #[allow(dead_code)]
    fn clear(&mut self) {
        self.mask.fill(0);
        self.has_bits = false;
    }
    #[allow(dead_code)]
    fn mask(&self) -> &[u8] {
        &self.mask
    }
    fn has_bits(&self) -> bool {
        self.has_bits
    }
    fn iter(&self) -> std::slice::Iter<u8> {
        self.mask.iter()
    }
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.mask.len()
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct MetaSector {
    id_chsn: DiskChsn,
    address_error: bool,
    data_error: bool,
    deleted_mark: bool,
    no_dam: bool,
    data: Vec<u8>,
    weak_mask: MetaMask,
    hole_mask: MetaMask,
}

impl MetaSector {
    pub fn read_data(&self) -> Vec<u8> {
        if self.no_dam {
            return Vec::new();
        }
        let mut data = self.data.clone();
        for (i, (weak_byte, hole_byte)) in self.weak_mask.iter().zip(self.hole_mask.iter()).enumerate() {
            let mask_byte = weak_byte | hole_byte;
            if mask_byte == 0 {
                continue;
            }
            let rand_byte = rand::random::<u8>();
            data[i] = data[i] & !mask_byte | rand_byte & mask_byte;
        }
        data
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MetaSectorTrack {
    pub(crate) ch: DiskCh,
    pub(crate) encoding: TrackDataEncoding,
    pub(crate) schema: Option<TrackSchema>,
    pub(crate) data_rate: TrackDataRate,
    pub(crate) sectors: Vec<MetaSector>,

    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) shared: Arc<Mutex<SharedDiskContext>>,
}

#[cfg_attr(feature = "serde", typetag::serde)]
impl Track for MetaSectorTrack {
    fn resolution(&self) -> TrackDataResolution {
        TrackDataResolution::MetaSector
    }
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_metasector_track(&self) -> Option<&MetaSectorTrack> {
        self.as_any().downcast_ref::<MetaSectorTrack>()
    }

    fn as_metasector_track_mut(&mut self) -> Option<&mut MetaSectorTrack> {
        self.as_any_mut().downcast_mut::<MetaSectorTrack>()
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
            density: None,
            rpm: None,
            bit_length: 0,
            sector_ct: self.sectors.len(),
        }
    }

    fn metadata(&self) -> Option<&TrackMetadata> {
        None
    }

    fn sector_ct(&self) -> usize {
        self.sectors.len()
    }

    fn has_sector_id(&self, sid: u8, id_chsn: Option<DiskChsn>) -> bool {
        self.sectors.iter().any(|sector| {
            if id_chsn.is_none() && sector.id_chsn.s() == sid {
                return true;
            }
            else if let Some(chsn) = id_chsn {
                if sector.id_chsn == chsn {
                    return true;
                }
            }
            false
        })
    }

    fn sector_list(&self) -> Vec<SectorMapEntry> {
        self.sectors
            .iter()
            .map(|s| SectorMapEntry {
                chsn: s.id_chsn,
                attributes: SectorAttributes {
                    address_error: s.address_error,
                    data_error: s.data_error,
                    deleted_mark: s.deleted_mark,
                    no_dam: false,
                },
            })
            .collect()
    }

    fn add_sector(&mut self, params: &AddSectorParams) -> Result<(), DiskImageError> {
        // Create an empty weak bit mask if none is provided.
        let weak_mask = match &params.weak_mask {
            Some(weak_buf) => MetaMask::from(weak_buf),
            None => MetaMask::empty(params.data.len()),
        };

        let hole_mask = match &params.hole_mask {
            Some(hole_buf) => MetaMask::from(hole_buf),
            None => MetaMask::empty(params.data.len()),
        };

        let new_sector = MetaSector {
            id_chsn: params.id_chsn,
            address_error: params.attributes.address_error,
            data_error: params.attributes.data_error,
            deleted_mark: params.attributes.deleted_mark,
            no_dam: params.attributes.no_dam,
            data: params.data.to_vec(),
            weak_mask,
            hole_mask,
        };

        if params.alternate {
            // Look for existing sector.
            let existing_sector = self.sectors.iter_mut().find(|s| s.id_chsn == params.id_chsn);

            if let Some(es) = existing_sector {
                // Update the existing sector.
                let mut xor_vec: Vec<u8> = Vec::with_capacity(es.data.len());

                // Calculate a bitmap representing the difference between the new sector data and the
                // existing sector data.
                for (i, (ns_byte, es_byte)) in new_sector.data.iter().zip(es.data.iter()).enumerate() {
                    xor_vec[i] = ns_byte ^ es_byte;
                }

                // Update the weak bit mask for the existing sector and return.
                es.weak_mask.or_slice(&xor_vec);
                return Ok(());
            }
        }

        self.sectors.push(new_sector);

        Ok(())
    }

    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags where are needed
    /// when handling MetaSector images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    fn read_sector(
        &self,
        id: DiskChsnQuery,
        _n: Option<u8>,
        _offset: Option<usize>,
        scope: RwScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        match scope {
            // Add 4 bytes for address mark and 2 bytes for CRC.
            RwScope::EntireElement => unimplemented!("DataElement scope not supported for MetaSector"),
            RwScope::DataOnly => {}
            _ => return Err(DiskImageError::ParameterError),
        };

        let sm = self.match_sectors(id, debug);

        if sm.len() == 0 {
            log::debug!("read_sector(): No sector found for id: {}", id);
            Ok(ReadSectorResult {
                not_found: true,
                wrong_cylinder: sm.wrong_cylinder,
                bad_cylinder: sm.bad_cylinder,
                wrong_head: sm.wrong_head,
                ..ReadSectorResult::default()
            })
        }
        else {
            if sm.len() > 1 {
                log::warn!(
                    "read_sector(): Found {} sector ids matching id query: {} (with {} different sizes). Using first.",
                    sm.len(),
                    id,
                    sm.sizes.len()
                );
            }
            let s = sm.sectors[0];

            // TODO: MetaSector doesn't have stored CRC, but we can calculate the read CRC
            Ok(ReadSectorResult {
                id_chsn: Some(s.id_chsn),
                data_range: 0..s.data.len(),
                read_buf: s.read_data(), // Calling read_data applies the weak bit and hole masks.
                deleted_mark: s.deleted_mark,
                not_found: false,
                no_dam: false,
                address_crc_error: s.address_error,
                data_crc_error: s.data_error,
                wrong_cylinder: sm.wrong_cylinder,
                bad_cylinder: sm.bad_cylinder,
                wrong_head: sm.wrong_head,
                ..ReadSectorResult::default()
            })
        }
    }

    fn scan_sector(&self, id: DiskChsnQuery, _offset: Option<usize>) -> Result<ScanSectorResult, DiskImageError> {
        let sm = self.match_sectors(id, false);

        if sm.len() == 0 {
            log::debug!("scan_sector(): No sector found for id query: {}", id);
            Ok(ScanSectorResult {
                not_found: true,
                no_dam: false,
                deleted_mark: false,
                address_error: false,
                data_error: false,
                wrong_cylinder: sm.wrong_cylinder,
                bad_cylinder: sm.bad_cylinder,
                wrong_head: sm.wrong_head,
            })
        }
        else {
            log::warn!(
                "scan_sector(): Found {} sector ids matching query: {} (with {} different sizes). Using first.",
                sm.len(),
                id,
                sm.sizes.len()
            );
            let s = sm.sectors[0];

            Ok(ScanSectorResult {
                deleted_mark: s.deleted_mark,
                not_found: false,
                no_dam: false,
                address_error: s.address_error,
                data_error: s.data_error,
                wrong_cylinder: sm.wrong_cylinder,
                bad_cylinder: sm.bad_cylinder,
                wrong_head: sm.wrong_head,
            })
        }
    }

    fn write_sector(
        &mut self,
        id: DiskChsnQuery,
        _offset: Option<usize>,
        write_data: &[u8],
        _scope: RwScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let mut sm = self.match_sectors_mut(id, debug);

        if sm.len() > 1 {
            log::error!(
                "write_sector(): Could not identify unique target sector. (Found {} sector ids matching query: {})",
                sm.len(),
                id,
            );
            return Err(DiskImageError::UniqueIdError);
        }
        else if sm.len() == 0 {
            log::debug!("write_sector(): No sector found for id query: {}", id);
            return Ok(WriteSectorResult {
                not_found: false,
                no_dam: false,
                address_crc_error: false,
                wrong_cylinder: sm.wrong_cylinder,
                bad_cylinder: sm.bad_cylinder,
                wrong_head: sm.wrong_head,
            });
        }

        let write_data_len = write_data.len();
        if DiskChsn::n_to_bytes(sm.sectors[0].id_chsn.n()) != write_data_len {
            // Caller didn't provide correct buffer size.
            log::error!(
                "write_sector(): Data buffer size mismatch, expected: {} got: {}",
                DiskChsn::n_to_bytes(sm.sectors[0].id_chsn.n()),
                write_data_len
            );
            return Err(DiskImageError::ParameterError);
        }

        if sm.sectors[0].no_dam || sm.sectors[0].address_error {
            log::debug!(
                "write_sector(): Sector {} is unwritable due to no DAM or bad address CRC.",
                sm.sectors[0].id_chsn
            );
        }
        else {
            sm.sectors[0].data.copy_from_slice(write_data);
            sm.sectors[0].deleted_mark = write_deleted;
        }

        sm.shared.lock().unwrap().writes += 1;

        Ok(WriteSectorResult {
            not_found: false,
            no_dam: sm.sectors[0].no_dam,
            address_crc_error: sm.sectors[0].address_error,
            wrong_cylinder: sm.wrong_cylinder,
            bad_cylinder: sm.bad_cylinder,
            wrong_head: sm.wrong_head,
        })
    }

    fn recalculate_sector_crc(&mut self, id: DiskChsnQuery, offset: Option<usize>) -> Result<(), DiskImageError> {
        // First, read the sector data.
        let rr = self.read_sector(id, None, offset, RwScope::DataOnly, false)?;

        // Write the data back to the sector, which will recalculate the CRC.
        self.write_sector(id, offset, &rr.read_buf, RwScope::DataOnly, rr.deleted_mark, false)?;

        Ok(())
    }

    fn hash(&mut self) -> Digest {
        let mut hasher = sha1_smol::Sha1::new();
        let rtr = self.read_all_sectors(self.ch, 0xFF, 0xFF).unwrap();
        hasher.update(&rtr.read_buf);
        hasher.digest()
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling MetaSector images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, _ch: DiskCh, n: u8, track_len: u8) -> Result<ReadTrackResult, DiskImageError> {
        let track_len = track_len as u16;
        let sector_data_len = DiskChsn::n_to_bytes(n);
        let mut track_read_vec = Vec::with_capacity(sector_data_len * self.sectors.len());
        let mut address_crc_error = false;
        let mut data_crc_error = false;
        let mut deleted_mark = false;

        let mut not_found = true;
        let mut sectors_read = 0;

        for s in &self.sectors {
            log::trace!("read_all_sectors(): Found sector_id: {}", s.id_chsn,);
            not_found = false;

            // TODO - do we stop after reading sector ID specified by EOT, or
            //        or upon reaching it?
            if sectors_read >= track_len {
                log::trace!(
                    "read_all_sectors(): Reached track_len at sector: {} \
                        sectors_read: {}, track_len: {}",
                    s.id_chsn,
                    sectors_read,
                    track_len
                );
                break;
            }

            track_read_vec.extend(&s.read_data());
            sectors_read = sectors_read.saturating_add(1);

            if s.address_error {
                address_crc_error |= true;
            }

            if s.data_error {
                data_crc_error |= true;
            }

            if s.deleted_mark {
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

    fn next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
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
        }
        else {
            None
        }
    }

    fn read(&mut self, _overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn read_raw(&mut self, _overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn has_weak_bits(&self) -> bool {
        self.sectors.iter().any(|s| s.weak_mask.has_bits())
    }

    fn format(
        &mut self,
        _standard: System34Standard,
        _format_buffer: Vec<DiskChsn>,
        _fill_pattern: &[u8],
        _gap3: usize,
    ) -> Result<(), DiskImageError> {
        // TODO: Implement format for MetaSectorTrack
        Err(DiskImageError::UnsupportedFormat)
    }

    fn analysis(&self) -> Result<TrackAnalysis, DiskImageError> {
        let sector_ct = self.sectors.len();
        let mut analysis = TrackAnalysis::default();

        let mut n_set: FoxHashSet<u8> = FoxHashSet::new();
        let mut last_n = 0;
        for (si, sector) in self.sectors.iter().enumerate() {
            if sector.id_chsn.s() != si as u8 + 1 {
                analysis.nonconsecutive_sectors = true;
            }
            if sector.data_error {
                analysis.data_error = true;
            }
            if sector.address_error {
                analysis.address_error = true;
            }
            if sector.deleted_mark {
                analysis.deleted_data = true;
            }
            last_n = sector.id_chsn.n();
            n_set.insert(sector.id_chsn.n());
        }

        if n_set.len() > 1 {
            analysis.consistent_sector_size = None;
        }
        else {
            analysis.consistent_sector_size = Some(last_n);
        }

        analysis.sector_ct = sector_ct;
        Ok(analysis)
    }

    fn stream(&self) -> Option<&TrackDataStream> {
        None
    }

    fn stream_mut(&mut self) -> Option<&mut TrackDataStream> {
        None
    }
}

impl MetaSectorTrack {
    #[allow(dead_code)]
    fn add_write(&mut self, _bytes: usize) {
        self.shared.lock().unwrap().writes += 1;
    }

    fn match_sectors(&self, id: DiskChsnQuery, _debug: bool) -> SectorMatch {
        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;

        let mut sizes = FoxHashSet::new();
        let matching_sectors: Vec<&MetaSector> = self
            .sectors
            .iter()
            .filter(|s| {
                if id.c().is_some() && s.id_chsn.c() != id.c().unwrap() {
                    wrong_cylinder = true;
                }
                if s.id_chsn.c() == 0xFF {
                    bad_cylinder = true;
                }
                if id.h().is_some() && s.id_chsn.h() != id.h().unwrap() {
                    wrong_head = true;
                }
                sizes.insert(s.id_chsn.n());
                id.matches(&s.id_chsn)
            })
            .collect();

        SectorMatch {
            sectors: matching_sectors,
            sizes: sizes.iter().cloned().collect(),
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
        }
    }

    fn match_sectors_mut(&mut self, id: DiskChsnQuery, _debug: bool) -> SectorMatchMut {
        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;

        let mut sizes = FoxHashSet::new();
        let matching_sectors: Vec<&mut MetaSector> = self
            .sectors
            .iter_mut()
            .filter(|s| {
                if id.c().is_some() && s.id_chsn.c() != id.c().unwrap() {
                    wrong_cylinder = true;
                }
                if s.id_chsn.c() == 0xFF {
                    bad_cylinder = true;
                }
                if id.h().is_some() && s.id_chsn.h() != id.h().unwrap() {
                    wrong_head = true;
                }
                sizes.insert(s.id_chsn.n());
                id.matches(&s.id_chsn)
            })
            .collect();

        SectorMatchMut {
            sectors: matching_sectors,
            sizes: sizes.iter().cloned().collect(),
            wrong_cylinder,
            bad_cylinder,
            wrong_head,
            shared: self.shared.clone(),
        }
    }
}
