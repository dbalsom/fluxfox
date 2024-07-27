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

pub mod mfm;
pub mod raw;

use crate::diskimage::TrackDataStream;
use crate::io::Seek;
use crate::EncodingSync;
use bit_vec::BitVec;
use std::ops::Index;

pub trait TrackDataStreamT: Iterator + Seek + Index<usize> {}

impl Iterator for TrackDataStream {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TrackDataStream::Raw(data) => data.next(),
            TrackDataStream::Mfm(data) => data.next(),
            _ => None,
        }
    }
}

impl Index<usize> for TrackDataStream {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            TrackDataStream::Raw(data) => &data[index],
            TrackDataStream::Mfm(data) => &data[index],
            _ => &false,
        }
    }
}

impl TrackDataStream {
    pub fn len(&self) -> usize {
        match self {
            TrackDataStream::Raw(data) => data.len(),
            TrackDataStream::Mfm(data) => data.len(),
            _ => 0,
        }
    }

    pub fn get_sync(&self) -> Option<EncodingSync> {
        match self {
            TrackDataStream::Mfm(data) => data.get_sync(),
            _ => None,
        }
    }
}

pub fn find_idam(track: &mut TrackDataStream, start_idx: usize) -> Option<usize> {
    let mut i = start_idx;

    let mut shift_reg: u32 = 0;
    let mut last_bit = false;
    let mut last2_bit = false;
    let mut data_ct = 0;

    for track_bit in track {
        let cur_bit = track_bit;
        shift_reg = shift_reg << 1 | (cur_bit as u32);

        last2_bit = last_bit;
        last_bit = cur_bit;
        i += 2;

        if shift_reg == 0x4E4E4E4E {
            // In gap
            log::trace!("In gap");
        }
        if shift_reg == 0xC2C2C2FC {
            // Found IDAM
            log::trace!("Found IDAM marker at offset: {}", i - 32);
            return Some(i - 32);
        }

        if i % 16 == 0 {
            log::trace!("Shift reg: {:08X}", shift_reg);
        }
    }

    log::trace!("Decoded {} bytes of data", data_ct);
    None
}

pub fn decode_mfm(track: &BitVec, start_idx: usize) -> BitVec {
    let mut data_bits = BitVec::with_capacity(track.len() / 2);

    let mut i = start_idx;
    while i < track.len() {
        data_bits.push(track[i]);
        i += 2;
    }
    data_bits
}
