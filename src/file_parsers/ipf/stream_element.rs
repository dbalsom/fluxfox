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

use binrw::{binrw, BinResult};
use bit_vec::BitVec;
use modular_bitfield::prelude::*;

#[derive(BitfieldSpecifier, Eq, PartialEq, Debug)]
pub enum DataType {
    End,
    Sync,
    Data,
    Gap,
    Raw,
    Fuzzy,
    Invalid0,
    Invalid1,
}

#[bitfield]
#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct DataHead {
    #[bits = 3]
    pub(crate) data_type: DataType,
    pub(crate) unused: B2,
    pub(crate) data_size_width: B3,
}

impl DataHead {
    pub(crate) fn is_null(&self) -> bool {
        self.data_type() == DataType::End && self.data_size_width() == 0
    }
}

// impl Endian for DataHead {
//     const ENDIANNESS: Endian = Endian::Big;
// }
pub(crate) enum DataSample {
    Bytes(Vec<u8>),
    Bits(BitVec),
}

//#[br(if(data_head.data_type() != DataType::Fuzzy))]

#[binrw]
#[brw(big)]
#[br(import(data_is_bits: bool, data_bytes: usize))]
#[bw(import(data_is_bits: bool, data_bytes: usize))]
pub struct DataStreamElement {
    #[br(parse_with = parse_data_head)]
    #[bw(write_with = write_data_head)]
    pub(crate) data_head: DataHead,

    #[br(count = data_head.data_size_width() as usize)]
    pub(crate) encoded_data_size: Vec<u8>,

    #[br(calc = calculate_sample_size(&encoded_data_size))]
    #[bw(ignore)]
    pub(crate) sample_size_decoded: usize,

    #[br(parse_with = read_samples, args(sample_size_decoded, data_bytes, data_is_bits, data_head.data_type() == DataType::Fuzzy))]
    #[bw(write_with = write_samples, args(sample_size_decoded, data_bytes, data_is_bits, data_head.data_type() == DataType::Fuzzy))]
    pub(crate) data_sample: Option<DataSample>,
}

#[binrw::parser(reader)]
fn parse_data_head() -> BinResult<DataHead> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    let dh = DataHead::from_bytes(buf);
    log::debug!("Parsed data head: {:?}", dh);
    Ok(dh)
}

#[binrw::parser(reader: r)]
fn read_samples(
    sample_size: usize,
    _data_bytes: usize,
    data_is_bits: bool,
    data_is_fuzzy: bool,
) -> BinResult<Option<DataSample>> {
    if data_is_fuzzy {
        return Ok(None);
    }

    // if sample_size != 1 {
    //     log::warn!("Sample size is not 1: {}", sample_size);
    // }

    let sample_bytes = match data_is_bits {
        true => (sample_size + 7) / 8, // Round to the next byte
        false => sample_size,
    };

    let mut sample_buf = vec![0u8; sample_bytes];
    r.read_exact(&mut sample_buf)?;

    let samples = match data_is_bits {
        true => DataSample::Bits(BitVec::from_bytes(&sample_buf)),
        false => DataSample::Bytes(sample_buf),
    };
    Ok(Some(samples))
}

#[allow(dead_code)]
#[allow(unused_variables)]
#[binrw::writer(writer: r)]
fn write_samples(
    value: &Option<DataSample>,
    sample_size: &usize,
    data_bytes: usize,
    data_is_bits: bool,
    data_is_fuzzy: bool,
) -> BinResult<()> {
    Ok(())
}

#[binrw::writer(writer: w)]
fn write_data_head(value: &DataHead) -> BinResult<()> {
    //let byte = value.into_bytes();
    _ = w.write(&value.into_bytes())?;
    Ok(())
}

fn calculate_sample_size(data_size_encoded: &[u8]) -> usize {
    let mut final_size = 0usize;
    for byte in data_size_encoded {
        final_size = (final_size << 8) | *byte as usize;
    }
    log::debug!(
        "Decoded sample size of {} using {} bytes",
        final_size,
        data_size_encoded.len()
    );
    final_size
}
