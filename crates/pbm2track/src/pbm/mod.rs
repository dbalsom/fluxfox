/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2026 Daniel Balsom

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

use std::{fs, io};
use std::io::Read;

#[derive(Debug)]
#[allow(dead_code)]
pub enum PbmError {
    Io(io::Error),
    BadMagic,
    BadHeader,
    NotEnoughPixels,
}
impl From<io::Error> for PbmError {
    fn from(e: io::Error) -> Self {
        PbmError::Io(e)
    }
}

#[derive(Debug)]
pub struct Pbm {
    pub width: usize,
    pub height: usize,
    /// true = white (flux), false = black (no flux)
    /// row-major, y:0 -> top, y:height-1 -> bottom
    pixels_white: Vec<bool>,
}
impl Pbm {
    pub fn at(&self, x: usize, y: usize) -> bool {
        self.pixels_white[y * self.width + x]
    }

    pub fn load(path: &str) -> Result<Pbm, PbmError> {
        let mut f = fs::File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        if buf.len() < 3 {
            return Err(PbmError::BadMagic);
        }
        if !(buf[0] == b'P' && (buf[1] == b'1' || buf[1] == b'4')) {
            return Err(PbmError::BadMagic);
        }
        let binary = buf[1] == b'4';

        // Tokenizer over ASCII part (header and P1 data).
        let mut i = 2usize;
        while i < buf.len() && buf[i].is_ascii_whitespace() {
            i += 1;
        }
        let next_token = |start: &mut usize| -> Option<String> {
            let n = buf.len();
            let mut s = *start;
            loop {
                while s < n && buf[s].is_ascii_whitespace() {
                    s += 1;
                }
                if s >= n {
                    return None;
                }
                if buf[s] == b'#' {
                    while s < n && buf[s] != b'\n' {
                        s += 1;
                    }
                    continue;
                }
                let mut e = s;
                while e < n && !buf[e].is_ascii_whitespace() {
                    e += 1;
                }
                let tok = String::from_utf8_lossy(&buf[s..e]).to_string();
                *start = e;
                return Some(tok);
            }
        };

        let w: usize = next_token(&mut i)
            .ok_or(PbmError::BadHeader)?
            .parse()
            .map_err(|_| PbmError::BadHeader)?;
        let h: usize = next_token(&mut i)
            .ok_or(PbmError::BadHeader)?
            .parse()
            .map_err(|_| PbmError::BadHeader)?;

        let mut pixels_white = vec![false; w * h];
        if !binary {
            // P1: '0' = white, '1' = black
            for y in 0..h {
                for x in 0..w {
                    let t = next_token(&mut i).ok_or(PbmError::NotEnoughPixels)?;
                    match t.as_str() {
                        "0" => pixels_white[y * w + x] = true,
                        "1" => pixels_white[y * w + x] = false,
                        _ => return Err(PbmError::BadHeader),
                    }
                }
            }
        }
        else {
            // P4: rows top->bottom, MSB first, 1=black, 0=white
            while i < buf.len() && buf[i].is_ascii_whitespace() {
                i += 1;
            }
            let bpr = (w + 7) / 8;
            let need = h * bpr;
            if buf.len() < i + need {
                return Err(PbmError::NotEnoughPixels);
            }
            let mut k = i;
            for y in 0..h {
                for xb in 0..bpr {
                    let byte = buf[k];
                    k += 1;
                    for bit in 0..8 {
                        let x = xb * 8 + bit;
                        if x >= w {
                            break;
                        }
                        let v = (byte >> (7 - bit)) & 1;
                        pixels_white[y * w + x] = v == 0;
                    }
                }
            }
        }

        Ok(Pbm {
            width: w,
            height: h,
            pixels_white,
        })
    }
}