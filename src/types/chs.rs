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

//! The `chs` module defines several structures for working with Cylinder-Head-Sector (CHS)
//! addressing and sector IDs.

use crate::MAXIMUM_SECTOR_SIZE;
use std::fmt::Display;

/// A structure representing a query against the four components of sector header:
///  - Cylinder ID (c)
///  - Head ID (h)
///  - Sector ID (s)
///  - Sector Size (n)
///
/// The only required field in a `DiskChsnQuery` is the Sector ID field.
/// Any other field may be set to None to indicate that it should be ignored when matching.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Default)]
pub struct DiskChsnQuery {
    c: Option<u16>,
    h: Option<u8>,
    s: u8,
    n: Option<u8>,
}

impl Display for DiskChsnQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c_str = self.c.as_ref().map_or("*".to_string(), |c| c.to_string());
        let h_str = self.h.as_ref().map_or("*".to_string(), |h| h.to_string());
        let n_str = self.n.as_ref().map_or("*".to_string(), |n| n.to_string());
        write!(f, "[c:{} h:{:?} s:{} n:{:?}]", c_str, h_str, self.s, n_str)
    }
}

#[allow(dead_code)]
impl DiskChsnQuery {
    /// Create a new DiskChsnQuery structure from the four sector ID components.
    pub fn new(c: impl Into<Option<u16>>, h: impl Into<Option<u8>>, s: u8, n: impl Into<Option<u8>>) -> Self {
        Self {
            c: c.into(),
            h: h.into(),
            s,
            n: n.into(),
        }
    }
    /// Return the cylinder (c) field.
    pub fn c(&self) -> Option<u16> {
        self.c
    }
    /// Return the head (h) field.
    pub fn h(&self) -> Option<u8> {
        self.h
    }
    /// Return the sector id (s) field.
    pub fn s(&self) -> u8 {
        self.s
    }
    /// Return the size (n) field.
    pub fn n(&self) -> Option<u8> {
        self.n
    }
    /// Return the size of the 'n' parameter in bytes, or None if n is not set.
    /// The formula for calculating size from n is (128 * 2^n)
    /// We enforce a maximum size of 8192 bytes for a single sector.
    pub fn n_size(&self) -> Option<usize> {
        self.n
            .map(|n| std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(n as u32).0))
    }
    /// Return a boolean indicating whether the specified `DiskChsn` matches the query.
    pub fn matches(&self, id_chsn: DiskChsn) -> bool {
        if self.s != id_chsn.s() {
            return false;
        }
        if let Some(c) = self.c {
            if c != id_chsn.c() {
                return false;
            }
        }
        if let Some(h) = self.h {
            if h != id_chsn.h() {
                return false;
            }
        }
        if let Some(n) = self.n {
            if n != id_chsn.n() {
                return false;
            }
        }
        true
    }
}

impl From<DiskChsn> for DiskChsnQuery {
    fn from(chsn: DiskChsn) -> Self {
        Self {
            c: Some(chsn.c()),
            h: Some(chsn.h()),
            s: chsn.s(),
            n: Some(chsn.n()),
        }
    }
}

impl From<DiskChs> for DiskChsnQuery {
    fn from(chs: DiskChs) -> Self {
        Self {
            c: Some(chs.c()),
            h: Some(chs.h()),
            s: chs.s(),
            n: None,
        }
    }
}

/// A structure representing the four components of Sector ID:
///  - Cylinder (c)
///  - Head (h)
///  - Sector ID (s)
///  - Sector Size (n)
///
/// A DiskChsn may represent a Sector ID or an overall disk geometry.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiskChsn {
    chs: DiskChs,
    n:   u8,
}

impl Default for DiskChsn {
    fn default() -> Self {
        Self {
            chs: DiskChs::default(),
            n:   2,
        }
    }
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
        write!(f, "[c:{:2} h:{} s:{:3} n:{}]", self.c(), self.h(), self.s(), self.n)
    }
}

#[allow(dead_code)]
impl DiskChsn {
    /// Create a new DiskChsn structure from the four sector ID components.
    pub fn new(c: u16, h: u8, s: u8, n: u8) -> Self {
        Self {
            chs: DiskChs::from((c, h, s)),
            n,
        }
    }

    /// Return all four sector ID components.
    /// # Returns:
    /// A tuple containing the cylinder, head, sector ID, and sector size.
    pub fn get(&self) -> (u16, u8, u8, u8) {
        (self.c(), self.h(), self.s(), self.n())
    }
    /// Return the cylinder (c) field.
    pub fn c(&self) -> u16 {
        self.chs.c()
    }
    /// Return the head (h) field.
    pub fn h(&self) -> u8 {
        self.chs.h()
    }
    /// Return the sector id (s) field.
    pub fn s(&self) -> u8 {
        self.chs.s()
    }
    /// Return the size (n) field.
    pub fn n(&self) -> u8 {
        self.n
    }
    /// Return the size of the 'n' parameter in bytes.
    /// The formula for calculating size from n is (128 * 2^n)
    /// We enforce a maximum size of 8192 bytes for a single sector.
    pub fn n_size(&self) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(self.n as u32).0)
    }

    /// Convert the value of the sector size field (n) into bytes.
    pub fn n_to_bytes(n: u8) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(n as u32).0)
    }

    /// Convert a size in bytes into a valid sector size field value (n)
    pub fn bytes_to_n(size: usize) -> u8 {
        let mut n = 0;
        let mut size = size;
        while size > 128 {
            size >>= 1;
            n += 1;
        }
        n
    }

    /// Set the four components of a sector ID.
    pub fn set(&mut self, c: u16, h: u8, s: u8, n: u8) {
        self.set_c(c);
        self.set_h(h);
        self.set_s(s);
        self.n = n;
    }
    /// Set the cylinder component of a sector ID.
    pub fn set_c(&mut self, c: u16) {
        self.chs.set_c(c)
    }
    /// Set the head component of a sector ID.
    pub fn set_h(&mut self, h: u8) {
        self.chs.set_h(h)
    }
    /// Set the sector ID component of a sector ID.
    pub fn set_s(&mut self, s: u8) {
        self.chs.set_s(s)
    }

    pub fn seek(&mut self, dst_chs: &DiskChs) {
        self.chs = *dst_chs;
    }

    /// Return the number of sectors represented by a `DiskChsn` structure, interpreted as drive geometry.
    pub fn get_sector_count(&self) -> u32 {
        self.chs.get_sector_count()
    }

    /// Convert a `DiskChsn` struct to an LBA sector address. A reference drive geometry is required to
    /// calculate the address.
    pub fn to_lba(&self, geom: &DiskChs) -> usize {
        self.chs.to_lba(geom)
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    /// A reference drive geometry is required to calculate the address.
    /// This function is deprecated. Seeking cannot be performed directly on a `DiskChs` structure.
    #[deprecated]
    #[allow(deprecated)]
    pub(crate) fn get_next_sector(&self, geom: &DiskChs) -> DiskChs {
        self.chs.get_next_sector(geom)
    }

    #[deprecated]
    #[allow(deprecated)]
    pub(crate) fn seek_forward(&mut self, sectors: u32, geom: &DiskChs) -> &mut Self {
        self.chs.seek_forward(sectors, geom);
        self
    }

    pub(crate) fn ch(&self) -> DiskCh {
        DiskCh::new(self.c(), self.h())
    }
}

/// A structure representing three of the four components of Sector ID:
///  - Cylinder (c)
///  - Head (h)
///  - Sector ID (s)
///
/// A DiskChs may represent a Sector ID, where size is ignored, or an overall disk geometry.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Create a new DiskChs structure from the three sector ID components.
    pub fn new(c: u16, h: u8, s: u8) -> Self {
        Self { c, h, s }
    }

    /// Return all three sector ID components.
    /// # Returns:
    /// A tuple containing the cylinder, head, and sector ID.
    pub fn get(&self) -> (u16, u8, u8) {
        (self.c, self.h, self.s)
    }
    /// Return the cylinder (c) field.
    pub fn c(&self) -> u16 {
        self.c
    }
    /// Return the head (h) field.
    pub fn h(&self) -> u8 {
        self.h
    }
    /// Return the sector id (s) field.
    pub fn s(&self) -> u8 {
        self.s
    }
    /// Set the three components of a `DiskChs`
    pub fn set(&mut self, c: u16, h: u8, s: u8) {
        self.c = c;
        self.h = h;
        self.s = s;
    }
    /// Set the cylinder (c) component of a `DiskChs`
    pub fn set_c(&mut self, c: u16) {
        self.c = c;
    }
    /// Set the head (h) component of a `DiskChs`
    pub fn set_h(&mut self, h: u8) {
        self.h = h;
    }
    /// Set the sector ID (s) component of a `DiskChs`
    pub fn set_s(&mut self, s: u8) {
        self.s = s;
    }

    /// Seek to the specified CHS.
    /// This function is deprecated. Seeking cannot be performed directly on a `DiskChs` structure,
    /// as sector IDs are not always sequential.
    #[deprecated]
    #[allow(deprecated)]
    pub fn seek(&mut self, c: u16, h: u8, s: u8) {
        self.seek_to(&DiskChs::from((c, h, s)));
    }

    /// Seek to the specified CHS.
    /// This function is deprecated. Seeking cannot be performed directly on a `DiskChs` structure,
    /// as sector IDs are not always sequential.
    #[deprecated]
    pub fn seek_to(&mut self, dst_chs: &DiskChs) {
        self.c = dst_chs.c;
        self.h = dst_chs.h;
        self.s = dst_chs.s;
    }

    /// Return the number of sectors represented by a DiskChs structure, interpreted as drive geometry.
    pub fn get_sector_count(&self) -> u32 {
        (self.c as u32) * (self.h as u32) * (self.s as u32)
    }

    /// Convert a `DiskChs` struct to an LBA sector address.
    /// A reference drive geometry is required to calculate the address.
    /// Only valid for standard disk formats.
    pub fn to_lba(&self, geom: &DiskChs) -> usize {
        let hpc = geom.h as usize;
        let spt = geom.s as usize;
        (self.c as usize * hpc + (self.h as usize)) * spt + (self.s as usize - 1)
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    /// This function is deprecated. Seeking cannot be performed directly on a `DiskChs` structure,
    /// as sector IDs are not always sequential.
    #[deprecated]
    pub(crate) fn get_next_sector(&self, geom: &DiskChs) -> DiskChs {
        if self.s < geom.s {
            // Not at last sector, just return next sector
            DiskChs::from((self.c, self.h, self.s + 1))
        }
        else if self.h < geom.h - 1 {
            // At last sector, but not at last head, go to next head, same cylinder, sector 1
            DiskChs::from((self.c, self.h + 1, 1))
        }
        else if self.c < geom.c - 1 {
            // At last sector and last head, go to next cylinder, head 0, sector 1
            DiskChs::from((self.c + 1, 0, 1))
        }
        else {
            // Return start of drive? TODO: Research what does this do on real hardware
            DiskChs::from((0, 0, 1))
        }
    }

    /// Return a new CHS that is the next sector on the disk.
    /// If the current CHS is the last sector on the disk, the next CHS will be the first sector on the disk.
    /// This function is deprecated. Seeking cannot be performed directly on a `DiskChs` structure,
    /// as sector IDs are not always sequential.
    #[deprecated]
    #[allow(deprecated)]
    pub(crate) fn seek_forward(&mut self, sectors: u32, geom: &DiskChs) -> &mut Self {
        for _i in 0..sectors {
            *self = self.get_next_sector(geom);
        }
        self
    }
}

/// A structure representing two of the four components of Sector ID:
///  - Cylinder (c)
///  - Head (h)
///
/// A `DiskCh` is usually used as a physical track specifier. It can hold the geometry of a disk,
/// or act as a cursor specifying a specific track on a disk.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

impl From<DiskChsn> for DiskCh {
    fn from(chsn: DiskChsn) -> Self {
        Self {
            c: chsn.c(),
            h: chsn.h(),
        }
    }
}

impl Display for DiskCh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[c:{} h:{}]", self.c, self.h)
    }
}

impl DiskCh {
    /// Create a new DiskCh structure from a Cylinder (c) and Head (h) specifier.
    pub fn new(c: u16, h: u8) -> Self {
        Self { c, h }
    }
    /// Return the cylinder (c) field.
    pub fn c(&self) -> u16 {
        self.c
    }
    /// Return the head (h) field.
    pub fn h(&self) -> u8 {
        self.h
    }
    /// Set the cylinder (c) field.
    pub fn set_c(&mut self, c: u16) {
        self.c = c
    }
    /// Set the head (h) field.
    pub fn set_h(&mut self, h: u8) {
        self.h = h
    }

    /// Return a new `DiskCh` that represents the next track on disk.
    /// # Arguments:
    /// * `heads` - The number of heads on the disk.
    /// # Returns:
    /// A new `DiskCh` representing the next track on disk.
    pub fn get_next_track(&self, heads: u8) -> DiskCh {
        if self.h < heads - 1 {
            // Not at least head, just return next head
            DiskCh::from((self.c, self.h + 1))
        }
        else {
            // Go to next track, head 0
            DiskCh::from((self.c + 1, 0))
        }
    }

    /// Treating the `DiskCh` as a track cursor, set it to reference the next logical track on the disk.
    /// # Arguments:
    /// * `heads` - The number of heads on the disk.
    pub fn seek_next_track(&mut self, heads: u8) {
        *self = self.get_next_track(heads);
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
        assert_eq!(DiskChsn::bytes_to_n(1024), 3);
    }

    #[test]
    fn diskchs_to_lba_calculates_correct_lba() {
        let geom = DiskChs::new(40, 2, 9);
        let chs = DiskChs::new(2, 1, 5);
        assert_eq!(chs.to_lba(&geom), 49);
    }

    #[test]
    fn diskch_get_next_track_wraps_correctly() {
        let ch = DiskCh::new(1, 1);
        let next_ch = ch.get_next_track(2);
        assert_eq!(next_ch, DiskCh::new(2, 0));
    }
}