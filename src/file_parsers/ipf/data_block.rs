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

// Flags for block_flags field
bitflags::bitflags! {
    #[derive(Debug)]
    pub(crate) struct BlockFlags: u32 {
        const FORWARD_GAP  = 0b0001; // Bit 0: Indicates associated forward gap stream elements
        const BACKWARD_GAP = 0b0010; // Bit 1: Indicates associated backward gap stream elements
        const DATA_IN_BITS = 0b0100; // Bit 2: Data stream sample length: 0 = bytes, 1 = bits
    }
}

#[binrw]
#[brw(big)]
#[br(import(encoder_type: u32))]
#[bw(import(encoder_type: u32))]
#[derive(Debug)]
pub(crate) struct BlockDescriptor {
    pub(crate) data_bits: u32, // Size in bits of the decoded block data
    pub(crate) gap_bits:  u32, // Size in bits of the decoded gap

    #[br(if(encoder_type == 1))]
    #[bw(if(encoder_type == 1))]
    pub(crate) data_bytes: Option<u32>, // Parsed only if encoder_type == 1

    #[br(if(encoder_type == 1))]
    #[bw(if(encoder_type == 1))]
    pub(crate) gap_bytes: Option<u32>, // Parsed only if encoder_type == 1

    #[br(if(encoder_type == 2))]
    #[bw(if(encoder_type == 2))]
    pub(crate) gap_offset: Option<u32>, // Parsed only if encoder_type == 2

    #[br(if(encoder_type == 2))]
    #[bw(if(encoder_type == 2))]
    pub(crate) cell_type: Option<u32>, // Parsed only if encoder_type == 2

    pub(crate) blk_encoder_type: u32, // Block encoder type (not to be confused with INFO record encoder type)

    #[bw(calc = if encoder_type == 2 { block_flags.as_ref().map_or(0, |flags| flags.bits()) } else { 0 })]
    pub(crate) block_flags_raw: u32,

    #[br(calc = if encoder_type == 2 { Some(BlockFlags::from_bits_truncate(block_flags_raw)) } else { None })]
    #[bw(ignore)]
    pub(crate) block_flags: Option<BlockFlags>,

    pub(crate) gap_default: u32, // Default gap value
    pub(crate) data_offset: u32, // Offset to the data stream in the extra data area

    /// Save the input encoder type.
    #[br(calc = encoder_type)]
    #[bw(ignore)]
    saved_encoder_type: u32,
}

impl MapDump for BlockDescriptor {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        match self.saved_encoder_type {
            1 => {
                let mut record = map.add_child(parent, "V1 Block Descriptor", SourceValue::default());
                let node_index = record.index();
                #[rustfmt::skip]
                record
                    .add_child("dataBits", SourceValue::u32(self.data_bits))
                    .add_sibling("gapBits", SourceValue::u32(self.gap_bits))
                    .add_sibling("dataBytes", SourceValue::u32(self.data_bytes.unwrap()))
                    .add_sibling("gapBytes", SourceValue::u32(self.gap_bytes.unwrap()))
                    .add_sibling("blockEncoderType", SourceValue::u32(self.blk_encoder_type))
                    .add_sibling("gapDefault", SourceValue::hex_u32(self.gap_default))
                    .add_sibling("dataOffset", SourceValue::u32(self.data_offset));
                node_index
            }
            2 => {
                let mut record = map.add_child(parent, "V2 Block Descriptor", SourceValue::default());
                let node_index = record.index();
                #[rustfmt::skip]
                record
                    .add_child("dataBits", SourceValue::u32(self.data_bits))
                    .add_sibling("gapBits", SourceValue::u32(self.gap_bits))
                    .add_sibling("gapOffset", SourceValue::u32(self.gap_offset.unwrap()))
                    .add_sibling("cellType", SourceValue::u32(self.cell_type.unwrap()))
                    .add_sibling("blockEncoderType", SourceValue::u32(self.blk_encoder_type))
                    .add_sibling("blockFlags", SourceValue::hex_u32(self.block_flags.as_ref().unwrap().bits()).comment(&format!("{:?}",self.block_flags.as_ref().unwrap_or(&BlockFlags::empty()))),)
                    .add_sibling("gapDefault", SourceValue::hex_u32(self.gap_default).comment("Default gap value if no gap stream"))
                    .add_sibling("dataOffset", SourceValue::u32(self.data_offset));
                node_index
            }
            _ => {
                let record = map.add_child(parent, "Unknown Block Descriptor", SourceValue::default());
                record.index()
            }
        }
    }
}
