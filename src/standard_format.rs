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

    src/standard_format.rs

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
use crate::diskimage::DiskDescriptor;
use crate::{DiskCh, DiskChs, DiskChsn, DiskDataEncoding, DiskDataRate, DiskDensity, DiskRpm, DEFAULT_SECTOR_SIZE};
use std::fmt::{Display, Formatter};

/// An enumeration describing the type of disk image.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum StandardFormat {
    Invalid,
    PcFloppy160,
    PcFloppy180,
    PcFloppy320,
    PcFloppy360,
    PcFloppy720,
    PcFloppy1200,
    PcFloppy1440,
    PcFloppy2880,
}

impl Display for StandardFormat {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            StandardFormat::Invalid => write!(f, "Invalid"),
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

impl StandardFormat {
    /// Returns the CHSN geometry corresponding to the DiskImageType.
    pub fn get_chsn(&self) -> DiskChsn {
        match self {
            StandardFormat::Invalid => DiskChsn::new(1, 1, 1, 2),
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

    /// Returns the CHS geometry corresponding to the DiskImageType.
    pub fn get_chs(&self) -> DiskChs {
        self.get_chsn().into()
    }

    /// Returns the CH geometry corresponding to the DiskImageType.
    pub fn get_ch(&self) -> DiskCh {
        self.get_chs().into()
    }

    pub fn get_encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

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
            _ => DiskDataRate::Rate250Kbps(1.0),
        }
    }

    pub fn get_density(&self) -> DiskDensity {
        DiskDensity::from(self.get_data_rate())
    }

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
            _ => DiskRpm::Rpm300,
        }
    }

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
            _ => 100_000,
        }
    }

    /// Return a standard default GAP3 value for the disk format.
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
            _ => 0x54,
        }
    }

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
            _ => 0,
        }
    }
}

impl From<StandardFormat> for usize {
    fn from(format: StandardFormat) -> Self {
        format.size()
    }
}

impl From<usize> for StandardFormat {
    fn from(size: usize) -> Self {
        match size {
            163_840 => StandardFormat::PcFloppy160,
            184_320 => StandardFormat::PcFloppy180,
            327_680 => StandardFormat::PcFloppy320,
            368_640 => StandardFormat::PcFloppy360,
            737_280 => StandardFormat::PcFloppy720,
            1_228_800 => StandardFormat::PcFloppy1200,
            1_474_560 => StandardFormat::PcFloppy1440,
            2_949_120 => StandardFormat::PcFloppy2880,
            _ => StandardFormat::Invalid,
        }
    }
}
