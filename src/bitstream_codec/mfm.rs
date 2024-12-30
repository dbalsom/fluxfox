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

use std::ops::Index;

use crate::{
    bit_ring::BitRing,
    bitstream_codec::{EncodingVariant, MarkerEncoding, TrackCodec},
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom},
    range_check::RangeChecker,
    types::{TrackDataEncoding, TrackRegion},
};
use bit_vec::BitVec;

pub const MFM_BYTE_LEN: usize = 16;
pub const MFM_MARKER_LEN: usize = 64;
pub const MFM_MARKER_CLOCK: u64 = 0x0220_0220_0220_0000;

#[doc(hidden)]
#[macro_export]
macro_rules! mfm_offset {
    ($x:expr) => {
        $x * 16
    };
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MfmCodec {
    bits: BitRing,
    clock_map: BitRing,
    error_map: BitRing,
    weak_enabled: bool,
    weak_mask: BitRing,
    initial_phase: usize,
    bit_cursor: usize,
    track_padding: usize,
    data_ranges: RangeChecker,
    data_ranges_filtered: RangeChecker,
}

pub fn get_mfm_sync_offset(track: &BitVec) -> Option<bool> {
    match find_sync(track, 0) {
        Some(offset) => {
            if offset % 2 == 0 {
                Some(false)
            }
            else {
                Some(true)
            }
        }
        None => None,
    }
}

pub fn find_sync(track: &BitVec, start_idx: usize) -> Option<usize> {
    let mut shift_reg: u32 = 0;

    for (i, bit) in track.into_iter().skip(start_idx).enumerate() {
        shift_reg = shift_reg << 1 | (bit as u32);

        if i >= 32 && shift_reg == 0xAA_AA_AA_AA {
            return Some(i - 32);
        }
    }
    None
}

#[cfg_attr(feature = "serde", typetag::serde)]
impl TrackCodec for MfmCodec {
    fn encoding(&self) -> TrackDataEncoding {
        TrackDataEncoding::Mfm
    }

    fn len(&self) -> usize {
        self.bits.len()
    }

    fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    fn replace(&mut self, new_bits: BitVec) {
        self.bits = BitRing::from(new_bits);
    }

    fn data(&self) -> &BitVec {
        self.bits.bits()
    }

    fn data_mut(&mut self) -> &mut BitVec {
        self.bits.bits_mut()
    }

    fn data_copied(&self) -> Vec<u8> {
        self.bits.to_bytes()
    }

    fn set_clock_map(&mut self, clock_map: BitVec) {
        assert_eq!(clock_map.len(), self.bits.len());
        self.clock_map = BitRing::from(clock_map);
        // Set the wrap value for the clock map to false. This disables index adjustment when
        // we read across the track index.
    }

    fn clock_map(&self) -> &BitVec {
        self.clock_map.bits()
    }

    fn clock_map_mut(&mut self) -> &mut BitVec {
        self.clock_map.bits_mut()
    }

    fn enable_weak(&mut self, enable: bool) {
        self.weak_enabled = enable;
    }

    fn weak_mask(&self) -> &BitVec {
        self.weak_mask.bits()
    }

    fn weak_mask_mut(&mut self) -> &mut BitVec {
        self.weak_mask.bits_mut()
    }

    fn weak_data(&self) -> Vec<u8> {
        self.weak_mask.to_bytes()
    }

    fn set_weak_mask(&mut self, new: BitVec) {
        self.weak_mask = new.into();
    }

    fn has_weak_bits(&self) -> bool {
        !self.detect_weak_bits(6).0 > 0
    }

    fn error_map(&self) -> &BitVec {
        self.error_map.bits()
    }

    fn set_track_padding(&mut self) {
        let mut wrap_buffer: [u8; 4] = [0; 4];

        if self.bits.len() % 8 == 0 {
            // Track length was an even multiple of 8, so it is possible track data was padded to
            // byte margins.

            let found_pad = false;
            // Read buffer across the end of the track and see if all bytes are the same.
            for pad in 1..16 {
                log::trace!(
                    "bitcells: {} data bits: {} window_start: {}",
                    self.bits.len(),
                    self.bits.len() / 2,
                    self.bits.len() - (8 * 2)
                );
                let wrap_addr = (self.bits.len() / 2) - (8 * 2);

                self.track_padding = pad;

                self.seek(SeekFrom::Start(wrap_addr as u64)).unwrap();
                self.read_exact(&mut wrap_buffer).unwrap();

                log::trace!(
                    "set_track_padding(): wrap_buffer at {}, pad {}: {:02X?}",
                    wrap_addr,
                    pad,
                    wrap_buffer
                );
            }

            if !found_pad {
                // No padding found
                log::debug!("set_track_padding(): Unable to determine track padding",);
                self.track_padding = 0;
            }
        }
        else {
            // Track length is not an even multiple of 8 - the only explanation is that there is no
            // track padding.
            self.track_padding = 0;
        }
    }

    fn read_raw_u8(&self, index: usize) -> Option<u8> {
        let mut byte = 0;
        for bi in index..index + 8 {
            byte = (byte << 1) | self.bits[bi] as u8;
        }
        Some(byte)
    }

    fn read_raw_buf(&self, buf: &mut [u8], offset: usize) -> usize {
        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            *byte = self.read_raw_u8(offset + (bytes_read * 8)).unwrap();
            bytes_read += 1;
        }
        bytes_read
    }

    fn write_raw_u8(&mut self, index: usize, byte: u8) {
        for (i, bi) in (index..index + 8).enumerate() {
            self.bits.set(bi, (byte & 0x80 >> i) != 0);
        }
    }

    /// This is essentially a reimplementation of Read + Iterator that avoids mutation.
    /// This allows us to read track data through an immutable reference.
    fn read_decoded_u8(&self, index: usize) -> Option<u8> {
        let mut byte = 0;
        let mut cursor = index;

        // If we are not pointing to a clock bit, advance to the next clock bit.
        cursor += !self.clock_map[cursor] as usize;
        // Now that we are aligned to a clock bit, point to the next data bit
        cursor += 1;

        for _ in 0..8 {
            let decoded_bit = if self.weak_enabled && !self.weak_mask.is_empty() && self.weak_mask[cursor] {
                // Weak bits return random data
                rand::random()
            }
            else {
                self.bits[cursor]
            };
            byte = (byte << 1) | decoded_bit as u8;
            // Advance to next data bit.
            cursor += 2;
        }
        Some(byte)
    }

    fn read_decoded_u32_le(&self, index: usize) -> u32 {
        let mut dword = 0;
        let mut cursor = index;

        // If we are not pointing to a clock bit, advance to the next clock bit.
        cursor += !self.clock_map[cursor] as usize;
        // Now that we are aligned to a clock bit, point to the next data bit
        cursor += 1;

        for b in 0..4 {
            let mut byte = 0;
            for _ in 0..8 {
                let decoded_bit = if self.weak_enabled && !self.weak_mask.is_empty() && self.weak_mask[cursor] {
                    // Weak bits return random data
                    rand::random()
                }
                else {
                    self.bits[cursor]
                };
                byte = (byte << 1) | decoded_bit as u32;
                // Advance to next data bit.
                cursor += 2;
            }
            dword |= byte << (b * 8);
        }
        dword
    }

    fn read_decoded_u32_be(&self, index: usize) -> u32 {
        let mut dword = 0;
        let mut cursor = index;

        // If we are not pointing to a clock bit, advance to the next clock bit.
        cursor += !self.clock_map[cursor] as usize;
        // Now that we are aligned to a clock bit, point to the next data bit
        cursor += 1;

        for _ in 0..32 {
            let decoded_bit = if self.weak_enabled && !self.weak_mask.is_empty() && self.weak_mask[cursor] {
                // Weak bits return random data
                rand::random()
            }
            else {
                self.bits[cursor]
            };
            dword = (dword << 1) | decoded_bit as u32;
            // Advance to next data bit.
            cursor += 2;
        }
        dword
    }

    fn read_decoded_buf(&self, buf: &mut [u8], offset: usize) -> usize {
        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            *byte = self.read_decoded_u8(offset + (bytes_read * MFM_BYTE_LEN)).unwrap();
            bytes_read += 1;
        }
        bytes_read
    }

    fn write_encoded_buf(&mut self, buf: &[u8], offset: usize) -> usize {
        let mut offset = offset;
        let encoded_buf = self.encode(buf, false, EncodingVariant::Data);

        // let mut copy_len = encoded_buf.len();
        // if self.bits.len() < offset + encoded_buf.len() {
        //     copy_len = self.bits.len() - offset;
        // }

        let mut bits_written = 0;

        // If we landed on a data bit, advance to the next clock bit.
        // If the next bit is not a clock bit either, we are in an unsynchronized region, so don't
        // bother.
        if !self.clock_map[offset] && self.clock_map[offset + 1] {
            offset += 1;
        }

        for (i, bit) in encoded_buf.into_iter().enumerate() {
            self.bits.set(offset + i, bit);
            bits_written += 1;
        }

        (bits_written + 7) / 8
    }

    fn write_raw_buf(&mut self, buf: &[u8], offset: usize) -> usize {
        let mut bytes_written = 0;
        let mut offset = offset;
        for byte in buf {
            for bit_pos in 0..8 {
                self.bits.set(offset, byte & (0x80 >> bit_pos) != 0);
                offset += 1;
            }
            bytes_written += 1;
        }
        bytes_written
    }

    fn encode(&self, data: &[u8], prev_bit: bool, encoding_type: EncodingVariant) -> BitVec {
        let mut bitvec = BitVec::new();
        let mut bit_count = 0;

        for &byte in data {
            for i in 0..8 {
                //let bit = ;
                if (byte & (0x80 >> i)) != 0 {
                    // 1 is encoded as 01
                    bitvec.push(false);
                    bitvec.push(true);
                }
                else {
                    // 0 is encoded as 10 if previous bit was 0, otherwise 00
                    let previous_bit = if bitvec.is_empty() {
                        prev_bit
                    }
                    else {
                        bitvec[bitvec.len() - 1]
                    };

                    if previous_bit {
                        bitvec.push(false);
                    }
                    else {
                        bitvec.push(true);
                    }
                    bitvec.push(false);
                }

                bit_count += 1;

                // Omit clock bit between source bits 3 and 4 for address marks
                if let EncodingVariant::AddressMark = encoding_type {
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

    fn find_marker(&self, marker: &MarkerEncoding, start: usize, limit: Option<usize>) -> Option<(usize, u16)> {
        //log::debug!("Mfm::find_marker(): Searching for marker: {:016X}", marker);
        if self.bits.is_empty() {
            return None;
        }

        let mut shift_reg: u64 = 0;
        let mut shift_ct: u32 = 0;

        let search_limit = if let Some(provided_limit) = limit {
            std::cmp::min(provided_limit, self.bits.len())
        }
        else {
            self.bits.len()
        };

        for bi in start..search_limit {
            shift_reg = (shift_reg << 1) | self.bits[bi] as u64;
            shift_ct += 1;
            if shift_ct >= 64 && ((shift_reg & marker.mask) == marker.bits) {
                return Some(((bi - marker.len) + 1, (shift_reg & 0xFFFF) as u16));
            }
        }
        log::trace!("find_marker(): Failed to find marker!");
        None
    }

    fn set_data_ranges(&mut self, ranges: Vec<(usize, usize)>) {
        // Don't set ranges for overlapping sectors. This avoids visual discontinuities during
        // visualization.
        let filtered_ranges = ranges
            .iter()
            .filter(|(start, end)| !(*start >= self.bits.len() || *end >= self.bits.len()))
            .map(|(start, end)| (*start, *end))
            .collect::<Vec<(usize, usize)>>();

        self.data_ranges_filtered = RangeChecker::new(filtered_ranges);
        self.data_ranges = RangeChecker::new(ranges);
    }

    fn is_data(&self, index: usize, wrapping: bool) -> bool {
        if wrapping {
            self.data_ranges.contains(index)
        }
        else {
            self.data_ranges_filtered.contains(index)
        }
    }

    fn debug_marker(&self, index: usize) -> String {
        let mut shift_reg: u64 = 0;
        for bi in index..std::cmp::min(index + 64, self.bits.len()) {
            shift_reg = (shift_reg << 1) | self.bits[bi] as u64;
        }
        format!("{:16X}/{:064b}", shift_reg, shift_reg)
    }

    fn debug_decode(&self, index: usize) -> String {
        let mut shift_reg: u32 = 0;
        let start = index << 1;
        for bi in (start..std::cmp::min(start + 64, self.bits.len())).step_by(2) {
            shift_reg = (shift_reg << 1) | self.bits[self.initial_phase + bi] as u32;
        }
        format!("{:08X}/{:032b}", shift_reg, shift_reg)
    }
}

impl MfmCodec {
    pub const WEAK_BIT_RUN: usize = 6;

    pub fn new(mut bits: BitVec, bit_ct: Option<usize>, weak_mask: Option<BitVec>) -> Self {
        // If a bit count was provided, we can trim the bit vector to that length.
        if let Some(bit_ct) = bit_ct {
            bits.truncate(bit_ct);
        }

        let encoding_sync = get_mfm_sync_offset(&bits).unwrap_or(false);
        let sync = encoding_sync.into();

        let clock_map = BitVec::from_elem(bits.len(), encoding_sync);
        let weak_mask = match weak_mask {
            Some(mask) => mask,
            None => BitVec::from_elem(bits.len(), false),
        };

        if weak_mask.len() < bits.len() {
            panic!("MfmCodec::new(): Weak mask must be the same length as the bit vector");
        }

        let error_bits = MfmCodec::create_error_map(&bits);
        let error_bit_ct = error_bits.count_ones();

        if error_bit_ct > 16 {
            log::warn!("MfmCodec::new(): created error map with {} error bits", error_bit_ct);
        }
        let error_map = BitRing::from(error_bits);

        MfmCodec {
            bits: BitRing::from(bits),
            clock_map: BitRing::from(clock_map),
            error_map,
            weak_enabled: true,
            weak_mask: BitRing::from(weak_mask),
            initial_phase: sync,
            bit_cursor: sync,
            track_padding: 0,
            data_ranges: Default::default(),
            data_ranges_filtered: Default::default(),
        }
    }

    pub fn set_weak_mask(&mut self, weak_mask: BitVec) -> Result<()> {
        if weak_mask.len() != self.bits.len() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Weak mask must be the same length as the bit vector",
            ));
        }
        self.weak_mask = BitRing::from(weak_mask);

        Ok(())
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
                }
                else {
                    // 0 is encoded as 10 if previous bit was 0, otherwise 00
                    if !previous_bit {
                        accum = (accum << 2) | 0b10;
                    }
                    else {
                        accum <<= 2;
                    }
                }
                previous_bit = bit;
            }
        }
        accum
    }

    #[allow(dead_code)]
    fn read_bit(self) -> Option<bool> {
        if self.weak_enabled && self.weak_mask[self.bit_cursor] {
            // Weak bits return random data
            Some(rand::random())
        }
        else {
            Some(self.bits[self.bit_cursor])
        }
    }

    fn ref_bit_at(&self, index: usize) -> &bool {
        let p_off: usize = self.clock_map[index] as usize;
        if self.weak_enabled && self.weak_mask[p_off + (index << 1)] {
            // Weak bits return random data
            // TODO: precalculate random table and return reference to it.
            &self.bits[p_off + (index << 1)]
        }
        else {
            &self.bits[p_off + (index << 1)]
        }
    }

    pub(crate) fn detect_weak_bits(&self, run: usize) -> (usize, usize) {
        let mut region_ct = 0;
        let mut weak_bit_ct = 0;
        let mut zero_ct = 0;

        for bit in self.bits.iter_revolution() {
            if !bit {
                zero_ct += 1;
            }
            else {
                if zero_ct >= run {
                    region_ct += 1;
                }
                zero_ct = 0;
            }

            if zero_ct > 3 {
                weak_bit_ct += 1;
            }
        }

        (region_ct, weak_bit_ct)
    }

    #[allow(dead_code)]
    pub(crate) fn detect_weak_regions(&self, run: usize) -> Vec<TrackRegion> {
        let mut regions = Vec::new();

        let mut zero_ct = 0;
        let mut region_start = 0;
        for (i, bit) in self.bits.iter().enumerate() {
            if !bit {
                zero_ct += 1;
            }
            else {
                if zero_ct >= run {
                    regions.push(TrackRegion {
                        start: region_start,
                        end:   i - 1,
                    });
                }
                zero_ct = 0;
            }

            if zero_ct == run {
                region_start = i;
            }
        }

        regions
    }

    /// Not every format will have a separate weak bit mask, but that doesn't mean weak bits cannot
    /// be encoded. Formats can encode weak bits as a run of 4 or more zero bits. Here we detect
    /// such runs and extract them into a weak bit mask as a BitVec.
    pub(crate) fn create_weak_bit_mask(&self, run: usize) -> BitVec {
        let mut weak_bitvec = BitVec::with_capacity(self.bits.len());
        let mut zero_ct = 0;
        log::debug!("create_weak_bit_mask(): bits: {}", self.bits.len());
        for bit in self.bits.iter_revolution() {
            if !bit {
                zero_ct += 1;
            }
            else {
                zero_ct = 0;
            }

            if zero_ct > run {
                weak_bitvec.push(true);
            }
            else {
                weak_bitvec.push(false);
            }
        }

        log::warn!(
            "create_weak_bit_mask(): bits: {} weak: {}",
            self.bits.len(),
            weak_bitvec.len(),
        );
        assert_eq!(weak_bitvec.len(), self.bits.len());

        weak_bitvec
    }

    /// Create an error map that marks where MFM clock violations occur
    fn create_error_map(bits: &BitVec) -> BitVec {
        let mut error_bitvec = BitVec::with_capacity(bits.len());

        let mut zero_ct = 0;
        let mut in_bad_region = false;

        for bit in bits.iter() {
            if !bit {
                zero_ct += 1;
                if zero_ct > 3 {
                    in_bad_region = true;
                }
            }
            else {
                if zero_ct < 4 {
                    in_bad_region = false;
                }
                zero_ct = 0;
            }
            error_bitvec.push(in_bad_region);
        }

        error_bitvec
    }
}

impl Iterator for MfmCodec {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        // The bit cursor should always be aligned to a clock bit. If it is not, we can try to nudge
        // it to the next clock bit. If the next bit is also not a clock bit, we are in an
        // unsynchronized region and can't really do anything about it.
        if !self.clock_map[self.bit_cursor] && self.clock_map[self.bit_cursor + 1] {
            self.bit_cursor += 1;
            log::debug!("next(): nudging to next clock bit @ {:05X}", self.bit_cursor);
        }
        // Now that we are (hopefully) aligned to a clock bit, retrieve the next bit which should
        // be a data bit, or return a random bit if weak bits are enabled and the current bit is weak.
        let decoded_bit = if self.weak_enabled && self.weak_mask[self.bit_cursor + 1] {
            rand::random()
        }
        else {
            self.bits[self.bit_cursor + 1]
        };

        // Advance to the next clock bit.
        self.bit_cursor += 2;
        Some(decoded_bit)
    }
}

impl Seek for MfmCodec {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        if self.bits.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Cannot seek on an empty bitstream"));
        }

        let mut new_cursor = match pos {
            SeekFrom::Start(offset) => offset as usize,
            SeekFrom::End(offset) => self.bits.len().saturating_add_signed(offset as isize),
            SeekFrom::Current(offset) => self.bit_cursor.saturating_add_signed(offset as isize),
        };

        // If we have seeked to a data bit, nudge the bit cursor to the next clock bit.
        // Don't bother if the next bit isn't a clock bit either, as we're in some unsynchronized
        // track region.
        if !self.clock_map[new_cursor] && self.clock_map[new_cursor + 1] {
            new_cursor += 1;
            log::debug!("seek(): nudging to next clock bit @ {:05X}", new_cursor);
        }

        self.bit_cursor = new_cursor;
        Ok(self.bit_cursor as u64)
    }
}

impl Read for MfmCodec {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.bits.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Cannot read an empty bitstream"));
        }
        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            let mut byte_val = 0;
            for _ in 0..8 {
                if let Some(bit) = self.next() {
                    byte_val = (byte_val << 1) | bit as u8;
                }
                else {
                    break;
                }
            }
            *byte = byte_val;
            bytes_read += 1;
        }
        Ok(bytes_read)
    }
}

impl Index<usize> for MfmCodec {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.bits.len() {
            panic!("index out of bounds");
        }

        // Decode the bit here (implement the MFM decoding logic)
        self.ref_bit_at(index)
    }
}
