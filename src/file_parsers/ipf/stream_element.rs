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
use binrw::{binrw, BinResult};
use bit_vec::BitVec;
use modular_bitfield::prelude::*;

// Set a maximum sample size as sanity check. An extended density track could be 400,000+ bitcells
// long, or around 50K.  So 100KiB feels like a reasonable limit.
const MAX_SAMPLE_SIZE: usize = 100_000;

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

#[allow(dead_code)]
#[bitfield]
#[derive(Copy, Clone, Debug)]
pub(crate) struct DataHead {
    #[bits = 3]
    pub(crate) data_type: DataType,
    #[skip]
    pub(crate) unused: B2,
    pub(crate) data_size_width: B3,
}

impl DataHead {
    pub(crate) fn is_null(&self) -> bool {
        self.data_type() == DataType::End && self.data_size_width() == 0
    }
}

#[binrw::parser(reader)]
fn parse_data_head() -> BinResult<DataHead> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    let dh = DataHead::from_bytes(buf);
    log::debug!("Parsed data head: {:?}", dh);
    Ok(dh)
}

#[binrw::writer(writer: w)]
fn write_data_head(value: &DataHead) -> BinResult<()> {
    //let byte = value.into_bytes();
    _ = w.write(&value.into_bytes())?;
    Ok(())
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

    #[br(parse_with = read_data_samples, args(sample_size_decoded, data_bytes, data_is_bits, data_head.data_type() == DataType::Fuzzy))]
    #[bw(write_with = write_data_samples, args(sample_size_decoded, data_bytes, data_is_bits, data_head.data_type() == DataType::Fuzzy))]
    pub(crate) data_sample: Option<DataSample>,
}

impl MapDump for DataStreamElement {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let record = map.add_child(parent, "Data Stream Element", SourceValue::default());
        let element_idx = record.index();
        #[rustfmt::skip]
        let sample_record = record
            .add_child("dataSizeWidth", SourceValue::u32(self.data_head.data_size_width() as u32))
            .add_sibling("dataType", SourceValue::string(&format!("{:?}" , self.data_head.data_type())))
            .add_sibling("dataSize", SourceValue::u32(self.sample_size_decoded as u32));

        match &self.data_sample {
            Some(DataSample::Bytes(bytes)) => {
                // Add a maximum of 8 bytes of data as a comment
                sample_record.add_sibling(
                    "dataSample(Bytes)",
                    SourceValue::string(&format!("{}", bytes.len()))
                        .comment(&format!("{:02X?}", &bytes[0..std::cmp::min(8, bytes.len())])),
                );
            }
            Some(DataSample::Bits(bits)) => {
                sample_record.add_sibling("dataSample(Bits)", SourceValue::string(&format!("{}", bits.len())));
            }
            None => {
                sample_record.add_child("Unknown sample type!", SourceValue::default().bad());
            }
        };

        element_idx
    }
}

pub(crate) enum GapSample {
    RepeatCt(usize),
    Sample(BitVec),
}

#[derive(BitfieldSpecifier, Eq, PartialEq, Debug)]
pub enum GapType {
    End,
    GapLength,
    SampleLength,
    Invalid0,
}

#[allow(dead_code)]
#[bitfield]
#[derive(Copy, Clone, Debug)]
pub(crate) struct GapHead {
    #[bits = 2]
    pub(crate) gap_type: GapType,
    #[skip]
    pub(crate) unused: B3,
    pub(crate) data_size_width: B3,
}

impl GapHead {
    pub(crate) fn is_null(&self) -> bool {
        self.gap_type() == GapType::End && self.data_size_width() == 0
    }
}

#[binrw]
#[brw(big)]
pub struct GapStreamElement {
    #[br(parse_with = parse_gap_head)]
    #[bw(write_with = write_gap_head)]
    pub(crate) gap_head: GapHead,

    #[br(count = gap_head.data_size_width() as usize)]
    pub(crate) encoded_data_size: Vec<u8>,

    #[br(calc = calculate_sample_size(&encoded_data_size))]
    #[bw(ignore)]
    pub(crate) sample_size_decoded: usize,

    #[br(parse_with = read_gap_samples, args(sample_size_decoded, gap_head.gap_type()))]
    #[bw(write_with = write_gap_samples, args(sample_size_decoded, gap_head.gap_type()))]
    pub(crate) gap_sample: Option<GapSample>,
}

impl MapDump for GapStreamElement {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let record = map.add_child(parent, "Gap Stream Element", SourceValue::default());
        let element_idx = record.index();
        #[rustfmt::skip]
        let sample_record = record
            .add_child("gapSizeWidth", SourceValue::u32(self.gap_head.data_size_width() as u32))
            .add_sibling("gapType", SourceValue::string(&format!("{:?}" , self.gap_head.gap_type())))
            .add_sibling("gapSize", SourceValue::u32(self.sample_size_decoded as u32));

        if let Some(sample) = &self.gap_sample {
            match sample {
                GapSample::RepeatCt(ct) => {
                    sample_record.add_sibling("repeatCt", SourceValue::string(&format!("{}", ct)));
                }
                GapSample::Sample(bits) => {
                    sample_record.add_sibling("sample", SourceValue::string(&format!("{}", bits)));
                }
            }
        }

        element_idx
    }
}

#[binrw::parser(reader)]
fn parse_gap_head() -> BinResult<GapHead> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    let gh = GapHead::from_bytes(buf);
    log::debug!("Parsed gap head: {:?}", gh);
    Ok(gh)
}

#[binrw::writer(writer: w)]
fn write_gap_head(value: &GapHead) -> BinResult<()> {
    //let byte = value.into_bytes();
    _ = w.write(&value.into_bytes())?;
    Ok(())
}

#[binrw::parser(reader: r)]
fn read_data_samples(
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
fn write_data_samples(
    value: &Option<DataSample>,
    sample_size: &usize,
    data_bytes: usize,
    data_is_bits: bool,
    data_is_fuzzy: bool,
) -> BinResult<()> {
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

#[binrw::parser(reader: r)]
fn read_gap_samples(sample_size: usize, sample_type: GapType) -> BinResult<Option<GapSample>> {
    match sample_type {
        GapType::GapLength => {
            log::debug!("read_gap_samples(): Read repeat length of {}", sample_size);
            // Nothing to actually read - repeat count is sample_size
            Ok(Some(GapSample::RepeatCt(sample_size)))
        }
        GapType::SampleLength => {
            log::debug!("read_gap_samples(): Read sample length of {}", sample_size);
            // Read sample_size bits
            let sample_bytes = (sample_size + 7) / 8;
            let mut sample_buf = vec![0u8; sample_bytes];
            r.read_exact(&mut sample_buf)?;

            // Convert to BitVec
            let mut bits = BitVec::from_bytes(&sample_buf);
            // Trim bits to actual size
            bits.truncate(sample_size);

            Ok(Some(GapSample::Sample(bits)))
        }
        _ => {
            log::warn!("read_gap_samples(): Unhandled gap type: {:?}", sample_type);
            Ok(None)
        }
    }
}

#[allow(dead_code)]
#[allow(unused_variables)]
#[binrw::writer(writer: r)]
fn write_gap_samples(value: &Option<GapSample>, sample_size: &usize, sample_type: GapType) -> BinResult<()> {
    Ok(())
}
