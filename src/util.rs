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

//! The `util` module provides various utility functions.

use regex::Regex;
use std::{cmp::Ordering, path::PathBuf};

use crate::{
    io::{Read, Seek, SeekFrom},
    DiskImageError,
};

/// The initial seed value for CRC-CCITT and related checksums.
pub const CRC_CCITT_INITIAL: u16 = 0xFFFF;

pub(crate) fn get_length<T: Seek>(source: &mut T) -> Result<u64, crate::io::Error> {
    // Seek to the end of the source
    let length = source.seek(SeekFrom::End(0))?;
    // Seek back to the beginning of the source
    source.seek(SeekFrom::Start(0))?;
    Ok(length)
}

pub(crate) fn read_ascii<T: Read>(
    source: &mut T,
    terminator: Option<u8>,
    max_len: Option<usize>,
) -> (Option<String>, u8) {
    let mut string = String::new();
    let byte_iter = source.bytes();
    let terminator = terminator.unwrap_or(0);
    let mut terminating_byte = 0;

    for (i, byte) in byte_iter.enumerate() {
        match byte {
            Ok(b) => {
                if b == terminator || b == 0 {
                    terminating_byte = b;
                    break;
                }
                else if b >= 32 && b.is_ascii() {
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
    }
    else {
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
            }
            else {
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
        }
        else {
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
        out.write_fmt(format_args!("{:05X} | ", r * bytes_per_row + start_address))?;
        for b in 0..bytes_per_row {
            out.write_fmt(format_args!("{:02X} ", data_slice[r * bytes_per_row + b]))?;
        }
        out.write_fmt(format_args!("| "))?;
        for b in 0..bytes_per_row {
            let byte = data_slice[r * bytes_per_row + b];
            out.write_fmt(format_args!(
                "{}",
                if (40..=126).contains(&byte) { byte as char } else { '.' }
            ))?;
        }

        out.write_fmt(format_args!("\n"))?;
    }

    // Print last incomplete row, if any bytes left over.
    if last_row_size > 0 {
        out.write_fmt(format_args!("{:05X} | ", rows * bytes_per_row))?;
        for b in 0..bytes_per_row {
            if b < last_row_size {
                out.write_fmt(format_args!("{:02X} ", data_slice[rows * bytes_per_row + b]))?;
            }
            else {
                out.write_fmt(format_args!("   "))?;
            }
        }
        out.write_fmt(format_args!("| "))?;
        for b in 0..bytes_per_row {
            if b < last_row_size {
                let byte = data_slice[rows * bytes_per_row + b];
                out.write_fmt(format_args!(
                    "{}",
                    if (40..=126).contains(&byte) { byte as char } else { '.' }
                ))?;
            }
            else {
                out.write_fmt(format_args!(" "))?;
            }
        }
        out.write_fmt(format_args!("\n"))?;
    }

    Ok(())
}

pub fn dump_string(data_slice: &[u8]) -> String {
    let mut out = String::new();
    for &byte in data_slice {
        out.push(if (40..=126).contains(&byte) { byte as char } else { '.' });
    }
    out
}

/// Sort `PathBuf`s in a natural order, by breaking them down into numeric and non-numeric parts.
/// This function is used to sort directory names in a natural order, so that Disk11 is sorted after
/// Disk2, etc.
#[allow(clippy::ptr_arg)]
pub fn natural_sort(a: &PathBuf, b: &PathBuf) -> Ordering {
    let re = Regex::new(r"(\D+)|(\d+)").expect("Invalid regex");

    let a_str = a.iter().next().and_then(|s| s.to_str()).unwrap_or("");
    let b_str = b.iter().next().and_then(|s| s.to_str()).unwrap_or("");

    let a_parts = re.captures_iter(a_str);
    let b_parts = re.captures_iter(b_str);

    for (a_part, b_part) in a_parts.zip(b_parts) {
        // Handle non-numeric parts, converting to lowercase for case-insensitive comparison
        if let (Some(a_text), Some(b_text)) = (a_part.get(1), b_part.get(1)) {
            let ordering = a_text.as_str().to_lowercase().cmp(&b_text.as_str().to_lowercase());
            if ordering != Ordering::Equal {
                return ordering;
            }
            continue;
        }

        // Handle numeric parts
        let a_num = a_part.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        let b_num = b_part.get(2).and_then(|m| m.as_str().parse::<u32>().ok());

        match (a_num, b_num) {
            (Some(a_num), Some(b_num)) => {
                let ordering = a_num.cmp(&b_num);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            // Fallback to lexicographic comparison if parsing fails
            _ => return a_str.to_lowercase().cmp(&b_str.to_lowercase()),
        }
    }

    // Fallback to comparing the full path if the directory names are identical
    a_str.to_lowercase().cmp(&b_str.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_natural_sort() {
        let mut paths = vec![
            PathBuf::from("Disk1"),
            PathBuf::from("disk10"),
            PathBuf::from("Disk2"),
            PathBuf::from("Disk3"),
            PathBuf::from("disk11"),
            PathBuf::from("Disk12"),
            PathBuf::from("Disk9"),
        ];

        // Sort using natural_sort function
        paths.sort_by(natural_sort);

        // Expected order: Disk1, Disk2, Disk3, Disk9, Disk10, Disk11, Disk12
        let expected_order = vec![
            PathBuf::from("Disk1"),
            PathBuf::from("Disk2"),
            PathBuf::from("Disk3"),
            PathBuf::from("Disk9"),
            PathBuf::from("disk10"),
            PathBuf::from("disk11"),
            PathBuf::from("Disk12"),
        ];

        assert_eq!(paths, expected_order);
    }

    #[test]
    fn test_natural_sort_with_paths() {
        let mut paths = vec![
            PathBuf::from("Disk10/track00.0.raw"),
            PathBuf::from("Disk11/track00.0.raw"),
            PathBuf::from("Disk12/track00.0.raw"),
            PathBuf::from("Disk13/track00.0.raw"),
            PathBuf::from("Disk14/track00.0.raw"),
            PathBuf::from("Disk15/track00.0.raw"),
            PathBuf::from("Disk1/track00.0.raw"),
            PathBuf::from("Disk2/track00.0.raw"),
            PathBuf::from("Disk3/track00.0.raw"),
            PathBuf::from("Disk4/track00.0.raw"),
            PathBuf::from("Disk5/track00.0.raw"),
            PathBuf::from("Disk6/track00.0.raw"),
        ];

        // Sort using natural_sort function
        paths.sort_by(natural_sort);

        // Expected order: Disk1, Disk2, ..., Disk15
        let expected_order = vec![
            PathBuf::from("Disk1/track00.0.raw"),
            PathBuf::from("Disk2/track00.0.raw"),
            PathBuf::from("Disk3/track00.0.raw"),
            PathBuf::from("Disk4/track00.0.raw"),
            PathBuf::from("Disk5/track00.0.raw"),
            PathBuf::from("Disk6/track00.0.raw"),
            PathBuf::from("Disk10/track00.0.raw"),
            PathBuf::from("Disk11/track00.0.raw"),
            PathBuf::from("Disk12/track00.0.raw"),
            PathBuf::from("Disk13/track00.0.raw"),
            PathBuf::from("Disk14/track00.0.raw"),
            PathBuf::from("Disk15/track00.0.raw"),
        ];

        assert_eq!(paths, expected_order);
    }
}
