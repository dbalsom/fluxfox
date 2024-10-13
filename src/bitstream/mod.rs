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

pub mod fm;
pub mod mfm;

use crate::io::{Read, Seek};
use crate::{DiskDataEncoding, EncodingPhase};
use bit_vec::BitVec;
use std::ops::Index;

pub enum EncodingVariant {
    Data,
    AddressMark,
}
pub trait TrackCodec {
    fn encoding(&self) -> DiskDataEncoding;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn replace(&mut self, new_bits: BitVec);
    fn data(&self) -> Vec<u8>;
    fn set_clock_map(&mut self, clock_map: BitVec);
    fn clock_map(&self) -> &BitVec;
    fn clock_map_mut(&mut self) -> &mut BitVec;
    fn get_sync(&self) -> Option<EncodingPhase>;
    fn weak_mask(&self) -> &BitVec;
    fn has_weak_bits(&self) -> bool;
    fn weak_data(&self) -> Vec<u8>;
    fn set_track_padding(&mut self);
    fn read_byte(&self, index: usize) -> Option<u8>;
    fn read_decoded_byte(&self, index: usize) -> Option<u8>;
    fn write_buf(&mut self, buf: &[u8], offset: usize) -> Option<usize>;
    fn write_raw_buf(&mut self, buf: &[u8], offset: usize) -> usize;
    fn encode(&self, data: &[u8], prev_bit: bool, encoding_type: EncodingVariant) -> BitVec;
    fn find_marker(&self, marker: u64, mask: Option<u64>, start: usize, limit: Option<usize>) -> Option<(usize, u16)>;

    fn debug_marker(&self, index: usize) -> String;
    fn debug_decode(&self, index: usize) -> String;
}

//pub trait TrackDataStreamT: TrackCodec + Iterator + Read + Seek + Index<usize> {}
pub trait TrackDataStreamT: TrackCodec + Read + Seek + Index<usize> {}

//pub type TrackDataStream = Box<dyn TrackDataStreamT<Item = bool, Output = bool>>;
pub type TrackDataStream = Box<dyn TrackDataStreamT<Output = bool>>;
