/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

use std::fmt::{self, Display, Formatter};

use crate::StandardFormat;

/// The type of computer system that a disk image is intended to be used with - not necessarily the
/// system that the disk image was created on.
///
/// A `Platform` may be used as a hint to a disk image format parser, or provided in a
/// [BitStreamTrackParams] struct to help determine the appropriate [TrackSchema] for a track.
/// A `Platform` may not be specified (or reliable) in all disk image formats, nor can it always
/// be determined from a [DiskImage] (High density MFM Macintosh 3.5" diskettes look nearly
/// identical to PC 3.5" diskettes, unless you examine the boot sector).
/// It may be the most pragmatic option to have the user specify the platform when loading/saving a
/// disk image.
#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, strum::EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Platform {
    /// IBM PC and compatibles
    IbmPc,
    /// Commodore Amiga
    Amiga,
    /// Apple Macintosh
    Macintosh,
    /// Atari ST
    AtariSt,
    /// Apple II
    AppleII,
}

impl Display for Platform {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Platform::IbmPc => write!(f, "IBM PC"),
            Platform::Amiga => write!(f, "Commodore Amiga"),
            Platform::Macintosh => write!(f, "Apple Macintosh"),
            Platform::AtariSt => write!(f, "Atari ST"),
            Platform::AppleII => write!(f, "Apple II"),
        }
    }
}

impl From<StandardFormat> for Platform {
    fn from(format: StandardFormat) -> Self {
        use crate::types::standard_format::StandardFormat::*;
        match format {
            PcFloppy160 | PcFloppy180 | PcFloppy320 | PcFloppy360 | PcFloppy720 | PcFloppy1200 | PcFloppy1440
            | PcFloppy2880 => Platform::IbmPc,
            #[cfg(feature = "amiga")]
            AmigaFloppy880 | AmigaFloppy1760 => Platform::Amiga,
        }
    }
}

impl Platform {
    // Try to convert a [TrackSchema] to a [Platform] based on a list of platforms to consider.
    // The list is required because some track schemas may be ambiguous (e.g. System34 used by
    // IBM PC, Macintosh and Atari ST).
    // pub fn try_from_schema(schema: TrackSchema, platforms: &[Platform]) -> Option<Self> {
    //     for platform in platforms {
    //         // if platforms.contains(&Platform::from(schema)) {
    //         //     return Some(Platform::from(schema));
    //         // }
    //
    //         match schema {
    //             TrackSchema::System34 => {
    //                 if platforms.contains(platform) {
    //                     Some(platform)
    //                 }
    //                 else {
    //                     None
    //                 }
    //             }
    //             #[cfg(feature = "amiga")]
    //             TrackSchema::Amiga => {
    //                 if platforms.contains(&Platform::Amiga) {
    //                     Some(Platform::Amiga)
    //                 }
    //                 else {
    //                     None
    //                 }
    //             }
    //             #[cfg(feature = "macintosh")]
    //             TrackSchema::Macintosh => {
    //                 if platforms.contains(&Platform::Macintosh) {
    //                     Some(Platform::Macintosh)
    //                 }
    //                 else {
    //                     None
    //                 }
    //             }
    //             #[cfg(feature = "atari_st")]
    //             TrackSchema::AtariSt => {
    //                 if platforms.contains(&Platform::AtariSt) {
    //                     Some(Platform::AtariSt)
    //                 }
    //                 else {
    //                     None
    //                 }
    //             }
    //         }
    //     }
    // }
}
