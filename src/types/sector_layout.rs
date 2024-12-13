use std::fmt::Display;

use crate::types::{DiskCh, DiskChIterator, DiskChs, DiskChsIterator, DiskChsn, DiskChsnIterator};

/// A structure representing how sectors are laid out on a disk (assuming standard format)
///  - Cylinder (c)
///  - Head (h)
///  - Sector count (s)
///
/// Plus a sector ID offset (s_off) to represent whether a standard sector id starts at 0 or 1.
///
/// A DiskChs may represent a Sector ID, where size is ignored, or an overall disk geometry.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SectorLayout {
    pub(crate) c: u16,
    pub(crate) h: u8,
    pub(crate) s: u8,
    pub(crate) s_off: u8,
    pub(crate) size: usize,
}

impl Display for SectorLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[c:{:2} h:{} s:{:2} s_off:{}]", self.c, self.h, self.s, self.s_off)
    }
}

impl SectorLayout {
    /// Create a new `SectorLayout` structure from cylinder, head and sector id components.
    pub fn new(c: u16, h: u8, s: u8, s_off: u8, size: usize) -> Self {
        Self { c, h, s, s_off, size }
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
    /// Return the sector count (s) field.
    #[inline]
    pub fn s(&self) -> u8 {
        self.s
    }
    /// Return the sector id offset (s_off) field.
    #[inline]
    pub fn s_off(&self) -> u8 {
        self.s_off
    }
    #[inline]
    /// Return the size of a sector in bytes.
    pub fn size(&self) -> usize {
        self.size
    }
    /// Return the equivalent 'n' size parameter for the specified byte size.
    pub fn n(&self) -> u8 {
        DiskChsn::bytes_to_n(self.size)
    }
    /// Return a [DiskCh] structure representing the cylinder and head count components of a [SectorLayout].
    #[inline]
    pub fn ch(&self) -> DiskCh {
        DiskCh::new(self.c, self.h)
    }
    /// Return a [DiskChs] structure representing the cylinder, head and sector count components of a [SectorLayout].
    #[inline]
    pub fn chs(&self) -> DiskChs {
        DiskChs::new(self.c, self.h, self.s)
    }
    /// Return a [DiskChsn] structure representing the cylinder, head and sector counts of a [SectorLayout].
    #[inline]
    pub fn chsn(&self) -> DiskChsn {
        DiskChsn::new(self.c, self.h, self.s, DiskChsn::bytes_to_n(self.size))
    }
    /// Set the cylinder count (c) component of a [SectorLayout].
    #[inline]
    pub fn set_c(&mut self, c: u16) {
        self.c = c;
    }
    /// Set the head count (h) component of a [SectorLayout].
    #[inline]
    pub fn set_h(&mut self, h: u8) {
        self.h = h;
    }
    /// Set the sector count (s) component of a [SectorLayout].
    #[inline]
    pub fn set_s(&mut self, s: u8) {
        self.s = s;
    }
    /// Return the number of sectors represented by a [SectorLayout].
    pub fn total_sectors(&self) -> usize {
        (self.c as usize) * (self.h as usize) * (self.s as usize)
    }
    /// Return a boolean indicating whether this [SectorLayout] contains the specified [DiskChs]
    /// representing a sector id.
    pub fn contains(&self, chs: impl Into<DiskChs>) -> bool {
        let chs = chs.into();
        self.c > chs.c && self.h > chs.h && self.s > (chs.s.saturating_sub(self.s_off))
    }

    pub fn ch_iter(&self) -> DiskChIterator {
        DiskCh::new(self.c, self.h).iter()
    }

    pub fn chs_iter(&self) -> DiskChsIterator {
        DiskChs::new(self.c, self.h, self.s).iter(*self)
    }

    pub fn chsn_iter(&self) -> DiskChsnIterator {
        DiskChsn::new(self.c, self.h, self.s, self.n()).iter(*self)
    }
}
