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

use crate::{track::Track, DiskChsn};
use std::marker::PhantomData;

pub struct SectorSpecifier {
    id_chsn: DiskChsn,
    offset:  Option<usize>,
}

pub struct SectorIterator<'a, T: Track> {
    track:   &'a T,
    cursor:  SectorSpecifier,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Track> Iterator for SectorIterator<'a, T> {
    type Item = SectorSpecifier;

    fn next(&mut self) -> Option<Self::Item> {
        // Logic to find the next sector in the track
        if let Some(current_id) = self.cursor {
            if let Some(sector) = self.track.get_sector(current_id) {
                // Update the iterator state
                self.cursor = self.track.next_sector_id(current_id);
                //self.cursor.offset = self.track.get_bit_offset(self.current_sector);
                return Some(sector);
            }
        }

        // No more sectors
        None
    }
}
