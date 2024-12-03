/*
    Original code from retrocompressor by Daniel Gordon
    https://github.com/dfgordon/retrocompressor/
    Copyright (c) 2023 Daniel Gordon

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.
*/

//! LZW Compression
//!
//! This currently supports fixed code widths only, other parameters are flexible.
//! Efficiency is probably not optimal, we rely on `std::collections::HashMap` to perform
//! fast lookups on keys of the type `(usize,usize)`.

#![allow(dead_code)]

use crate::FoxHashMap;
use bit_vec::BitVec;

use super::{CompressionError, DYNERR};
use crate::io::{Cursor, ErrorKind, Read, Seek, SeekFrom, Write};

#[derive(Copy, Clone, Debug)]
pub enum Ord {
    Msb0,
    Lsb0,
}

/// Options controlling compression
#[derive(Clone)]
pub struct Options {
    /// Length in bits of the header preceding each chunk, can be 0.
    /// Can be used with fixed code width in lieu of clear code.
    pub header_bits: usize,
    /// header contains bit count divided by this number
    pub header_divisor: usize,
    /// starting position in the input file
    pub in_offset: u64,
    /// starting position in the output file
    pub out_offset: u64,
    /// number of codes to write before a reset
    pub chunk_size: usize,
    /// minimum value of a symbol, currently must be 0
    pub min_symbol: usize,
    /// maximum value of a symbol, usually 255, currently cannot exceed 255 or there will be a panic
    pub max_symbol: usize,
    /// clear code, usually max_symbol+1 or max_symbol+2, match codes will skip over
    pub clear_code: Option<usize>,
    /// stop code, usually max_symbol+1 or max_symbol+2, match codes will skip over
    pub stop_code: Option<usize>,
    /// min code width in bits, currently must be same as max_code_width
    pub min_code_width: usize,
    /// max code with in bits
    pub max_code_width: usize,
    /// bit packing strategy,
    pub ord: Ord,
    /// return error if file is larger
    pub max_file_size: u64,
}

impl Default for Options {
    // Default options for Teledisk v1 compression
    fn default() -> Self {
        Options::from(OptionsPreset::Teledisk)
    }
}

pub enum OptionsPreset {
    Standard,
    Teledisk,
}

impl From<OptionsPreset> for Options {
    fn from(preset: OptionsPreset) -> Options {
        match preset {
            OptionsPreset::Standard => Options {
                header_bits: 0,
                header_divisor: 1,
                in_offset: 0,
                out_offset: 0,
                chunk_size: 4096,
                min_symbol: 0,
                max_symbol: 255,
                clear_code: Some(256),
                stop_code: Some(257),
                min_code_width: 12,
                max_code_width: 12,
                ord: Ord::Lsb0,
                max_file_size: u32::MAX as u64 / 4,
            },
            OptionsPreset::Teledisk => Options {
                header_bits: 16,
                header_divisor: 4,
                in_offset: 12,
                out_offset: 12,
                chunk_size: 4096,
                min_symbol: 0,
                max_symbol: 255,
                clear_code: None,
                stop_code: None,
                min_code_width: 12,
                max_code_width: 12,
                ord: Ord::Lsb0,
                max_file_size: 3_000_000,
            },
        }
    }
}

/// bit_vec crate only handles MSB, this assumes starting alignment
fn bits_to_bytes_lsb0(bits: &BitVec) -> Vec<u8> {
    let mut ans = Vec::new();
    let byte_count = bits.len() / 8;
    let rem = bits.len() % 8;
    for i in 0..byte_count {
        let mut val = 0;
        for b in 0..8 {
            val |= (bits.get(i * 8 + b).unwrap() as u8) << b;
        }
        ans.push(val);
    }
    if rem > 0 {
        let mut val = 0;
        for b in 0..rem {
            val |= (bits.get(byte_count * 8 + b).unwrap() as u8) << b;
        }
        ans.push(val);
    }
    ans
}

/// bit_vec crate only handles MSB, this assumes starting alignment
fn bytes_to_bits_lsb0(bytes: &[u8]) -> BitVec {
    let mut ans = BitVec::new();
    for i in 0..bytes.len() {
        let val = bytes[i];
        for b in 0..8 {
            ans.push((val & (1 << b)) != 0);
        }
    }
    ans
}

#[derive(Clone)]
struct LZWCoder {
    bits:  BitVec,
    ptr:   usize,
    ord:   Ord,
    count: usize,
}

struct LZWDecoder {
    bits:  BitVec,
    ptr:   usize,
    ord:   Ord,
    count: usize,
}

impl LZWCoder {
    pub fn new(ord: Ord) -> Self {
        Self {
            bits: BitVec::new(),
            ptr: 0,
            ord,
            count: 0,
        }
    }
    /// keep the bit vector small, we don't need the bits behind us
    fn drop_leading_bits(&mut self) {
        let cpy = self.bits.clone();
        self.bits = BitVec::new();
        for i in self.ptr..cpy.len() {
            self.bits.push(cpy.get(i).unwrap());
        }
        self.ptr = 0;
    }
    /// output `num_bits` of `code` in given bit-order, the bits are always
    /// written to the output stream (sometimes backing up and rewriting) such that
    /// the start of the bit vector stays aligned.
    pub fn put_code<W: Write + Seek>(&mut self, num_bits: usize, mut code: usize, writer: &mut W) {
        let bytes = match self.ord {
            Ord::Msb0 => {
                code <<= usize::BITS as usize - num_bits;
                let msk = 1 << (usize::BITS - 1);
                for _i in 0..num_bits {
                    self.bits.push(code & msk > 0);
                    code <<= 1;
                    self.ptr += 1;
                }
                self.bits.to_bytes()
            }
            Ord::Lsb0 => {
                for _i in 0..num_bits {
                    self.bits.push(code & 1 > 0);
                    code >>= 1;
                    self.ptr += 1;
                }
                bits_to_bytes_lsb0(&self.bits)
            }
        };
        _ = writer.write(&bytes.as_slice()).expect("write err");
        if self.bits.len() % 8 > 0 {
            writer.seek(SeekFrom::Current(-1)).expect("seek err");
            self.ptr = 8 * (self.bits.len() / 8);
            self.drop_leading_bits();
        }
        else {
            self.bits = BitVec::new();
            self.ptr = 0;
        }
        self.count += 1;
    }
}

impl LZWDecoder {
    pub fn new(ord: Ord) -> Self {
        Self {
            bits: BitVec::new(),
            ptr: 0,
            ord,
            count: 0,
        }
    }
    /// Keep the bit vector small, we don't need the bits behind us
    fn drop_leading_bits(&mut self) {
        let cpy = self.bits.clone();
        self.bits = BitVec::new();
        for i in self.ptr..cpy.len() {
            self.bits.push(cpy.get(i).unwrap());
        }
        self.ptr = 0;
    }
    /// Get the next bit reading from the stream as needed.
    /// When EOF is reached 0 is returned (behavior comes from LZHUF.C).
    /// `reader` should not be advanced outside this function until decoding is done.
    fn get_bit<R: Read>(&mut self, reader: &mut R) -> Result<u8, std::io::Error> {
        match self.bits.get(self.ptr) {
            Some(bit) => {
                self.ptr += 1;
                Ok(bit as u8)
            }
            None => {
                let mut by: [u8; 1] = [0];
                match reader.read_exact(&mut by) {
                    Ok(()) => {
                        if self.bits.len() > 512 {
                            self.drop_leading_bits();
                        }
                        match self.ord {
                            Ord::Msb0 => self.bits.append(&mut BitVec::from_bytes(&by)),
                            Ord::Lsb0 => self.bits.append(&mut bytes_to_bits_lsb0(&by)),
                        }
                        self.get_bit(reader)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }
    pub fn get_code<R: Read>(&mut self, num_bits: usize, reader: &mut R) -> Result<usize, std::io::Error> {
        let mut ans: usize = 0;
        match self.ord {
            Ord::Msb0 => {
                for _i in 0..num_bits {
                    ans <<= 1;
                    ans |= self.get_bit(reader)? as usize;
                }
            }
            Ord::Lsb0 => {
                for i in 0..num_bits {
                    ans |= (self.get_bit(reader)? as usize) << i;
                }
            }
        }
        self.count += 1;
        Ok(ans)
    }
}

/// Dictionary element, can be a key or value.
/// This stores an LZW code and a symbol, which typically is what we need to do a lookup during
/// encoding, or reconstruct a string during decoding.
#[derive(Clone)]
struct Link {
    code: usize,
    sym:  usize,
}

impl Link {
    fn root(code: usize) -> Self {
        // root can be identified by setting sym to any consistent
        // value that is out of range of valid codes
        Self { code, sym: usize::MAX }
    }
    fn create(code: usize, sym: usize) -> Self {
        Self { code, sym }
    }
    fn hash(&self) -> (usize, usize) {
        (self.code, self.sym)
    }
}

/// Structure to perform LZW compression.
struct LZW {
    opt: Options,
    /// when used in compression, (base_code,sym) maps to {code,*}.
    /// when used in expansion, (code,*) maps to {base_code,sym}
    dictionary: FoxHashMap<(usize, usize), Link>,
    /// the code most recently added to the dictionary
    curr_code: Option<usize>,
    /// the key that has just been matched
    curr_match: Option<Link>,
}

impl LZW {
    /// Create LZW structures, including initial dictionary, can
    /// also be used to reset LZW for a new block.
    /// Allowed to panic if options cannot be satisfied.
    fn create(opt: Options) -> Self {
        if opt.min_code_width != opt.max_code_width {
            panic!("variable code width not supported");
        }
        if opt.min_symbol != 0 {
            panic!("minimum symbol value must be 0");
        }
        let mut lzw = Self {
            opt: opt.clone(),
            dictionary: FoxHashMap::new(),
            curr_code: None,
            curr_match: None,
        };
        for i in opt.min_symbol..=opt.max_symbol {
            lzw.dictionary.insert(Link::root(i).hash(), Link::create(i, i));
        }
        lzw
    }
    /// Walk back through the concatenation sequence to form the string, this does a lookup
    /// for every symbol, so this may be where we pay the biggest price for suboptimal hashing.
    fn get_string(&self, mut code: usize) -> Vec<u8> {
        let mut rev = Vec::new();
        loop {
            let val = self.dictionary.get(&Link::root(code).hash()).unwrap();
            rev.push(val.sym as u8);
            if val.sym == val.code && code >= self.opt.min_symbol && code <= self.opt.max_symbol {
                break;
            }
            code = val.code
        }
        rev.iter().rev().map(|x| *x).collect()
    }
    /// Return the next available code, or None if bit width would be exceeded,
    /// Also updates `self.curr_code`, unless None is returned, in which case
    /// it retains the maximum value.
    fn advance_code(&mut self) -> Option<usize> {
        let max_code = (1usize << self.opt.max_code_width) - 1;
        let mut new_code = match self.curr_code {
            None => 0,
            Some(c) => c + 1,
        };
        loop {
            let test = new_code;
            if let Some(clear) = self.opt.clear_code {
                if new_code == clear {
                    new_code += 1;
                }
            }
            if let Some(stop) = self.opt.stop_code {
                if new_code == stop {
                    new_code += 1;
                }
            }
            if new_code >= self.opt.min_symbol && new_code <= self.opt.max_symbol {
                new_code = self.opt.max_symbol + 1;
            }
            if new_code == test {
                break;
            }
        }
        if new_code > max_code {
            self.curr_code = Some(max_code);
            return None;
        }
        self.curr_code = Some(new_code);
        Some(new_code)
    }
    /// Try to match concatenation of `self.curr_match` with `next_sym`.
    /// If matching, update `self.curr_match` and return `true`, caller should call again with the next symbol.
    /// If not matching, create a new dictionary entry and return `false`, caller should write the code for `self.curr_match`,
    /// then set `self.curr_match` to `None` and call again with the next symbol.
    /// If not matching and no more symbols available return `None`, caller can proceed as if `false` was returned,
    /// or choose to reset the dictionary.
    /// After calling this, `self.curr_match` should always be `Some`, assuming a valid dictionary.
    fn check_match(&mut self, next_sym: usize) -> Option<bool> {
        let search_key = match &self.curr_match {
            Some(curr_match) => {
                let base = self.dictionary.get(&curr_match.hash()).unwrap();
                Link::create(base.code, next_sym)
            }
            None => Link::root(next_sym),
        };
        match self.dictionary.contains_key(&search_key.hash()) {
            true => {
                self.curr_match = Some(search_key.clone());
                Some(true)
            }
            false => match self.advance_code() {
                Some(code) => {
                    self.dictionary.insert(search_key.hash(), Link::create(code, 0));
                    Some(false)
                }
                None => None,
            },
        }
    }
}

/// Main compression function.
/// `expanded_in` is an object with `Read` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<&[u8]>`.
/// `compressed_out` is an object with `Write` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<Vec<u8>>`.
/// Returns (in_size,out_size) or error.  Can panic if options are inconsistent.
pub fn compress<R, W>(expanded_in: &mut R, compressed_out: &mut W, opt: &Options) -> Result<(u64, u64), DYNERR>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let reader = expanded_in;
    let mut writer = compressed_out;
    let mut coder = LZWCoder::new(opt.ord);

    let mut expanded_length = reader.seek(SeekFrom::End(0))?;
    if opt.in_offset > expanded_length {
        return Err(Box::new(CompressionError::FileFormatMismatch));
    }
    expanded_length -= opt.in_offset;
    if expanded_length > opt.max_file_size {
        return Err(Box::new(CompressionError::FileTooLarge));
    }
    let mut write_offset_header = opt.out_offset;
    let mut read_chunk_offset = opt.in_offset;
    let mut old_coder_state = LZWCoder::new(opt.ord);
    let mut sym_in: [u8; 1] = [0];

    log::debug!("entering loop over chunks");
    loop {
        log::debug!("create LZW dictionary");
        let mut lzw = LZW::create(opt.clone());
        reader.seek(SeekFrom::Start(read_chunk_offset))?;
        writer.seek(SeekFrom::Start(write_offset_header))?;
        //placeholder for header
        if opt.header_bits > 0 {
            coder.put_code(opt.header_bits, 0, &mut writer);
        }
        coder.count = 0;
        //let mut lookahead = 0;
        log::debug!("entering loop over matches");
        loop {
            lzw.curr_match = None;
            // loop to build the longest possible match
            loop {
                match reader.read_exact(&mut sym_in) {
                    Ok(()) => {
                        match lzw.check_match(sym_in[0] as usize) {
                            Some(true) => {
                                // keep matching
                            }
                            Some(false) => {
                                // didn't match
                                break;
                            }
                            None => {
                                // didn't match and no more codes,
                                // choose to keep going with stale dictionary
                                break;
                            }
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                        if let Some(curr) = &lzw.curr_match {
                            let val = lzw.dictionary.get(&curr.hash()).unwrap(); // should never panic
                            coder.put_code(opt.max_code_width, val.code, &mut writer);
                        }
                        if let Some(code) = opt.stop_code {
                            coder.put_code(opt.max_code_width, code, &mut writer);
                        }
                        if opt.header_bits > 0 {
                            writer.seek(SeekFrom::Start(write_offset_header))?;
                            old_coder_state.put_code(
                                opt.header_bits,
                                coder.count * opt.max_code_width / opt.header_divisor,
                                &mut writer,
                            );
                        }
                        log::debug!("last chunk has {} codes", coder.count);
                        writer.seek(SeekFrom::End(0))?; // coder could be rewound
                        writer.flush()?;
                        return Ok((expanded_length, writer.stream_position()? - opt.out_offset));
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }
            // should never panic
            let curr = lzw.dictionary.get(&lzw.curr_match.as_ref().unwrap().hash()).unwrap();
            log::trace!("code: {}", curr.code);
            coder.put_code(opt.max_code_width, curr.code, &mut writer);
            // backup to try the character that didn't match again
            reader.seek_relative(-1)?;

            if coder.count >= opt.chunk_size {
                log::debug!("close chunk with {} codes", coder.count);
                if let Some(code) = opt.clear_code {
                    coder.put_code(opt.max_code_width, code, &mut writer);
                }
                let save_offset = writer.stream_position()?;
                if opt.header_bits > 0 {
                    writer.seek(SeekFrom::Start(write_offset_header))?;
                    old_coder_state.put_code(
                        opt.header_bits,
                        coder.count * opt.max_code_width / opt.header_divisor,
                        &mut writer,
                    );
                }
                old_coder_state = coder.clone();
                write_offset_header = save_offset;
                // back up to catch the character left in the dictionary that will be cleared
                read_chunk_offset = reader.stream_position()?; // - 1;
                break;
            }
        }
    }
}

/// Main decompression function.
/// `compressed_in` is an object with `Read` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<&[u8]>`.
/// `expanded_out` is an object with `Write` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<Vec<u8>>`.
/// Returns (in_size,out_size) or error.  Can panic if options are inconsistent.
pub fn expand<R, W>(compressed_in: &mut R, expanded_out: &mut W, opt: &Options) -> Result<(u64, u64), DYNERR>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let mut reader = compressed_in;
    let writer = expanded_out;
    let mut decoder = LZWDecoder::new(opt.ord);
    let mut compressed_size = reader.seek(SeekFrom::End(0))?;
    if opt.in_offset > compressed_size {
        return Err(Box::new(CompressionError::FileFormatMismatch));
    }
    compressed_size -= opt.in_offset;
    if compressed_size > opt.max_file_size {
        return Err(Box::new(CompressionError::FileTooLarge));
    }
    reader.seek(SeekFrom::Start(opt.in_offset))?;
    writer.seek(SeekFrom::Start(opt.out_offset))?;

    let mut end_of_data = false;
    log::trace!("expand(): entering loop over chunks");
    loop {
        log::trace!("expand(): creating LZW dictionary");
        let mut lzw = LZW::create(opt.clone());

        let chunk_bits = match opt.header_bits {
            0 => usize::MAX,
            num_bits => {
                log::trace!("expand(): read length of chunk");
                match decoder.get_code(num_bits, &mut reader) {
                    Ok(code) => opt.header_divisor * code,
                    Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                        break;
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }
        };
        lzw.curr_code = None;
        let mut prev_code = None;
        let mut prev_str = Vec::new();
        let mut bit_count = 0;

        log::trace!("expand(): enter main LZW loop");
        while bit_count < chunk_bits {
            let code = match decoder.get_code(opt.max_code_width, &mut reader) {
                Ok(c) => c,
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                    end_of_data = true;
                    break;
                }
                Err(e) => return Err(Box::new(e)),
            };
            if let Some(stop) = opt.stop_code {
                if code == stop {
                    end_of_data = true;
                    break;
                }
            }
            if let Some(clear) = opt.clear_code {
                if code == clear {
                    break;
                }
            }
            bit_count += opt.max_code_width;
            let next_code = match prev_code {
                None => None,
                Some(_) => lzw.advance_code(),
            };
            match lzw.dictionary.contains_key(&Link::root(code).hash()) {
                false => {
                    prev_str.push(prev_str[0]);
                    if next_code.is_none() {
                        log::error!("expand(): new code was needed but none were available");
                        return Err(Box::new(CompressionError::FileFormatMismatch));
                    }
                    if code != next_code.unwrap() {
                        log::error!("expand(): Bad LZW code, expected {}, got {}", next_code.unwrap(), code);
                        return Err(Box::new(CompressionError::FileFormatMismatch));
                    }
                }
                true => {
                    prev_str = lzw.get_string(code);
                }
            };
            if let (Some(next_code), Some(prev_code)) = (next_code, prev_code) {
                lzw.dictionary.insert(
                    Link::root(next_code).hash(),
                    Link::create(prev_code, prev_str[0] as usize),
                );
                log::trace!("expand(): add {} linking to {}.{}", next_code, prev_code, prev_str[0]);
            }
            _ = writer.write(&prev_str)?;
            log::trace!("expand():   write {} as {:?}", code, prev_str);
            prev_code = Some(code);
        }
        log::debug!("expand(): chunk completed with {} bits", bit_count);
        if end_of_data {
            break;
        }
    }
    log::debug!("expand(): end of data, closing stream");
    writer.flush()?;
    Ok((compressed_size, writer.stream_position()? - opt.out_offset))
}

/// Convenience function, calls `compress` with a slice returning a Vec
pub fn compress_slice(slice: &[u8], opt: &Options) -> Result<Vec<u8>, DYNERR> {
    let mut src = Cursor::new(slice);
    let mut ans: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    compress(&mut src, &mut ans, opt)?;
    Ok(ans.into_inner())
}

/// Convenience function, calls `expand` with a slice returning a Vec
pub fn expand_slice(slice: &[u8], opt: &Options) -> Result<Vec<u8>, DYNERR> {
    let mut src = Cursor::new(slice);
    let mut ans: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    expand(&mut src, &mut ans, opt)?;
    Ok(ans.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compression_works() {
        // Example adapted from wikipedia; in their example there are 26 symbols and # is a stop code.
        // Here # and newline are symbols, and the stop code is 0x101.
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        let test_data = "TOBEORNOTTOBEORTOBEORNOT#\n".as_bytes();
        let lzw_str = "054 04F 042 045 04F 052 04E 04F 054 102 104 106 10B 105 107 109 023 00A 101 0";
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        assert_eq!(compressed, hex::decode(lzw_str.replace(" ", "")).unwrap());
    }

    #[test]
    fn compression_works_16() {
        // Example adapted from wikipedia as above but with 16 bit codes
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        opt.min_code_width = 16;
        opt.max_code_width = 16;
        let test_data = "TOBEORNOTTOBEORTOBEORNOT#\n".as_bytes();
        let lzw_str = "0054 004F 0042 0045 004F 0052 004E 004F 0054 0102 0104 0106 010B 0105 0107 0109 0023 000A 0101";
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        assert_eq!(compressed, hex::decode(lzw_str.replace(" ", "")).unwrap());
    }

    #[test]
    fn compression_works_with_clear() {
        // Example adapted from wikipedia; in their example there are 26 symbols and # is a stop code.
        // Here # and newline are symbols, the stop code is 0x101, and we clear with 0x100 after 14 codes.
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        opt.chunk_size = 14;
        let test_data = "TOBEORNOTTOBEORTOBEORNOT#\n".as_bytes();
        let lzw_str = "054 04F 042 045 04F 052 04E 04F 054 102 104 106 10B 105 100 052 04E 04F 054 023 00A 101";
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        assert_eq!(compressed, hex::decode(lzw_str.replace(" ", "")).unwrap());
    }

    /*    #[test]
    fn compression_works_td_mode() {
        // Example adapted from wikipedia; in their example there are 26 symbols and # is a stop code.
        // Here # and newline are symbols, there is a header, and no stop code.
        let mut opt = Options::from(OptionsPreset::Teledisk);
        opt.in_offset = 0;
        opt.out_offset = 0;
        let test_data = "TOBEORNOTTOBEORTOBEORNOT#\n".as_bytes();
        let lzw_str = "36 00 54 F0 04 42 50 04 4F 20 05 4E F0 04 54 00 10 02 41 10 09 31 10 05 71 10 23 A0 00";
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        assert_eq!(compressed, hex::decode(lzw_str.replace(" ", "")).unwrap());
    }*/

    #[test]
    fn invertibility() {
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        let expanded = expand_slice(&compressed, &opt).expect("expansion failed");
        assert_eq!(test_data.to_vec(), expanded);
    }

    #[test]
    fn invertibility_16() {
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        opt.min_code_width = 16;
        opt.max_code_width = 16;
        let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        let expanded = expand_slice(&compressed, &opt).expect("expansion failed");
        assert_eq!(test_data.to_vec(), expanded);
    }

    #[test]
    fn invertibility_td_mode() {
        let mut opt = Options::from(OptionsPreset::Teledisk);
        opt.in_offset = 0;
        opt.out_offset = 0;
        let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        let expanded = expand_slice(&compressed, &opt).expect("expansion failed");
        assert_eq!(test_data.to_vec(), expanded);
    }

    #[test]
    fn invertibility_with_clear() {
        let mut opt = Options::from(OptionsPreset::Standard);
        opt.ord = Ord::Msb0;
        opt.chunk_size = 14;
        let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
        let compressed = compress_slice(test_data, &opt).expect("compression failed");
        let expanded = expand_slice(&compressed, &opt).expect("expansion failed");
        assert_eq!(test_data.to_vec(), expanded);
    }
}
