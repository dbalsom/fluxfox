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

    src/bit_ring/mod.rs

    Implements a ring buffer for bits based off a BitVec.

    A BitRing allows an overrideable length at which to wrap around, to support
    adjustable track wrapping strategies.
*/

use crate::io::{self, Read};
use bit_vec::BitVec;

pub struct BitRing {
    bits: BitVec,
    wrap: usize,
    cursor: usize,
}

impl From<BitVec> for BitRing {
    fn from(bits: BitVec) -> BitRing {
        let wrap = bits.len();
        BitRing { bits, wrap, cursor: 0 }
    }
}

impl From<&[u8]> for BitRing {
    fn from(bytes: &[u8]) -> BitRing {
        let bits = BitVec::from_bytes(bytes);
        let wrap = bits.len();
        BitRing { bits, wrap, cursor: 0 }
    }
}

#[allow(dead_code)]
impl BitRing {
    pub fn from_elem(len: usize, elem: bool) -> BitRing {
        BitRing {
            bits: BitVec::from_elem(len, elem),
            wrap: len,
            cursor: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.bits.len()
    }

    pub fn wrap_len(&self) -> usize {
        self.wrap
    }

    /// Set the wrapping point of the BitRing.
    /// `set_wrap` will not allow the length to be set longer than the underlying BitVec.
    pub fn set_wrap(&mut self, wrap_len: usize) {
        self.wrap = std::cmp::max(wrap_len, self.bits.len());
    }

    #[inline]
    fn incr_cursor(&mut self) {
        self.cursor = self.wrap_cursor(self.cursor + 1);
    }

    #[inline]
    fn wrap_cursor(&mut self, cursor: usize) -> usize {
        cursor % self.wrap
    }
}

impl Iterator for BitRing {
    type Item = bool;
    fn next(&mut self) -> Option<Self::Item> {
        let bit = self.bits[self.cursor];
        self.incr_cursor();
        Some(bit)
    }
}

impl Read for BitRing {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read = 0;
        for buf_byte in buf.iter_mut() {
            let mut byte = 0;
            for i in 0..8 {
                let bit = self.bits[self.cursor];
                byte |= (bit as u8) << i;
                self.incr_cursor();
            }
            *buf_byte = byte;
            read += 1;
        }
        Ok(read)
    }
}
