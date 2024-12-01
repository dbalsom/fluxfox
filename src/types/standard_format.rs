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

    types/standard_format.rs

    Represents information about standard (non-copy-protected) disk formats,
    such as those that can be represented with a raw sector image (IMG).

    Since the formats are well known, we can provide many default parameters
    for them.

    fluxfox currently supports (or aims to support) the following formats:

        160K  DD Single-Sided 5.25"
        180K  DD Single-Sided 5.25"
        320K  DD Double-Sided 5.25"
        360K  DD Double-Sided 5.25"
        720K  DD Double-Sided 3.5"
        1.2M  HD Double-Sided 5.25"
        1.44M HD Double-Sided 3.5"
        2.88M ED Double-Sided 3.5"
*/

//! The `standard_format` module defines the `StandardFormat` enum that defines parameters for
//! several standard PC disk formats.

use crate::{
    types::structs::DiskDescriptor,
    DiskCh,
    DiskChs,
    DiskChsn,
    DiskDataEncoding,
    DiskDataRate,
    DiskDensity,
    DiskRpm,
    DEFAULT_SECTOR_SIZE,
};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

/// An enumeration describing one of several standard PC disk formats.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StandardFormat {
    /// A single-sided, 8-sectored, 48tpi, double-density disk.
    PcFloppy160,
    /// A single-sided, 9-sectored, 48tpi, double-density disk.
    PcFloppy180,
    /// A double-sided, 8-sectored, 48tpi, double-density disk.
    PcFloppy320,
    /// A double-sided, 9-sectored, 48tpi, double-density disk.
    PcFloppy360,
    /// A double-sided, 9-sectored, 96tpi, double-density disk.
    PcFloppy720,
    /// A double-sided, 15-sectored, 96tpi, high-density disk.
    PcFloppy1200,
    /// A double-sided, 18-sectored, 96tpi, high-density disk.
    PcFloppy1440,
    /// A double-sided, 36-sectored, 96tpi, high-density disk.
    PcFloppy2880,
}

impl Display for StandardFormat {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            StandardFormat::PcFloppy160 => write!(f, "160K 5.25\" DD"),
            StandardFormat::PcFloppy180 => write!(f, "180K 5.25\" DD"),
            StandardFormat::PcFloppy320 => write!(f, "320K 5.25\" DD"),
            StandardFormat::PcFloppy360 => write!(f, "360K 5.25\" DD"),
            StandardFormat::PcFloppy720 => write!(f, "720K 3.5\" DD"),
            StandardFormat::PcFloppy1200 => write!(f, "1.2M 5.25\" HD"),
            StandardFormat::PcFloppy1440 => write!(f, "1.44M 3.5\" HD"),
            StandardFormat::PcFloppy2880 => write!(f, "2.88M 3.5\" ED"),
        }
    }
}

impl FromStr for StandardFormat {
    type Err = String;
    /// Implement FromStr for StandardFormat.
    /// This can be used by utilities that wish to take a StandardFormat as a command-line argument.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "160k" => Ok(StandardFormat::PcFloppy160),
            "180k" => Ok(StandardFormat::PcFloppy180),
            "320k" => Ok(StandardFormat::PcFloppy320),
            "360k" => Ok(StandardFormat::PcFloppy360),
            "720k" => Ok(StandardFormat::PcFloppy720),
            "1200k" => Ok(StandardFormat::PcFloppy1200),
            "1440k" => Ok(StandardFormat::PcFloppy1440),
            "2880k" => Ok(StandardFormat::PcFloppy2880),
            _ => Err(format!("Invalid format: {}", s)),
        }
    }
}

impl StandardFormat {
    /// Return a vector of all StandardFormat variants.
    pub fn list(&self) -> Vec<StandardFormat> {
        vec![
            StandardFormat::PcFloppy160,
            StandardFormat::PcFloppy180,
            StandardFormat::PcFloppy320,
            StandardFormat::PcFloppy360,
            StandardFormat::PcFloppy720,
            StandardFormat::PcFloppy1200,
            StandardFormat::PcFloppy1440,
            StandardFormat::PcFloppy2880,
        ]
    }

    /// Returns the geometry corresponding to the `StandardFormat` as a `DiskChsn` struct.
    pub fn get_chsn(&self) -> DiskChsn {
        match self {
            StandardFormat::PcFloppy160 => DiskChsn::new(40, 1, 8, 2),
            StandardFormat::PcFloppy180 => DiskChsn::new(40, 1, 9, 2),
            StandardFormat::PcFloppy320 => DiskChsn::new(40, 2, 8, 2),
            StandardFormat::PcFloppy360 => DiskChsn::new(40, 2, 9, 2),
            StandardFormat::PcFloppy720 => DiskChsn::new(80, 2, 9, 2),
            StandardFormat::PcFloppy1200 => DiskChsn::new(80, 2, 15, 2),
            StandardFormat::PcFloppy1440 => DiskChsn::new(80, 2, 18, 2),
            StandardFormat::PcFloppy2880 => DiskChsn::new(80, 2, 36, 2),
        }
    }

    /// Returns the geometry corresponding to the D`StandardFormat` as a `DiskChs` struct.
    pub fn get_chs(&self) -> DiskChs {
        self.get_chsn().into()
    }

    /// Returns the geometry corresponding to the `StandardFormat` as a `DiskCh` struct.
    pub fn get_ch(&self) -> DiskCh {
        self.get_chs().into()
    }

    /// Returns the `DiskDataEncoding` corresponding to the `StandardFormat`.
    pub fn get_encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

    /// Returns the `DiskDataRate` corresponding to the `StandardFormat`.
    pub fn get_data_rate(&self) -> DiskDataRate {
        match self {
            StandardFormat::PcFloppy160 => DiskDataRate::Rate250Kbps(1.0),
            StandardFormat::PcFloppy180 => DiskDataRate::Rate250Kbps(1.0),
            StandardFormat::PcFloppy320 => DiskDataRate::Rate250Kbps(1.0),
            StandardFormat::PcFloppy360 => DiskDataRate::Rate250Kbps(1.0),
            StandardFormat::PcFloppy720 => DiskDataRate::Rate250Kbps(1.0),
            StandardFormat::PcFloppy1200 => DiskDataRate::Rate500Kbps(1.0),
            StandardFormat::PcFloppy1440 => DiskDataRate::Rate500Kbps(1.0),
            StandardFormat::PcFloppy2880 => DiskDataRate::Rate1000Kbps(1.0),
        }
    }

    /// Returns the `DiskDensity` corresponding to the `StandardFormat`.
    pub fn get_density(&self) -> DiskDensity {
        DiskDensity::from(self.get_data_rate())
    }

    /// Returns the default `DiskRpm` corresponding to the `StandardFormat`.
    /// Note: The actual RPM of an image may vary depending on the drive used to create the disk image.
    pub fn get_rpm(&self) -> DiskRpm {
        match self {
            StandardFormat::PcFloppy160 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy180 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy320 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy360 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy720 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy1200 => DiskRpm::Rpm360,
            StandardFormat::PcFloppy1440 => DiskRpm::Rpm300,
            StandardFormat::PcFloppy2880 => DiskRpm::Rpm300,
        }
    }

    /// Return the number of bitcells per track corresponding to the `StandardFormat`.
    pub fn get_bitcell_ct(&self) -> usize {
        match self {
            StandardFormat::PcFloppy160 => 100_000,
            StandardFormat::PcFloppy180 => 100_000,
            StandardFormat::PcFloppy320 => 100_000,
            StandardFormat::PcFloppy360 => 100_000,
            StandardFormat::PcFloppy720 => 100_000,
            StandardFormat::PcFloppy1200 => 166_666,
            StandardFormat::PcFloppy1440 => 200_000,
            StandardFormat::PcFloppy2880 => 400_000,
        }
    }

    /// Return a standard default GAP3 value corresponding to the `StandardFormat`.
    pub fn get_gap3(&self) -> usize {
        match self {
            StandardFormat::PcFloppy160 => 0x50,
            StandardFormat::PcFloppy180 => 0x50,
            StandardFormat::PcFloppy320 => 0x50,
            StandardFormat::PcFloppy360 => 0x50,
            StandardFormat::PcFloppy720 => 0x50,
            StandardFormat::PcFloppy1200 => 0x54,
            StandardFormat::PcFloppy1440 => 0x6C,
            StandardFormat::PcFloppy2880 => 0x53,
        }
    }

    /// Return a standard `DiskDescriptor` struct corresponding to the `StandardFormat`.
    pub fn get_descriptor(&self) -> DiskDescriptor {
        DiskDescriptor {
            geometry: self.get_ch(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            data_encoding: DiskDataEncoding::Mfm,
            density: self.get_density(),
            data_rate: self.get_data_rate(),
            rpm: Some(self.get_rpm()),
            write_protect: None,
        }
    }

    /// Return the size in bytes of a raw sector image corresponding to the `StandardFormat`.
    pub fn size(&self) -> usize {
        match self {
            StandardFormat::PcFloppy160 => 163_840,
            StandardFormat::PcFloppy180 => 184_320,
            StandardFormat::PcFloppy320 => 327_680,
            StandardFormat::PcFloppy360 => 368_640,
            StandardFormat::PcFloppy720 => 737_280,
            StandardFormat::PcFloppy1200 => 1_228_800,
            StandardFormat::PcFloppy1440 => 1_474_560,
            StandardFormat::PcFloppy2880 => 2_949_120,
        }
    }
}

impl TryFrom<usize> for StandardFormat {
    type Error = String;

    /// Convert a size in bytes to a `StandardFormat` variant.
    fn try_from(size: usize) -> Result<Self, Self::Error> {
        let size = match size {
            163_840 => StandardFormat::PcFloppy160,
            184_320 => StandardFormat::PcFloppy180,
            327_680 => StandardFormat::PcFloppy320,
            368_640 => StandardFormat::PcFloppy360,
            737_280 => StandardFormat::PcFloppy720,
            1_228_800 => StandardFormat::PcFloppy1200,
            1_474_560 => StandardFormat::PcFloppy1440,
            2_949_120 => StandardFormat::PcFloppy2880,
            _ => return Err("Invalid size".to_string()),
        };
        Ok(size)
    }
}

impl From<StandardFormat> for usize {
    /// Convert a `StandardFormat` variant into a size in bytes.
    fn from(format: StandardFormat) -> Self {
        format.size()
    }
}
