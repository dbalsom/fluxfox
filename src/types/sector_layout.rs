/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

    enums.rs

    Defines common enum types
*/

use crate::{
    types::{DiskCh, DiskChIterator, DiskChs, DiskChsIterator, DiskChsn, DiskChsnIterator},
    DEFAULT_SECTOR_SIZE,
};
use std::{fmt::Display, ops::Range};

/// A [SectorLayoutRange] can be used to generalize sector layouts for platforms that had varying
/// track counts and sector counts per track. For example, it was common on the Atari ST to have 8-11
/// sectors per track, unlike the Ibm PC where more than 9 sectors per track was rare.
pub struct SectorLayoutRange {
    pub c: Range<u16>,
    pub h: u8,
    pub s: Range<u8>,
    pub s_off: u8,
    pub size: usize,
}

impl SectorLayoutRange {
    pub fn new(c: Range<u16>, h: u8, s: Range<u8>, s_off: u8, size: usize) -> Self {
        Self { c, h, s, s_off, size }
    }

    /// Return a Range<usize> representing the byte range of the sector layout range.
    /// Note it may be possible for ranges to overlap.
    pub fn byte_range(&self) -> Range<usize> {
        (self.c.start as usize * self.h as usize * self.s.start as usize * self.size)
            ..(self.c.end as usize * self.h as usize * self.s.end as usize * self.size)
    }
}

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

impl TryFrom<usize> for SectorLayout {
    type Error = &'static str;

    fn try_from(size: usize) -> Result<Self, Self::Error> {
        let sector_size = DEFAULT_SECTOR_SIZE;
        let total_sectors = size / sector_size;
        if total_sectors % sector_size != 0 {
            return Err("Invalid sector size");
        }
        let c = 80;
        let h = 2;
        let s = 10;
        let s_off = 1;
        Ok(Self { c, h, s, s_off, size })
    }
}

trait TryFromRawSize<T> {
    type Error;
    fn try_from_raw_size(size: usize, sector_size: Option<usize>) -> Result<Vec<T>, Self::Error>;
}

impl TryFromRawSize<SectorLayout> for SectorLayout {
    type Error = &'static str;
    fn try_from_raw_size(size: usize, sector_size: Option<usize>) -> Result<Vec<Self>, Self::Error> {
        let sector_size = sector_size.unwrap_or(DEFAULT_SECTOR_SIZE);
        let total_sectors = size / sector_size;
        if total_sectors % sector_size != 0 {
            return Err("Raw size must be multiple of sector size");
        }
        Self::derive_matches(size, Some(sector_size))
    }
}

impl SectorLayout {
    /// Create a new `SectorLayout` structure from cylinder, head and sector id components.
    pub fn new(c: u16, h: u8, s: u8, s_off: u8, size: usize) -> Self {
        Self { c, h, s, s_off, size }
    }
    pub fn get(&self) -> (u16, u8, u8, u8, usize) {
        (self.c, self.h, self.s, self.s_off, self.size)
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
    /// Set the sector id offset (s_off) component of a [SectorLayout].
    #[inline]
    pub fn set_s_off(&mut self, s_off: u8) {
        self.s_off = s_off;
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

    fn derive_matches(size: usize, sector_size: Option<usize>) -> Result<Vec<Self>, &'static str> {
        // Overall cylinder range is 39-85
        // We allow one less cylinder than normal, this is sometimes seen in ST files
        let cylinder_range = 39usize..=85;
        // Consider anything from 45-79 as an invalid cylinder range. Would indicate under-dumped image.
        let invalid_cylinders = 45usize..79;
        let sector_size = sector_size.unwrap_or(DEFAULT_SECTOR_SIZE);
        let total_sectors = size / sector_size;
        if size % sector_size != 0 {
            return Err("Raw size must be multiple of sector size");
        }

        //let mut layout_match = None;
        let mut layout_matches = Vec::with_capacity(2);

        for spt in 8..=18 {
            // Iterate over possible sectors per track
            if total_sectors % spt != 0 {
                continue; // Skip if total_sectors is not divisible by spt
            }

            let total_tracks = total_sectors / spt; // Calculate total tracks

            // Determine the number of heads (1 or 2) and corresponding track count
            let heads = if total_tracks % 2 == 0 { 2 } else { 1 };

            let tracks = total_tracks / heads;
            if cylinder_range.contains(&tracks) && !invalid_cylinders.contains(&tracks) {
                layout_matches.push(SectorLayout {
                    c: tracks as u16,
                    h: heads as u8,
                    s: spt as u8,
                    s_off: 0,
                    size: sector_size,
                });
            }
        }

        if !layout_matches.is_empty() {
            layout_matches
                .sort_by(|a, b| Self::normal_cylinder_distance(a.c).cmp(&Self::normal_cylinder_distance(b.c)));

            let vec = layout_matches.iter().flat_map(|layout| layout.equivalents()).collect();
            Ok(vec)
        }
        else {
            Err("No match for raw image size")
        }
    }

    fn normal_cylinder_distance(c: u16) -> u16 {
        if c < 60 {
            40i16.abs_diff(c as i16)
        }
        else {
            80i16.abs_diff(c as i16)
        }
    }

    fn equivalents(&self) -> Vec<Self> {
        let mut equivalents = Vec::with_capacity(2);
        let mut layout = *self;
        // Add the original layout
        equivalents.push(layout);

        // If the track count is >= 79, we could have either a double-sided 5.25" disk or a
        // single sided 3.5" disk. We can't determine which from the raw size alone.
        if layout.c >= 79 && layout.c % 2 == 0 && layout.h == 1 {
            layout.c /= 2;
            layout.h = 2;
            equivalents.push(layout);
        }
        else if layout.c <= 45 && layout.h == 2 {
            // Otherwise, if the track count is small enough to be a 48TPI 5.25" disk with two
            // sides, it might also be a 96tpi 3.5" disk with one side.
            layout.c *= 2;
            layout.h = 1;
            equivalents.push(layout);
        }
        equivalents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive() {
        // Test that we can determine sector layout from common raw image sizes.
        // The following sizes were discovered from a collection of ST image files.
        // Test cases: (raw_size, expected_sector_size, expected_spt, expected_heads, s_off, expected_tracks)
        let test_cases = [
            (1427456, (82, 2, 17, 0, 512)),
            (1064960, (80, 2, 13, 0, 512)),
            (1032192, (84, 2, 12, 0, 512)),
            (995328, (81, 2, 12, 0, 512)),
            (983040, (80, 2, 12, 0, 512)),
            (946176, (84, 2, 11, 0, 512)),
            (934912, (83, 2, 11, 0, 512)),
            (923648, (82, 2, 11, 0, 512)),
            (912384, (81, 2, 11, 0, 512)),
            (901120, (80, 2, 11, 0, 512)),
            (860160, (84, 2, 10, 0, 512)),
            (849920, (83, 2, 10, 0, 512)),
            (839680, (82, 2, 10, 0, 512)),
            (829440, (81, 2, 10, 0, 512)),
            (819200, (80, 2, 10, 0, 512)),
            (808960, (79, 2, 10, 0, 512)),
            (764928, (83, 2, 9, 0, 512)),
            (755712, (82, 2, 9, 0, 512)),
            (746496, (81, 2, 9, 0, 512)),
            (737280, (80, 2, 9, 0, 512)),
            (728064, (79, 2, 9, 0, 512)),
            (461824, (82, 1, 11, 0, 512)),
            (456192, (81, 1, 11, 0, 512)),
            (450560, (80, 1, 11, 0, 512)),
            (424960, (83, 1, 10, 0, 512)),
            (419840, (82, 1, 10, 0, 512)),
            (414720, (81, 1, 10, 0, 512)),
            (409600, (80, 1, 10, 0, 512)),
            (404480, (79, 1, 10, 0, 512)),
            (377856, (82, 1, 9, 0, 512)),
            (373248, (81, 1, 9, 0, 512)),
            (368640, (80, 1, 9, 0, 512)),
            (364032, (79, 1, 9, 0, 512)),
        ];

        for (i, (raw_size, expected)) in test_cases.iter().enumerate() {
            println!("Test case {}: {:?}", i, test_cases[i]);
            match SectorLayout::derive_matches(*raw_size, Some(512)) {
                Ok(layouts) => {
                    println!("Layouts: {:?}", layouts);
                    let test_layout = SectorLayout::new(expected.0, expected.1, expected.2, expected.3, expected.4);
                    assert!(layouts.contains(&test_layout));
                }
                Err(e) => {
                    panic!("Failed for raw_size: {} with error: {}", raw_size, e);
                }
            }
        }
    }
}
