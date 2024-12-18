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
#![allow(dead_code)]
//! A module for handling odd/even meta-encoding, such as that used by the
//! Commodore Amiga trackdisk system.
//!
//! Odd-even encoding operates on the principle of `block sizes` that control
//! the interleaving of data bits. The block size is the number of bits that
//! are interleaved in a single unit.
//!
//! For example, the Amiga defines three block sizes:
//! * `LONG`     - a 32-bit value, encoded as 16 odd 16 even bits.
//! * `LONGx4`   - 4x 32-bit values, encoded as 4x16 odd and 4x16 even bits.
//!                This block size is only used for a sectors `Sector Label` area.
//! * `BYTEx512` - 512x 8-bit values, encoded as 256 odd and 256 even bits.

const EVN_BITS_U64: u64 = 0x5555_5555_5555_5555;
const ODD_BITS_U64: u64 = 0xAAAA_AAAA_AAAA_AAAA;
const DATA_BITS_U64: u64 = EVN_BITS_U64;

const EVN_BITS_U32: u32 = 0x5555_5555;
const ODD_BITS_U32: u32 = 0xAAAA_AAAA;
const DATA_BITS_U32: u32 = EVN_BITS_U32;

const EVN_BITS_U16: u16 = 0x5555;
const ODD_BITS_U16: u16 = 0xAAAA;
const DATA_BITS_U16: u16 = EVN_BITS_U16;

const EVN_BITS_U8: u8 = 0x55;
const ODD_BITS_U8: u8 = 0xAA;

/// Decode interleaved Amiga data from raw, clock-aligned FM/MFM bytes.
/// This function will expect 8 raw FM/MFM bytes which will yield a single decoded u32 value.
/// Insufficient bytes will be return 0.
pub(crate) fn odd_even_decode_raw_u32_from_slice(bytes: &[u8]) -> u32 {
    if bytes.len() < 8 {
        return 0;
    }
    let odd_long = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let even_long = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    // Clock aligned FM/MFM data bits are always "even" as the MSB should always be a clock bit.
    // making the LSB a data bit.

    // Thus the 'odd bits', encoded first, need to be shifted left by 1 bit to become the odd
    // bits of the decoded value.

    // 0x55 => 0b0101_0101 (even bits (data))
    let dword = ((odd_long & 0x5555_5555) << 1) | (even_long & 0x5555_5555);
    dword
}

pub(crate) fn odd_even_decode_u8_pair(o: u8, e: u8) -> (u8, u8) {
    let mut o = (o as u16) << 8;
    let mut e = e as u16;

    // Distribute even bits to even positions
    e = (e | (e << 4)) & 0x0F0F;
    e = (e | (e << 2)) & 0x3333;
    e = (e | (e << 1)) & 0x5555;

    // Distribute odd bits to odd positions
    o = (o | (o >> 4)) & 0xF0F0;
    o = (o | (o >> 2)) & 0xCCCC;
    o = (o | (o >> 1)) & 0xAAAA;

    // Combine even and odd bits
    o |= e;
    ((o >> 8) as u8, o as u8)
}

pub(crate) fn odd_even_encode_u8_pair(x: u8, y: u8) -> (u8, u8) {
    let w = (x as u16) << 8 | y as u16;
    // Extract even bits into e, odd bits into o (adjusted)
    let mut e = w & 0x5555;
    let mut o = (w & 0xAAAA) >> 1;

    // Compress even bits downward into lower 8 bits
    e = (e | (e >> 1)) & 0x3333;
    e = (e | (e >> 2)) & 0x0F0F;
    e = (e | (e >> 4)) & 0x00FF;

    // Compress odd bits upward into upper 8 bits
    o = (o | (o << 1)) & 0xCCCC;
    o = (o | (o << 2)) & 0xF0F0;
    o = (o | (o << 4)) & 0xFF00;

    // Split out word
    ((o >> 8) as u8, e as u8)
}

/// Decode a pair of u32 values, representing odd and even bits, into two u32 values representing
/// the upper dword and lower dword of a 64 bit value (big endian)
pub(crate) fn odd_even_decode_u32_pair(o: u32, e: u32) -> (u32, u32) {
    let mut o = (o as u64) << 32;
    let mut e = e as u64;

    // Distribute even bits to even positions
    e = (e | (e << 16)) & 0x0000_FFFF_0000_FFFF;
    e = (e | (e << 8)) & 0x00FF_00FF_00FF_00FF;
    e = (e | (e << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    e = (e | (e << 2)) & 0x3333_3333_3333_3333;
    e = (e | (e << 1)) & 0x5555_5555_5555_5555;

    // Distribute odd bits to odd positions
    o = (o | (o >> 16)) & 0xFFFF_0000_FFFF_0000;
    o = (o | (o >> 8)) & 0xFF00_FF00_FF00_FF00;
    o = (o | (o >> 4)) & 0xF0F0_F0F0_F0F0_F0F0;
    o = (o | (o >> 2)) & 0xCCCC_CCCC_CCCC_CCCC;
    o = (o | (o >> 1)) & 0xAAAA_AAAA_AAAA_AAAA;

    // Combine even and odd bits
    o |= e;
    ((o >> 32) as u32, o as u32)
}

pub(crate) fn odd_even_encode_u32_pair(x: u32, y: u32) -> (u32, u32) {
    let w = (x as u64) << 32 | y as u64;
    // Extract even bits into e, odd bits into o (adjusted)
    let mut e = w & 0x5555_5555_5555_5555;
    let mut o = (w & 0xAAAA_AAAA_AAAA_AAAA) >> 1;

    // Compress even bits downward into lower 32 bits
    e = (e | (e >> 1)) & 0x3333_3333_3333_3333;
    e = (e | (e >> 2)) & 0x0F0F_0F0F_0F0F_0F0F;
    e = (e | (e >> 4)) & 0x00FF_00FF_00FF_00FF;
    e = (e | (e >> 8)) & 0x0000_FFFF_0000_FFFF;
    e = (e | (e >> 16)) & 0x0000_0000_FFFF_FFFF;

    // Compress odd bits upward into upper 8 bits
    o = (o | (o << 1)) & 0xCCCC_CCCC_CCCC_CCCC;
    o = (o | (o << 2)) & 0xF0F0_F0F0_F0F0_F0F0;
    o = (o | (o << 4)) & 0xFF00_FF00_FF00_FF00;
    o = (o | (o << 8)) & 0xFFFF_0000_FFFF_0000;
    o = (o | (o << 16)) & 0xFFFF_FFFF_0000_0000;

    // Split out dwords
    ((o >> 32) as u32, e as u32)
}

pub(crate) fn odd_even_decode_u32(x: u32) -> u32 {
    let mut o = x & 0xFFFF0000; // Extract odd bits compressed in upper 16 bits
    let mut e = x & 0x0000FFFF; // Extract even bits compressed in lower 16 bits

    // Distribute even bits to even positions
    e = (e | (e << 8)) & 0x00FF00FF;
    e = (e | (e << 4)) & 0x0F0F0F0F;
    e = (e | (e << 2)) & 0x33333333;
    e = (e | (e << 1)) & 0x55555555;

    // Distribute odd bits to odd positions
    o = (o | (o >> 8)) & 0xFF00FF00;
    o = (o | (o >> 4)) & 0xF0F0F0F0;
    o = (o | (o >> 2)) & 0xCCCCCCCC;
    o = (o | (o >> 1)) & 0xAAAAAAAA;

    // Combine even and odd bits
    o | e
}

/// Encode a normal u32 value into odd/even bit words
pub(crate) fn odd_even_encode_u32(x: u32) -> u32 {
    // Extract even bits into e, odd bits into o (adjusted)
    let mut e = x & 0x55555555;
    let mut o = (x & 0xAAAAAAAA) >> 1;

    // Compress even bits downward into lower 16 bits
    e = (e | (e >> 1)) & 0x33333333;
    e = (e | (e >> 2)) & 0x0F0F0F0F;
    e = (e | (e >> 4)) & 0x00FF00FF;
    e = (e | (e >> 8)) & 0x0000FFFF;

    // Compress odd bits upward into upper 16 bits
    o = (o | (o << 1)) & 0xCCCCCCCC;
    o = (o | (o << 2)) & 0xF0F0F0F0;
    o = (o | (o << 4)) & 0xFF00FF00;
    o = (o | (o << 8)) & 0xFFFF0000;

    // Combine both words
    o | e
}

/// Decode the odd/even-encoded u8 data in the `src` buffer into the `dst` buffer.
/// Odd/even split is assumed to be half source slice length. Both slices should be equal length.
pub(crate) fn odd_even_decode_u8_buf(src: &[u8], dst: &mut [u8]) {
    let (odds, evens) = src.split_at(src.len() / 2);
    for ((&odd_byte, &even_byte), dst_pair) in odds.iter().zip(evens).zip(dst.chunks_exact_mut(2)) {
        (dst_pair[0], dst_pair[1]) = odd_even_decode_u8_pair(odd_byte, even_byte);
    }
}

/// Encode the u8 data from `src` with odd/even encoding, saving to `dst` u8 buffer
/// Odd/even split is assumed to be half source slice length. Both slices should be equal length.
pub(crate) fn odd_even_encode_u8_buf(src: &[u8], dst: &mut [u8]) {
    let (odds, evens) = dst.split_at_mut(src.len() / 2);
    for ((dst_byte0, dst_byte1), src_pair) in odds.iter_mut().zip(evens).zip(src.chunks_exact(2)) {
        let (odd, even) = odd_even_encode_u8_pair(src_pair[0], src_pair[1]);
        *dst_byte0 = odd;
        *dst_byte1 = even;
    }
}

/// Decode the odd/even-encoded u32 data in the `src` buffer into the `dst` buffer.
/// Odd/even split is assumed to be half source slice length. Both slices should be equal length.
pub(crate) fn odd_even_decode_u32_buf(src: &[u32], dst: &mut [u32]) {
    let (odds, evens) = src.split_at(src.len() / 2);
    for ((&odd_byte, &even_byte), dst_pair) in odds.iter().zip(evens).zip(dst.chunks_exact_mut(2)) {
        (dst_pair[0], dst_pair[1]) = odd_even_decode_u32_pair(odd_byte, even_byte);
    }
}

/// Encode the sector data from `src` into odd/even encoding, saving to `dst` buffer
/// Odd/even split is assumed to be half source slice length. Both slices should be equal length.
pub(crate) fn odd_even_encode_u32_buf(src: &[u32], dst: &mut [u32]) {
    let (odds, evens) = dst.split_at_mut(src.len() / 2);
    for ((dst_byte0, dst_byte1), src_pair) in odds.iter_mut().zip(evens).zip(src.chunks_exact(2)) {
        let (odd, even) = odd_even_encode_u32_pair(src_pair[0], src_pair[1]);
        *dst_byte0 = odd;
        *dst_byte1 = even;
    }
}

/// Encode interleaved Amiga data from the u32 `value` into a buffer of 8, clock-aligned MFM bytes.
/// This function expects a mutable reference to a slice of at least 8 bytes, or else it does
/// nothing.
fn odd_even_encode_mfm_u32(value: u32, bytes: &mut [u8]) {
    if bytes.len() < 8 {
        return;
    }

    let odd_long = (value >> 1) & 0x5555_5555;
    let even_long = value & 0x5555_5555;
    odd_long
        .to_be_bytes()
        .iter()
        .enumerate()
        .for_each(|(i, &b)| bytes[i] = b);
    even_long
        .to_be_bytes()
        .iter()
        .enumerate()
        .for_each(|(i, &b)| bytes[i + 4] = b);
}

/// Encode interleaved Amiga data from the u32 `value` into a buffer of 8, clock-aligned FM bytes.
/// This function expects a mutable reference to a slice of at least 8 bytes, or else it does
/// nothing.
fn odd_even_encode_fm_u32(value: u32, bytes: &mut [u8]) {
    if bytes.len() < 8 {
        return;
    }

    let odd_long = ((value >> 1) & 0x5555_5555) | 0xAAAA_AAAA;
    let even_long = (value & 0x5555_5555) | 0xAAAA_AAAA;
    odd_long
        .to_be_bytes()
        .iter()
        .enumerate()
        .for_each(|(i, &b)| bytes[i] = b);
    even_long
        .to_be_bytes()
        .iter()
        .enumerate()
        .for_each(|(i, &b)| bytes[i + 4] = b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_odd_even_decode_u8_pair() {
        let (b0, b1) = odd_even_decode_u8_pair(0x00, 0xFF);
        println!("b0: {:02x}, b1: {:02x}", b0, b1);
        assert_eq!(b0, 0x55);
        assert_eq!(b1, 0x55);

        let (b0, b1) = odd_even_decode_u8_pair(0xFF, 0x00);
        println!("b0: {:02x}, b1: {:02x}", b0, b1);
        assert_eq!(b0, 0xAA);
        assert_eq!(b1, 0xAA);
    }

    #[test]
    fn test_odd_even_encode_u8_pair() {
        let (b0, b1) = odd_even_encode_u8_pair(0x55, 0x55);
        assert_eq!(b0, 0x00);
        assert_eq!(b1, 0xFF);

        let (b0, b1) = odd_even_encode_u8_pair(0xAA, 0xAA);
        assert_eq!(b0, 0xFF);
        assert_eq!(b1, 0x00);
    }

    #[test]
    fn test_odd_even_decode_u32_pair() {
        let (b0, b1) = odd_even_decode_u32_pair(0u32, !0u32);
        println!("b0: {:02x}, b1: {:02x}", b0, b1);
        assert_eq!(b0, EVN_BITS_U32);
        assert_eq!(b1, EVN_BITS_U32);

        let (b0, b1) = odd_even_decode_u32_pair(!0u32, 0x00);
        println!("b0: {:02x}, b1: {:02x}", b0, b1);
        assert_eq!(b0, ODD_BITS_U32);
        assert_eq!(b1, ODD_BITS_U32);
    }

    #[test]
    fn test_odd_even_encode_u32_pair() {
        let (dw0, dw1) = odd_even_encode_u32_pair(EVN_BITS_U32, EVN_BITS_U32);
        assert_eq!(dw0, 0u32);
        assert_eq!(dw1, !0u32);

        let (b0, b1) = odd_even_encode_u32_pair(ODD_BITS_U32, ODD_BITS_U32);
        assert_eq!(b0, !0u32);
        assert_eq!(b1, 0);
    }

    #[test]
    fn test_odd_even_decode_u8_buf() {
        // Create a test sector, with all the even bits set.
        let mut v = vec![0u8; 256];
        v.extend(vec![0xFFu8; 256]);

        // Create a buffer to decode into.
        let mut decoded = vec![0u8; 512];

        odd_even_decode_u8_buf(&v, &mut decoded);
        assert_eq!(decoded, vec![0x55u8; 512]);

        // Create a test sector, with all the odd bits set.
        let mut v = vec![0xFFu8; 256];
        v.extend(vec![0x00u8; 256]);

        odd_even_decode_u8_buf(&v, &mut decoded);
        assert_eq!(decoded, vec![0xAAu8; 512]);
    }

    #[test]
    fn test_odd_even_encode_u8_buf() {
        let v = vec![0x55u8; 512];
        let mut encoded = vec![0u8; 512];

        odd_even_encode_u8_buf(&v, &mut encoded);
        assert_eq!(encoded[..256], vec![0u8; 256]);
        assert_eq!(encoded[256..], vec![0xFFu8; 256]);
    }

    #[test]
    fn test_odd_even_decode_u32_buf() {
        // Create a test sector label, 4 entries of 0 and 4 entries all bits set.
        let mut v = vec![0u32; 4];
        v.extend(vec![!0u32; 4]);

        // Create a buffer to decode into.
        let mut decoded = vec![0u32; 4];

        odd_even_decode_u32_buf(&v, &mut decoded);
        assert_eq!(decoded, vec![EVN_BITS_U32; 4]);

        // Create a test sector label, with all the odd bits set, even bits 0.
        let mut v = vec![!0u32; 4];
        v.extend(vec![0u32; 4]);

        odd_even_decode_u32_buf(&v, &mut decoded);
        // Decoded buffer should have only odd bits set.
        assert_eq!(decoded, vec![ODD_BITS_U32; 4]);
    }

    #[test]
    fn test_odd_even_encode_u32_buf() {
        let v = vec![0x55u8; 512];
        let mut encoded = vec![0u8; 512];

        odd_even_encode_u8_buf(&v, &mut encoded);
        assert_eq!(encoded[..256], vec![0u8; 256]);
        assert_eq!(encoded[256..], vec![0xFFu8; 256]);
    }

    #[test]
    fn test_odd_even_encode_u32() {
        let original: u32 = 0xFF00_000B;
        let encoded = odd_even_encode_u32(original);
        assert_eq!(encoded, 0xF003_F001);
    }

    #[test]
    fn test_odd_even_encode_decode_u32() {
        let original: u32 = 0xFF00_000B;
        let encoded = odd_even_encode_u32(original);
        let decoded = odd_even_decode_u32(encoded);
        assert_eq!(original, decoded);
    }
}
