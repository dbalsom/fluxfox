/*
    ffedit
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
use anyhow::{anyhow, Error};
use core::fmt;
use fluxfox::{DiskCh, DiskChs};
use std::fmt::Display;

/// Track the selection level
#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum SelectionLevel {
    #[default]
    Disk = 0,
    Head = 1,
    Cylinder = 2,
    Sector = 3,
}

#[derive(Copy, Clone)]
pub struct DiskSelection {
    pub level: SelectionLevel,
    pub head: Option<u8>,
    pub cylinder: Option<u16>,
    pub sector: Option<u8>,
}

impl Default for DiskSelection {
    fn default() -> Self {
        DiskSelection {
            level: SelectionLevel::Cylinder,
            head: Some(0),
            cylinder: Some(0),
            sector: None,
        }
    }
}

impl Display for DiskSelection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.level {
            SelectionLevel::Disk => write!(f, ""),
            SelectionLevel::Head => write!(f, "[h:{}]", self.head.unwrap_or(0)),
            SelectionLevel::Cylinder => write!(f, "[h:{} c:{}]", self.head.unwrap_or(0), self.cylinder.unwrap_or(0)),
            SelectionLevel::Sector => write!(
                f,
                "[h:{} c:{} s:{}]",
                self.head.unwrap_or(0),
                self.cylinder.unwrap_or(0),
                self.sector.unwrap_or(0)
            ),
        }
    }
}

impl DiskSelection {
    pub(crate) fn level(&self) -> SelectionLevel {
        self.level
    }
    pub(crate) fn into_ch(&self) -> Result<DiskCh, Error> {
        if self.level < SelectionLevel::Cylinder {
            return Err(anyhow!("Cylinder not selected"));
        }
        let c = self.cylinder.ok_or(anyhow!("No cylinder selected!"))?;
        let h = self.head.ok_or(anyhow!("No head selected!"))?;
        Ok(DiskCh::new(c, h))
    }

    pub(crate) fn into_chs(&self) -> Result<DiskChs, Error> {
        let ch = self.into_ch()?;
        if self.level < SelectionLevel::Sector {
            return Err(anyhow!("Sector not selected"));
        }
        let s = self.sector.ok_or(anyhow!("No sector selected!"))?;
        Ok(DiskChs::from((ch, s)))
    }
}
