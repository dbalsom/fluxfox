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
use crate::io::{Read, Seek, SeekFrom};
use crate::{DiskImageError, ASCII_EOF};

pub const CRC_CCITT_INITIAL: u16 = 0xFFFF;

pub(crate) fn get_length<T: Seek>(source: &mut T) -> Result<u64, crate::io::Error> {
    // Seek to the end of the source
    let length = source.seek(SeekFrom::End(0))?;
    // Seek back to the beginning of the source
    source.seek(SeekFrom::Start(0))?;
    Ok(length)
}

pub(crate) fn read_ascii<T: Read>(source: &mut T, max_len: Option<usize>) -> (Option<String>, u8) {
    let mut string = String::new();
    let byte_iter = source.bytes();

    let mut terminating_byte = 0;

    for (i, byte) in byte_iter.enumerate() {
        match byte {
            Ok(b) => {
                if b < 32 || b == ASCII_EOF || !b.is_ascii() {
                    terminating_byte = b;
                    break;
                } else {
                    string.push(b as char);
                }
            }
            Err(_) => return (None, 0),
        }

        if i == max_len.unwrap_or(usize::MAX) {
            break;
        }
    }

    if string.is_empty() {
        (None, terminating_byte)
    } else {
        (Some(string), terminating_byte)
    }
}

/// Calculate a 16-bit checksum over a byte slice.
/// Note: previously attributed to CRC-CCITT.
/// See: https://reveng.sourceforge.io/crc-catalogue/16.htm
pub fn crc_ibm_3740(data: &[u8], start: Option<u16>) -> u16 {
    const POLY: u16 = 0x1021; // Polynomial x^16 + x^12 + x^5 + 1
    let mut crc: u16 = start.unwrap_or(0xFFFF);

    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ POLY;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Calculate a 16-bit checksum one byte at a time.
/// Note: previously attributed to CRC-CCITT.
/// See: https://reveng.sourceforge.io/crc-catalogue/16.htm
pub fn crc_ibm_3740_byte(byte: u8, crc: u16) -> u16 {
    const POLY: u16 = 0x1021; // Polynomial x^16 + x^12 + x^5 + 1
    let mut crc = crc;

    crc ^= (byte as u16) << 8;
    for _ in 0..8 {
        if (crc & 0x8000) != 0 {
            crc = (crc << 1) ^ POLY;
        } else {
            crc <<= 1;
        }
    }
    crc
}

pub fn dump_slice<W: crate::io::Write>(
    data_slice: &[u8],
    start_address: usize,
    bytes_per_row: usize,
    mut out: W,
) -> Result<(), DiskImageError> {
    let rows = data_slice.len() / bytes_per_row;
    let last_row_size = data_slice.len() % bytes_per_row;

    // Print all full rows.
    for r in 0..rows {
        out.write_fmt(format_args!("{:06X} | ", r * bytes_per_row + start_address))
            .unwrap();
        for b in 0..bytes_per_row {
            out.write_fmt(format_args!("{:02X} ", data_slice[r * bytes_per_row + b]))
                .unwrap();
        }
        out.write_fmt(format_args!("| ")).unwrap();
        for b in 0..bytes_per_row {
            let byte = data_slice[r * bytes_per_row + b];
            out.write_fmt(format_args!(
                "{}",
                if (40..=126).contains(&byte) { byte as char } else { '.' }
            ))
            .unwrap();
        }

        out.write_fmt(format_args!("\n")).unwrap();
    }

    // Print last incomplete row, if any bytes left over.
    if last_row_size > 0 {
        out.write_fmt(format_args!("{:06X} | ", rows * bytes_per_row)).unwrap();
        for b in 0..bytes_per_row {
            if b < last_row_size {
                out.write_fmt(format_args!("{:02X} ", data_slice[rows * bytes_per_row + b]))
                    .unwrap();
            } else {
                out.write_fmt(format_args!("   ")).unwrap();
            }
        }
        out.write_fmt(format_args!("| ")).unwrap();
        for b in 0..bytes_per_row {
            if b < last_row_size {
                let byte = data_slice[rows * bytes_per_row + b];
                out.write_fmt(format_args!(
                    "{}",
                    if (40..=126).contains(&byte) { byte as char } else { '.' }
                ))
                .unwrap();
            } else {
                out.write_fmt(format_args!(" ")).unwrap();
            }
        }
        out.write_fmt(format_args!("\n")).unwrap();
    }

    Ok(())
}
