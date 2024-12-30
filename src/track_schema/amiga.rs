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
*/

//! An indirect implementation of the [TrackSchemaParser] trait for the Amiga
//! trackdisk track schema.
//!
//! Amiga trackdisk images are MFM encoded and typically contain sequential
//! sectors from 0-10 without the inter-sector gaps seen on IBM PC diskettes.
//!
//! The MFM encoding strategy is also different from the IBM PC in that the
//! Amiga stores odd and even bits of data in separate data blocks which
//! must be reconstructed. Due to this requirement, track data must be read
//! and written via the [TrackDataStream] trait so that the interleaved data
//! can be properly encoded/decoded for schemas such as this that require
//! it.
//!
//! Good documentation on the Amiga trackdisk format can be found at:
//! http://lclevy.free.fr/adflib/adf_info.html
//!

use crate::{
    bitstream_codec::{
        mfm::{MfmCodec, MFM_BYTE_LEN},
        MarkerEncoding,
        TrackDataStream,
    },
    io::{Read, Seek, SeekFrom},
    mfm_offset,
    source_map::{OptionalSourceMap, SourceMap, SourceValue},
    track::{TrackAnalysis, TrackSectorScanResult},
    track_schema::{
        system34::System34Element,
        GenericTrackElement,
        TrackElement,
        TrackElementInstance,
        TrackMarker,
        TrackMarkerItem,
        TrackMetadata,
    },
    types::{chs::DiskChsn, IntegrityCheck, IntegrityField, RwScope},
    util::crc_ibm_3740,
    DiskImageError,
    FoxHashSet,
    SectorIdQuery,
};
use bit_vec::BitVec;
use std::ops::Range;

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
//pub const IAM_MARKER: u64 = 0x5224_5224_5224_5552;
//pub const IDAM_MARKER: u64 = 0x4489_4489_4489_5554;
pub const AMIGA_DAM_MARKER: u64 = 0x2AAA_AAAA_4489_4489;
pub const AMIGA_DAM_MASK: u64 = 0x7FFF_FFFF_FFFF_FFFF;

pub const DDAM_MARKER: u64 = 0x4489_4489_4489_5548;
pub const ANY_MARKER: u64 = 0x4489_4489_4489_0000;
pub const CLOCK_MASK: u64 = 0xAAAA_AAAA_AAAA_0000;
pub const DATA_MARK: u64 = 0x5555_5555_5555_5555;
pub const MARKER_MASK: u64 = 0xFFFF_FFFF_FFFF_0000;

pub const MFM_MARKER_CLOCK: u64 = 0x0220_0220_0220_0000;

pub const IAM_MARKER_BYTES: [u8; 4] = [0xC2, 0xC2, 0xC2, 0xFC];
pub const IDAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xFE];
pub const DAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xFB];
pub const DDAM_MARKER_BYTES: [u8; 4] = [0xA1, 0xA1, 0xA1, 0xF8];

/*#[derive(Debug)]
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
}*/

/// Not sure if there are any others to define, but if GCR has a different format, we can add it here.
pub enum AmigaVariant {
    MfmTrackDisk,
}

#[derive(Default, Debug)]
pub struct AmigaSectorQuery {
    pub t: Option<u8>,        // Track number
    pub s: u8,                // Sector number/id
    pub s_to_end: Option<u8>, // Sectors until end of track
}

/// The minimal specifier for an Amiga sector is a sector number, so we can convert from a u8.
impl From<u8> for AmigaSectorQuery {
    fn from(s: u8) -> Self {
        AmigaSectorQuery {
            t: None,
            s,
            s_to_end: None,
        }
    }
}

impl From<AmigaSectorQuery> for SectorIdQuery {
    fn from(asq: AmigaSectorQuery) -> Self {
        // Assume Amiga disks are double-sided
        let c = asq.t.map(|t| (t / 2) as u16);
        let h = asq.t.map(|t| t % 2);
        SectorIdQuery::new(c, h, asq.s, Some(2))
    }
}

#[allow(dead_code)]
#[derive(Default, Debug)]
struct AmigaSectorId {
    fmt: u8, // Usually 0xFF (Amiga v1.0 format)
    tt:  u8, // Track number (lba-type address)
    ss:  u8, // Sector number (not necessarily consecutive)
    sg:  u8, // Sectors until end (including this one)
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AmigaMarker {
    Sector,
}

impl From<AmigaMarker> for u64 {
    fn from(marker: AmigaMarker) -> u64 {
        match marker {
            AmigaMarker::Sector => AMIGA_DAM_MARKER,
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AmigaElement {
    Marker(AmigaMarker, Option<bool>),
    SectorHeader { chsn: DiskChsn, address_error: bool, data_missing: bool },
    SectorData { chsn: DiskChsn, address_error: bool, data_error: bool },
}

impl From<AmigaElement> for GenericTrackElement {
    fn from(elem: AmigaElement) -> Self {
        use AmigaElement::*;
        match elem {
            Marker(_, _) => GenericTrackElement::Marker,
            SectorHeader { address_error, .. } => match address_error {
                true => GenericTrackElement::SectorBadHeader,
                false => GenericTrackElement::SectorHeader,
            },
            SectorData {
                address_error,
                data_error,
                ..
            } => match address_error || data_error {
                true => GenericTrackElement::SectorBadData,
                false => GenericTrackElement::SectorData,
            },
        }
    }
}

impl AmigaElement {
    pub fn size(&self) -> usize {
        use AmigaElement::*;
        match self {
            Marker(_, _) => {
                // Amiga marker is 2 bytes (0xA1, 0xA1)
                2
            }
            SectorData { .. } => {
                // Sector data comprises:
                //  - data checksum (4 bytes)
                //  - data (512 bytes)
                4 + 512
            }
            SectorHeader { .. } => {
                // Sector header comprises:
                //  - marker (2 bytes)
                //  - info (4 bytes)
                //  - label (16 bytes)
                //  - header checksum (4 bytes)
                2 + 4 + 16 + 4
            }
        }
    }

    /// Provide a subset data range corresponding to the scope requested for the current element.
    pub fn range(&self, scope: RwScope) -> Range<usize> {
        // Most elements don't support a scope.
        use AmigaElement::*;
        match (self, scope) {
            (SectorData { .. }, RwScope::DataOnly) => {
                // Data scope is the data portion of the sector only.
                // Skip the data checksum field
                4..self.size()
            }
            (SectorData { .. }, RwScope::CrcOnly) => {
                // CRC scope is the data checksum field only (first 4 bytes of sector data).
                0..4
            }
            (SectorHeader { .. }, RwScope::CrcOnly) => {
                // CRC scope is the header checksum field only (last 4 bytes of header)
                self.size() - 4..self.size()
            }
            (_, _) => 0..self.size(),
        }
    }

    pub fn is_sector_data_marker(&self) -> bool {
        matches!(self, AmigaElement::Marker(AmigaMarker::Sector, _))
    }

    pub fn is_sector_data(&self) -> bool {
        matches!(self, AmigaElement::SectorData { .. })
    }

    pub fn is_sector_id(&self) -> (u8, bool) {
        match self {
            AmigaElement::SectorHeader {
                chsn, address_error, ..
            } => match address_error {
                true => (0, false),
                false => (chsn.s(), true),
            },
            _ => (0, false),
        }
    }
}

pub struct AmigaSchema;

impl AmigaSchema {
    // System34 masks clock bits in the MFM encoding of address marks.
    // This is to help differentiate markers from data.
    const MFM_MARKER_CLOCK_MASK: u64 = 0x5555_5555_5555_FFFF;
    const MFM_MARKER_CLOCK: u64 = 0x0088_0088_0088_0000;
    #[inline]
    pub fn encode_marker(pattern: &[u8]) -> u64 {
        let marker = MfmCodec::encode_marker(pattern);
        marker & Self::MFM_MARKER_CLOCK_MASK | Self::MFM_MARKER_CLOCK
    }

    /*    pub fn format_track_as_bytes(
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
    }*/

    pub(crate) fn set_track_markers(
        stream: &mut TrackDataStream,
        markers: Vec<(AmigaMarker, usize)>,
    ) -> Result<(), DiskImageError> {
        for (marker, offset) in markers {
            let marker_u64 = u64::from(marker);
            let marker_bit_index = offset * MFM_BYTE_LEN;
            let marker_bytes = marker_u64.to_be_bytes();

            //log::trace!("Setting marker {:X?} at bit index: {}", marker_bytes, marker_bit_index);
            stream.write_raw_buf(&marker_bytes, marker_bit_index);
        }
        Ok(())
    }
}

// Quasi-trait impl of TrackSchemaParser - called by enum dispatch
impl AmigaSchema {
    /// Find the next address marker in the track bitstream. The type of marker and its position in
    /// the bitstream is returned, or None.
    pub(crate) fn find_next_marker(stream: &TrackDataStream, offset: usize) -> Option<(TrackMarker, usize)> {
        // Amiga only has one marker type
        let marker = MarkerEncoding {
            bits: AMIGA_DAM_MARKER,
            mask: AMIGA_DAM_MASK,
            len:  32,
        };

        if let Some((index, _marker_u16)) = stream.find_marker(&marker, offset, None) {
            // if let Ok(marker) = marker_u16.try_into() {
            //     return Some((TrackMarker::Amiga(AmigaMarker::Sector), index));
            // }

            // Amiga only has one marker type
            return Some((TrackMarker::Amiga(AmigaMarker::Sector), index));
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
                TrackElement::Amiga(AmigaElement::SectorHeader {
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
                TrackElement::Amiga(AmigaElement::SectorData {
                    address_error,
                    data_error,
                    ..
                }) => {
                    if address_error {
                        analysis.address_error = true;
                    }
                    if data_error {
                        analysis.data_error = true
                    }
                }

                _ => {}
            }
        }

        analysis.sector_ct = sector_ct;
        analysis
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
        //let mut idam_chsn: Option<DiskChsn> = None;
        for (ei, instance) in elements.iter().enumerate() {
            if instance.start < index {
                continue;
            }

            let TrackElementInstance { element, .. } = instance;
            match element {
                TrackElement::Amiga(AmigaElement::SectorHeader {
                    chsn,
                    address_error,
                    data_missing,
                }) => {
                    if *data_missing {
                        // If this sector header has no DAM, we will return right away
                        // and set no_dam to true.
                        return TrackSectorScanResult::Found {
                            ei,
                            no_dam: true,
                            sector_chsn: *chsn,
                            address_error: *address_error,
                            data_error: false,
                            deleted_mark: false,
                        };
                    }

                    // Sector header should have a corresponding DAM marker which we will
                    // match in the next iteration, if this sector header matches.

                    // We match in two stages - first we match sector id if provided.
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

                        if id.matches(chsn) {
                            last_idam_matched = true;
                        }

                        log::debug!(
                            "find_sector_element(): Found sector header with id {}, matching against sector query: {}",
                            chsn,
                            id
                        );
                    }
                    //idam_chsn = Some(*chsn);
                }
                TrackElement::Amiga(AmigaElement::SectorData {
                    chsn,
                    address_error,
                    data_error,
                }) => {
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
                            ei,
                            sector_chsn: *chsn,
                            address_error: *address_error,
                            data_error: *data_error,
                            deleted_mark: false,
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

    /// Amiga trackdisk images encode odd and even bits of data in separate blocks. This function
    /// decodes the interleaved data into the provided buffer.
    pub(crate) fn decode_element(
        stream: &TrackDataStream,
        element: &TrackElementInstance,
        scope: RwScope,
        buf: &mut [u8],
    ) -> (Range<usize>, Option<IntegrityCheck>) {
        log::debug!("AmigaSchema::decode_element(): Decoding {:?}", element);
        match element.element {
            TrackElement::Amiga(AmigaElement::SectorData { .. }) => {
                // Read the data checksum.
                let recorded_checksum = Self::decode_checksum(stream, element.start);

                log::warn!("decode_element(): got buf size of {}", buf.len());

                // It's easier to interleave the data via raw MFM than try to spread decoded bits
                // back out. We need a buffer that is twice the size of the provided buffer to hold the
                // raw MFM data (8 data bits = 16 MFM bits)
                let mut raw_buf = vec![0u8; buf.len() * 2];
                let raw_bytes_read = stream.read_raw_buf(&mut raw_buf, element.start + mfm_offset!(4));

                if raw_bytes_read < 1024 {
                    log::warn!(
                        "AmigaSchema::decode_element(): Sector data read less than 1024 raw bytes: {}",
                        raw_bytes_read
                    );
                }

                // The checksum is calculated over the data before it is interleaved, so do that now.
                let calculated_checksum = Self::checksum_u32(
                    stream,
                    element.start + mfm_offset!(4),
                    element.start + mfm_offset!(4 + 512),
                );

                // I'm not sure if there's any point in trying to decode the data if bytes_read < 512,
                // but this would basically try anyway. The result can be discarded by the caller if
                // this is meaningless.
                // for (i, out_byte) in buf.iter_mut().take(raw_bytes_read / 2).enumerate() {
                //     let odd_byte = raw_buf[i];
                //     let even_byte = raw_buf[i + raw_bytes_read / 2];
                //     // Bits are numbered from 0-7 with LSB being 0.
                //     // Therefore, the first even bit is 0 - so we need to shift even bits right by 1.
                //     let combined = ((even_byte & 0x55) >> 1) | (odd_byte & 0x55); // Interleave even and odd bits
                //     *out_byte = combined;
                // }

                // Write the decoded CRC to the buffer.
                for (b, cb) in buf.iter_mut().zip(recorded_checksum.to_be_bytes().iter()) {
                    *b = *cb;
                }

                // Decode the sector data to the buffer.
                for (i, out_byte) in buf.iter_mut().skip(4).take((raw_bytes_read / 2) - 4).enumerate() {
                    let odd_byte = raw_buf[i] & 0x55;
                    let even_byte = raw_buf[i + 512] & 0x55;
                    *out_byte = (odd_byte << 1) | even_byte;
                }

                let check = IntegrityCheck::Checksum16(IntegrityField::new(
                    recorded_checksum as u16,
                    calculated_checksum as u16,
                ));

                log::debug!(
                    "AmigaSchema::decode_element(): Decoded sector data with {} checksum",
                    check
                );

                (element.element.range(scope).unwrap_or_default(), Some(check))
            }
            _ => (Range::default(), None),
        }
    }

    pub(crate) fn encode_element(
        _stream: &mut TrackDataStream,
        _element: &TrackElementInstance,
        _scope: RwScope,
        _buf: &[u8],
    ) -> usize {
        0
    }

    pub(crate) fn scan_markers(stream: &TrackDataStream) -> Vec<TrackMarkerItem> {
        let mut bit_cursor: usize = 0;
        let mut markers = Vec::new();

        // Amiga has no IAM marker. Just look for sector markers

        while let Some((marker, marker_offset)) = Self::find_next_marker(stream, bit_cursor) {
            log::trace!(
                "AmigaSchema::scan_track_markers(): Found marker of type {:?} at bit offset: {}",
                marker,
                marker_offset
            );

            markers.push(TrackMarkerItem {
                elem_type: marker,
                start: marker_offset,
            });
            bit_cursor = marker_offset + 4 * MFM_BYTE_LEN;
        }
        markers
    }

    /// Decode interleaved Amiga data. The Amiga uses a 64-bit interleaved format for sector
    /// header info block and checksum values. It is easier to do this via raw MFM than try to
    /// spread the decoded bits back out.
    fn decode_interleaved_u32(stream: &TrackDataStream, index: usize) -> u32 {
        let mut info_block_buf = vec![0; 8];
        let mut debug_buf = vec![0; 4];
        stream.read_raw_buf(&mut info_block_buf, index);
        stream.read_decoded_buf(&mut debug_buf, index);
        let decoded_long = u32::from_be_bytes([debug_buf[0], debug_buf[1], debug_buf[2], debug_buf[3]]);

        log::trace!(
            "interleaved block: {:02X?} decoded: {:02X?} decoded_u32: {:08X}",
            info_block_buf,
            debug_buf,
            decoded_long
        );
        let odd_long = u32::from_be_bytes([
            info_block_buf[0],
            info_block_buf[1],
            info_block_buf[2],
            info_block_buf[3],
        ]);
        let even_long = u32::from_be_bytes([
            info_block_buf[4],
            info_block_buf[5],
            info_block_buf[6],
            info_block_buf[7],
        ]);

        log::debug!("odd_long: {:08X} even_long: {:08X}", odd_long, even_long);

        //let dword = odd_long & 0xAAAA_AAAA | ((even_long & 0xAAAA_AAAA) << 1);
        let dword = ((odd_long & 0x5555_5555) << 1) | (even_long & 0x5555_5555);
        log::trace!("Decoded interleaved DWORD: {:08X}", dword);
        dword
    }

    fn decode_sector_header(stream: &TrackDataStream, index: usize) -> AmigaSectorId {
        let dword = Self::decode_interleaved_u32(stream, index + mfm_offset!(2));

        let info_block: [u8; 4] = dword.to_be_bytes();
        let sector_header = AmigaSectorId {
            fmt: info_block[0],
            tt:  info_block[1],
            ss:  info_block[2],
            sg:  info_block[3],
        };

        log::trace!("Read {:X?}", sector_header);
        sector_header
    }

    fn decode_checksum(stream: &TrackDataStream, index: usize) -> u32 {
        let mut buf = vec![0; 4];
        stream.read_decoded_buf(&mut buf, index);
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
    }

    pub(crate) fn scan_for_elements(
        stream: &mut TrackDataStream,
        markers: Vec<TrackMarkerItem>,
    ) -> Vec<TrackElementInstance> {
        if markers.is_empty() {
            log::error!("scan_metadata(): No markers provided!");
            return Vec::new();
        }

        let mut elements = Vec::new();
        //let mut last_marker_opt: Option<AmigaMarker> = None;
        //let mut last_element_offset = 0;

        log::trace!("scan_metadata(): Scanning {} markers...", markers.len());

        for marker in &markers {
            let index = marker.start;
            if let TrackMarker::Amiga(_) = marker.elem_type {
                let sector_header = Self::decode_sector_header(stream, index);

                let c = sector_header.tt / 2;
                let h = sector_header.tt % 2;
                let chsn = DiskChsn::new(c as u16, h, sector_header.ss, 2);

                let mut byte_index = 2;

                let header_sum_calculated = Self::checksum_u32(
                    stream,
                    index + mfm_offset!(byte_index),
                    index + mfm_offset!(byte_index + 10),
                );
                log::debug!("Calculated header checksum: {:08X}", header_sum_calculated);
                // Advance past header checksum
                byte_index += 4;

                // Advance past sector label block
                for _ in 0..4 {
                    //let data = Self::decode_interleaved(stream, index + mfm_offset!(byte_index));
                    //log::trace!("Sector label: {:08X}", data);
                    byte_index += 4;
                }

                let header_sum = Self::decode_checksum(stream, index + mfm_offset!(byte_index));
                log::debug!(
                    "Recorded Header checksum: {:08X} Valid: {}",
                    header_sum,
                    header_sum == header_sum_calculated
                );
                // Advance past sector header checksum
                byte_index += 4;
                let data_sum = Self::decode_checksum(stream, index + mfm_offset!(byte_index));
                // Calculate the crc for remaining sector data
                let data_sum_calculated = Self::checksum_u32(
                    stream,
                    index + mfm_offset!(byte_index + 4),
                    index + mfm_offset!(byte_index + 4 + 512),
                );
                log::debug!("Calculated Data checksum: {:08X}", data_sum_calculated);
                log::debug!(
                    "Recorded Data checksum: {:08X} Valid: {}",
                    data_sum,
                    data_sum == data_sum_calculated
                );

                // Byte index currently points at sector data checksum field.

                elements.push(TrackElementInstance {
                    element: TrackElement::Amiga(AmigaElement::SectorHeader {
                        chsn,
                        address_error: header_sum != header_sum_calculated,
                        data_missing: false,
                    }),
                    start: marker.start,
                    end: marker.start + mfm_offset!(10),
                    chsn: Some(chsn),
                });

                elements.push(TrackElementInstance {
                    element: TrackElement::Amiga(AmigaElement::SectorData {
                        chsn,
                        address_error: header_sum != header_sum_calculated,
                        data_error: data_sum != data_sum_calculated,
                    }),
                    start: marker.start + mfm_offset!(byte_index),
                    end: marker.start + mfm_offset!(byte_index + 4 + 512),
                    chsn: Some(chsn),
                });
            }
        }

        // Sort elements by start offset.
        //elements.sort_by(|a, b| a.start.cmp(&b.start));
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
            if let TrackMarker::Amiga(_) = marker.elem_type {
                if last_marker_index > 0 {
                    // Clear the clock bit immediately before marker to allow for syncing to this
                    // starting clock.
                    clock_map.set(last_marker_index - 1, false);

                    for bi in (last_marker_index..marker.start).step_by(2) {
                        clock_map.set(bi, true);
                        clock_map.set(bi + 1, false);
                        bit_set += 2;
                    }
                }
                last_marker_index = marker.start;
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

    pub(crate) fn checksum_u32(stream: &TrackDataStream, bit_index: usize, end: usize) -> u32 {
        //const MFM_DATA_MASK: u32 = 0x5555_5555;

        let mut checksum_16: u16 = 0;
        let bytes_requested = (end - bit_index) / 16;
        let dwords_request = bytes_requested / 4;

        log::trace!(
            "Performing Checksum on {} bytes, {} dwords from bit index {}",
            bytes_requested,
            dwords_request,
            bit_index
        );

        let clock_map = stream.clock_map();
        if !clock_map[bit_index % clock_map.len()] {
            log::warn!("Checksum start bit is not a clock bit!");
        }

        let mut data = vec![0; bytes_requested];

        stream.read_decoded_buf(&mut data, bit_index);
        //stream.seek(SeekFrom::Start(bit_index as u64)).unwrap();
        //stream.read_exact(&mut data).unwrap();
        // for chunk in data.chunks_exact(4) {
        //     checksum_32 ^= u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // }

        for chunk in data.chunks_exact(2) {
            checksum_16 ^= u16::from_be_bytes([chunk[0], chunk[1]]);
        }

        checksum_16 as u32
    }

    fn checksum_u16_buf(buf: &[u8]) -> u16 {
        let mut checksum: u16 = 0;
        for chunk in buf.chunks_exact(2) {
            checksum ^= u16::from_be_bytes([chunk[0], chunk[1]]);
        }
        checksum
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
                        );
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
                        );
                }
                _ => {}
            }
        }
        element_map
    }
}
