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
}

// fn parse_into_slice<R: Read + binrw::io::Seek>(
//     reader: &mut R,
//     _endian: binrw::Endian,
//     _: (),
// ) -> binrw::BinResult<&'static [u8]> {
//     // Static empty buffer for fallback
//     const EMPTY_BUFFER: &[u8] = &[];
//
//     let mut temp_buffer = vec![0u8; 8]; // Example: Reading up to 8 bytes
//     let bytes_read = reader.read(&mut temp_buffer)?;
//
//     // Return the slice if bytes were read, or fall back to the empty buffer
//     if bytes_read == 0 {
//         Ok(EMPTY_BUFFER)
//     }
//     else {
//         Ok(&temp_buffer[..bytes_read])
//     }
// }
