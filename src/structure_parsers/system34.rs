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

    src/structure_parser/system34.rs

    An implementation of DiskStructureParser for the IBM System 34 disk format.
    This was the standard disk format used on IBM PCs and compatibles.

*/
use crate::bitstream::mfm::{MfmDecoder, MFM_BYTE_LEN, MFM_MARKER_LEN};
use crate::bitstream::TrackDataStream;
use crate::chs::DiskChsn;
use crate::io::{Read, Seek, SeekFrom};
use crate::mfm_offset;
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureGenericElement, DiskStructureMarker, DiskStructureMarkerItem,
    DiskStructureMetadataItem, DiskStructureParser,
};
use crate::util::crc_ccitt;
use bit_vec::BitVec;
use std::fmt::{Display, Formatter};

pub const IAM_MARKER: u64 = 0x5224522452245552;
pub const IDAM_MARKER: u64 = 0x4489448944895554;
pub const DAM_MARKER: u64 = 0x4489448944895545;
pub const DDAM_MARKER: u64 = 0x4489448944895548;
pub const ANY_MARKER: u64 = 0x4489448944890000;
pub const MARKER_MASK: u64 = 0xFFFFFFFFFFFF0000;

#[derive(Copy, Clone, Debug)]
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
            0x5554 => Ok(System34Marker::Idam),
            0x5545 => Ok(System34Marker::Dam),
            0x554A => Ok(System34Marker::Ddam),
            _ => {
                log::error!("Invalid System34 marker: {:04X}", self);
                Err(())
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum System34Element {
    Gap1,
    Gap2,
    Gap3,
    Gap4a,
    Gap4b,
    Sync,
    Marker(System34Marker, Option<bool>),
    SectorHeader(bool),
    Data {
        address_crc: bool,
        data_crc: bool,
        deleted: bool,
    },
}

impl From<System34Element> for DiskStructureGenericElement {
    fn from(elem: System34Element) -> Self {
        match elem {
            System34Element::Gap1 => DiskStructureGenericElement::NoElement,
            System34Element::Gap2 => DiskStructureGenericElement::NoElement,
            System34Element::Gap3 => DiskStructureGenericElement::NoElement,
            System34Element::Gap4a => DiskStructureGenericElement::NoElement,
            System34Element::Gap4b => DiskStructureGenericElement::NoElement,
            System34Element::Sync => DiskStructureGenericElement::NoElement,
            System34Element::Marker(_, _) => DiskStructureGenericElement::Marker,
            System34Element::SectorHeader(true) => DiskStructureGenericElement::SectorHeader,
            System34Element::SectorHeader(false) => DiskStructureGenericElement::SectorBadHeader,
            System34Element::Data {
                address_crc,
                data_crc,
                deleted,
            } => match (address_crc && data_crc, deleted) {
                (true, false) => DiskStructureGenericElement::SectorData,
                (false, false) => DiskStructureGenericElement::SectorBadData,
                (true, true) => DiskStructureGenericElement::SectorDeletedData,
                (false, true) => DiskStructureGenericElement::SectorBadDeletedData,
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
            System34Element::Data { .. } => 0,
            System34Element::SectorHeader(_) => 0,
        }
    }

    pub fn is_sector(&self) -> bool {
        match self {
            System34Element::Marker(System34Marker::Dam, _) => true,
            _ => false,
        }
    }
}

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

pub struct System34Parser;

impl System34Parser {
    // System34 masks clock bits in the MFM encoding of address marks.
    // This is to help differentiate markers from data.
    const MFM_MARKER_CLOCK_MASK: u64 = 0x5555_5555_5555_FFFF;
    const MFM_MARKER_CLOCK: u64 = 0x0088_0088_0088_0000;
    #[inline]
    pub fn encode_marker(pattern: &[u8]) -> u64 {
        let marker = MfmDecoder::encode_marker(pattern);
        marker & Self::MFM_MARKER_CLOCK_MASK | Self::MFM_MARKER_CLOCK
    }
}

impl DiskStructureParser for System34Parser {
    /// Find the provided pattern of bytes within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// The bit offset of the pattern is returned if found, otherwise None.
    /// The pattern length is limited to 8 characters.
    fn find_data_pattern(track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize> {
        let mut buffer = [0u8; 8];
        let len = pattern.len().min(8);
        buffer[(8 - len)..8].copy_from_slice(&pattern[..len]);
        let pat = u64::from_be_bytes(buffer);

        let pat_mask = u64::MAX >> (8 * (8 - len));
        let mut shift_reg = 0;

        //log::trace!("Constructed pattern: {:016X?}, mask: {:016X}", pat, pat_mask);

        let mut bit_ct = 0;
        for bi in offset..track.len() {
            shift_reg = shift_reg << 1 | track[bi] as u64;
            if (bit_ct >= (len * 8)) && (shift_reg & pat_mask) == pat {
                //log::trace!("find_data_pattern: shift_reg: {:064b} pat: {:064b}", shift_reg, pat);
                //log::trace!("find_data_pattern: Found pattern at bit offset: {}", bi);
                return Some(bi - len * 8 + 1);
            }
            bit_ct += 1;
        }
        None
    }

    /// Find the next address marker in the track bitstream. The type of marker and its position in
    /// the bitstream is returned, or None.
    fn find_next_marker(track: &TrackDataStream, offset: usize) -> Option<(DiskStructureMarker, usize)> {
        if let TrackDataStream::Mfm(mfm_stream) = track {
            if let Some((index, marker_u16)) = mfm_stream.find_next_marker(ANY_MARKER, MARKER_MASK, offset) {
                if let Ok(marker) = marker_u16.try_into() {
                    return Some((DiskStructureMarker::System34(marker), index));
                }
            }
        }
        None
    }

    fn find_marker(track: &TrackDataStream, marker: DiskStructureMarker, offset: usize) -> Option<usize> {
        if let DiskStructureMarker::System34(marker) = marker {
            let marker_u64 = u64::from(marker);

            if let TrackDataStream::Mfm(mfm_stream) = track {
                //log::trace!("find_marker(): Searching for marker at offset: {}", offset);
                return mfm_stream.find_marker(marker_u64, offset);
            }
        }
        None
    }

    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize> {
        if let DiskStructureElement::System34(element) = element {
            use System34Element::*;

            let (marker, _pattern) = match element {
                Gap1 | Gap2 | Gap3 | Gap4a | Gap4b => (System34Parser::encode_marker(&[0x4E; 4]), &[0x4E; 4]),
                Sync => (MfmDecoder::encode_marker(&[0x00; 4]), &[0x00; 4]),
                _ => return None,
            };

            //let marker = System34Parser::encode_marker(pattern);
            log::trace!(
                "find_element(): Encoded element: {:?} as {:016X}/{:064b}",
                element,
                marker,
                marker
            );

            if let TrackDataStream::Mfm(mfm_stream) = track {
                log::trace!("find_element(): Searching for element at offset: {}", offset);
                //let mfm_offset = System34Parser::find_data_pattern(track, pattern, offset >> 1);
                let raw_offset = mfm_stream.find_marker(marker, offset);
                /*                if let Some(mfm_offset) = mfm_offset {
                    log::trace!(
                        "find_element(): Found element in decoded stream: {:?} at offset: {}",
                        element,
                        mfm_offset << 1
                    );

                    log::trace!(
                        "find_element(): marker_bits (encoded) {}",
                        mfm_stream.debug_marker(mfm_offset << 1)
                    );
                    log::trace!(
                        "find_element(): marker_bits (decoded) {}",
                        mfm_stream.debug_decode(mfm_offset)
                    );
                }*/

                if let Some(offset) = raw_offset {
                    log::trace!(
                        "find_element(): Found element in raw stream: {:?} at offset: {}, sync: {} debug: {}",
                        element,
                        offset,
                        offset & 1,
                        mfm_stream.debug_marker(offset)
                    );
                    return Some(offset);
                }
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
    fn scan_track_markers(track: &mut TrackDataStream) -> Vec<DiskStructureMarkerItem> {
        let mut bit_cursor: usize = 0;
        let mut markers = Vec::new();

        // Look for the IAM marker first - but it may not be present (ISO standard encoding does
        // not require it).

        // TODO: Potential optimization:
        //       It may be unnecessary to scan the entire track for the IAM. If it is not present
        //       within the first 10% of the track it is probably not there.

        if let Some(marker_offset) =
            System34Parser::find_marker(track, DiskStructureMarker::System34(System34Marker::Iam), bit_cursor)
        {
            log::trace!(
                "scan_track_markers(): Found IAM marker at bit offset: {}",
                marker_offset
            );
            markers.push(DiskStructureMarkerItem {
                elem_type: DiskStructureMarker::System34(System34Marker::Iam),
                start: marker_offset,
            });
            bit_cursor = marker_offset + 4 * MFM_BYTE_LEN;
        }

        while let Some((marker, marker_offset)) = System34Parser::find_next_marker(track, bit_cursor) {
            /*
            log::trace!(
                "scan_track_markers(): Found marker of type {:?} at bit offset: {}",
                marker,
                marker_offset
            );*/

            markers.push(DiskStructureMarkerItem {
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
    /// proper functioning of the Read and Seek traits on MfmDecoder.
    fn scan_track_metadata(
        track: &mut TrackDataStream,
        markers: Vec<DiskStructureMarkerItem>,
    ) -> Vec<DiskStructureMetadataItem> {
        let mut elements = Vec::new();
        let mut last_marker_opt: Option<System34Marker> = None;
        let mut last_sector_id = SectorId::default();

        let mut last_element_offset = 0;

        for marker in &markers {
            let element_offset = marker.start;

            if let DiskStructureMarker::System34(sys34_marker) = marker.elem_type {
                match (last_marker_opt, sys34_marker) {
                    (_, System34Marker::Idam) => {
                        let mut sector_header = [0; 8];

                        // TODO: Don't unwrap in a library unless provably safe.
                        //       Consider removing option return type from read_decoded_byte.
                        sector_header[0] = track.read_decoded_byte(marker.start + mfm_offset!(0)).unwrap();
                        sector_header[1] = track.read_decoded_byte(marker.start + mfm_offset!(1)).unwrap();
                        sector_header[2] = track.read_decoded_byte(marker.start + mfm_offset!(2)).unwrap();
                        sector_header[3] = track.read_decoded_byte(marker.start + mfm_offset!(3)).unwrap();

                        log::trace!("Idam marker read: {:02X?}", &sector_header[0..4]);
                        sector_header[4] = track.read_decoded_byte(marker.start + mfm_offset!(4)).unwrap(); // Cylinder
                        sector_header[5] = track.read_decoded_byte(marker.start + mfm_offset!(5)).unwrap(); // Head
                        sector_header[6] = track.read_decoded_byte(marker.start + mfm_offset!(6)).unwrap(); // Sector
                        sector_header[7] = track.read_decoded_byte(marker.start + mfm_offset!(7)).unwrap(); // Sector size (b)
                        let crc_byte0 = track.read_decoded_byte(marker.start + mfm_offset!(8)).unwrap_or(0xAA);
                        let crc_byte1 = track.read_decoded_byte(marker.start + mfm_offset!(9)).unwrap_or(0xAA);

                        let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                        let calculated_crc = crc_ccitt(&sector_header[0..8]);

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
                        let data_len = last_sector_id.sector_size_in_bytes() * MFM_BYTE_LEN;
                        let data_end = element_offset + MFM_MARKER_LEN + data_len;

                        let log_prefix = match sys34_marker {
                            System34Marker::Dam => "",
                            System34Marker::Ddam => "Deleted ",
                            _ => "UNKNOWN",
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
                        dam_header[0] = track.read_decoded_byte(marker.start + mfm_offset!(0)).unwrap();
                        dam_header[1] = track.read_decoded_byte(marker.start + mfm_offset!(1)).unwrap();
                        dam_header[2] = track.read_decoded_byte(marker.start + mfm_offset!(2)).unwrap();
                        dam_header[3] = track.read_decoded_byte(marker.start + mfm_offset!(3)).unwrap();

                        //log::trace!("dam header verify: {:02X?}", dam_header);

                        let crc_byte0 = track.read_decoded_byte(data_end).unwrap_or(0xAA);
                        let crc_byte1 = track.read_decoded_byte(data_end + mfm_offset!(1)).unwrap_or(0xAA);
                        let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                        let calculated_crc = System34Parser::crc16(track, element_offset, data_end);
                        log::trace!("Data CRC16: {:04X} Calculated: {:04X}", crc, calculated_crc);

                        let crc_correct = crc == calculated_crc;
                        if !crc_correct {
                            log::warn!("Data CRC error detected at offset: {}", element_offset);
                        }

                        // Push a Sector Header metadata item spanning from IDAM to DAM.
                        let data_metadata = DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::SectorHeader(
                                last_sector_id.crc_valid,
                            )),
                            start: last_element_offset,
                            end: element_offset,
                            chsn: None,
                            _crc: None,
                        };
                        elements.push(data_metadata);

                        let element = match sys34_marker {
                            System34Marker::Dam => System34Element::Data {
                                address_crc: last_sector_id.crc_valid,
                                data_crc: crc_correct,
                                deleted: false,
                            },
                            System34Marker::Ddam => System34Element::Data {
                                address_crc: last_sector_id.crc_valid,
                                data_crc: crc_correct,
                                deleted: true,
                            },
                            _ => unreachable!(),
                        };

                        let data_metadata = DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(element),
                            start: element_offset,
                            end: data_end,
                            chsn: Some(DiskChsn::new(
                                last_sector_id.c as u16,
                                last_sector_id.h,
                                last_sector_id.s,
                                last_sector_id.b,
                            )),
                            _crc: None,
                        };
                        elements.push(data_metadata);
                    }
                    _ => {}
                }

                if let Some(last_marker) = last_marker_opt {
                    let last_marker_offset = element_offset - 4 * MFM_BYTE_LEN;
                    let last_marker_metadata = DiskStructureMetadataItem {
                        elem_type: DiskStructureElement::System34(System34Element::Marker(last_marker, None)),
                        start: last_marker_offset,
                        end: element_offset,
                        chsn: Some(DiskChsn::new(
                            last_sector_id.c as u16,
                            last_sector_id.h,
                            last_sector_id.s,
                            last_sector_id.b,
                        )),
                        _crc: None,
                    };
                    elements.push(last_marker_metadata);
                }

                // Save the last element seen.
                last_element_offset = element_offset;
                last_marker_opt = Some(sys34_marker);
            }
        }

        // Sort elements by start offset.
        elements.sort_by(|a, b| a.start.cmp(&b.start));
        elements
    }

    /// Use the list of track markers to create a clock phase map for the track. This a requirement
    /// for the proper functioning of the Read and Seek traits on MfmDecoder. A clock phase map is
    /// basically a bit vector congruent to the stream bitvec that indicates whether the
    /// corresponding stream bit is a clock or data bit.
    fn create_clock_map(markers: &[DiskStructureMarkerItem], clock_map: &mut BitVec) {
        let mut last_marker_index: usize = 0;

        log::trace!("Creating clock map from {} markers...", markers.len());
        #[allow(unused)]
        let mut bit_set = 0;
        for marker in markers {
            if let DiskStructureMarker::System34(_) = marker.elem_type {
                let bit_index = marker.start;

                if last_marker_index > 0 {
                    // Write one 'data' bit immediately before marker to allow for syncing to this
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

    fn crc16(track: &mut TrackDataStream, bit_index: usize, end: usize) -> u16 {
        let bytes_requested = ((end - bit_index) >> 1) / 8;

        log::trace!(
            "Performing CRC on {} bytes from bit index {}",
            bytes_requested,
            bit_index
        );
        if let TrackDataStream::Mfm(mfm_stream) = track {
            let mut data = vec![0; bytes_requested];

            mfm_stream.seek(SeekFrom::Start((bit_index >> 1) as u64)).unwrap();
            mfm_stream.read_exact(&mut data).unwrap();
            log::trace!(
                "First 16 bytes of sector: {:02X?} len: {}",
                &data[..std::cmp::min(16, data.len())],
                data.len()
            );
            crc_ccitt(&data)
        } else {
            0
        }
    }
}
