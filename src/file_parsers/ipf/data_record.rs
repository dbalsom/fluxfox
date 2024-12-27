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

use crate::source_map::{MapDump, OptionalSourceMap, SourceValue};
use binrw::binrw;

#[binrw]
#[brw(big)]
#[derive(Debug)]
pub(crate) struct DataRecord {
    pub(crate) length: u32,   // Length of the Extra Data Block (or 0)
    pub(crate) bit_size: u32, // Data area size in bits (length * 8)
    pub(crate) crc: u32,      // CRC32 of the Extra Data Block
    pub(crate) data_key: u32, // Unique key used to match the same key in an Image record.
}

impl DataRecord {
    pub(crate) fn key(&self) -> u32 {
        self.data_key
    }
}

impl MapDump for DataRecord {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let record = map.add_child(parent, "Data Record", SourceValue::default());
        let record_idx = record.index();
        record
            .add_child("length", SourceValue::u32(self.length))
            .add_sibling("bit_size", SourceValue::u32(self.bit_size))
            .add_sibling("crc", SourceValue::hex_u32(self.crc))
            .add_sibling("data_key", SourceValue::u32(self.data_key));
        record_idx
    }
}
