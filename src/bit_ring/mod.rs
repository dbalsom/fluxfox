/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

//! A [BitRing] is a binary ring buffer, which can be indexed and iterated over
//! infinitely. It is intended to represent the bitstream of a disk track, which
//! is a continuous topological ring. Disk read operations can often wrap around
//! from the end of the track to the beginning, so [BitRing] is designed to
//! emulate this behavior.

use crate::io::{self, Read};
use bit_vec::BitVec;
use std::ops::Index;

/// A [BitRingIter] may be used to iterate over the bits of a [BitRing], producing
/// a sequence of `bool` values.
pub struct BitRingIter<'a> {
    ring:   &'a BitRing,
    cursor: usize,
    limit:  Option<usize>, // Optional limit for one revolution
}

impl Iterator for BitRingIter<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(limit) = self.limit {
            if self.cursor >= limit {
                return None;
            }
        }

        let bit = self.ring.bits[self.cursor];
        self.cursor += 1;
        Some(bit)
    }
}
/// A [BitRing] is a binary ring buffer, which can be indexed and iterated over
/// infinitely. It is intended to represent the bitstream of a disk track, which
/// is a continuous topological ring. Disk read operations can often wrap around
/// from the end of the track to the beginning, and a [BitRing] can represent this
/// behavior.
///
/// [BitRing] is implemented as a wrapper around a [BitVec] from the bit_vec crate
/// (not to be confused with the bitvec crate).
///
/// A [BitRing] allows an overrideable length at which to wrap around, to support
/// adjustable track wrapping strategies. It may also be configured to return a
/// specific value when indexed beyond the wrap point - this is useful to ignore
/// any clock bits that may only be valid within the first revolution.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BitRing {
    bits: BitVec,
    wrap: usize,
    cursor: usize,
    wrap_value: Option<bool>,
}

/// Produce a [BitRing] from a [BitVec].
impl From<BitVec> for BitRing {
    fn from(bits: BitVec) -> BitRing {
        let wrap = bits.len();
        BitRing {
            bits,
            wrap,
            cursor: 0,
            wrap_value: None,
        }
    }
}

/// Produce a [BitRing] from a byte slice.
impl From<&[u8]> for BitRing {
    fn from(bytes: &[u8]) -> BitRing {
        let bits = BitVec::from_bytes(bytes);
        let wrap = bits.len();
        BitRing {
            bits,
            wrap,
            cursor: 0,
            wrap_value: None,
        }
    }
}

#[allow(dead_code)]
impl BitRing {
    /// Return an infinite iterator ([BitRingIter]) over the bits of the [BitRing], starting at
    /// the beginning of the track.
    pub fn iter(&self) -> BitRingIter {
        BitRingIter {
            ring:   self,
            cursor: 0,
            limit:  None,
        }
    }

    /// Return a single-revolution iterator ([BitRingIter]) over the bits of the [BitRing], starting
    /// at the beginning of the track and ending at the wrap point.
    pub fn iter_revolution(&self) -> BitRingIter {
        BitRingIter {
            ring:   self,
            cursor: 0,
            limit:  Some(self.wrap),
        }
    }

    /// Create a new [BitRing] from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> BitRing {
        BitRing::from(bytes)
    }

    /// Create a new [BitRing] with the specified length, containing the specified element.
    pub fn from_elem(len: usize, elem: bool) -> BitRing {
        BitRing {
            bits: BitVec::from_elem(len, elem),
            wrap: len,
            cursor: 0,
            wrap_value: None,
        }
    }

    /// Return the length of the [BitRing] in bits.
    #[inline]
    pub fn len(&self) -> usize {
        self.bits.len()
    }

    /// Return a bool indicating if the [BitRing] is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    /// Return the wrapping point of the [BitRing].
    #[inline]
    pub fn wrap_len(&self) -> usize {
        self.wrap
    }

    /// Return a reference to the underlying [BitVec] representation.
    #[inline]
    pub fn bits(&self) -> &BitVec {
        &self.bits
    }

    #[inline]
    pub fn bits_mut(&mut self) -> &mut BitVec {
        &mut self.bits
    }

    /// Return a copy of the [BitRing] data as a byte vector.
    /// Data beyond the bit length of the [BitRing] is undefined.
    #[inline]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.to_bytes()
    }

    /// Set the bit at `index` to the value of `bit`
    #[inline]
    pub fn set(&mut self, index: usize, bit: bool) {
        if index < self.wrap {
            self.bits.set(index, bit);
        }
        else {
            self.bits.set(index % self.wrap, bit);
        }
    }

    /// Set the wrapping point of the BitRing.
    /// `set_wrap` will not allow the length to be set longer than the underlying [BitVec].
    pub fn set_wrap(&mut self, wrap_len: usize) {
        self.wrap = std::cmp::min(wrap_len, self.bits.len());
    }

    /// Set an override value to return when the index wraps around. `None` will return the actual
    /// value, while `Some(bool)` will return the specified override value.
    pub fn set_wrap_value(&mut self, wrap_value: impl Into<Option<bool>>) {
        self.wrap_value = wrap_value.into();
    }

    #[inline]
    fn incr_cursor(&mut self) {
        self.cursor = self.wrap_cursor(self.cursor + 1);
    }

    #[inline]
    fn wrap_cursor(&self, cursor: usize) -> usize {
        if cursor != cursor % self.wrap {
            log::warn!("Cursor wrapped around at {}", cursor);
        }
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
            for _ in 0..8 {
                byte = (byte << 1) | (self.bits[self.cursor] as u8);
                self.incr_cursor();
            }
            *buf_byte = byte;
            read += 1;
        }
        Ok(read)
    }
}

impl Index<usize> for BitRing {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if index < self.wrap {
            &self.bits[index]
        }
        else {
            if index == self.wrap {
                //log::debug!("Index wrapped around at {}", index);
            }
            self.wrap_value
                .as_ref()
                .unwrap_or_else(|| &self.bits[index % self.wrap])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_vec::BitVec;

    #[test]
    fn test_from_bitvec() {
        // Initialize BitRing from a BitVec
        let bits = BitVec::from_bytes(&[0b1010_1010]);
        let ring = BitRing::from(bits.clone());

        assert_eq!(ring.len(), bits.len());
        for i in 0..bits.len() {
            assert_eq!(ring[i], bits[i]);
        }
    }

    #[test]
    fn test_from_bytes() {
        // Initialize BitRing from a byte slice
        let bytes = &[0b1010_1010, 0b1100_1100];
        let ring = BitRing::from_bytes(bytes);

        let expected_bits = BitVec::from_bytes(bytes);
        for i in 0..expected_bits.len() {
            assert_eq!(ring[i], expected_bits[i]);
        }
    }

    #[test]
    fn test_wrap_behavior_no_wrap_value() {
        // Test wrapping behavior with no wrap_value set
        let bits = BitVec::from_bytes(&[0b1010_1010]); // 8 bits
        let mut ring = BitRing::from(bits.clone());

        // Set wrap point at 8 (length of BitVec)
        ring.set_wrap(8);

        // Access beyond wrap point should wrap around to the beginning
        for i in 0..16 {
            if i == 8 {
                assert_eq![ring[i], true]
            }
            assert_eq!(ring[i], bits[i % 8]);
        }
    }

    #[test]
    fn test_wrap_behavior2() {
        // Test wrapping behavior with no wrap_value set
        let bits = BitVec::from_bytes(&[0b1111_1111, 0b0000_0000]); // 16 bits
        let mut ring = BitRing::from(bits.clone());

        // Set wrap point at 16 (length of BitVec)
        ring.set_wrap(16);

        // Access beyond wrap point should wrap around to the beginning
        for i in 0..32 {
            if i == 15 {
                assert_eq![ring[i], false]
            }
            if i == 16 {
                assert_eq![ring[i], true]
            }
            assert_eq!(ring[i], bits[i % 16]);
        }
    }

    #[test]
    fn test_wrap_behavior_with_wrap_value() {
        // Test wrapping behavior with a wrap_value set
        let bits = BitVec::from_bytes(&[0b1010_1010]); // 8 bits
        let mut ring = BitRing::from(bits);

        // Set wrap point at 8 (length of BitVec) and wrap_value to Some(false)
        ring.set_wrap(8);
        ring.set_wrap_value(Some(false));

        // Access within wrap point should return actual bits
        for i in 0..8 {
            assert_eq!(ring[i], ring.bits[i]);
        }

        // Access beyond wrap point should return wrap_value (false)
        for i in 8..16 {
            assert!(!ring[i]);
        }
    }

    #[test]
    fn test_iterate_over_bits() {
        // Test iterator behavior over BitRing
        let bytes = &[0b1010_1010];
        let ring = BitRing::from_bytes(bytes);

        // Collect the iterator output into a vector
        let collected: Vec<bool> = Iterator::take(ring, 9).collect();

        // Verify it matches the expected pattern
        let expected = vec![true, false, true, false, true, false, true, false, true];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_read_to_buffer() {
        // Test `Read` implementation
        let bytes = &[0b1010_1010];
        let mut ring = BitRing::from_bytes(bytes);

        let mut buf = [0; 1];
        let read_bytes = ring.read(&mut buf).expect("Failed to read from BitRing");

        // Ensure 1 byte is read and matches the input pattern
        assert_eq!(read_bytes, 1);
        assert_eq!(buf[0], 0b1010_1010);
    }

    #[test]
    fn test_custom_wrap_len() {
        // Test custom wrap length shorter than the full length
        let bits = BitVec::from_bytes(&[0b1010_1010, 0b1100_1100]); // 16 bits
        let mut ring = BitRing::from(bits);

        // Set a custom wrap length at 8 (half the total length)
        ring.set_wrap(8);

        // Accessing beyond 8 should wrap around to the beginning
        for i in 0..16 {
            assert_eq!(ring[i], ring.bits[i % 8]);
        }
    }

    #[test]
    fn test_iter_revolution_length() {
        // Initialize BitRing with a known bit pattern
        let bits = BitVec::from_bytes(&[0b1010_1010, 0b1100_1100]); // 16 bits
        let ring = BitRing::from(bits.clone());

        // Check that iter_revolution returns exactly `len()` elements
        let revolution: Vec<bool> = ring.iter_revolution().collect();
        assert_eq!(
            revolution.len(),
            ring.len(),
            "iter_revolution should return exactly len() elements"
        );

        // Optional: Verify the content of the revolution matches the original bits
        for i in 0..ring.len() {
            assert_eq!(
                revolution[i], ring.bits[i],
                "Mismatch at index {} in iter_revolution",
                i
            );
        }
    }
}
