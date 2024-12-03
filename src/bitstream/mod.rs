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

use crate::{
    io::{Read, Seek},
    types::DiskDataEncoding,
};
use bit_vec::BitVec;
use std::ops::Index;

pub enum EncodingVariant {
    Data,
    AddressMark,
}

#[cfg_attr(feature = "serde", typetag::serde(tag = "type"))]
pub trait TrackCodec: Read + Seek + Index<usize, Output = bool> + Send + Sync {
    /// Return the `[DiskDataEncoding]` of the data on this track.
    /// A single track may only have one encoding.
    fn encoding(&self) -> DiskDataEncoding;
    /// Return the length of the track in bits.
    fn len(&self) -> usize;
    /// Return a bool indicating if the track is empty.
    fn is_empty(&self) -> bool;
    /// Replace the data bits of the track with the provided bits.
    fn replace(&mut self, new_bits: BitVec);
    /// Return a reference to the data bits of the track as a `BitVec`.
    fn data(&self) -> &BitVec;
    /// Return a mutable reference to the data bits of the track as a `BitVec`.
    fn data_mut(&mut self) -> &mut BitVec;
    /// Return a copy of the track data as a `Vec<u8>`.
    fn data_copied(&self) -> Vec<u8>;
    /// Set the clock map for the track.
    /// A clock map is a `BitVec` where each 1 bit set corresponds to a clock bit.
    /// Maintaining a clock map enables random access to a track.
    fn set_clock_map(&mut self, clock_map: BitVec);
    /// Return a reference to the clock map of the track as a `BitVec`.
    fn clock_map(&self) -> &BitVec;

    fn clock_map_mut(&mut self) -> &mut BitVec;
    fn enable_weak(&mut self, enable: bool);
    fn weak_mask(&self) -> &BitVec;
    fn weak_mask_mut(&mut self) -> &mut BitVec;
    fn weak_data(&self) -> Vec<u8>;
    fn set_weak_mask(&mut self, mask: BitVec);
    fn has_weak_bits(&self) -> bool;
    fn error_map(&self) -> &BitVec;
    fn set_track_padding(&mut self);
    fn read_raw_byte(&self, index: usize) -> Option<u8>;
    fn write_raw_byte(&mut self, index: usize, byte: u8);
    fn read_decoded_byte(&self, index: usize) -> Option<u8>;
    fn read_decoded_buf(&self, buf: &mut [u8], offset: usize) -> usize;
    fn write_encoded_buf(&mut self, buf: &[u8], offset: usize) -> usize;
    fn write_raw_buf(&mut self, buf: &[u8], offset: usize) -> usize;
    fn encode(&self, data: &[u8], prev_bit: bool, encoding_type: EncodingVariant) -> BitVec;
    fn find_marker(&self, marker: u64, mask: Option<u64>, start: usize, limit: Option<usize>) -> Option<(usize, u16)>;

    fn set_data_ranges(&mut self, ranges: Vec<(usize, usize)>);
    fn is_data(&self, index: usize, wrapping: bool) -> bool;
    fn debug_marker(&self, index: usize) -> String;
    fn debug_decode(&self, index: usize) -> String;
}

pub type TrackDataStream = Box<dyn TrackCodec<Output = bool>>;
