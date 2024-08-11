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

use crate::MAXIMUM_SECTOR_SIZE;
use std::fmt::Display;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Default)]
pub struct DiskChsn {
    chs: DiskChs,
    n: u8,
}

impl From<(u16, u8, u8, u8)> for DiskChsn {
    fn from((c, h, s, n): (u16, u8, u8, u8)) -> Self {
        Self {
            chs: DiskChs::from((c, h, s)),
            n,
        }
    }
}

impl From<(DiskChs, u8)> for DiskChsn {
    fn from((chs, n): (DiskChs, u8)) -> Self {
        Self { chs, n }
    }
}

impl Display for DiskChsn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[c:{} h:{} s:{} n: {}]", self.c(), self.h(), self.s(), self.n)
    }
}

#[allow(dead_code)]
impl DiskChsn {
    pub fn new(c: u16, h: u8, s: u8, n: u8) -> Self {
        Self {
            chs: DiskChs::from((c, h, s)),
            n,
        }
    }

    pub fn get(&self) -> (u16, u8, u8, u8) {
        (self.c(), self.h(), self.s(), self.n())
    }
    pub fn c(&self) -> u16 {
        self.chs.c()
    }
    pub fn h(&self) -> u8 {
        self.chs.h()
    }
    pub fn s(&self) -> u8 {
        self.chs.s()
    }
    pub fn n(&self) -> u8 {
        self.n
    }
    /// Return the size of the 'n' parameter in bytes.
    /// The formula for calculating size from n is (128 * 2^n)
    /// We enforce a maximum size of 8192 bytes for a single sector.
    pub fn n_size(&self) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(self.n as u32).0)
    }

    pub fn n_to_bytes(n: u8) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(n as u32).0)
    }

    pub fn bytes_to_n(size: usize) -> u8 {
        let mut n = 0;
        let mut size = size;
        while size > 128 {
            size >>= 1;
            n += 1;
        }
        n
    }

    pub fn set(&mut self, c: u16, h: u8, s: u8, n: u8) {
        self.set_c(c);
        self.set_h(h);
        self.set_s(s);
        self.n = n;
    }
    pub fn set_c(&mut self, c: u16) {
        self.chs.set_c(c)
    }
    pub fn set_h(&mut self, h: u8) {
        self.chs.set_h(h)
    }
    pub fn set_s(&mut self, s: u8) {
        self.chs.set_s(s)
    }
    pub fn seek(&mut self, dst_chs: &DiskChs) {
        self.chs = *dst_chs;
    }

    /// Return the number of sectors represented by a DiskChs structure, interpreted as drive geometry.
    pub fn get_sector_count(&self) -> u32 {
        self.chs.get_sector_count()
    }

    /// Convert a DiskChs struct to an LBA sector address. A reference drive geometry is required to calculate the
    /// address.
    pub fn to_lba(&self, geom: &DiskChs) -> usize {
        self.chs.to_lba(geom)
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    pub(crate) fn get_next_sector(&self, geom: &DiskChs) -> DiskChs {
        self.chs.get_next_sector(geom)
    }

    pub(crate) fn seek_forward(&mut self, sectors: u32, geom: &DiskChs) -> &mut Self {
        self.chs.seek_forward(sectors, geom);
        self
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct DiskChs {
    c: u16,
    h: u8,
    s: u8,
}

impl Default for DiskChs {
    fn default() -> Self {
        Self { c: 0, h: 0, s: 1 }
    }
}

impl From<DiskChsn> for DiskChs {
    fn from(chsn: DiskChsn) -> Self {
        chsn.chs
    }
}

impl From<(u16, u8, u8)> for DiskChs {
    fn from((c, h, s): (u16, u8, u8)) -> Self {
        Self { c, h, s }
    }
}

impl From<DiskChs> for (u16, u8, u8) {
    fn from(chs: DiskChs) -> Self {
        (chs.c, chs.h, chs.s)
    }
}

impl From<(DiskCh, u8)> for DiskChs {
    fn from((ch, s): (DiskCh, u8)) -> Self {
        Self {
            c: ch.c(),
            h: ch.h(),
            s,
        }
    }
}

impl Display for DiskChs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[c:{} h:{} s:{}]", self.c, self.h, self.s)
    }
}

impl DiskChs {
    pub fn new(c: u16, h: u8, s: u8) -> Self {
        Self { c, h, s }
    }

    pub fn get(&self) -> (u16, u8, u8) {
        (self.c, self.h, self.s)
    }
    pub fn c(&self) -> u16 {
        self.c
    }
    pub fn h(&self) -> u8 {
        self.h
    }
    pub fn s(&self) -> u8 {
        self.s
    }

    pub fn set(&mut self, c: u16, h: u8, s: u8) {
        self.c = c;
        self.h = h;
        self.s = s;
    }
    pub fn set_c(&mut self, c: u16) {
        self.c = c;
    }
    pub fn set_h(&mut self, h: u8) {
        self.h = h;
    }
    pub fn set_s(&mut self, s: u8) {
        self.s = s;
    }

    /// Seek to the specified CHS. This should be called over 'set' as eventually it will calculate appropriate
    /// timings.
    pub fn seek(&mut self, c: u16, h: u8, s: u8) {
        self.seek_to(&DiskChs::from((c, h, s)));
    }

    /// Seek to the specified CHS. This should be called over 'set' as eventually it will calculate appropriate
    /// timings.
    pub fn seek_to(&mut self, dst_chs: &DiskChs) {
        self.c = dst_chs.c;
        self.h = dst_chs.h;
        self.s = dst_chs.s;
    }

    /// Return the number of sectors represented by a DiskChs structure, interpreted as drive geometry.
    pub fn get_sector_count(&self) -> u32 {
        (self.c as u32) * (self.h as u32) * (self.s as u32)
    }

    /// Convert a DiskChs struct to an LBA sector address. A reference drive geometry is required to calculate the
    /// address.
    pub fn to_lba(&self, geom: &DiskChs) -> usize {
        let hpc = geom.h as usize;
        let spt = geom.s as usize;
        (self.c as usize * hpc + (self.h as usize)) * spt + (self.s as usize - 1)
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    pub(crate) fn get_next_sector(&self, geom: &DiskChs) -> DiskChs {
        if self.s < geom.s {
            // Not at last sector, just return next sector
            DiskChs::from((self.c, self.h, self.s + 1))
        } else if self.h < geom.h - 1 {
            // At last sector, but not at last head, go to next head, same cylinder, sector 1
            DiskChs::from((self.c, self.h + 1, 1))
        } else if self.c < geom.c - 1 {
            // At last sector and last head, go to next cylinder, head 0, sector 1
            DiskChs::from((self.c + 1, 0, 1))
        } else {
            // Return start of drive? TODO: Research what does this do on real hardware
            DiskChs::from((0, 0, 1))
        }
    }

    pub(crate) fn seek_forward(&mut self, sectors: u32, geom: &DiskChs) -> &mut Self {
        for _i in 0..sectors {
            *self = self.get_next_sector(geom);
        }
        self
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Default)]
pub struct DiskCh {
    pub(crate) c: u16,
    pub(crate) h: u8,
}

impl From<(u16, u8)> for DiskCh {
    fn from((c, h): (u16, u8)) -> Self {
        Self { c, h }
    }
}

impl From<DiskChs> for DiskCh {
    fn from(chs: DiskChs) -> Self {
        Self { c: chs.c, h: chs.h }
    }
}

impl Display for DiskCh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[c:{} h:{}]", self.c, self.h)
    }
}

impl DiskCh {
    pub fn new(c: u16, h: u8) -> Self {
        Self { c, h }
    }

    pub fn c(&self) -> u16 {
        self.c
    }
    pub fn h(&self) -> u8 {
        self.h
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    pub fn get_next_track(&self, geom: &DiskChs) -> DiskCh {
        if self.h < geom.h - 1 {
            // At last sector, but not at last head, go to next head, same cylinder, sector 1
            DiskCh::from((self.c, self.h + 1))
        } else if self.c < geom.c - 1 {
            // At last sector and last head, go to next cylinder, head 0, sector 1
            DiskCh::from((self.c + 1, 0))
        } else {
            // Return start of drive? TODO: Research what does this do on real hardware
            DiskCh::from((0, 0))
        }
    }

    pub fn seek_next_track(&mut self, geom: &DiskChs) -> &mut Self {
        *self = self.get_next_track(geom);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskchsn_new_creates_correct_instance() {
        let chsn = DiskChsn::new(1, 2, 3, 4);
        assert_eq!(chsn.c(), 1);
        assert_eq!(chsn.h(), 2);
        assert_eq!(chsn.s(), 3);
        assert_eq!(chsn.n(), 4);
    }

    #[test]
    fn diskchsn_n_size_calculates_correct_size() {
        let chsn = DiskChsn::new(0, 0, 0, 3);
        assert_eq!(chsn.n_size(), 1024);
    }

    #[test]
    fn diskchsn_n_size_enforces_maximum_size() {
        let chsn = DiskChsn::new(0, 0, 0, 7);
        assert_eq!(chsn.n_size(), 8192);
    }

    #[test]
    fn diskchsn_size_to_n_calculates_correct_n() {
        assert_eq!(DiskChsn::size_to_n(1024), 3);
    }

    #[test]
    fn diskchs_to_lba_calculates_correct_lba() {
        let geom = DiskChs::new(40, 2, 9);
        let chs = DiskChs::new(2, 1, 5);
        assert_eq!(chs.to_lba(&geom), 49);
    }

    #[test]
    fn diskchs_get_next_sector_wraps_correctly() {
        let chs = DiskChs::new(1, 1, 2);
        let geom = DiskChs::new(40, 2, 2);

        let next_chs = chs.get_next_sector(&geom);
        assert_eq!(next_chs, DiskChs::new(2, 0, 1));
    }

    #[test]
    fn diskch_get_next_track_wraps_correctly() {
        let geom = DiskChs::new(2, 2, 2);
        let ch = DiskCh::new(1, 1);
        let next_ch = ch.get_next_track(&geom);
        assert_eq!(next_ch, DiskCh::new(0, 0));
    }
}
