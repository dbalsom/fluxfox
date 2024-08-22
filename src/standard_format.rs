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

    Represents information about a standard (non-copy-protected) disk format,
    such as one that can be represented with a raw sector image (IMG)
*/
use crate::diskimage::DiskDescriptor;
use crate::{DiskCh, DiskChs, DiskDataEncoding, DiskDataRate, DiskRpm, DEFAULT_SECTOR_SIZE};

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

impl StandardFormat {
    /// Returns the CHS geometry corresponding to the DiskImageType.
    pub fn get_chs(&self) -> DiskChs {
        match self {
            StandardFormat::Invalid => DiskChs::new(1, 1, 1),
            StandardFormat::PcFloppy160 => DiskChs::new(40, 1, 8),
            StandardFormat::PcFloppy180 => DiskChs::new(40, 1, 9),
            StandardFormat::PcFloppy320 => DiskChs::new(40, 2, 8),
            StandardFormat::PcFloppy360 => DiskChs::new(40, 2, 9),
            StandardFormat::PcFloppy720 => DiskChs::new(80, 2, 9),
            StandardFormat::PcFloppy1200 => DiskChs::new(80, 2, 15),
            StandardFormat::PcFloppy1440 => DiskChs::new(80, 2, 18),
            StandardFormat::PcFloppy2880 => DiskChs::new(80, 2, 36),
        }
    }

    pub fn get_ch(&self) -> DiskCh {
        self.get_chs().into()
    }

    pub fn get_encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

    pub fn get_data_rate(&self) -> DiskDataRate {
        match self {
            StandardFormat::PcFloppy160 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy180 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy320 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy360 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy720 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy1200 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy1440 => DiskDataRate::Rate500Kbps,
            StandardFormat::PcFloppy2880 => DiskDataRate::Rate500Kbps,
            _ => DiskDataRate::Rate500Kbps,
        }
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

    pub fn get_image_format(&self) -> DiskDescriptor {
        DiskDescriptor {
            geometry: self.get_ch(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            data_encoding: DiskDataEncoding::Mfm,
            data_rate: DiskDataRate::Rate500Kbps,
            rpm: Some(DiskRpm::Rpm300),
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
