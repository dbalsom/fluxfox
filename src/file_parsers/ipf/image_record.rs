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
use crate::types::DiskCh;
use binrw::binrw;
use std::cmp::Ordering;
use strum::IntoEnumIterator;

#[repr(u32)]
#[derive(Copy, Clone, Debug, strum::EnumIter)]
pub enum IpfTrackDensity {
    Unknown,
    Noise = 1,
    Auto = 2,
    CopylockAmiga = 3,
    CopylockAmigaNew = 4,
    CopylockSt = 5,
    SpeedlockAmiga = 6,
    OldSpeedlockAmiga = 7,
    AdamBrierleyAmiga = 8,
    AdamBrierleyDensityKeyAmiga = 9,
}

impl From<u32> for IpfTrackDensity {
    /// Since IpfTrackDensity has an Unknown variant, we can implement From<u32> directly for it.
    /// All undefined values will be mapped to Unknown.
    fn from(value: u32) -> IpfTrackDensity {
        IpfTrackDensity::iter()
            .find(|x| *x as u32 == value)
            .unwrap_or(IpfTrackDensity::Unknown)
    }
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub(crate) struct ImageRecord {
    pub(crate) track: u32,   // Track (cylinder) number
    pub(crate) side: u32,    // Side (head) number
    pub(crate) density: u32, // Density of the track
    #[bw(ignore)]
    #[br(calc = <IpfTrackDensity>::from(density))]
    pub(crate) density_enum: IpfTrackDensity,
    pub(crate) signal_type: u32,     // Signal processing type
    pub(crate) track_bytes: u32,     // Rounded number of decoded bytes on track
    pub(crate) start_byte_pos: u32,  // Rounded start byte position (useless)
    pub(crate) start_bit_pos: u32,   // Start position in bits of the first sync bit
    pub(crate) data_bits: u32,       // Number of decoded data bits (clock + data)
    pub(crate) gap_bits: u32,        // Number of decoded gap bits (clock + data)
    pub(crate) track_bits: u32,      // Total number of bits on the track (useless)
    pub(crate) block_count: u32,     // Number of blocks describing one track
    pub(crate) encoder_process: u32, // Encoder process
    pub(crate) track_flags: u32,     // Track flags
    pub(crate) data_key: u32,        // Unique key matching the DATA record
    pub(crate) reserved: [u32; 3],   // Reserved for future use
}

impl ImageRecord {
    #[inline]
    pub fn key(&self) -> u32 {
        self.data_key
    }

    pub fn ch(&self) -> DiskCh {
        DiskCh {
            c: self.track as u16,
            h: self.side as u8,
        }
    }
}

impl Eq for ImageRecord {}

impl PartialEq<Self> for ImageRecord {
    fn eq(&self, other: &Self) -> bool {
        self.track == other.track && self.side == other.side
    }
}

impl PartialOrd for ImageRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImageRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        // Construct DiskCh for comparison
        let self_disk_ch = DiskCh {
            c: self.track as u16,
            h: self.side as u8,
        };
        let other_disk_ch = DiskCh {
            c: other.track as u16,
            h: other.side as u8,
        };
        self_disk_ch.cmp(&other_disk_ch)
    }
}
