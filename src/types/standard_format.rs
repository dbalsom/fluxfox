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

        PC   160K  DD Single-Sided 5.25"
        PC   180K  DD Single-Sided 5.25"
        PC   320K  DD Double-Sided 5.25"
        PC   360K  DD Double-Sided 5.25"
        PC   720K  DD Double-Sided 3.5"
        PC   1.2M  HD Double-Sided 5.25"
        PC   1.44M HD Double-Sided 3.5"
        PC   2.88M ED Double-Sided 3.5"
*/

//! The `standard_format` module defines the [StandardFormat] enum that defines parameters for
//! several standard PC disk formats.

use crate::{
    types::{DiskDataEncoding, DiskDataRate, DiskDensity, DiskDescriptor, DiskRpm},
    DiskCh,
    DiskChs,
    DiskChsn,
    DEFAULT_SECTOR_SIZE,
};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

/// A newtype for [StandardFormat] for use in parsing [StandardFormat] from user-provided strings,
/// such as command-line arguments.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct StandardFormatParam(pub StandardFormat);

impl FromStr for StandardFormatParam {
    type Err = String;
    /// Implement FromStr for StandardFormat.
    /// This can be used by utilities that wish to take a StandardFormat as a command-line argument.
    /// For backwards compatibility, formats strings can specify a pc_ prefix to refer to PC disk
    /// formats, but it is not required.
    /// Non-pc formats will require the appropriate prefix.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s
            .to_lowercase()
            .strip_prefix("pc_")
            .unwrap_or(s.to_lowercase().as_str())
        {
            "160k" => Ok(StandardFormatParam(StandardFormat::PcFloppy160)),
            "180k" => Ok(StandardFormatParam(StandardFormat::PcFloppy180)),
            "320k" => Ok(StandardFormatParam(StandardFormat::PcFloppy320)),
            "360k" => Ok(StandardFormatParam(StandardFormat::PcFloppy360)),
            "720k" => Ok(StandardFormatParam(StandardFormat::PcFloppy720)),
            "1200k" => Ok(StandardFormatParam(StandardFormat::PcFloppy1200)),
            "1440k" => Ok(StandardFormatParam(StandardFormat::PcFloppy1440)),
            "2880k" => Ok(StandardFormatParam(StandardFormat::PcFloppy2880)),
            _ => Err(format!("Invalid format: {}", s)),
        }
    }
}

impl Display for StandardFormatParam {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self.0 {
            StandardFormat::PcFloppy160 => write!(f, "pc_160k"),
            StandardFormat::PcFloppy180 => write!(f, "pc_180k"),
            StandardFormat::PcFloppy320 => write!(f, "pc_320k"),
            StandardFormat::PcFloppy360 => write!(f, "pc_360k"),
            StandardFormat::PcFloppy720 => write!(f, "pc_720k"),
            StandardFormat::PcFloppy1200 => write!(f, "pc_1200k"),
            StandardFormat::PcFloppy1440 => write!(f, "pc_1440k"),
            StandardFormat::PcFloppy2880 => write!(f, "pc_2880k"),
        }
    }
}

impl From<StandardFormat> for StandardFormatParam {
    fn from(format: StandardFormat) -> Self {
        StandardFormatParam(format)
    }
}

impl StandardFormatParam {
    /// Return a list of all supported StandardFormats and their string representations
    /// as StandardFormatParam's. This method can be used to generate help text for utilities
    /// that accept StandardFormat as a command-line argument.
    pub fn list() -> Vec<(String, StandardFormat)> {
        vec![
            ("pc_160k".to_string(), StandardFormat::PcFloppy160),
            ("pc_180k".to_string(), StandardFormat::PcFloppy180),
            ("pc_320k".to_string(), StandardFormat::PcFloppy320),
            ("pc_360k".to_string(), StandardFormat::PcFloppy360),
            ("pc_720k".to_string(), StandardFormat::PcFloppy720),
            ("pc_1200k".to_string(), StandardFormat::PcFloppy1200),
            ("pc_1440k".to_string(), StandardFormat::PcFloppy1440),
            ("pc_2880k".to_string(), StandardFormat::PcFloppy2880),
        ]
    }
}

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

impl From<StandardFormatParam> for StandardFormat {
    fn from(param: StandardFormatParam) -> Self {
        param.0
    }
}

impl StandardFormat {
    /// Return a vector of all StandardFormat variants.
    pub fn list() -> Vec<StandardFormat> {
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
    pub fn chsn(&self) -> DiskChsn {
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

    pub fn normalized_track_ct(track_ct: usize) -> Option<usize> {
        match track_ct {
            35..50 => Some(40),
            75..100 => Some(80),
            _ => None,
        }
    }

    pub fn sectors_per_track(&self) -> u8 {
        self.chsn().s()
    }

    /// Return the sector size in bytes corresponding to the `StandardFormat`.
    /// Note: This is always 512 for standard PC disk formats.
    pub fn sector_size(&self) -> usize {
        self.chsn().n_size()
    }

    /// Returns the geometry corresponding to the D`StandardFormat` as a `DiskChs` struct.
    pub fn chs(&self) -> DiskChs {
        self.chsn().into()
    }

    /// Returns the geometry corresponding to the `StandardFormat` as a `DiskCh` struct.
    pub fn ch(&self) -> DiskCh {
        self.chs().into()
    }

    /// Returns the `DiskDataEncoding` corresponding to the `StandardFormat`.
    pub fn encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

    /// Returns the `DiskDataRate` corresponding to the `StandardFormat`.
    pub fn data_rate(&self) -> DiskDataRate {
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
    pub fn density(&self) -> DiskDensity {
        DiskDensity::from(self.data_rate())
    }

    /// Returns the default `DiskRpm` corresponding to the `StandardFormat`.
    /// Note: The actual RPM of an image may vary depending on the drive used to create the disk image.
    pub fn rpm(&self) -> DiskRpm {
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
    pub fn bitcell_ct(&self) -> usize {
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
    pub fn gap3(&self) -> usize {
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
    pub fn descriptor(&self) -> DiskDescriptor {
        DiskDescriptor {
            geometry: self.ch(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            data_encoding: DiskDataEncoding::Mfm,
            density: self.density(),
            data_rate: self.data_rate(),
            rpm: Some(self.rpm()),
            write_protect: None,
        }
    }

    /// Return the size in bytes of a raw sector image corresponding to the `StandardFormat`.
    pub fn disk_size(&self) -> usize {
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

impl From<StandardFormat> for DiskCh {
    /// Convert a `StandardFormat` variant into a `DiskCh` struct.
    fn from(format: StandardFormat) -> Self {
        format.ch()
    }
}

impl From<StandardFormat> for DiskChs {
    /// Convert a `StandardFormat` variant into a `DiskChs` struct.
    fn from(format: StandardFormat) -> Self {
        format.chs()
    }
}

impl From<StandardFormat> for DiskChsn {
    /// Convert a `StandardFormat` variant into a `DiskChsn` struct.
    fn from(format: StandardFormat) -> Self {
        format.chsn()
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

impl TryFrom<DiskChs> for StandardFormat {
    type Error = String;
    /// Convert a `DiskChs` struct into a `StandardFormat` variant.
    fn try_from(chs: DiskChs) -> Result<Self, Self::Error> {
        StandardFormat::try_from(&chs)
    }
}

impl TryFrom<&DiskChs> for StandardFormat {
    type Error = String;
    /// Convert a `DiskChs` struct into a `StandardFormat` variant.
    fn try_from(chs: &DiskChs) -> Result<Self, Self::Error> {
        let chs = match chs.get() {
            (40, 1, 8) => StandardFormat::PcFloppy160,
            (40, 1, 9) => StandardFormat::PcFloppy180,
            (40, 2, 8) => StandardFormat::PcFloppy320,
            (40, 2, 9) => StandardFormat::PcFloppy360,
            (80, 2, 9) => StandardFormat::PcFloppy720,
            (80, 2, 15) => StandardFormat::PcFloppy1200,
            (80, 2, 18) => StandardFormat::PcFloppy1440,
            (80, 2, 36) => StandardFormat::PcFloppy2880,
            _ => return Err("Invalid geometry".to_string()),
        };
        Ok(chs)
    }
}

impl From<StandardFormat> for usize {
    /// Convert a `StandardFormat` variant into a size in bytes.
    fn from(format: StandardFormat) -> Self {
        format.disk_size()
    }
}
