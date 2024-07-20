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
use crate::ASCII_EOF;

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
                if b == ASCII_EOF || !b.is_ascii() {
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
