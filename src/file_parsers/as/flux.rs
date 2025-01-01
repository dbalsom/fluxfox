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

//! Module for flux conversion for MOOF and WOZ formats

//! Explanation of the flux format from AS documentation:
//!
//! > A quick explanation of the flux encoding: Each byte represents a single flux transition and
//! > its value is the number of ticks since the previous flux transition. A single tick is 125
//! > nanoseconds. Therefore the normal 2 microsecond spacing between sequential GCR 1 bits is
//! > represented by approximately 16 ticks. This also puts 101 and 1001 bit sequences at
//! > approximately 32 and 48 ticks. You are probably thinking to yourself that when it comes to
//! > longer runs of no transitions, how is this unsigned byte going to handle representing the
//! > time? That is taken care of via the special value of 255. When you encounter a 255, you need
//! > to keep adding the values up until you reach a byte that has a non-255 value. You then add
//! > this value to all of your accumulated 255s to give you the tick count. For example 255, 255,
//! > 10 should be treated as 255 + 255 + 10 = 520 ticks.

pub const AS_TICK_RES: f64 = 125e-9;

/// Decode AS-encoded flux data into a vector of flux times in seconds, and the total decoded time
/// in seconds.
pub fn decode_as_flux(buf: &[u8]) -> (Vec<f64>, f64) {
    let mut fts = Vec::new();
    let mut time = 0.0;
    let mut ticks = 0;
    for &byte in buf {
        if byte == 255 {
            //log::warn!("rollover!");
            ticks += 255;
        }
        else if byte > 0 {
            ticks += byte as u64;
            let ft_time = (ticks as f64) * AS_TICK_RES;
            time += ft_time;
            fts.push(ft_time);
            ticks = 0;
        }
    }

    if buf[buf.len() - 1] == 255 {
        log::warn!("decode_as_flux(): illegal last tick count (255)");
    }
    (fts, time)
}
