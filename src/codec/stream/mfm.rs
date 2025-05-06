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

const MARKER_MASK: u64 = 0xFFFF_FFFF_FFFF_0000;
//const CLOCK_MASK: u64 = 0xAAAA_AAAA_AAAA_0000;

pub struct MfmStreamDecoder {
    shift_reg: u64,
    nibbles: Vec<Nibble>,
    clock: bool,
    synced: bool,
    markers: Vec<u64>,
    nibble_bit_ct: u8,
    nibble: u8,
}

impl MfmStreamDecoder {
    pub fn with_markers(markers: &[u64]) -> Self {
        MfmStreamDecoder {
            shift_reg: 0,
            nibbles: Vec::with_capacity(128),
            clock: true,
            synced: false,
            markers: markers.to_vec(),
            nibble_bit_ct: 0,
            nibble: 0,
        }
    }
}

impl StreamDecoder for MfmStreamDecoder {
    fn reset(&mut self) {
        self.nibbles.clear();
        self.shift_reg = 0;
        self.clock = true;
        self.synced = false;
        self.nibble_bit_ct = 0;
        self.nibble = 0;
    }

    #[inline]
    fn is_synced(&self) -> bool {
        self.synced
    }

    fn encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

    fn push_bit(&mut self, bit: bool) {
        self.shift_reg = (self.shift_reg << 1) | (bit as u64);
        for marker in &self.markers {
            if self.shift_reg & MARKER_MASK == *marker {
                self.synced = true;
            }

            if self.synced {}
        }
        self.clock = !self.clock;
    }

    fn bits_remaining(&self) -> usize {
        self.nibble_bit_ct as usize
    }

    fn has_nibble(&self) -> bool {
        !self.nibbles.is_empty()
    }

    fn peek_nibble(&self) -> Option<Nibble> {
        self.nibbles.last().copied()
    }

    fn pop_nibble(&mut self) -> Option<Nibble> {
        self.nibbles.pop()
    }
}
