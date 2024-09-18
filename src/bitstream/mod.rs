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

use crate::bitstream::mfm::MfmDecoder;
use crate::bitstream::raw::RawDecoder;
use crate::io::{Read, Seek};
use crate::{DiskImageError, EncodingPhase};
use bit_vec::BitVec;
use std::ops::Index;

pub trait TrackDataStreamT: Iterator + Seek + Index<usize> {}

#[derive(Debug)]
pub enum TrackDataStream {
    Raw(RawDecoder),
    Mfm(MfmDecoder),
    Fm(BitVec),
    Gcr(BitVec),
}

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

    pub fn data(&self) -> Vec<u8> {
        match self {
            TrackDataStream::Raw(_data) => panic!("Unsupported operation"),
            TrackDataStream::Mfm(data) => {
                //let data_len = data.len() / 8;
                data.data()
            }
            _ => panic!("Unsupported operation"),
        }
    }

    pub fn set_clock_map(&mut self, clock_map: BitVec) {
        match self {
            TrackDataStream::Mfm(data) => data.set_clock_map(clock_map),
            _ => {}
        }
    }

    pub fn clock_map_mut(&mut self) -> Option<&mut BitVec> {
        match self {
            TrackDataStream::Mfm(data) => Some(data.clock_map_mut()),
            _ => None,
        }
    }

    pub fn get_sync(&self) -> Option<EncodingPhase> {
        match self {
            TrackDataStream::Mfm(data) => data.get_sync(),
            _ => None,
        }
    }

    pub fn set_track_padding(&mut self) {
        match self {
            TrackDataStream::Mfm(data) => data.set_track_padding(),
            _ => {}
        }
    }

    pub fn read_byte(&self, index: usize) -> Option<u8> {
        match self {
            TrackDataStream::Raw(data) => data.read_byte(index),
            TrackDataStream::Mfm(data) => data.read_byte(index),
            _ => None,
        }
    }

    pub fn read_decoded_byte(&self, index: usize) -> Option<u8> {
        match self {
            TrackDataStream::Raw(data) => data.read_byte(index),
            TrackDataStream::Mfm(data) => data.read_decoded_byte(index),
            _ => None,
        }
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Option<usize> {
        match self {
            TrackDataStream::Raw(data) => data.read_exact(buf).ok().map(|_| buf.len()),
            TrackDataStream::Mfm(data) => data.read_exact(buf).ok().map(|_| buf.len()),
            _ => None,
        }
    }

    pub fn debug_marker(&self, index: usize) -> String {
        match self {
            TrackDataStream::Mfm(data) => data.debug_marker(index),
            _ => String::new(),
        }
    }
}
