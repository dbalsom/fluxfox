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

//! Encoding and decoding utilities for various character sets / code pages.

pub mod cp_437;
pub mod iec8559_1;

use std::fmt::{self, Display, Formatter};

use cp_437::CP437;
use iec8559_1::ISO_IEC_8859_1;

const UNPRINTABLE: char = '.';

/// A simple enum to represent printable and unprintable characters.
/// The enum value contains the actual character if required.
/// The `display` method returns a display-compatible character, substituting
/// unprintable characters with the character specified by `UNPRINTABLE`.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
enum Chr {
    P(char),
    C(char),
}

impl Chr {
    fn display(&self) -> char {
        match self {
            Chr::P(c) => *c,
            Chr::C(c) => UNPRINTABLE,
        }
    }

    #[inline]
    fn encode(&self) -> char {
        match self {
            Chr::P(c) => *c,
            Chr::C(c) => *c,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Copy, Clone, Default, Debug, PartialEq, strum::EnumIter)]
pub enum CharacterEncoding {
    Cp437,
    #[default]
    IsoIec8559_1,
}

use CharacterEncoding::*;

impl Display for CharacterEncoding {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Cp437 => write!(f, "IBM OEM CP437"),
            IsoIec8559_1 => write!(f, "ISO/IEC 8859-1"),
        }
    }
}

impl CharacterEncoding {
    pub fn display_byte(&self, byte: u8) -> char {
        match self {
            Cp437 => CP437[byte as usize].display(),
            IsoIec8559_1 => ISO_IEC_8859_1[byte as usize].display(),
        }
    }

    pub fn slice_to_string(&self, bytes: &[u8]) -> String {
        let mut result = String::new();
        for byte in bytes.iter() {
            let char = match self {
                Cp437 => CP437[*byte as usize].display(),
                IsoIec8559_1 => ISO_IEC_8859_1[*byte as usize].display(),
            };
            result.push(char);
        }
        result
    }
}
