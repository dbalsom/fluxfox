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
use crate::EncodingSync;
use bit_vec::BitVec;
use std::ops::Index;

pub struct MfmDecoder {
    bit_vec: BitVec,
    weak_mask: BitVec,
    sync: usize,
    bit_cursor: usize,
}

pub fn get_mfm_sync_offset(track: &BitVec) -> Option<EncodingSync> {
    match find_sync(track, 0) {
        Some(offset) => {
            if offset % 2 == 0 {
                Some(EncodingSync::Even)
            } else {
                Some(EncodingSync::Odd)
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
        let encoding_sync = get_mfm_sync_offset(&bit_vec).unwrap_or(EncodingSync::Even);
        let sync = encoding_sync.into();

        let weak_mask = match weak_mask {
            Some(mask) => mask,
            None => BitVec::from_elem(bit_vec.len(), false),
        };

        if weak_mask.len() < bit_vec.len() {
            panic!("Weak mask must be the same length as the bit vector");
        }

        MfmDecoder {
            bit_vec,
            weak_mask,
            sync,
            bit_cursor: sync,
        }
    }

    pub fn len(&self) -> usize {
        self.bit_vec.len() / 2
    }

    pub fn get_sync(&self) -> Option<EncodingSync> {
        match self.sync {
            0 => Some(EncodingSync::Even),
            _ => Some(EncodingSync::Odd),
        }
    }
    // Add other necessary methods here
}

impl Iterator for MfmDecoder {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bit_cursor >= self.bit_vec.len() {
            return None;
        }

        // TODO: For now we just skip clock bits. We should probably implement proper MFM decoding
        let decoded_bit = self.bit_vec[self.bit_cursor];
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

        let mut new_pos = base.checked_add(offset).ok_or(Error::new(
            ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowed position",
        ))?;

        // Force new_pos even if sync is even, odd if sync is odd
        match self.sync {
            0 => {
                new_pos = new_pos & !1;
            }
            _ => {
                if new_pos % 2 == 0 && new_pos as usize == self.bit_vec.len() {
                    return Err(Error::new(ErrorKind::InvalidInput, "invalid seek"));
                }
                new_pos = new_pos | 1;
            }
        }

        if new_pos < 0 || new_pos > self.bit_vec.len() as isize {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowed position",
            ));
        }

        self.bit_cursor = new_pos as usize;

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

impl MfmDecoder {
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
        if self.weak_mask[self.sync + (index << 1)] {
            // Weak bits return random data
            Some(rand::random())
        } else {
            Some(self.bit_vec[self.sync + (index << 1)])
        }
    }

    fn ref_bit_at(&self, index: usize) -> &bool {
        if self.weak_mask[self.sync + (index << 1)] {
            // Weak bits return random data
            // TODO: precalculate random table and return reference to it.
            &self.bit_vec[self.sync + (index << 1)]
        } else {
            &self.bit_vec[self.sync + (index << 1)]
        }
    }
}
