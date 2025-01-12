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

/// 'CRC32 Reverse' Hasher for IPF files.
pub(crate) struct IpfCrcHasher {
    crc: u32,
}

impl IpfCrcHasher {
    pub(crate) fn new() -> Self {
        Self { crc: 0xFFFF_FFFF }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        for &byte in data.iter() {
            self.crc ^= byte as u32;
            for _ in 0..8 {
                if self.crc & 1 != 0 {
                    self.crc = (self.crc >> 1) ^ 0xEDB8_8320;
                }
                else {
                    self.crc >>= 1;
                }
            }
        }
    }

    pub(crate) fn finalize(&self) -> u32 {
        !self.crc
    }
}

impl Default for IpfCrcHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference implementation of the CRC algorithm used by IPF files.
#[allow(dead_code)]
pub(crate) fn ipf_crc_u32r(data: &[u8], start: Option<u32>) -> u32 {
    let mut crc = start.unwrap_or(0xFFFF_FFFF);
    for byte in data.iter() {
        crc ^= *byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            }
            else {
                crc >>= 1;
            }
        }
    }
    !crc
}
