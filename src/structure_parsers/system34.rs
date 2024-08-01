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
use crate::diskimage::TrackDataStream;
use crate::io::{Read, Seek, SeekFrom};
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureMarker, DiskStructureMarkerItem, DiskStructureMetadata,
    DiskStructureMetadataItem, DiskStructureParser,
};
use crate::{mfm_offset, EncodingPhase};
use bit_vec::BitVec;
use std::fmt::{Display, Formatter};

pub const IAM_MARKER: u64 = 0x5224522452245552;
pub const IDAM_MARKER: u64 = 0x4489448944895554;
pub const DAM_MARKER: u64 = 0x4489448944895545;

#[derive(Copy, Clone, Debug)]
pub enum System34Marker {
    Iam,
    Idam,
    Dam,
}

impl From<System34Marker> for u64 {
    fn from(marker: System34Marker) -> u64 {
        match marker {
            System34Marker::Iam => IAM_MARKER,
            System34Marker::Idam => IDAM_MARKER,
            System34Marker::Dam => DAM_MARKER,
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
    Data(bool),
}

impl System34Element {
    pub fn len(&self) -> usize {
        match self {
            System34Element::Gap1 => 8,
            System34Element::Gap2 => 8,
            System34Element::Gap3 => 8,
            System34Element::Gap4a => 8,
            System34Element::Gap4b => 8,
            System34Element::Sync => 8,
            System34Element::Marker(_, _) => 4,
            System34Element::Data(_) => 0,
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

    fn find_marker(track: &TrackDataStream, marker: DiskStructureMarker, offset: usize) -> Option<usize> {
        if let DiskStructureMarker::System34(marker) = marker {
            let marker_u64 = u64::from(marker);

            if let TrackDataStream::Mfm(mfm_stream) = track {
                log::trace!("find_marker(): Searching for marker at offset: {}", offset);
                return mfm_stream.find_marker(marker_u64, offset);
            }
        }
        None
    }

    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize> {
        if let DiskStructureElement::System34(element) = element {
            use System34Element::*;

            let (marker, pattern) = match element {
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

    fn scan_track_markers(track: &mut TrackDataStream) -> Vec<DiskStructureMarkerItem> {
        let mut bit_cursor: usize = 0;
        let mut markers = Vec::new();
        let mut last_marker_opt: Option<System34Marker> = None;
        let mut last_marker_offset = 0;
        let mut marker = System34Marker::Iam;

        while let Some(marker_offset) =
            System34Parser::find_marker(track, DiskStructureMarker::System34(marker), bit_cursor)
        {
            log::trace!(
                "scan_track_markers(): Found marker: {:?} at bit offset: {}",
                marker,
                marker_offset
            );

            markers.push(DiskStructureMarkerItem {
                elem_type: DiskStructureMarker::System34(marker),
                start: marker_offset,
            });

            // Save the last element seen. We calculate the size of the element when we find the
            // next one.
            last_marker_offset = marker_offset;
            last_marker_opt = Some(marker);
            // Pick next element to look for.
            marker = match marker {
                System34Marker::Iam => System34Marker::Idam,
                System34Marker::Idam => System34Marker::Dam,
                System34Marker::Dam => System34Marker::Idam,
            };

            // Advance offset past element.
            bit_cursor = marker_offset + 4 * MFM_BYTE_LEN;
        }

        markers
    }

    fn scan_track_metadata(
        track: &mut TrackDataStream,
        markers: Vec<DiskStructureMarkerItem>,
    ) -> Vec<DiskStructureMetadataItem> {
        let mut bit_cursor: usize = 0;
        let mut elements = Vec::new();

        let mut last_marker_opt: Option<System34Marker> = None;
        let mut last_marker_offset = 0;

        let mut last_sector_id = SectorId::default();

        for marker in &markers {
            let element_offset = marker.start;

            if let DiskStructureMarker::System34(sys34_marker) = marker.elem_type {
                match (last_marker_opt, sys34_marker) {
                    (_, System34Marker::Idam) => {
                        let mut sector_header = vec![0; 8];

                        log::trace!("marker_debug: {}", track.debug_marker(marker.start));
                        sector_header[0] = track.read_encoded_byte(marker.start + mfm_offset!(0)).unwrap();
                        sector_header[1] = track.read_encoded_byte(marker.start + mfm_offset!(1)).unwrap();
                        sector_header[2] = track.read_encoded_byte(marker.start + mfm_offset!(2)).unwrap();
                        sector_header[3] = track.read_encoded_byte(marker.start + mfm_offset!(3)).unwrap();

                        log::trace!("Idam marker read: {:02X?}", &sector_header[0..4]);
                        sector_header[4] = track.read_encoded_byte(marker.start + mfm_offset!(4)).unwrap();
                        sector_header[5] = track.read_encoded_byte(marker.start + mfm_offset!(5)).unwrap();
                        sector_header[6] = track.read_encoded_byte(marker.start + mfm_offset!(6)).unwrap();
                        sector_header[7] = track.read_encoded_byte(marker.start + mfm_offset!(7)).unwrap();
                        let crc_byte0 = track.read_encoded_byte(marker.start + mfm_offset!(8)).unwrap_or(0xAA);
                        let crc_byte1 = track.read_encoded_byte(marker.start + mfm_offset!(9)).unwrap_or(0xAA);

                        let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                        let calculated_crc = crc_ccitt(&sector_header[0..8]);

                        let sector_id = SectorId {
                            c: sector_header[4],
                            h: sector_header[5],
                            s: sector_header[6],
                            b: sector_header[7],
                            crc,
                        };
                        log::trace!(
                            "Sector ID: {} Size: {} calculated CRC: {:04X}",
                            sector_id,
                            sector_id.sector_size_in_bytes(),
                            calculated_crc
                        );
                        last_sector_id = sector_id;
                    }
                    (Some(System34Marker::Idam), System34Marker::Dam) => {
                        let data_len = last_sector_id.sector_size_in_bytes() * MFM_BYTE_LEN;
                        let data_end = element_offset + MFM_MARKER_LEN + data_len;
                        log::trace!(
                            "Data marker at offset: {}, data size: {} crc_start:{} crc_end:{}",
                            element_offset,
                            data_len,
                            element_offset,
                            data_end
                        );

                        let mut dam_header = [0; 4];
                        dam_header[0] = track.read_encoded_byte(marker.start + mfm_offset!(0)).unwrap();
                        dam_header[1] = track.read_encoded_byte(marker.start + mfm_offset!(1)).unwrap();
                        dam_header[2] = track.read_encoded_byte(marker.start + mfm_offset!(2)).unwrap();
                        dam_header[3] = track.read_encoded_byte(marker.start + mfm_offset!(3)).unwrap();

                        log::trace!("dam header verify: {:02X?}", dam_header);

                        let crc_byte0 = track.read_encoded_byte(data_end).unwrap_or(0xAA);
                        let crc_byte1 = track.read_encoded_byte(data_end + mfm_offset!(1)).unwrap_or(0xAA);
                        let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                        let calculated_crc = System34Parser::crc16(track, element_offset, data_end);
                        log::trace!("Data CRC16: {:04X} Calculated: {:04X}", crc, calculated_crc);

                        let crc_correct = crc == calculated_crc;
                        if !crc_correct {
                            log::warn!("Data CRC error detected at offset: {}", element_offset);
                        }

                        let data_metadata = DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::Data(crc_correct)),
                            start: element_offset,
                            end: data_end,
                            crc: None,
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
                        crc: None,
                    };
                    elements.push(last_marker_metadata);
                }

                // Save the last element seen. We calculate the size of the element when we find the
                // next one.
                last_marker_offset = element_offset;
                last_marker_opt = Some(sys34_marker);
            }
        }

        elements
    }
    /*
    fn scan_track_metadata(
        track: &mut TrackDataStream,
        markers: Vec<DiskStructureMarkerItem>,
    ) -> Vec<DiskStructureMetadataItem> {
        let mut bit_cursor: usize = 0;
        let mut elements = Vec::new();

        let mut last_element_opt: Option<System34Element> = None;
        let mut last_element_offset = 0;
        let mut element = System34Element::Iam;

        let mut last_sector_id = Default::default();

        log::trace!("Scanning track...");
        while let Some(element_offset) =
            System34Parser::find_element(track, DiskStructureElement::System34(element), bit_cursor)
        {
            let phase = EncodingPhase::from(element_offset & 1 == 0);

            log::trace!(
                "scan_track_elements(): Found element: {:?} at offset: {}",
                element,
                element_offset
            );
            if let TrackDataStream::Mfm(mfm_stream) = track {
                log::trace!("scan_track_elements(): {}", mfm_stream.debug_marker(element_offset));
            }

            match (last_element_opt, element) {
                (_, System34Element::Idam) => {
                    let mut sector_header = vec![0; 8];

                    log::trace!("marker_debug: {}", track.debug_marker(element_offset));
                    sector_header[0] = track.read_encoded_byte(element_offset + mfm_offset!(0), phase).unwrap();
                    sector_header[1] = track.read_encoded_byte(element_offset + mfm_offset!(1), phase).unwrap();
                    sector_header[2] = track.read_encoded_byte(element_offset + mfm_offset!(2), phase).unwrap();
                    sector_header[3] = track.read_encoded_byte(element_offset + mfm_offset!(3), phase).unwrap();

                    log::trace!("Idam marker read: {:02X?}", &sector_header[0..4]);
                    sector_header[4] = track.read_encoded_byte(element_offset + mfm_offset!(4), phase).unwrap();
                    sector_header[5] = track.read_encoded_byte(element_offset + mfm_offset!(5), phase).unwrap();
                    sector_header[6] = track.read_encoded_byte(element_offset + mfm_offset!(6), phase).unwrap();
                    sector_header[7] = track.read_encoded_byte(element_offset + mfm_offset!(7), phase).unwrap();
                    let crc_byte0 = track
                        .read_encoded_byte(element_offset + mfm_offset!(8), phase)
                        .unwrap_or(0xAA);
                    let crc_byte1 = track
                        .read_encoded_byte(element_offset + mfm_offset!(9), phase)
                        .unwrap_or(0xAA);

                    let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                    let calculated_crc = crc_ccitt(&sector_header[0..8]);

                    let sector_id = SectorId {
                        c: sector_header[4],
                        h: sector_header[5],
                        s: sector_header[6],
                        b: sector_header[7],
                        crc,
                    };
                    log::trace!("Sector ID: {} calculated CRC: {:04X}", sector_id, calculated_crc);
                    last_sector_id = sector_id;
                }
                (Some(System34Element::Idam), System34Element::Dam) => {
                    let data_end = element_offset + 32 + last_sector_id.sector_size_in_bytes() * MFM_BYTE_LEN;

                    let crc_byte0 = track.read_byte(data_end).unwrap_or(0xAA);
                    let crc_byte1 = track.read_byte(data_end + 8).unwrap_or(0xAA);
                    let crc = u16::from_be_bytes([crc_byte0, crc_byte1]);
                    let calculated_crc = System34Parser::crc16(track, element_offset >> 1, data_end);
                    log::trace!("Data CRC16: {:04X} Calculated: {:04X}", crc, calculated_crc);

                    let crc_correct = crc == calculated_crc;

                    let data_metadata = DiskStructureMetadataItem {
                        elem_type: DiskStructureElement::System34(System34Element::Data(crc_correct)),
                        start: element_offset,
                        end: data_end,
                        crc: None,
                    };
                    elements.push(data_metadata);
                }
                _ => {}
            }

            if let Some(last_element) = last_element_opt {
                let last_element_offset = element_offset - last_element.len() * MFM_BYTE_LEN;
                let last_element_metadata = DiskStructureMetadataItem {
                    elem_type: DiskStructureElement::System34(last_element),
                    start: last_element_offset,
                    end: element_offset,
                    crc: None,
                };
                elements.push(last_element_metadata);
            }

            // Save the last element seen. We calculate the size of the element when we find the
            // next one.
            last_element_offset = element_offset;
            last_element_opt = Some(element);
            // Pick next element to look for.
            element = match element {
                System34Element::Iam => System34Element::Idam,
                System34Element::Idam => System34Element::Dam,
                System34Element::Dam => System34Element::Idam,
                _ => break,
            };

            // Advance offset past element.
            bit_cursor = element_offset + element.len() * MFM_BYTE_LEN;

            log::trace!("-----------------------------------------------------------------");
        }
        elements
    }*/

    fn create_clock_map(markers: &Vec<DiskStructureMarkerItem>, clock_map: &mut BitVec) {
        let mut last_marker_index: usize = 0;

        log::trace!("Creating clock map from {} markers...", markers.len());
        let mut bit_set = 0;
        for marker in markers {
            if let DiskStructureMarker::System34(_) = marker.elem_type {
                let bit_index = marker.start;

                log::trace!("marker clock phase: {}", bit_index & 1);
                if last_marker_index > 0 {
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
        for bi in (last_marker_index..(clock_map.len() - 1)).step_by(2) {
            clock_map.set(bi, true);
            clock_map.set(bi + 1, false);
            bit_set += 2;
        }

        log::warn!("Clock map set {} bits", bit_set);
    }

    /// Read `length` bytes from the sector containing the specified sector_id from a
    /// TrackBitStream. If Some value of sector_n is provided, the value of n must match as well
    /// for data to be returned. The `length` parameter allows data to be returned after the end
    /// of the sector, allowing reading into inter-sector gaps.
    fn read_sector(track: &TrackDataStream, sector_id: u8, sector_n: Option<u8>, length: usize) -> Option<Vec<u8>> {
        None
    }

    fn crc16(track: &mut TrackDataStream, bit_index: usize, end: usize) -> u16 {
        const POLY: u16 = 0x1021; // Polynomial x^16 + x^12 + x^5 + 1
        let mut crc: u16 = 0xFFFF;

        let bytes_requested = ((end - bit_index) >> 1) / 8;

        if let TrackDataStream::Mfm(mfm_stream) = track {
            let mut data = vec![0; bytes_requested];

            mfm_stream.seek(SeekFrom::Start((bit_index >> 1) as u64)).unwrap();
            mfm_stream.read_exact(&mut data).unwrap();
            //log::trace!(
            //    "First 16 bytes of sector: {:02X?} len: {}",
            //    &data[..std::cmp::min(16, data.len())],
            //    data.len()
            //);
            crc_ccitt(&data)
        } else {
            0
        }
    }
}

fn crc_ccitt(data: &[u8]) -> u16 {
    const POLY: u16 = 0x1021; // Polynomial x^16 + x^12 + x^5 + 1
    let mut crc: u16 = 0xFFFF;

    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ POLY;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
