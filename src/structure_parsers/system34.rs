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
use crate::diskimage::TrackDataStream;
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem, DiskStructureParser,
};
use std::fmt::{Display, Formatter};

#[derive(Copy, Clone, Debug)]
pub enum System34Element {
    Gap1,
    Gap2,
    Gap3,
    Gap4a,
    Gap4b,
    Sync,
    Iam,
    Idam,
    Dam,
    Data,
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
            System34Element::Iam => 4,
            System34Element::Idam => 4,
            System34Element::Dam => 4,
            System34Element::Data => 0,
        }
    }
}

#[derive(Default)]
pub struct SectorId {
    pub c: u8,
    pub h: u8,
    pub s: u8,
    pub b: u8,
    pub crc: u8,
}

impl SectorId {
    pub fn sector_size_in_bytes(&self) -> usize {
        128 << self.b
    }
}

impl Display for SectorId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "[C: {} H: {} S: {} B: {} CRC: {}]",
            self.c, self.h, self.s, self.b, self.crc
        )
    }
}

pub struct System34Parser;

impl DiskStructureParser for System34Parser {
    /// Find the provided pattern of bytes within the specified bitstream, starting at `offset` bits
    /// into the track.
    /// The bit offset of the pattern is returned if found, otherwise None.
    /// The pattern length is limited to 8 characters.
    fn find_pattern(track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize> {
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
                return Some(bi - len * 8 + 1);
            }
            bit_ct += 1;
        }
        None
    }

    fn find_element(track: &TrackDataStream, element: DiskStructureElement, offset: usize) -> Option<usize> {
        if let DiskStructureElement::System34(element) = element {
            use System34Element::*;

            let pattern: &[u8] = match element {
                Gap1 | Gap2 | Gap3 | Gap4a | Gap4b => &[0x4E; 8],
                Sync => &[0x00; 8],
                Iam => &[0xC2, 0xC2, 0xC2, 0xFC],
                Idam => &[0xA1, 0xA1, 0xA1, 0xFE],
                Dam => &[0xA1, 0xA1, 0xA1, 0xFB],
                _ => return None,
            };

            //log::trace!("Searching for pattern: {:02X?} at offset {}", pattern, offset);
            return System34Parser::find_pattern(track, pattern, offset);
        };

        None
    }

    fn scan_track_elements(track: &TrackDataStream) -> Vec<DiskStructureMetadataItem> {
        let mut bit_cursor: usize = 0;
        let mut elements = Vec::new();

        let mut last_element_opt: Option<System34Element> = None;
        let mut last_element_offset = 0;
        let mut element = System34Element::Iam;

        let mut last_sector_id = Default::default();

        while let Some(element_offset) =
            System34Parser::find_element(track, DiskStructureElement::System34(element), bit_cursor)
        {
            log::trace!("Found element: {:?} at offset: {}", element, element_offset);

            match (last_element_opt, element) {
                (_, System34Element::Idam) => {
                    /*
                    let mut idam_marker = [0; 4];

                    idam_marker[0] = track.read_byte(element_offset).unwrap();
                    idam_marker[1] = track.read_byte(element_offset + 8).unwrap();
                    idam_marker[2] = track.read_byte(element_offset + 16).unwrap();
                    idam_marker[3] = track.read_byte(element_offset + 24).unwrap();

                    log::trace!("Idam marker read: {:02X?}", idam_marker);*/

                    let cylinder_id = track.read_byte(element_offset + 4 * 8).unwrap();
                    let head_id = track.read_byte(element_offset + 5 * 8).unwrap();
                    let sector_id = track.read_byte(element_offset + 6 * 8).unwrap();
                    let sector_size = track.read_byte(element_offset + 7 * 8).unwrap();
                    let crc = track.read_byte(element_offset + 8 * 8).unwrap();

                    let sector_id = SectorId {
                        c: cylinder_id,
                        h: head_id,
                        s: sector_id,
                        b: sector_size,
                        crc,
                    };
                    log::trace!("Sector ID: {}", sector_id);
                    last_sector_id = sector_id;
                }
                (Some(System34Element::Idam), System34Element::Dam) => {
                    let data_metadata = DiskStructureMetadataItem {
                        elem_type: DiskStructureElement::System34(System34Element::Data),
                        start: element_offset,
                        end: element_offset + last_sector_id.sector_size_in_bytes() * 8,
                        crc: None,
                    };
                    elements.push(data_metadata);
                }
                _ => {}
            }

            if let Some(last_element) = last_element_opt {
                let last_element_offset = element_offset - last_element.len() * 8;
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
            bit_cursor = element_offset + element.len() * 8;
        }
        elements
    }

    /// Read `length` bytes from the sector containing the specified sector_id from a
    /// TrackBitStream. If Some value of sector_n is provided, the value of n must match as well
    /// for data to be returned. The `length` parameter allows data to be returned after the end
    /// of the sector, allowing reading into inter-sector gaps.
    fn read_sector(track: &TrackDataStream, sector_id: u8, sector_n: Option<u8>, length: usize) -> Option<Vec<u8>> {
        None
    }
}
