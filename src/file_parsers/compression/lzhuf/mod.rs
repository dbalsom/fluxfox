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

pub mod adaptive_huff;
pub mod lzhuf;
pub mod node_pool;
pub mod ring_buffer;

/// Options controlling compression
#[derive(Clone)]
pub struct Options {
    /// whether to include an optional header
    header: bool,
    /// starting position in the input file
    in_offset: u64,
    /// starting position in the output file
    out_offset: u64,
    /// size of window, e.g., for LZSS dictionary
    window_size: usize,
    /// threshold, e.g. minimum length of match to encode
    threshold: usize,
    /// lookahead, e.g. for LZSS matches
    lookahead: usize,
    /// precursor symbol, e.g. backfill symbol for LZSS dictionary
    precursor: u8,
}

#[allow(unused)]
pub const STD_OPTIONS: Options = Options {
    header: true,
    in_offset: 0,
    out_offset: 0,
    window_size: 4096,
    threshold: 2,
    lookahead: 60,
    precursor: b' ',
};

pub const TD0_READ_OPTIONS: Options = Options {
    header: false,
    in_offset: 12,
    out_offset: 12,
    window_size: 4096,
    threshold: 2,
    lookahead: 60,
    precursor: b' ',
};

pub use lzhuf::expand;
