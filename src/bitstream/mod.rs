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
    types::TrackDataEncoding,
};
use bit_vec::BitVec;
use dyn_clone::{clone_trait_object, DynClone};
use std::ops::Index;
// fn find_marker(&self, marker: u64, mask: Option<u64>, start: usize, limit: Option<usize>) -> Option<(usize, u16)>;

/// Defines the bit pattern and mask for an FM or MFM track marker.
pub struct MarkerEncoding {
    pub bits: u64,
    pub mask: u64,
    pub len:  usize,
}

impl Default for MarkerEncoding {
    fn default() -> Self {
        MarkerEncoding {
            bits: 0,
            mask: !0,
            len:  64,
        }
    }
}

/// When encoding data with a `TrackCodex`, an `EncodingVariant` specifies if the encoding should
/// use the standard `Data` encoding, or encode the data using the special clock pattern to make
/// it an `AddressMark`.
#[derive(Copy, Clone, Debug)]
pub enum EncodingVariant {
    Data,
    AddressMark,
}

/// A `TrackCodec` is a trait that represents the data encoding of a disk track.
/// Data encodings
#[cfg_attr(feature = "serde", typetag::serde(tag = "type"))]
pub trait TrackCodec: DynClone + Read + Seek + Index<usize, Output = bool> + Send + Sync {
    /// Return the `[DiskDataEncoding]` of the data on this track.
    /// A single track may only have one encoding.
    fn encoding(&self) -> TrackDataEncoding;
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
    /// Return a reference to the clock map of the track as a &[BitVec].
    fn clock_map(&self) -> &BitVec;
    /// Return a mutable reference to the clock map of the track as a &mut [BitVec].
    fn clock_map_mut(&mut self) -> &mut BitVec;
    /// Control whether weak bits should be calculated when reading the track.
    fn enable_weak(&mut self, enable: bool);
    /// Return a reference to the weak bit mask as a &[BitVec].
    fn weak_mask(&self) -> &BitVec;
    /// Return a mutable reference to the weak bit mask as a &mut [BitVec].
    fn weak_mask_mut(&mut self) -> &mut BitVec;
    /// Return a copy of the weak bits of the track as a `Vec<u8>`.
    fn weak_data(&self) -> Vec<u8>;
    /// Replace the weak bits of the track with the provided [BitVec].
    fn set_weak_mask(&mut self, mask: BitVec);
    /// Return a bool indicating if the track has bits set in the weak bit mask.
    fn has_weak_bits(&self) -> bool;
    /// Return a reference to the error map of the track as a &[BitVec].
    fn error_map(&self) -> &BitVec;
    fn set_track_padding(&mut self);
    /// Read a raw (encoded) byte from the track at the specified bit index.
    fn read_raw_u8(&self, index: usize) -> Option<u8>;
    /// Fill a buffer with raw (encoded) bytes from the track starting at the specified bit index.
    fn read_raw_buf(&self, buf: &mut [u8], offset: usize) -> usize;
    /// Write a raw (encoded) byte to the track at the specified bit index.
    fn write_raw_u8(&mut self, index: usize, byte: u8);
    /// Write a buffer of raw (encoded) bytes to the track starting at the specified bit index.
    fn write_raw_buf(&mut self, buf: &[u8], offset: usize) -> usize;
    /// Read a decoded byte from the track at the specified bit index.
    fn read_decoded_u8(&self, index: usize) -> Option<u8>;
    fn read_decoded_u32_le(&self, index: usize) -> u32;
    fn read_decoded_u32_be(&self, index: usize) -> u32;
    /// Fill a buffer with decoded bytes from the track starting at the specified bit index.
    fn read_decoded_buf(&self, buf: &mut [u8], offset: usize) -> usize;
    /// Encode a buffer of data and write it to the track starting at the specified bit index.
    fn write_encoded_buf(&mut self, buf: &[u8], offset: usize) -> usize;
    /// Encode a buffer of data and return it as a `BitVec`.
    fn encode(&self, data: &[u8], prev_bit: bool, encoding_type: EncodingVariant) -> BitVec;
    /// Find the next marker in the track starting at the specified bit index, searching up to
    /// `limit` bit index if provided.
    fn find_marker(&self, marker: &MarkerEncoding, start: usize, limit: Option<usize>) -> Option<(usize, u16)>;
    fn set_data_ranges(&mut self, ranges: Vec<(usize, usize)>);
    /// Return a bool indicating if the bit at the specified index is inside sector data.
    /// Requires the track to have data ranges set with set_data_ranges().
    fn is_data(&self, index: usize, wrapping: bool) -> bool;
    fn debug_marker(&self, index: usize) -> String;
    fn debug_decode(&self, index: usize) -> String;
}

clone_trait_object!(TrackCodec);

pub type TrackDataStream = Box<dyn TrackCodec<Output = bool>>;
