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

    src/fm.rs

    Implements a wrapper around a BitVec to provide FM encoding and decoding.

*/
use crate::bitstream::{EncodingVariant, TrackCodec, TrackDataStreamT};
use crate::diskimage::TrackRegion;
use crate::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};
use crate::range_check::RangeChecker;
use crate::{DiskDataEncoding, EncodingPhase};
use bit_vec::BitVec;
use std::ops::Index;

pub const FM_BYTE_LEN: usize = 16;
pub const FM_MARKER_LEN: usize = 64;

//pub const FM_MARKER_CLOCK_MASK: u64 = 0xAAAA_AAAA_AAAA_0000;
pub const FM_MARKER_DATA_MASK: u64 = 0x0000_0000_0000_5555;

pub const FM_MARKER_CLOCK_MASK: u64 = 0xAAAA_AAAA_AAAA_AAAA;
pub const FM_MARKER_CLOCK_PATTERN: u64 = 0xAAAA_AAAA_AAAA_A02A;

#[macro_export]
macro_rules! fm_offset {
    ($x:expr) => {
        $x * 16
    };
}

pub struct FmCodec {
    bit_vec: BitVec,
    clock_map: BitVec,
    weak_enabled: bool,
    weak_mask: BitVec,
    initial_phase: usize,
    bit_cursor: usize,
    track_padding: usize,
    data_ranges: RangeChecker,
}

pub enum FmEncodingType {
    Data,
    AddressMark,
}

pub fn get_fm_sync_offset(track: &BitVec) -> Option<EncodingPhase> {
    match find_sync(track, 0) {
        Some(offset) => {
            if offset % 2 == 0 {
                Some(EncodingPhase::Even)
            }
            else {
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

        if i >= 32 && shift_reg == 0xAA_AA_AA_AA {
            return Some(i - 32);
        }
    }
    None
}

impl TrackCodec for FmCodec {
    fn encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Fm
    }

    fn len(&self) -> usize {
        self.bit_vec.len()
    }

    fn is_empty(&self) -> bool {
        self.bit_vec.is_empty()
    }

    fn replace(&mut self, new_bits: BitVec) {
        self.bit_vec = new_bits;
    }

    fn data(&self) -> Vec<u8> {
        self.bit_vec.to_bytes()
    }

    fn set_clock_map(&mut self, clock_map: BitVec) {
        self.clock_map = clock_map;
    }

    fn clock_map(&self) -> &BitVec {
        &self.clock_map
    }

    fn clock_map_mut(&mut self) -> &mut BitVec {
        &mut self.clock_map
    }

    fn get_sync(&self) -> Option<EncodingPhase> {
        match self.initial_phase {
            0 => Some(EncodingPhase::Even),
            _ => Some(EncodingPhase::Odd),
        }
    }

    fn enable_weak(&mut self, enable: bool) {
        self.weak_enabled = enable;
    }

    fn weak_mask(&self) -> &BitVec {
        &self.weak_mask
    }

    fn has_weak_bits(&self) -> bool {
        !self.detect_weak_bits(6).0 > 0
    }

    fn weak_data(&self) -> Vec<u8> {
        self.weak_mask.to_bytes()
    }

    fn set_track_padding(&mut self) {
        let mut wrap_buffer: [u8; 4] = [0; 4];

        if self.bit_vec.len() % 8 == 0 {
            // Track length was an even multiple of 8, so it is possible track data was padded to
            // byte margins.

            let found_pad = false;
            // Read buffer across the end of the track and see if all bytes are the same.
            for pad in 1..16 {
                log::trace!(
                    "bitcells: {} data bits: {} window_start: {}",
                    self.bit_vec.len(),
                    self.bit_vec.len() / 2,
                    self.bit_vec.len() - (8 * 2)
                );
                let wrap_addr = (self.bit_vec.len() / 2) - (8 * 2);

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
                log::warn!("set_track_padding(): Unable to determine track padding.");
                self.track_padding = 0;
            }
        }
        else {
            // Track length is not an even multiple of 8 - the only explanation is that there is no
            // track padding.
            self.track_padding = 0;
        }
    }

    fn read_raw_byte(&self, index: usize) -> Option<u8> {
        if index >= self.len() {
            return None;
        }

        let mut byte = 0;
        for bi in index..std::cmp::min(index + 8, self.bit_vec.len()) {
            byte = (byte << 1) | self.bit_vec[bi] as u8;
        }
        Some(byte)
    }

    fn read_decoded_byte(&self, index: usize) -> Option<u8> {
        if index >= self.bit_vec.len() || index >= self.clock_map.len() {
            log::error!(
                "read_decoded_byte(): index out of bounds: {} vec: {} clock_map:{}",
                index,
                self.bit_vec.len(),
                self.clock_map.len()
            );
            return None;
        }
        let p_off: usize = self.clock_map[index] as usize;
        let mut byte = 0;
        for bi in (index..std::cmp::min(index + FM_BYTE_LEN, self.bit_vec.len()))
            .skip(p_off)
            .step_by(2)
        {
            byte = (byte << 1) | self.bit_vec[bi] as u8;
        }
        Some(byte)
    }

    fn read_decoded_byte2(&self, index: usize) -> Option<u8> {
        if index >= self.bit_vec.len() || index >= self.clock_map.len() {
            log::error!(
                "read_decoded_byte(): index out of bounds: {} vec: {} clock_map:{}",
                index,
                self.bit_vec.len(),
                self.clock_map.len()
            );
            return None;
        }
        let p_off: usize = self.clock_map[index] as usize;
        let mut byte = 0;
        for bi in (index..std::cmp::min(index + FM_BYTE_LEN, self.bit_vec.len()))
            .skip(p_off)
            .step_by(2)
        {
            byte = (byte << 1) | self.bit_vec[bi] as u8;
        }
        Some(byte)
    }

    fn write_buf(&mut self, buf: &[u8], offset: usize) -> Option<usize> {
        let encoded_buf = Self::encode(buf, false, EncodingVariant::Data);

        let mut copy_len = encoded_buf.len();
        if self.bit_vec.len() < offset + encoded_buf.len() {
            copy_len = self.bit_vec.len() - offset;
        }

        let mut bits_written = 0;

        let phase = !self.clock_map[offset] as usize;
        println!("write_buf(): offset: {} phase: {}", offset, phase);

        for (i, bit) in encoded_buf.into_iter().enumerate().take(copy_len) {
            self.bit_vec.set(offset + phase + i, bit);
            bits_written += 1;
        }

        let bytes_written = bits_written + 7 / 8;
        Some(bytes_written)
    }

    fn write_raw_buf(&mut self, buf: &[u8], offset: usize) -> usize {
        let mut bytes_written = 0;
        let mut offset = offset;

        for byte in buf {
            for bit_pos in (0..8).rev() {
                let bit = byte & (0x01 << bit_pos) != 0;
                self.bit_vec.set(offset, bit);
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
            for i in (0..8).rev() {
                let bit = (byte & (1 << i)) != 0;
                if bit {
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

    fn find_marker(&self, marker: u64, mask: Option<u64>, start: usize, limit: Option<usize>) -> Option<(usize, u16)> {
        //log::debug!("Fm::find_marker(): Searching for marker {:016X} at {}", marker, start);
        if self.bit_vec.is_empty() {
            return None;
        }

        let mask = mask.unwrap_or(!0);

        let mut shift_reg: u64 = 0;
        let mut shift_ct: u32 = 0;

        let search_limit = if let Some(provided_limit) = limit {
            std::cmp::min(provided_limit, self.bit_vec.len())
        }
        else {
            self.bit_vec.len()
        };

        for bi in start..search_limit {
            shift_reg = (shift_reg << 1) | self.bit_vec[bi] as u64;
            shift_ct += 1;

            let have_marker = (shift_reg & FM_MARKER_CLOCK_MASK) == FM_MARKER_CLOCK_PATTERN;
            let have_data = (shift_reg & FM_MARKER_DATA_MASK & mask) == marker & FM_MARKER_DATA_MASK & mask;

            if shift_ct >= 64 && have_marker {
                log::debug!(
                    "found marker clock at {}: Shift reg {:16X}: data: {:16X} mask: {:16X}, marker data: {:16X}",
                    bi - 64,
                    shift_reg & FM_MARKER_CLOCK_MASK,
                    shift_reg & FM_MARKER_DATA_MASK,
                    mask,
                    marker & FM_MARKER_DATA_MASK
                );
            }

            if shift_ct >= 64 && have_marker && have_data {
                log::debug!(
                    "Fm::find_marker(): Found marker at {} data match: {}",
                    bi - 64,
                    have_data
                );
                return Some(((bi - 64) + 1, (shift_reg & 0xFFFF) as u16));
            }
        }
        log::debug!("Fm::find_marker(): Failed to find marker!");
        None
    }

    fn set_data_ranges(&mut self, ranges: Vec<(usize, usize)>) {
        self.data_ranges = RangeChecker::new(ranges);
    }

    fn is_data(&self, index: usize) -> bool {
        self.data_ranges.contains(index)
    }

    fn debug_marker(&self, index: usize) -> String {
        let mut shift_reg: u64 = 0;
        for bi in index..std::cmp::min(index + 64, self.bit_vec.len()) {
            shift_reg = (shift_reg << 1) | self.bit_vec[bi] as u64;
        }
        format!("{:16X}/{:064b}", shift_reg, shift_reg)
    }

    fn debug_decode(&self, index: usize) -> String {
        let mut shift_reg: u32 = 0;
        let start = index << 1;
        for bi in (start..std::cmp::min(start + 64, self.bit_vec.len())).step_by(2) {
            shift_reg = (shift_reg << 1) | self.bit_vec[self.initial_phase + bi] as u32;
        }
        format!("{:08X}/{:032b}", shift_reg, shift_reg)
    }
}

impl FmCodec {
    pub const WEAK_BIT_RUN: usize = 6;

    pub fn new(mut bit_vec: BitVec, bit_ct: Option<usize>, weak_mask: Option<BitVec>) -> Self {
        // If a bit count was provided, we can trim the bit vector to that length.
        if let Some(bit_ct) = bit_ct {
            bit_vec.truncate(bit_ct);
        }

        let encoding_sync = get_fm_sync_offset(&bit_vec).unwrap_or(EncodingPhase::Even);
        let sync = encoding_sync.into();

        let clock_map = BitVec::from_elem(bit_vec.len(), encoding_sync.into());
        let weak_mask = match weak_mask {
            Some(mask) => mask,
            None => BitVec::from_elem(bit_vec.len(), false),
        };

        if weak_mask.len() < bit_vec.len() {
            panic!("Weak mask must be the same length as the bit vector");
        }

        FmCodec {
            bit_vec,
            clock_map,
            weak_enabled: true,
            weak_mask,
            initial_phase: sync,
            bit_cursor: sync,
            track_padding: 0,
            data_ranges: Default::default(),
        }
    }

    pub fn weak_data(&self) -> Vec<u8> {
        self.weak_mask.to_bytes()
    }

    pub fn set_weak_mask(&mut self, weak_mask: BitVec) -> Result<()> {
        if weak_mask.len() != self.bit_vec.len() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Weak mask must be the same length as the bit vector",
            ));
        }
        self.weak_mask = weak_mask;

        Ok(())
    }

    pub fn encode(data: &[u8], prev_bit: bool, encoding_type: EncodingVariant) -> BitVec {
        let mut bitvec = BitVec::new();
        let mut bit_count = 0;

        for &byte in data {
            for i in (0..8).rev() {
                let bit = (byte & (1 << i)) != 0;
                if bit {
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
            Some(self.bit_vec[self.bit_cursor])
        }
    }
    #[allow(dead_code)]
    fn read_bit_at(&self, index: usize) -> Option<bool> {
        if self.weak_enabled && self.weak_mask[self.initial_phase + (index << 1)] {
            // Weak bits return random data
            Some(rand::random())
        }
        else {
            Some(self.bit_vec[self.initial_phase + (index << 1)])
        }
    }

    fn ref_bit_at(&self, index: usize) -> &bool {
        let p_off: usize = self.clock_map[index] as usize;
        if self.weak_enabled && self.weak_mask[p_off + (index << 1)] {
            // Weak bits return random data
            // TODO: precalculate random table and return reference to it.
            &self.bit_vec[p_off + (index << 1)]
        }
        else {
            &self.bit_vec[p_off + (index << 1)]
        }
    }

    pub(crate) fn detect_weak_bits(&self, run: usize) -> (usize, usize) {
        let mut region_ct = 0;
        let mut weak_bit_ct = 0;
        let mut zero_ct = 0;

        for bit in self.bit_vec.iter() {
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
        for (i, bit) in self.bit_vec.iter().enumerate() {
            if !bit {
                zero_ct += 1;
            }
            else {
                if zero_ct >= run {
                    regions.push(TrackRegion {
                        start: region_start,
                        end: i - 1,
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
    #[allow(dead_code)]
    pub(crate) fn create_weak_bit_mask(&self, run: usize) -> BitVec {
        let mut weak_bitvec = BitVec::new();
        let mut zero_ct = 0;
        for bit in self.bit_vec.iter() {
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

        assert_eq!(weak_bitvec.len(), self.bit_vec.len());

        weak_bitvec
    }
}

impl Iterator for FmCodec {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bit_cursor >= (self.bit_vec.len() - 1) {
            return None;
        }

        // The bit cursor should always be aligned to a clock bit.
        // So retrieve the next bit which is the data bit, then point to the next clock.
        let mut data_idx = self.bit_cursor + 1;
        if data_idx > (self.bit_vec.len() - self.track_padding) {
            // Wrap around to the beginning of the track
            data_idx = 0;
        }

        let decoded_bit = if self.weak_enabled && self.weak_mask[data_idx] {
            // Weak bits return random data
            rand::random()
        }
        else {
            self.bit_vec[data_idx]
        };

        let new_cursor = data_idx + 1;
        if new_cursor >= (self.bit_vec.len() - self.track_padding) {
            // Wrap around to the beginning of the track
            self.bit_cursor = 0;
        }
        else {
            self.bit_cursor = new_cursor;
        }

        Some(decoded_bit)
    }
}

impl Seek for FmCodec {
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
        /*
        let mut debug_vec = Vec::new();
        for i in 0..5 {
            debug_vec.push(self.clock_map[new_cursor - 2 + i]);
        }

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
        //log::trace!("seek(): new_pos: {}", self.bit_cursor);

        Ok(self.bit_cursor as u64)
    }
}

impl Read for FmCodec {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
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

impl Index<usize> for FmCodec {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.bit_vec.len() {
            panic!("index out of bounds");
        }

        // Decode the bit here (implement the MFM decoding logic)
        self.ref_bit_at(index)
    }
}

impl TrackDataStreamT for FmCodec {}
