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

    src/mfm.rs

    Implements a wrapper around a BitVec to provide MFM encoding and decoding.

*/
use crate::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};
use crate::EncodingPhase;
use bit_vec::BitVec;
use std::ops::Index;

pub const MFM_BYTE_LEN: usize = 16;
pub const MFM_MARKER_LEN: usize = 64;

#[macro_export]
macro_rules! mfm_offset {
    ($x:expr) => {
        $x * 16
    };
}

#[derive(Debug)]
pub struct MfmDecoder {
    bit_vec: BitVec,
    clock_map: BitVec,
    weak_mask: BitVec,
    initial_phase: usize,
    bit_cursor: usize,
}

pub enum MfmEncodingType {
    Data,
    AddressMark,
}

pub fn encode_mfm(data: &[u8], encoding_type: MfmEncodingType) -> BitVec {
    let mut bitvec = BitVec::new();
    let mut bit_count = 0;

    // Add initial zero bit
    bitvec.push(false);

    for &byte in data {
        for i in (0..8).rev() {
            let bit = (byte & (1 << i)) != 0;
            if bit {
                // 1 is encoded as 01
                bitvec.push(false);
                bitvec.push(true);
            } else {
                // 0 is encoded as 10 if previous bit was 0, otherwise 00
                if !bitvec.is_empty() && !bitvec[bitvec.len() - 1] {
                    bitvec.push(true);
                } else {
                    bitvec.push(false);
                }
                bitvec.push(false);
            }

            bit_count += 1;

            // Omit clock bit between source bits 3 and 4 for address marks
            if let MfmEncodingType::AddressMark = encoding_type {
                if bit_count == 4 {
                    // Clear the previous clock bit (which is between bit 3 and 4)
                    bitvec.set(bitvec.len() - 2, false);
                }
            }
        }

        // Reset bit_count for the next byte
        bit_count = 0;
    }

    bitvec
}

pub fn get_mfm_sync_offset(track: &BitVec) -> Option<EncodingPhase> {
    match find_sync(track, 0) {
        Some(offset) => {
            if offset % 2 == 0 {
                Some(EncodingPhase::Even)
            } else {
                Some(EncodingPhase::Odd)
            }
        }
        None => None,
    }
}

pub fn find_sync(track: &BitVec, start_idx: usize) -> Option<usize> {
    let mut shift_reg: u32 = 0;

    for (i, bit) in track.into_iter().skip(start_idx).enumerate() {
        shift_reg = shift_reg << 1 | (bit as u32);

        if shift_reg == 0xAA_AA_AA_AA {
            return Some(i - 32);
        }
    }
    None
}

impl MfmDecoder {
    pub fn new(bit_vec: BitVec, weak_mask: Option<BitVec>) -> Self {
        let encoding_sync = get_mfm_sync_offset(&bit_vec).unwrap_or(EncodingPhase::Even);
        let sync = encoding_sync.into();

        let clock_map = BitVec::from_elem(bit_vec.len(), encoding_sync.into());
        let weak_mask = match weak_mask {
            Some(mask) => mask,
            None => BitVec::from_elem(bit_vec.len(), false),
        };

        if weak_mask.len() < bit_vec.len() {
            panic!("Weak mask must be the same length as the bit vector");
        }

        MfmDecoder {
            bit_vec,
            clock_map,
            weak_mask,
            initial_phase: sync,
            bit_cursor: sync,
        }
    }

    pub fn len(&self) -> usize {
        self.bit_vec.len()
    }

    pub fn data(&self) -> Vec<u8> {
        self.bit_vec.to_bytes()
    }

    pub fn get_sync(&self) -> Option<EncodingPhase> {
        match self.initial_phase {
            0 => Some(EncodingPhase::Even),
            _ => Some(EncodingPhase::Odd),
        }
    }

    pub fn set_clock_map(&mut self, clock_map: BitVec) {
        self.clock_map = clock_map;
    }

    pub fn clock_map_mut(&mut self) -> &mut BitVec {
        &mut self.clock_map
    }

    /// Encode an MFM address mark.
    /// `data` must be a 4-byte slice.
    /// Returns the encoded value in a u64 suitable for comparison to a shift register used to search
    /// a BitVec.
    pub fn encode_marker(data: &[u8]) -> u64 {
        assert_eq!(data.len(), 4);

        let mut accum: u64 = 0;
        // A mark is always preceded by a SYNC block of 0's, so we know the previous bit will always
        // be 0.
        let mut previous_bit = false;

        for &byte in data {
            for i in (0..8).rev() {
                let bit = (byte & (1 << i)) != 0;
                if bit {
                    // 1 is encoded as 01
                    accum = (accum << 2) | 0b01;
                } else {
                    // 0 is encoded as 10 if previous bit was 0, otherwise 00
                    if !previous_bit {
                        accum = (accum << 2) | 0b10;
                    } else {
                        accum <<= 2;
                    }
                }
                previous_bit = bit;
            }
        }
        accum
    }

    pub fn find_next_marker(&self, marker: u64, mask: u64, start: usize) -> Option<(usize, u16)> {
        let mut shift_reg: u64 = 0;
        let mut shift_ct: u32 = 0;

        for bi in start..self.bit_vec.len() {
            shift_reg = (shift_reg << 1) | self.bit_vec[bi] as u64;
            shift_ct += 1;

            /*            if (bi == 2528 + 64) {
                log::trace!("find_marker(): {:016X}, {:016X}", shift_reg, marker);
                log::trace!("find_marker(): debug_marker(): {}", self.debug_marker(bi - 64));
                log::trace!("find_marker(): binary diff: {:064b}", shift_reg ^ marker);
            }*/

            if shift_ct >= 64 && ((shift_reg & mask) == marker) {
                return Some(((bi - 64) + 1, (shift_reg & 0xFFFF) as u16));
            }
        }
        log::trace!("find_next_marker(): Failed to find marker!");
        None
    }

    pub fn find_marker(&self, marker: u64, start: usize) -> Option<usize> {
        let mut shift_reg: u64 = 0;
        let mut shift_ct: u32 = 0;
        for bi in start..self.bit_vec.len() {
            shift_reg = (shift_reg << 1) | self.bit_vec[bi] as u64;
            shift_ct += 1;

            /*            if (bi == 2528 + 64) {
                log::trace!("find_marker(): {:016X}, {:016X}", shift_reg, marker);
                log::trace!("find_marker(): debug_marker(): {}", self.debug_marker(bi - 64));
                log::trace!("find_marker(): binary diff: {:064b}", shift_reg ^ marker);
            }*/

            if shift_ct >= 64 && (shift_reg == marker) {
                return Some((bi - 64) + 1);
            }
        }
        log::trace!("find_marker(): Failed to find marker!");
        None
    }

    pub fn debug_marker(&self, index: usize) -> String {
        let mut shift_reg: u64 = 0;
        for bi in index..std::cmp::min(index + 64, self.bit_vec.len()) {
            shift_reg = (shift_reg << 1) | self.bit_vec[bi] as u64;
        }
        format!("{:16X}/{:064b}", shift_reg, shift_reg)
    }

    pub fn debug_decode(&self, index: usize) -> String {
        let mut shift_reg: u32 = 0;
        let start = index << 1;
        for bi in (start..std::cmp::min(start + 64, self.bit_vec.len())).step_by(2) {
            shift_reg = (shift_reg << 1) | self.bit_vec[self.initial_phase + bi] as u32;
        }
        format!("{:08X}/{:032b}", shift_reg, shift_reg)
    }

    #[allow(dead_code)]
    fn read_bit(self) -> Option<bool> {
        if self.weak_mask[self.bit_cursor] {
            // Weak bits return random data
            Some(rand::random())
        } else {
            Some(self.bit_vec[self.bit_cursor])
        }
    }
    #[allow(dead_code)]
    fn read_bit_at(&self, index: usize) -> Option<bool> {
        if self.weak_mask[self.initial_phase + (index << 1)] {
            // Weak bits return random data
            Some(rand::random())
        } else {
            Some(self.bit_vec[self.initial_phase + (index << 1)])
        }
    }
    pub fn read_byte(&self, index: usize) -> Option<u8> {
        if index >= self.len() {
            return None;
        }

        let mut byte_val = 0;
        for i in 0..8 {
            byte_val = (byte_val << 1)
                //| if self.bit_vec[self.sync + ((index + i) << 1)] {
                | if *self.ref_bit_at(index + i) {
                1
            } else {
                0
            };
        }
        Some(byte_val)
    }

    pub fn read_decoded_byte(&self, index: usize) -> Option<u8> {
        if index >= self.bit_vec.len() || index >= self.clock_map.len() {
            log::error!(
                "read_encoded_byte(): index out of bounds: {} vec: {} clock_map:{}",
                index,
                self.bit_vec.len(),
                self.clock_map.len()
            );
            return None;
        }
        let p_off: usize = self.clock_map[index] as usize;
        let mut byte = 0;
        for bi in (index..std::cmp::min(index + MFM_BYTE_LEN, self.bit_vec.len()))
            .skip(p_off)
            .step_by(2)
        {
            byte = (byte << 1) | self.bit_vec[bi] as u8;
        }
        Some(byte)
    }

    fn ref_bit_at(&self, index: usize) -> &bool {
        let p_off: usize = self.clock_map[index] as usize;
        if self.weak_mask[p_off + (index << 1)] {
            // Weak bits return random data
            // TODO: precalculate random table and return reference to it.
            &self.bit_vec[p_off + (index << 1)]
        } else {
            &self.bit_vec[p_off + (index << 1)]
        }
    }
}

impl Iterator for MfmDecoder {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bit_cursor >= (self.bit_vec.len() - 1) {
            return None;
        }

        // The bit cursor should always be aligned to a clock bit.
        // So retrieve the next bit which is the data bit, then point to the next clock.
        let decoded_bit = self.bit_vec[self.bit_cursor + 1];
        self.bit_cursor += 2;
        Some(decoded_bit)
    }
}

impl Seek for MfmDecoder {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let (base, offset) = match pos {
            // TODO: avoid casting to isize
            SeekFrom::Start(offset) => (0, offset as isize),
            SeekFrom::End(offset) => (self.bit_vec.len() as isize, offset as isize),
            SeekFrom::Current(offset) => (self.bit_cursor as isize, offset as isize),
        };

        let new_pos = base.checked_add(offset).ok_or(Error::new(
            ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowed position",
        ))?;

        let mut new_cursor = (new_pos as usize) << 1;

        let mut debug_vec = Vec::new();
        for i in 0..5 {
            debug_vec.push(self.clock_map[new_cursor - 2 + i]);
        }
        /*
        log::debug!(
            "seek() clock_map[{}]: {} {:?}",
            new_cursor,
            self.clock_map[new_cursor],
            debug_vec
        );
        */

        // If we have seeked to a data bit, nudge the bit cursor to the next clock bit.
        if !self.clock_map[new_cursor] {
            //log::trace!("seek(): nudging to next clock bit");
            new_cursor += 1;
        }

        if new_cursor > self.bit_vec.len() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowed position",
            ));
        }

        self.bit_cursor = new_cursor;
        log::trace!("seek(): new_pos: {}", self.bit_cursor);

        Ok(self.bit_cursor as u64)
    }
}

impl Read for MfmDecoder {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            let mut byte_val = 0;
            for _ in 0..8 {
                if let Some(bit) = self.next() {
                    byte_val = (byte_val << 1) | bit as u8;
                } else {
                    break;
                }
            }
            *byte = byte_val;
            bytes_read += 1;
        }
        Ok(bytes_read)
    }
}

impl Index<usize> for MfmDecoder {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.bit_vec.len() {
            panic!("index out of bounds");
        }

        // Decode the bit here (implement the MFM decoding logic)
        self.ref_bit_at(index)
    }
}
