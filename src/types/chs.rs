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

use crate::{types::sector_layout::SectorLayout, MAXIMUM_SECTOR_SIZE};
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
        write!(f, "[c:{:2} h:{} s:{:3} n:{}]", c_str, h_str, self.s, n_str)
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
    pub fn matches(&self, id_chsn: &DiskChsn) -> bool {
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
    #[inline]
    pub fn get(&self) -> (u16, u8, u8, u8) {
        (self.c(), self.h(), self.s(), self.n())
    }
    /// Return the cylinder (c) field.
    #[inline]
    pub fn c(&self) -> u16 {
        self.chs.c()
    }
    /// Return the head (h) field.
    #[inline]
    pub fn h(&self) -> u8 {
        self.chs.h()
    }
    /// Return the sector id (s) field.
    #[inline]
    pub fn s(&self) -> u8 {
        self.chs.s()
    }
    /// Return a `DiskCh` structure representing the cylinder and head components of a DiskChsn.
    #[inline]
    pub fn ch(&self) -> DiskCh {
        self.chs.ch()
    }
    /// Return the size (n) field.
    #[inline]
    pub fn n(&self) -> u8 {
        self.n
    }
    /// Return the size of the 'n' parameter in bytes.
    /// The formula for calculating size from n is (128 * 2^n)
    /// We enforce a maximum size of 8192 bytes for a single sector.
    #[inline]
    pub fn n_size(&self) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(self.n as u32).0)
    }

    /// Convert the value of the sector size field (n) into bytes.
    #[inline]
    pub fn n_to_bytes(n: u8) -> usize {
        std::cmp::min(MAXIMUM_SECTOR_SIZE, 128usize.overflowing_shl(n as u32).0)
    }

    /// Convert a size in bytes into a valid sector size field value (n)
    #[inline]
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
    #[inline]
    pub fn set_c(&mut self, c: u16) {
        self.chs.set_c(c)
    }
    /// Set the head component of a sector ID.
    #[inline]
    pub fn set_h(&mut self, h: u8) {
        self.chs.set_h(h)
    }
    /// Set the sector ID component of a sector ID.
    #[inline]
    pub fn set_s(&mut self, s: u8) {
        self.chs.set_s(s)
    }

    /// Return the number of sectors represented by a `DiskChsn` structure, interpreted as drive geometry.
    pub fn sector_count(&self) -> u32 {
        self.chs.sector_count()
    }

    /// Return a boolean indicating whether this `DiskChsn`, interpreted as drive geometry, contains
    /// the specified `DiskChs` representing a sector.
    #[inline]
    pub fn contains(&self, other: impl Into<DiskChs>) -> bool {
        let other = other.into();
        self.chs.contains(other)
    }

    /// Convert a `DiskChsn` struct to an LBA sector address. A reference drive geometry is required to
    /// calculate the address.
    #[inline]
    pub fn to_lba(&self, geom: &SectorLayout) -> usize {
        self.chs.to_lba(geom)
    }

    /// Return a new `DiskChsn` that is the next sector on the disk, according to the specified
    /// geometry.
    /// Returns None if the current `DiskChsn` represents the last sector of the specified geometry.
    /// This function should only be used for iterating through sectors in a standard disk format.
    /// It will not work correctly for non-standard disk formats.
    pub fn next_sector(&self, geom: &SectorLayout) -> Option<DiskChsn> {
        self.chs.next_sector(geom).map(|chs| DiskChsn::from((chs, self.n)))
    }

    /// Return a new `Option<DiskChsn>` that is `sectors` number of sectors advanced from the current
    /// `DiskChsn`, according to a provided geometry.
    /// Returns None if advanced past the end of the disk.
    /// # Arguments:
    /// * `geom` - Any type implementing `Into<DiskChs>`, representing the number of heads,
    ///            cylinders, and sectors per track on the disk.
    pub(crate) fn offset_sectors(&mut self, sectors: u32, geom: &SectorLayout) -> Option<DiskChsn> {
        self.chs
            .offset_sectors(sectors, geom)
            .map(|chs| DiskChsn::from((chs, self.n)))
    }

    pub fn iter(&self, geom: SectorLayout) -> DiskChsnIterator {
        DiskChsnIterator { geom, chs: None }
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
    pub(crate) c: u16,
    pub(crate) h: u8,
    pub(crate) s: u8,
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
        write!(f, "[c:{:2} h:{} s:{:3}]", self.c, self.h, self.s)
    }
}

impl DiskChs {
    /// Create a new `DiskChs` structure from cylinder, head and sector id components.
    pub fn new(c: u16, h: u8, s: u8) -> Self {
        Self { c, h, s }
    }
    /// Return the cylinder, head and sector id components in a tuple.
    #[inline]
    pub fn get(&self) -> (u16, u8, u8) {
        (self.c, self.h, self.s)
    }
    /// Return the cylinder (c) field.
    #[inline]
    pub fn c(&self) -> u16 {
        self.c
    }
    /// Return the head (h) field.
    #[inline]
    pub fn h(&self) -> u8 {
        self.h
    }
    /// Return the sector id (s) field.
    #[inline]
    pub fn s(&self) -> u8 {
        self.s
    }
    /// Return a `DiskCh` structure representing the cylinder and head components of a DiskChs.
    #[inline]
    pub fn ch(&self) -> DiskCh {
        DiskCh::new(self.c, self.h)
    }
    /// Set the three components of a `DiskChs`
    pub fn set(&mut self, c: u16, h: u8, s: u8) {
        self.c = c;
        self.h = h;
        self.s = s;
    }
    /// Set the cylinder (c) component of a `DiskChs`
    #[inline]
    pub fn set_c(&mut self, c: u16) {
        self.c = c;
    }
    /// Set the head (h) component of a `DiskChs`
    #[inline]
    pub fn set_h(&mut self, h: u8) {
        self.h = h;
    }
    /// Set the sector ID (s) component of a `DiskChs`
    #[inline]
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
    pub fn sector_count(&self) -> u32 {
        (self.c as u32) * (self.h as u32) * (self.s as u32)
    }

    /// Return the number of sectors represented by a DiskChs structure, interpreted as drive geometry.
    pub fn total_sectors(&self) -> usize {
        (self.c as usize) * (self.h as usize) * (self.s as usize)
    }

    /// Return a boolean indicating whether this `DiskChs`, interpreted as drive geometry, contains
    /// the specified `DiskChs` representing a sector.
    pub fn contains(&self, other: impl Into<DiskChs>) -> bool {
        let other = other.into();
        self.c > other.c && self.h > other.h && self.s >= other.s
    }

    /// Convert a [DiskChs] struct to an LBA sector address.
    /// A reference [SectorLayout] is required to calculate the address.
    /// Only valid for standard disk formats.
    pub fn to_lba(&self, geom: &SectorLayout) -> usize {
        let hpc = geom.h() as usize;
        let spt = geom.s() as usize;
        (self.c as usize * hpc + (self.h as usize)) * spt + (self.s.saturating_sub(geom.s_off) as usize)
    }

    /// Convert an LBA sector address into a [DiskChs] struct and byte offset into the resulting sector.
    /// A reference drive geometry is required to calculate the address.
    /// Only valid for standard disk formats.
    /// # Arguments:
    /// * `lba` - The LBA sector address to convert.
    /// * `geom` - A [SectorLayout], representing the number of heads and cylinders on the disk.
    /// # Returns:
    /// * `Some(DiskChs)` representing the resulting CHS address.
    /// * `None` if the LBA address is invalid for the specified geometry.
    pub fn from_lba(lba: usize, geom: &SectorLayout) -> Option<DiskChs> {
        let hpc = geom.h() as usize;
        let spt = geom.s() as usize;
        let c = lba / (hpc * spt);
        let h = (lba / spt) % hpc;
        let s = (lba % spt) + geom.s_off as usize;

        if c >= geom.c() as usize || h >= hpc || s > spt {
            return None;
        }
        Some(DiskChs::from((c as u16, h as u8, s as u8)))
    }

    /// Convert a raw byte offset into a `DiskChs` struct and byte offset into the resulting sector.
    /// A reference standard disk geometry is required to calculate the address.
    /// Only valid for standard disk formats. This function is intended to assist seeking within a raw sector view.
    /// # Arguments:
    /// * `lba` - The LBA sector address to convert.
    /// * `lba` - The LBA sector address to convert.
    /// * `geom` - A [SectorLayout], representing the number of heads and cylinders on the disk.
    /// # Returns:
    /// A tuple containing the resulting `DiskChs` and the byte offset into the sector.
    pub fn from_raw_offset(offset: usize, geom: &SectorLayout) -> Option<(DiskChs, usize)> {
        let lba = offset / geom.size();
        DiskChs::from_lba(lba, geom).map(|chs| (chs, offset % geom.size()))
    }

    /// Convert a `DiskChs` into a raw byte offset
    /// A reference drive geometry is required to calculate the address.
    /// Only valid for standard disk formats. This function is intended to assist seeking within a raw sector view.
    /// # Arguments:
    /// * `lba` - The LBA sector address to convert.
    /// * `geom` - A [SectorLayout], representing the number of heads and cylinders on the disk.
    /// # Returns:
    /// A tuple containing the resulting `DiskChs` and the byte offset into the sector.
    pub fn to_raw_offset(&self, geom: &SectorLayout) -> Option<usize> {
        geom.contains(*self).then_some(self.to_lba(geom) * geom.size())
    }

    /// Return a new `DiskChs` that is the next sector on the disk, according to the specified
    /// geometry.
    /// Returns None if the current `DiskChs` represents the last sector of the specified geometry.
    /// This function should only be used for iterating through sectors in a standard disk format.
    /// It will not work correctly for non-standard disk formats.
    /// # Arguments:
    /// * `geom` - A [SectorLayout], representing the number of heads and cylinders on the disk.
    pub fn next_sector(&self, geom: &SectorLayout) -> Option<DiskChs> {
        if self.s < (geom.s() - 1 + geom.s_off) {
            // println!(
            //     "Geometry: {} current sector: {}, spt: {}, last valid sector:{} Next sector: {}",
            //     geom,
            //     self.s,
            //     geom.s(),
            //     geom.s() - 1 + geom.s_off,
            //     self.s + 1
            // );

            // Not at last sector, just return next sector
            Some(DiskChs::from((self.c, self.h, self.s + 1)))
        }
        else if self.h < geom.h().saturating_sub(1) {
            // At last sector, but not at last head, go to next head, same cylinder, sector 1
            Some(DiskChs::from((self.c, self.h + 1, geom.s_off)))
        }
        else if self.c < geom.c().saturating_sub(1) {
            // At last sector and last head, go to next cylinder, head 0, sector (s_off)
            Some(DiskChs::from((self.c + 1, 0, geom.s_off)))
        }
        else {
            // At end of disk.
            None
        }
    }

    /// Return a new `Option<DiskChs>` that is `sectors` number of sectors advanced from the current
    /// `DiskChs`, according to a provided geometry.
    /// Returns None if advanced past the end of the disk.
    /// # Arguments:
    /// * `geom` - A [SectorLayout], representing the number of heads and cylinders on the disk.
    pub fn offset_sectors(&mut self, sectors: u32, geom: &SectorLayout) -> Option<DiskChs> {
        let mut start_chs = *self;
        for _ in 0..sectors {
            start_chs = start_chs.next_sector(geom)?;
        }
        Some(start_chs)
    }

    /// Return a `DiskChsIterator` that will iterate through all sectors in order, interpreting the `DiskChs` as a standard disk geometry.
    /// This should only be used for standard disk formats. It will skip non-standard sectors, and may access sectors out of physical order.
    pub fn iter(&self, geom: SectorLayout) -> DiskChsIterator {
        DiskChsIterator { geom, chs: None }
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
    /// * `geom` - Any type implementing `Into<DiskCh>`, representing the number of heads and cylinders on the disk.
    /// # Returns:
    /// `Some(DiskCh)` representing the next track on disk.
    /// `None` if the current `DiskCh` is the last track on the disk.
    pub fn next_track(&self, geom: impl Into<DiskCh>) -> Option<DiskCh> {
        let geom = geom.into();
        if self.h < geom.h().saturating_sub(1) {
            // Not at last head, just return next head
            Some(DiskCh::from((self.c, self.h + 1)))
        }
        else if self.c < geom.c().saturating_sub(1) {
            // At last head, but not at last cylinder. Return next cylinder, head 0
            Some(DiskCh::from((self.c + 1, 0)))
        }
        else {
            // At last head and track, return None.
            None
        }
    }

    /// Return a new `DiskCh` that represents the next track on disk.
    /// # Arguments:
    /// * `heads` - A u8 value representing the number of heads on the disk.
    /// # Returns:
    /// A new `DiskCh` representing the next logical track.
    pub fn next_track_unchecked(&self, heads: u8) -> DiskCh {
        if self.h < heads.saturating_sub(1) {
            // Not at last head, just return next head
            DiskCh::from((self.c, self.h + 1))
        }
        else {
            // Advance to the next cylinder, head 0
            DiskCh::from((self.c + 1, 0))
        }
    }

    /// Treating the `DiskCh` as a track cursor, set it to reference the next logical track on the disk.
    /// If the current `DiskCh` is the last track on the disk, it will remain unchanged.
    /// # Arguments:
    /// * `geom` - Any type implementing `Into<DiskCh>`, representing the number of heads and cylinders on the disk.
    ///
    /// # Returns:
    /// A boolean indicating whether the track was successfully advanced. false indicates that the current
    /// track was the last track on the disk.
    pub fn seek_next_track(&mut self, geom: impl Into<DiskCh>) -> bool {
        let geom = geom.into();
        if self.c() == geom.c().saturating_sub(1) && self.h() >= geom.h().saturating_sub(1) {
            return false;
        }
        *self = self.next_track(geom).unwrap_or(*self);
        true
    }

    /// Treating the `DiskCh` as a track cursor, set it to reference the next logical track on the disk.
    /// The cylinder number will be allowed to advance unbounded. It may no longer represent a valid track.
    /// This routine is intended for building disk images, where the track number may grow as tracks
    /// are added.
    /// # Arguments:
    /// * `heads` - The number of heads on the disk.
    pub fn seek_next_track_unchecked(&mut self, heads: u8) {
        *self = self.next_track_unchecked(heads);
    }

    /// Return a `DiskChsIterator` that will iterate through all sectors in order, interpreting the `DiskChs` as a standard disk geometry.
    /// This should only be used for standard disk formats. It will skip non-standard sectors, and may access sectors out of physical order.
    pub fn iter(&self) -> DiskChIterator {
        DiskChIterator {
            geom: *self,
            ch:   None,
        }
    }
}

pub struct DiskChIterator {
    geom: DiskCh,
    ch:   Option<DiskCh>,
}

impl Iterator for DiskChIterator {
    type Item = DiskCh;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ch) = &mut self.ch {
            *ch = ch.next_track(self.geom)?;
        }
        else {
            self.ch = Some(DiskCh::new(0, 0));
        }
        self.ch
    }
}

pub struct DiskChsIterator {
    geom: SectorLayout,
    chs:  Option<DiskChs>,
}

impl Iterator for DiskChsIterator {
    type Item = DiskChs;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chs) = &mut self.chs {
            *chs = chs.next_sector(&self.geom)?;
        }
        else {
            self.chs = Some(DiskChs::new(0, 0, self.geom.s_off));
        }
        self.chs
    }
}

pub struct DiskChsnIterator {
    geom: SectorLayout,
    chs:  Option<DiskChs>,
}

impl Iterator for DiskChsnIterator {
    type Item = DiskChsn;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chs) = &mut self.chs {
            *chs = chs.next_sector(&self.geom)?;
        }
        else {
            self.chs = Some(DiskChs::new(0, 0, self.geom.s_off));
        }
        Some(DiskChsn::from((self.chs.unwrap(), self.geom.n())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StandardFormat;

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
        let geom = SectorLayout::new(40, 2, 9, 1, 512);
        let chs = DiskChs::new(2, 1, 5);

        // 2(cyl) * 2(heads) * 9(sectors) + (1(head) * 9(sectors)) + 5(sector) = 49
        assert_eq!(chs.to_lba(&geom), 49);
    }

    #[test]
    fn diskchs_from_lba_calculates_correct_chs() {
        let geom = SectorLayout::new(40, 2, 9, 1, 512);
        let lba = 49;
        let chs = DiskChs::from_lba(lba, &geom).unwrap();
        assert_eq!(chs, DiskChs::new(2, 1, 5));
    }

    #[test]
    fn diskchs_from_raw_offset_calculates_correct_chs() {
        let geom = StandardFormat::PcFloppy360.layout();

        let offset = 2560; // 5 sectors offset
        let (chs, byte_offset) = DiskChs::from_raw_offset(offset, &geom).unwrap();

        assert_eq!(byte_offset, 0);
        assert_eq!(DiskChs::new(0, 0, 6), chs);
    }

    #[test]
    fn diskchs_from_lba_returns_none_for_out_of_range() {
        let geom = SectorLayout::new(40, 2, 9, 1, 512);
        let lba = 720; // Out of range LBA for the given geometry
        let chs = DiskChs::from_lba(lba, &geom);
        assert!(chs.is_none());
    }

    #[test]
    fn diskchs_from_raw_offset_calculates_correct_chs_and_offset() {
        let geom = SectorLayout::new(40, 2, 9, 1, 1024);
        let offset = 5120; // 10 sectors offset
        let (chs, byte_offset) = DiskChs::from_raw_offset(offset, &geom).unwrap();
        assert_eq!(chs, DiskChs::new(0, 0, 6));
        assert_eq!(byte_offset, 0);

        let offset = 5123; // 10 sectors and 3 bytes offset
        let (chs, byte_offset) = DiskChs::from_raw_offset(offset, &geom).unwrap();

        assert_eq!(chs, DiskChs::new(0, 0, 6));
        assert_eq!(byte_offset, 3);
    }

    #[test]
    fn diskch_get_next_track_wraps_correctly() {
        let ch = DiskCh::new(1, 1);
        let next_ch = ch.next_track(StandardFormat::PcFloppy360);
        assert_eq!(next_ch, Some(DiskCh::new(2, 0)));
    }

    #[test]
    fn diskch_iter_works() {
        let geom = StandardFormat::PcFloppy360.ch();

        let ch = geom.iter().next().unwrap();
        assert_eq!(ch, DiskCh::new(0, 0));

        let last_chs = geom.iter().last().unwrap();
        assert_eq!(last_chs, DiskCh::new(geom.c() - 1, geom.h() - 1));

        let iter_ct = geom.iter().count();
        assert_eq!(iter_ct, geom.c() as usize * geom.h() as usize);
    }

    #[test]
    fn diskchs_iter_works() {
        let geom = StandardFormat::PcFloppy360.layout();
        let total_sectors = geom.total_sectors();

        let first_chs = geom.chs_iter().next().unwrap();
        assert_eq!(first_chs, DiskChs::new(0, 0, 1));

        let last_chs = geom.chs_iter().last().unwrap();
        assert_eq!(last_chs, DiskChs::new(geom.c() - 1, geom.h() - 1, geom.s()));

        let iter_ct = geom.chs_iter().count();
        assert_eq!(iter_ct, total_sectors);
    }

    #[test]
    #[cfg(feature = "amiga")]
    fn diskchs_iter_works_with_0_offset() {
        let geom = StandardFormat::AmigaFloppy880.layout();
        let total_sectors = geom.total_sectors();

        let first_chs = geom.chs_iter().next().unwrap();
        assert_eq!(first_chs, DiskChs::new(0, 0, 0));

        let last_chs = geom.chs_iter().last().unwrap();
        assert_eq!(last_chs, DiskChs::new(geom.c() - 1, geom.h() - 1, geom.s() - 1));

        let iter_ct = geom.chs_iter().count();
        assert_eq!(iter_ct, total_sectors);
    }
}
