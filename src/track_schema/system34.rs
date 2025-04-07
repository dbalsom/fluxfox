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
*/

//! An indirect implementation of the `TrackSchemaParser` trait for the IBM System 34
//! track schema, used by IBM PCs and compatibles and Macintosh 1.44MB HD disks.
//!
//! The System34 track schema supports both MFM and FM track encodings.

use core::ops::Range;
use std::fmt::{Display, Formatter};

use crate::{
    bitstream_codec::{
        mfm::{MfmCodec, MFM_BYTE_LEN, MFM_MARKER_LEN},
        MarkerEncoding,
        TrackDataStream,
    },
    io::{Read, Seek, SeekFrom},
    mfm_offset,
    prelude::{RwScope, TrackDataEncoding},
    source_map::{OptionalSourceMap, SourceMap, SourceValue},
    track::{TrackAnalysis, TrackSectorScanResult},
    track_schema::{
        GenericTrackElement,
        TrackElement,
        TrackElementInstance,
        TrackMarker,
        TrackMarkerItem,
        TrackMetadata,
    },
    types::{chs::DiskChsn, IntegrityCheck, IntegrityField},
    util::crc_ibm_3740,
    DiskImageError,
    FoxHashSet,
    SectorIdQuery,
};
use bit_vec::BitVec;

pub const DEFAULT_TRACK_SIZE_BYTES: usize = 6250;

pub const GAP_BYTE: u8 = 0x4E;
pub const SYNC_BYTE: u8 = 0;

pub const IBM_GAP3_DEFAULT: usize = 22;
pub const IBM_GAP4A: usize = 80;
pub const IBM_GAP1: usize = 50;
pub const IBM_GAP2: usize = 22;
pub const ISO_GAP1: usize = 32;
pub const ISO_GAP2: usize = 22;
pub const SYNC_LEN: usize = 12;
pub const PERPENDICULAR_GAP1: usize = 50;
pub const PERPENDICULAR_GAP2: usize = 41;

// Pre-encoded markers for IAM, IDAM, DAM and DDAM.
pub const IAM_MARKER: u64 = 0x5224_5224_5224_5552;
pub const IDAM_MARKER: u64 = 0x4489_4489_4489_5554;
pub const DAM_MARKER: u64 = 0x4489_4489_4489_5545;
pub const DDAM_MARKER: u64 = 0x4489_4489_4489_5548;
pub const ANY_MARKER: u64 = 0x4489_4489_4489_0000;
pub const CLOCK_MASK: u64 = 0xAAAA_AAAA_AAAA_0000;
pub const DATA_MARK: u64 = 0x5555_5555_5555_5555;
pub const MARKER_MASK: u64 = 0xFFFF_FFFF_FFFF_0000;

pub const FM_MARKER_CLOCK: u64 = 0xAAAA_AAAA_AAAA_0000;
pub const MFM_MARKER_CLOCK: u64 = 0x0220_0220_0220_0000;

pub const IAM_MARKER_FM: u64 = 0xFAAE_FAAE_FAAE_FFFA;

pub const IAM_MARKER_BYTES: [u8; 4] = [0xC2, 0xC2, 0xC2, 0xFC];
pub const IDAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xFE];
pub const DAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xFB];
pub const DDAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xF8];

pub enum System34Variant {
    Ibm3740,
    Ibm,
    Iso,
    Perpendicular,
}

#[derive(Debug)]
pub struct System34FormatBuffer {
    pub chs_vec: Vec<DiskChsn>,
}

impl From<&[u8]> for System34FormatBuffer {
    fn from(buffer: &[u8]) -> Self {
        let mut chs_vec = Vec::new();
        for i in (0..buffer.len()).step_by(4) {
            let c = buffer[i];
            let h = buffer[i + 1];
            let s = buffer[i + 2];
            let n = buffer[i + 3];
            chs_vec.push(DiskChsn::new(c as u16, h, s, n));
        }
        System34FormatBuffer { chs_vec }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum System34Standard {
    Ibm,
    Perpendicular,
    Iso,
}

impl System34Standard {
    pub fn gap2(&self) -> usize {
        match self {
            System34Standard::Ibm => IBM_GAP2,
            System34Standard::Perpendicular => PERPENDICULAR_GAP2,
            System34Standard::Iso => ISO_GAP2,
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum System34Marker {
    Iam,
    Idam,
    Dam,
    Ddam,
}

impl From<System34Marker> for u64 {
    fn from(marker: System34Marker) -> u64 {
        match marker {
            System34Marker::Iam => IAM_MARKER,
            System34Marker::Idam => IDAM_MARKER,
            System34Marker::Dam => DAM_MARKER,
            System34Marker::Ddam => DDAM_MARKER,
        }
    }
}

impl TryInto<System34Marker> for u16 {
    type Error = ();

    fn try_into(self) -> Result<System34Marker, Self::Error> {
        match self {
            0x5554 | 0xF57E => Ok(System34Marker::Idam),
            0x5545 | 0xF56F => Ok(System34Marker::Dam),
            0x554A => Ok(System34Marker::Ddam),
            _ => {
                log::error!("Invalid System34 marker: {:04X}", self);
                Err(())
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum System34Element {
    Gap1,
    Gap2,
    Gap3,
    Gap4a,
    Gap4b,
    Sync,
    Marker(System34Marker, Option<bool>),
    SectorHeader {
        chsn: DiskChsn,
        address_error: bool,
        data_missing: bool,
    },
    SectorData {
        chsn: DiskChsn,
        address_error: bool,
        data_error: bool,
        deleted: bool,
    },
}

impl From<System34Element> for GenericTrackElement {
    fn from(elem: System34Element) -> Self {
        match elem {
            System34Element::Gap1 => GenericTrackElement::NullElement,
            System34Element::Gap2 => GenericTrackElement::NullElement,
            System34Element::Gap3 => GenericTrackElement::NullElement,
            System34Element::Gap4a => GenericTrackElement::NullElement,
            System34Element::Gap4b => GenericTrackElement::NullElement,
            System34Element::Sync => GenericTrackElement::NullElement,
            System34Element::Marker(_, _) => GenericTrackElement::Marker,
            System34Element::SectorHeader { address_error, .. } => match address_error {
                true => GenericTrackElement::SectorBadHeader,
                false => GenericTrackElement::SectorHeader,
            },
            System34Element::SectorData {
                address_error,
                data_error,
                deleted,
                ..
            } => match (address_error || data_error, deleted) {
                (false, false) => GenericTrackElement::SectorData,
                (true, false) => GenericTrackElement::SectorBadData,
                (false, true) => GenericTrackElement::SectorDeletedData,
                (true, true) => GenericTrackElement::SectorBadDeletedData,
            },
        }
    }
}

impl System34Element {
    pub fn size(&self) -> usize {
        match self {
            System34Element::Gap1 => 8,
            System34Element::Gap2 => 8,
            System34Element::Gap3 => 8,
            System34Element::Gap4a => 8,
            System34Element::Gap4b => 8,
            System34Element::Sync => 8,
            System34Element::Marker(_, _) => 4,
            System34Element::SectorData { chsn, .. } => {
                // Data size is determined by the sector size in bytes, plus DAM and CRC.
                4 + 2 + chsn.n_size()
            }
            System34Element::SectorHeader { .. } => {
                // IDAM + Sector ID (4 bytes) + CRC (2 bytes)
                4 + 4 + 2
            }
        }
    }

    /// Provide a subset data range corresponding scope requested for the current element.
    pub fn range(&self, scope: RwScope) -> Range<usize> {
        // Most elements don't support a scope.
        match (self, scope) {
            (System34Element::SectorData { .. }, RwScope::DataOnly) => {
                // Data scope is the data portion of the sector only.
                // Skip the IDAM (4 bytes) and omit CRC field from end (2 bytes)
                4..(self.size() - 2)
            }
            (System34Element::SectorData { .. } | System34Element::SectorHeader { .. }, RwScope::CrcOnly) => {
                // CRC scope is the CRC field only (last two bytes).
                self.size() - 2..self.size()
            }
            (_, _) => 0..self.size(),
        }
    }

    pub fn is_sector_data_marker(&self) -> bool {
        matches!(self, System34Element::Marker(System34Marker::Dam, _))
    }

    pub fn is_sector_data(&self) -> bool {
        matches!(self, System34Element::SectorData { .. })
    }

    pub fn is_sector_id(&self) -> (u8, bool) {
        match self {
            System34Element::SectorHeader {
                chsn, address_error, ..
            } => match address_error {
                true => (0, false),
                false => (chsn.s(), true),
            },
            _ => (0, false),
        }
    }
}

// TODO: get rid of this duplicate sector id type. Use DiskChsn?
#[derive(Default)]
pub struct SectorId {
    pub c: u8,
    pub h: u8,
    pub s: u8,
    pub b: u8,
    pub crc: u16,
    pub crc_valid: bool,
}

impl SectorId {
    pub fn sector_size_in_bytes(&self) -> usize {
        std::cmp::min(8192, 128usize.overflowing_shl(self.b as u32).0)
    }
}

impl Display for SectorId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "[C: {} H: {} S: {} B: {} CRC: {:04X}]",
            self.c, self.h, self.s, self.b, self.crc
        )
    }
}

pub struct System34FormatResult {
    pub track_bytes: Vec<u8>,
    pub markers: Vec<(System34Marker, usize)>,
}

pub struct System34Schema;

impl System34Schema {
    // System34 masks clock bits in the MFM encoding of address marks.
    // This is to help differentiate markers from data.
    const MFM_MARKER_CLOCK_MASK: u64 = 0x5555_5555_5555_FFFF;
    const MFM_MARKER_CLOCK: u64 = 0x0088_0088_0088_0000;
    #[inline]
    pub fn encode_marker(pattern: &[u8]) -> u64 {
        let marker = MfmCodec::encode_marker(pattern);
        marker & Self::MFM_MARKER_CLOCK_MASK | Self::MFM_MARKER_CLOCK
    }

    pub fn format_track_as_bytes(
        standard: System34Standard,
        bitcell_ct: usize,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<System34FormatResult, DiskImageError> {
        if fill_pattern.is_empty() {
            log::error!("Fill pattern cannot be empty.");
            return Err(DiskImageError::ParameterError);
        }

        let track_byte_ct = (bitcell_ct + MFM_BYTE_LEN - 1) / MFM_BYTE_LEN;
        log::trace!(
            "format_track_as_bytes(): Formatting track with {} bitcells, {} bytes",
            bitcell_ct,
            track_byte_ct
        );
        let mut track_bytes: Vec<u8> = Vec::with_capacity(track_byte_ct);
        let mut markers = Vec::new();

        if matches!(standard, System34Standard::Ibm | System34Standard::Perpendicular) {
            // Write out GAP0, sync,IAM marker, and GAP1.
            track_bytes.extend_from_slice(&[GAP_BYTE; IBM_GAP4A]); // GAP0
            track_bytes.extend_from_slice(&[SYNC_BYTE; SYNC_LEN]); // Sync
            markers.push((System34Marker::Iam, track_bytes.len()));
        }
        else {
            // Just write Gap1 for ISO standard, there is no IAM marker.
            track_bytes.extend_from_slice(&[GAP_BYTE; ISO_GAP1]);
        }

        let mut pat_cursor = 0;

        for sector in format_buffer {
            track_bytes.extend_from_slice(&[SYNC_BYTE; SYNC_LEN]); // Write initial sync.
            markers.push((System34Marker::Idam, track_bytes.len()));
            let idam_crc_offset = track_bytes.len();
            track_bytes.extend_from_slice(IDAM_MARKER_BYTES.as_ref()); // Write IDAM marker.

            // Write CHSN bytes.
            track_bytes.push(sector.c() as u8);
            track_bytes.push(sector.h());
            track_bytes.push(sector.s());
            track_bytes.push(sector.n());

            // Write CRC word.
            //log::error!("Calculating crc over : {:X?}", &track_bytes[idam_crc_offset..]);
            let crc16 = crc_ibm_3740(&track_bytes[idam_crc_offset..], None);
            track_bytes.extend_from_slice(&crc16.to_be_bytes());

            // Write GAP2.
            track_bytes.extend_from_slice(&vec![GAP_BYTE; standard.gap2()]);

            // Write SYNC.
            track_bytes.extend_from_slice(&[SYNC_BYTE; SYNC_LEN]);

            // Write DAM marker.
            markers.push((System34Marker::Dam, track_bytes.len()));
            let dam_crc_offset = track_bytes.len();
            track_bytes.extend_from_slice(DAM_MARKER_BYTES.as_ref());

            // Write sector data using provided pattern buffer.
            if fill_pattern.len() == 1 {
                track_bytes.extend_from_slice(&vec![fill_pattern[0]; sector.n_size()]);
            }
            else {
                let mut sector_buffer = Vec::with_capacity(sector.n_size());
                while sector_buffer.len() < sector.n_size() {
                    let remain = sector.n_size() - sector_buffer.len();
                    let copy_pat = if pat_cursor + remain <= fill_pattern.len() {
                        &fill_pattern[pat_cursor..pat_cursor + remain]
                    }
                    else {
                        &fill_pattern[pat_cursor..]
                    };

                    sector_buffer.extend_from_slice(copy_pat);
                    //log::warn!("format: sector_buffer: {:X?}", sector_buffer);
                    pat_cursor = (pat_cursor + copy_pat.len()) % fill_pattern.len();
                }

                //log::warn!("sector buffer is now {} bytes", sector_buffer.len());
                track_bytes.extend_from_slice(&sector_buffer);
            }

            //log::warn!("format: track_bytes: {:X?}", track_bytes);
            //log::warn!("track_bytes is now {} bytes", track_bytes.len());

            // Write CRC word.
            let crc16 = crc_ibm_3740(&track_bytes[dam_crc_offset..], None);
            track_bytes.extend_from_slice(&crc16.to_be_bytes());

            // Write GAP3.
            track_bytes.extend_from_slice(&vec![GAP_BYTE; gap3]);
        }

        // Fill rest of track with GAP4B.
        if track_bytes.len() < track_byte_ct {
            track_bytes.extend_from_slice(&vec![GAP_BYTE; track_byte_ct - track_bytes.len()]);
        }

        if track_bytes.len() > track_byte_ct {
            log::warn!(
                "format_track_as_bytes(): Format operation passed index. Truncating track to {} bytes",
                track_byte_ct
            );
            track_bytes.truncate(track_byte_ct);
        }

        log::trace!(
            "format_track_as_bytes(): Wrote {} markers to track of size {} bitcells: {}",
            markers.len(),
            track_bytes.len(),
            track_bytes.len() * 8
        );

        Ok(System34FormatResult { track_bytes, markers })
    }

    pub(crate) fn set_track_markers(
        codec: &mut TrackDataStream,
        markers: Vec<(System34Marker, usize)>,
    ) -> Result<(), DiskImageError> {
        for (marker, offset) in markers {
            let marker_u64 = u64::from(marker);

            let marker_bit_index = offset * MFM_BYTE_LEN;

            let marker_bytes = marker_u64.to_be_bytes();

            //log::trace!("Setting marker {:X?} at bit index: {}", marker_bytes, marker_bit_index);
            codec.write_raw_buf(&marker_bytes, marker_bit_index);
        }

        Ok(())
    }
}

// Quasi-trait impl of TrackSchema - called by enum dispatch
impl System34Schema {
    /// Find the provided pattern of bytes within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// The bit offset of the pattern is returned if found, otherwise None.
    /// The pattern length is limited to 8 characters.
    #[allow(dead_code)]
    pub(crate) fn find_data_pattern(stream: &TrackDataStream, pattern: &[u8], index: usize) -> Option<usize> {
        let mut buffer = [0u8; 8];
        let len = pattern.len().min(8);
        buffer[(8 - len)..8].copy_from_slice(&pattern[..len]);
        let pat = u64::from_be_bytes(buffer);
        let pat_mask = u64::MAX >> (8 * (8 - len));
        let mut shift_reg = 0;

        //log::trace!("Constructed pattern: {:016X?}, mask: {:016X}", pat, pat_mask);

        for (bit_ct, bi) in (index..stream.len()).enumerate() {
            shift_reg = shift_reg << 1 | stream[bi] as u64;
            if (bit_ct >= (len * 8)) && (shift_reg & pat_mask) == pat {
                //log::trace!("find_data_pattern: shift_reg: {:064b} pat: {:064b}", shift_reg, pat);
                //log::trace!("find_data_pattern: Found pattern at bit offset: {}", bi);
                return Some(bi - len * 8 + 1);
            }
        }
        None
    }

    pub(crate) fn analyze_elements(metadata: &TrackMetadata) -> TrackAnalysis {
        let mut analysis = TrackAnalysis::default();
        let mut n_set: FoxHashSet<u8> = FoxHashSet::new();
        let mut last_n = 0;

        let sector_ids = metadata.sector_ids();
        let sector_ct = sector_ids.len();

        for (si, sector_id) in sector_ids.iter().enumerate() {
            if sector_id.s() != si as u8 + 1 {
                analysis.nonconsecutive_sectors = true;
            }
            last_n = sector_id.n();
            n_set.insert(sector_id.n());
        }

        if n_set.len() > 1 {
            //log::warn!("get_track_consistency(): Variable sector sizes detected: {:?}", n_set);
            analysis.consistent_sector_size = None;
        }
        else {
            //log::warn!("get_track_consistency(): Consistent sector size: {}", last_n);
            analysis.consistent_sector_size = Some(last_n);
        }

        for ei in metadata.elements() {
            match ei.element {
                TrackElement::System34(System34Element::SectorData {
                    address_error,
                    data_error,
                    deleted,
                    ..
                }) => {
                    if address_error {
                        analysis.address_error = true;
                    }
                    if data_error {
                        analysis.data_error = true
                    }
                    if deleted {
                        analysis.deleted_data = true;
                    }
                }
                TrackElement::System34(System34Element::SectorHeader {
                    address_error,
                    data_missing,
                    ..
                }) => {
                    if address_error {
                        analysis.address_error = true;
                    }
                    if data_missing {
                        analysis.no_dam = true;
                    }
                }
                _ => {}
            }
        }

        analysis.sector_ct = sector_ct;
        analysis
    }

    /// Find the next address marker in the track bitstream. The type of marker and its position in
    /// the bitstream is returned, or None.
    pub(crate) fn find_next_marker(stream: &TrackDataStream, offset: usize) -> Option<(TrackMarker, usize)> {
        match stream.encoding() {
            TrackDataEncoding::Mfm => {
                let marker = MarkerEncoding {
                    bits: ANY_MARKER,
                    mask: MARKER_MASK,
                    ..MarkerEncoding::default()
                };

                if let Some((index, marker_u16)) = stream.find_marker(&marker, offset, None) {
                    if let Ok(marker) = marker_u16.try_into() {
                        return Some((TrackMarker::System34(marker), index));
                    }
                }
            }
            TrackDataEncoding::Fm => {
                let marker = MarkerEncoding {
                    bits: ANY_MARKER,
                    mask: MARKER_MASK,
                    ..MarkerEncoding::default()
                };

                if let Some((index, marker_u16)) = stream.find_marker(&marker, offset, None) {
                    if let Ok(marker) = marker_u16.try_into() {
                        return Some((TrackMarker::System34(marker), index));
                    }
                }
            }
            _ => {
                // System34 only supports MFM and FM encodings.
                log::warn!(
                    "find_next_marker(): Unsupported stream encoding: {:?}",
                    stream.encoding()
                );
                return None;
            }
        }
        None
    }

    pub(crate) fn find_marker(
        stream: &TrackDataStream,
        marker: TrackMarker,
        index: usize,
        limit: Option<usize>,
    ) -> Option<(usize, u16)> {
        if let TrackMarker::System34(marker) = marker {
            let marker = MarkerEncoding {
                bits: u64::from(marker),
                ..MarkerEncoding::default()
            };
            return stream.find_marker(&marker, index, limit);
        }
        None
    }

    pub(crate) fn find_sector_element(
        id: impl Into<SectorIdQuery>,
        elements: &[TrackElementInstance],
        index: usize,
        _limit: Option<usize>,
    ) -> TrackSectorScanResult {
        let id = id.into();
        let mut wrong_cylinder = false;
        let mut bad_cylinder = false;
        let mut wrong_head = false;

        let mut last_idam_matched = false;
        //let mut last_idam_chsn: Option<DiskChsn> = None;
        for (ei, instance) in elements.iter().enumerate() {
            if instance.start < index {
                continue;
            }

            let TrackElementInstance {
                element, last_sector, ..
            } = instance;
            match element {
                TrackElement::System34(System34Element::SectorHeader {
                    chsn,
                    address_error,
                    data_missing,
                }) => {
                    if chsn.s() == id.s() {
                        // if c is 0xFF, we set the flag for bad cylinder.
                        if chsn.c() == 0xFF {
                            bad_cylinder = true;
                        }

                        // If c differs, we set the flag for wrong cylinder.
                        if id.c().is_some() && chsn.c() != id.c().unwrap() {
                            wrong_cylinder = true;
                        }

                        // If h differs, we set the flag for wrong head.
                        if id.h().is_some() && chsn.h() != id.h().unwrap() {
                            wrong_head = true;
                        }

                        last_idam_matched = id.matches(chsn);

                        // A bad header CRC or missing DAM will short-circuit the search.
                        if *address_error || *data_missing {
                            return TrackSectorScanResult::Found {
                                ei,
                                sector_chsn: *chsn,
                                address_error: *address_error,
                                data_error: false,
                                deleted_mark: false,
                                no_dam: *data_missing,
                                last_sector: *last_sector,
                            };
                        }
                    }
                    //idam_chsn = Some(*chsn);
                }
                TrackElement::System34(System34Element::SectorData {
                    chsn,
                    address_error,
                    data_error,
                    deleted,
                }) => {
                    // log::trace!(
                    //     "find_sector_element(): Found sector {} data at index: {} last idam matched? {}",
                    //     idam_chsn,
                    //     mdi.start,
                    //     last_idam_matched
                    // );

                    // If we matched the last sector header, then this is the sector data
                    // we are looking for. Return the info.
                    if last_idam_matched {
                        return TrackSectorScanResult::Found {
                            ei,
                            sector_chsn: *chsn,
                            address_error: *address_error,
                            data_error: *data_error,
                            deleted_mark: *deleted,
                            no_dam: false,
                            last_sector: *last_sector,
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

    pub(crate) fn decode_element(
        stream: &TrackDataStream,
        element: &TrackElementInstance,
        scope: RwScope,
        buf: &mut [u8],
    ) -> (Range<usize>, Option<IntegrityCheck>) {
        // Read the element into the buffer
        stream.read_decoded_buf(buf, element.start);

        match element.element {
            TrackElement::System34(System34Element::SectorHeader { .. }) => {
                // Calculate the CRC16 of the sector header
                let (recorded_crc, calculated_crc) = Self::crc16_bytes(buf);
                let check = IntegrityCheck::Crc16(IntegrityField::new(recorded_crc, calculated_crc));
                (element.element.range(scope).unwrap_or_default(), Some(check))
            }
            TrackElement::System34(System34Element::SectorData { data_error, .. }) => {
                // Calculate the CRC16 of the data.
                let (recorded_crc, calculated_crc) = Self::crc16_bytes(buf);
                let check = IntegrityCheck::Crc16(IntegrityField::new(recorded_crc, calculated_crc));

                if data_error != check.is_error() {
                    log::warn!("Data CRC state out of sync with metadata!");
                }

                (element.element.range(scope).unwrap_or_default(), Some(check))
            }
            _ => (element.element.range(scope).unwrap_or_default(), None),
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn encode_element(
        stream: &mut TrackDataStream,
        element: &mut TrackElementInstance,
        _scope: RwScope,
        buf: &[u8],
    ) -> usize {
        // TODO: Detect and properly encode markers

        match &mut element.element {
            TrackElement::System34(System34Element::SectorHeader { .. }) => {
                // TODO: Implement marker writes
                0
            }
            TrackElement::System34(System34Element::SectorData {
                data_error, deleted, ..
            }) => {
                let marker_bytes = match deleted {
                    true => &IDAM_MARKER_BYTES,
                    false => &DAM_MARKER_BYTES,
                };
                let marker_crc = crc_ibm_3740(marker_bytes, None);

                // Calculate the CRC16 of the data.
                let crc = crc_ibm_3740(buf, Some(marker_crc));
                log::debug!(
                    "encode_element(): Calculated CRC16 over {} bytes: {:04X}",
                    buf.len(),
                    crc
                );

                let mut bytes = stream.write_encoded_buf(buf, element.start + mfm_offset!(4));
                let crc_bytes = crc.to_be_bytes();

                bytes += stream.write_encoded_buf(&crc_bytes, element.start + mfm_offset!(4 + buf.len()));
                *data_error = false;
                bytes
            }
            _ => stream.write_encoded_buf(buf, element.start),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn find_element(stream: &TrackDataStream, element: TrackElement, index: usize) -> Option<usize> {
        if let TrackElement::System34(element) = element {
            use System34Element::*;

            let (marker_u64, _pattern) = match element {
                Gap1 | Gap2 | Gap3 | Gap4a | Gap4b => (System34Schema::encode_marker(&[0x4E; 4]), &[0x4E; 4]),
                Sync => (MfmCodec::encode_marker(&[0x00; 4]), &[0x00; 4]),
                _ => return None,
            };

            //let marker = System34Parser::encode_marker(pattern);
            log::trace!(
                "find_element(): Encoded element: {:?} as {:016X}/{:064b}",
                element,
                marker_u64,
                marker_u64
            );

            log::trace!("find_element(): Searching for element at offset: {}", index);
            let marker = MarkerEncoding {
                bits: marker_u64,
                ..MarkerEncoding::default()
            };
            let found_marker = stream.find_marker(&marker, index, None);

            if let Some(marker_pos) = found_marker {
                log::trace!(
                    "find_element(): Found element in raw stream: {:?} at index: {}, sync: {} debug: {}",
                    element,
                    marker_pos.0,
                    marker_pos.0 & 1,
                    stream.debug_marker(index)
                );
                return Some(index);
            }

            //log::trace!("Searching for pattern: {:02X?} at offset {}", pattern, offset);
            //return System34Parser::find_pattern(track, pattern, offset);
        };

        None
    }

    /// Scan a track bitstream for address markers, including the IAM, IDAM and DAM markers. Return
    /// their positions. The marker positions will be used to create the clock phase map for the
    /// track, which must be performed before we can read the data off the disk which is done in
    /// a second pass.
    pub(crate) fn scan_markers(stream: &TrackDataStream) -> Vec<TrackMarkerItem> {
        let mut bit_cursor: usize = 0;
        let mut markers = Vec::new();

        // Look for the IAM marker first - but it may not be present (ISO standard encoding does
        // not require it).

        if let Some(marker) = System34Schema::find_marker(
            stream,
            TrackMarker::System34(System34Marker::Iam),
            bit_cursor,
            Some(5_000),
        ) {
            log::trace!("scan_track_markers(): Found IAM marker at bit index: {}", marker.0);
            markers.push(TrackMarkerItem {
                elem_type: TrackMarker::System34(System34Marker::Iam),
                start: marker.0,
            });
            bit_cursor = marker.0 + 4 * MFM_BYTE_LEN;
        }

        while let Some((marker, marker_offset)) = System34Schema::find_next_marker(stream, bit_cursor) {
            /*
            log::trace!(
                "scan_track_markers(): Found marker of type {:?} at bit offset: {}",
                marker,
                marker_offset
            );*/

            markers.push(TrackMarkerItem {
                elem_type: marker,
                start: marker_offset,
            });
            bit_cursor = marker_offset + 4 * MFM_BYTE_LEN;
        }
        markers
    }

    /// Scan a track bitstream using the pre-scanned marker positions to extract marker data such
    /// as Sector ID values and CRCs. This is done in a second pass after the markers have been
    /// found by scan_track_markers() and a clock phase map created for the track - required for the
    /// proper functioning of the Read and Seek traits on MfmCodec.
    pub(crate) fn scan_metadata(
        stream: &mut TrackDataStream,
        markers: Vec<TrackMarkerItem>,
    ) -> Vec<TrackElementInstance> {
        let mut elements = Vec::new();
        let mut last_marker_opt: Option<System34Marker> = None;
        let mut last_sector_id = SectorId::default();
        let mut last_element_offset = 0;

        for marker in &markers {
            let element_offset = marker.start;

            if let TrackMarker::System34(sys34_marker) = marker.elem_type {
                match (last_marker_opt, sys34_marker) {
                    (Some(System34Marker::Idam), System34Marker::Idam) => {
                        // Encountered IDAMs back to back. This is sometimes seen in copy-protection methods
                        // such as XELOK v1.

                        // Push a Sector Header metadata item spanning from last IDAM to this IDAM.
                        let metadata = TrackElementInstance {
                            element: TrackElement::System34(System34Element::SectorHeader {
                                chsn: DiskChsn::from((
                                    last_sector_id.c as u16,
                                    last_sector_id.h,
                                    last_sector_id.s,
                                    last_sector_id.b,
                                )),
                                address_error: !last_sector_id.crc_valid,
                                data_missing: true, // Flag data as missing.
                            }),
                            start: last_element_offset,
                            end: element_offset,
                            chsn: None,
                            last_sector: false,
                        };
                        elements.push(metadata)
                    }
                    (_, System34Marker::Idam) => {
                        // Encountered a sector ID address mark (sector header), after any element.
                        let mut sector_header = [0; 8];

                        // TODO: Don't unwrap in a library unless provably safe.
                        //       Consider removing option return type from read_decoded_byte.
                        sector_header[0] = stream.read_decoded_u8(marker.start + mfm_offset!(0)).unwrap();
                        sector_header[1] = stream.read_decoded_u8(marker.start + mfm_offset!(1)).unwrap();
                        sector_header[2] = stream.read_decoded_u8(marker.start + mfm_offset!(2)).unwrap();
                        sector_header[3] = stream.read_decoded_u8(marker.start + mfm_offset!(3)).unwrap();

                        log::trace!("Idam marker read: {:02X?}", &sector_header[0..4]);
                        sector_header[4] = stream.read_decoded_u8(marker.start + mfm_offset!(4)).unwrap(); // Cylinder
                        sector_header[5] = stream.read_decoded_u8(marker.start + mfm_offset!(5)).unwrap(); // Head
                        sector_header[6] = stream.read_decoded_u8(marker.start + mfm_offset!(6)).unwrap(); // Sector
                        sector_header[7] = stream.read_decoded_u8(marker.start + mfm_offset!(7)).unwrap(); // Sector size (b)
                        let crc_byte0 = stream.read_decoded_u8(marker.start + mfm_offset!(8)).unwrap_or(0xAA);
                        let crc_byte1 = stream.read_decoded_u8(marker.start + mfm_offset!(9)).unwrap_or(0xAA);

                        let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                        let calculated_crc = crc_ibm_3740(&sector_header[0..8], None);

                        let sector_id = SectorId {
                            c: sector_header[4],
                            h: sector_header[5],
                            s: sector_header[6],
                            b: sector_header[7],
                            crc,
                            crc_valid: crc == calculated_crc,
                        };
                        log::trace!(
                            "Sector ID: {} Size: {} crc: {:04X} calculated CRC: {:04X}",
                            sector_id,
                            sector_id.sector_size_in_bytes(),
                            crc,
                            calculated_crc
                        );
                        last_sector_id = sector_id;
                    }
                    (Some(System34Marker::Idam), System34Marker::Dam | System34Marker::Ddam) => {
                        // Encountered a DAM or DDAM after a sector header (IDAM). This is the sector data.
                        let data_len = last_sector_id.sector_size_in_bytes() * MFM_BYTE_LEN;
                        let data_end = element_offset + MFM_MARKER_LEN + data_len;

                        let log_prefix = match sys34_marker {
                            System34Marker::Dam => "",
                            System34Marker::Ddam => "Deleted ",
                            _ => "UNKNOWN ",
                        };

                        log::trace!(
                            "{}Data marker at offset: {}, data size: {} crc_start:{} crc_end:{}",
                            log_prefix,
                            element_offset,
                            data_len,
                            element_offset,
                            data_end
                        );

                        let mut dam_header = [0; 4];
                        dam_header[0] = stream.read_decoded_u8(marker.start + mfm_offset!(0)).unwrap();
                        dam_header[1] = stream.read_decoded_u8(marker.start + mfm_offset!(1)).unwrap();
                        dam_header[2] = stream.read_decoded_u8(marker.start + mfm_offset!(2)).unwrap();
                        dam_header[3] = stream.read_decoded_u8(marker.start + mfm_offset!(3)).unwrap();

                        //log::debug!("DAM header verification: {:02X?}", dam_header);

                        let (data_crc, calculated_crc) = System34Schema::crc16(stream, element_offset, data_end);
                        log::trace!("Data CRC16: {:04X} Calculated: {:04X}", data_crc, calculated_crc);

                        let crc_correct = data_crc == calculated_crc;
                        if !crc_correct {
                            log::warn!("Data CRC error detected at offset: {}", element_offset);
                        }

                        // Push a Sector Header metadata item spanning from IDAM to DAM.
                        let data_metadata = TrackElementInstance {
                            element: TrackElement::System34(System34Element::SectorHeader {
                                chsn: DiskChsn::from((
                                    last_sector_id.c as u16,
                                    last_sector_id.h,
                                    last_sector_id.s,
                                    last_sector_id.b,
                                )),
                                address_error: !last_sector_id.crc_valid,
                                data_missing: false,
                            }),
                            start: last_element_offset,
                            end: element_offset,
                            chsn: None,
                            last_sector: false,
                        };
                        elements.push(data_metadata);

                        let element = match sys34_marker {
                            System34Marker::Dam => System34Element::SectorData {
                                chsn: DiskChsn::new(
                                    last_sector_id.c as u16,
                                    last_sector_id.h,
                                    last_sector_id.s,
                                    last_sector_id.b,
                                ),
                                address_error: !last_sector_id.crc_valid,
                                data_error: !crc_correct,
                                deleted: false,
                            },
                            System34Marker::Ddam => System34Element::SectorData {
                                chsn: DiskChsn::new(
                                    last_sector_id.c as u16,
                                    last_sector_id.h,
                                    last_sector_id.s,
                                    last_sector_id.b,
                                ),
                                address_error: !last_sector_id.crc_valid,
                                data_error: !crc_correct,
                                deleted: true,
                            },
                            _ => unreachable!(),
                        };

                        let data_metadata = TrackElementInstance {
                            element: TrackElement::System34(element),
                            start: element_offset,
                            end: data_end,
                            chsn: Some(DiskChsn::new(
                                last_sector_id.c as u16,
                                last_sector_id.h,
                                last_sector_id.s,
                                last_sector_id.b,
                            )),
                            last_sector: false,
                        };
                        elements.push(data_metadata);
                    }
                    _ => {}
                }

                // Push marker as Metadata item.
                let marker_metadata = TrackElementInstance {
                    element: TrackElement::System34(System34Element::Marker(sys34_marker, None)),
                    start: marker.start,
                    end: marker.start + 4 * MFM_BYTE_LEN,
                    chsn: Some(DiskChsn::new(
                        last_sector_id.c as u16,
                        last_sector_id.h,
                        last_sector_id.s,
                        last_sector_id.b,
                    )),
                    last_sector: false,
                };
                elements.push(marker_metadata);

                // Save the last element seen.
                last_element_offset = element_offset;
                last_marker_opt = Some(sys34_marker);
            }
        }

        if let Some(System34Marker::Idam) = last_marker_opt {
            // Track ends with an IDAM marker. Push a Sector Header metadata item spanning from last
            // IDAM to some point after (range is not important except for viz)
            let data_metadata = TrackElementInstance {
                element: TrackElement::System34(System34Element::SectorHeader {
                    chsn: DiskChsn::from((
                        last_sector_id.c as u16,
                        last_sector_id.h,
                        last_sector_id.s,
                        last_sector_id.b,
                    )),
                    address_error: last_sector_id.crc_valid,
                    data_missing: true, // Flag data as missing.
                }),
                start: last_element_offset,
                end: last_element_offset + 256,
                chsn: None,
                last_sector: false,
            };
            elements.push(data_metadata)
        }

        // Sort elements by start offset.
        elements.sort_by(|a, b| a.start.cmp(&b.start));

        // Mark the last elements as the last sector.
        if let Some(last) = elements.last_mut() {
            last.last_sector = true;
        }

        elements
    }

    /// Use the list of track markers to create a clock phase map for the track. This a requirement
    /// for the proper functioning of the Read and Seek traits on MfmCodec. A clock phase map is
    /// basically a bit vector congruent to the stream `BitVec` that indicates whether the
    /// corresponding stream bit is a clock or data bit.
    pub(crate) fn create_clock_map(markers: &[TrackMarkerItem], clock_map: &mut BitVec) {
        let mut last_marker_index: usize = 0;

        log::trace!("Creating clock map from {} markers...", markers.len());
        #[allow(unused)]
        let mut bit_set = 0;
        for marker in markers {
            if let TrackMarker::System34(_element) = marker.elem_type {
                let bit_index = marker.start;

                if last_marker_index > 0 {
                    // Clear the clock bit immediately before marker to allow for syncing to this
                    // starting clock.
                    clock_map.set(last_marker_index - 1, false);

                    for bi in (last_marker_index..bit_index).step_by(2) {
                        clock_map.set(bi, true);
                        clock_map.set(bi + 1, false);
                        bit_set += 2;
                    }
                }
                last_marker_index = bit_index;
            }
        }

        // Set phase from last marker to end of track.
        if last_marker_index > 0 {
            clock_map.set(last_marker_index - 1, false);
        }

        for bi in (last_marker_index..(clock_map.len() - 1)).step_by(2) {
            clock_map.set(bi, true);
            clock_map.set(bi + 1, false);
            bit_set += 2;
        }
    }

    pub(crate) fn crc16(track: &mut TrackDataStream, bit_index: usize, end: usize) -> (u16, u16) {
        let bytes_requested = (end - bit_index) / 16;

        log::trace!(
            "Performing CRC on {} bytes from bit index {}",
            bytes_requested,
            bit_index
        );

        let mut data = vec![0; bytes_requested + 2];
        track.seek(SeekFrom::Start(bit_index as u64)).unwrap();
        track.read_exact(&mut data).unwrap();

        //log::debug!("Buffer: {:02X?}", data);
        let recorded = u16::from_be_bytes([data[bytes_requested], data[bytes_requested + 1]]);
        let calculated = crc_ibm_3740(&data[0..bytes_requested], None);

        (recorded, calculated)
    }

    pub(crate) fn crc16_bytes(data: &[u8]) -> (u16, u16) {
        //log::debug!("Buffer: {:02X?}", data);
        let recorded = u16::from_be_bytes([data[data.len().saturating_sub(2)], data[data.len().saturating_sub(1)]]);
        let calculated = crc_ibm_3740(&data[..data.len().saturating_sub(2)], None);

        (recorded, calculated)
    }

    pub(crate) fn build_element_map(elements: &[TrackElementInstance]) -> SourceMap {
        let mut element_map = SourceMap::new();

        for (_i, ei) in elements.iter().enumerate() {
            match ei.element {
                TrackElement::System34(System34Element::SectorHeader {
                    chsn,
                    address_error,
                    data_missing,
                }) => {
                    element_map
                        .add_child(0, &format!("IDAM: {}", chsn), SourceValue::default())
                        .add_child(
                            if address_error { "Address Error" } else { "Address OK" },
                            SourceValue::default(),
                        )
                        .add_sibling(
                            if data_missing {
                                "No associated DAM"
                            }
                            else {
                                "Matching DAM"
                            },
                            SourceValue::default(),
                        )
                        .add_sibling("Start", SourceValue::u32(ei.start as u32))
                        .add_sibling("End", SourceValue::u32(ei.end as u32));
                }
                TrackElement::System34(System34Element::SectorData {
                    chsn,
                    address_error,
                    data_error,
                    deleted,
                }) => {
                    let cursor = if deleted {
                        element_map.add_child(0, &format!("DDAM: {}", chsn), SourceValue::default())
                    }
                    else {
                        element_map.add_child(0, &format!("DAM: {}", chsn), SourceValue::default())
                    };

                    cursor
                        .add_child(
                            if address_error { "Address Error" } else { "Address OK" },
                            SourceValue::default(),
                        )
                        .add_sibling(
                            if data_error { "Data Error" } else { "Data OK" },
                            SourceValue::default(),
                        )
                        .add_sibling("Start", SourceValue::u32(ei.start as u32))
                        .add_sibling("End", SourceValue::u32(ei.end as u32));
                }
                _ => {}
            }
        }
        element_map
    }
}
