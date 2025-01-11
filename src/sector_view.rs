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

    sector_io.rs

    Implement a sector I/O interface for reading and writing sectors.
    The SectorIo struct implements Read + Write + Seek and can be given to a
    any function that takes a Read + Write + Seek object for direct reading and
    writing of a fluxfox DiskImage as if it were a raw sector image.
*/
use crate::{
    io::{Read, Seek, Write},
    types::DiskCh,
    DiskImage,
    DiskImageError,
    SectorIdQuery,
    StandardFormat,
};

use crate::{
    disk_lock::{DiskLock, LockContext, NonTrackingDiskLock, NullContext},
    file_system::FileSystemError,
    prelude::{DiskChs, DiskChsn},
};
use std::sync::{Arc, RwLock};

pub struct StandardSectorView {
    disk: NonTrackingDiskLock<DiskImage>,
    disk_format: StandardFormat,
    track_cursor: DiskCh,
    sector_id_cursor: u8,
    spt: u8,
    sector_buffer: Box<[u8]>,
    sector_size: usize,
    sector_dirty: bool,
    sector_byte_cursor: usize,
    eod: bool, // End-of-disk flag. All read/write operations that exceed the end of the current sector will fail.
}

impl StandardSectorView {
    pub fn new(
        disk_lock: impl Into<NonTrackingDiskLock<DiskImage>>,
        format: StandardFormat,
    ) -> Result<Self, DiskImageError> {
        let disk = disk_lock.into();
        let mut new = StandardSectorView {
            disk,
            disk_format: format,
            track_cursor: DiskCh::new(0, 0),
            sector_id_cursor: 1,
            spt: format.layout().s(),
            sector_buffer: vec![0; format.sector_size()].into_boxed_slice(),
            sector_size: format.sector_size(),
            sector_dirty: false,
            sector_byte_cursor: 0,
            eod: false,
        };

        // Read the first sector into the buffer.
        new.read_sector(new.sector_id_cursor)?;
        Ok(new)
    }

    pub fn format(&self) -> StandardFormat {
        self.disk_format
    }

    pub fn chsn(&self) -> DiskChsn {
        DiskChsn::from((
            DiskChs::from((self.track_cursor, self.sector_id_cursor)),
            self.disk_format.chsn().n(),
        ))
    }

    /// Seek to the specified CHS address within the sector view
    pub fn seek_to_chs(&mut self, chs: impl Into<DiskChs>) -> crate::io::Result<u64> {
        let chs = chs.into();
        let offset = chs
            .to_raw_offset(&self.disk_format.layout())
            .ok_or(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid CHS"))?;

        self.seek_to_offset(offset)
    }

    fn seek_to_offset(&mut self, offset: usize) -> crate::io::Result<u64> {
        let (chs, sector_offset) = match DiskChs::from_raw_offset(offset, &self.disk_format.layout()) {
            Some(chs) => chs,
            None => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid offset")),
        };

        if DiskChs::from((self.track_cursor, self.sector_id_cursor)) == chs {
            // We're already at the correct CHS, so just set the byte cursor.
            self.sector_byte_cursor = sector_offset;
            return Ok(offset as u64);
        }

        // Do we need to change sectors?
        if DiskChs::from((self.track_cursor, self.sector_id_cursor)) != chs {
            log::trace!("seek_to_offset(): Seeking to CHS: {}", chs);

            // Do we need to switch tracks?
            if chs.ch() != self.track_cursor {
                // Update the track cursor
                self.track_cursor = DiskCh::from(chs);
            }

            // Commit the current sector
            self.commit_sector()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

            // Read the specified sector on this track into the sector buffer.
            self.sector_id_cursor = chs.s();
            self.read_sector(self.sector_id_cursor)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        }

        // Set the sector byte cursor to the specified offset.
        self.sector_byte_cursor = sector_offset;

        Ok(offset as u64)
    }

    fn offset(&self) -> usize {
        let chs = DiskChs::from((self.track_cursor, self.sector_id_cursor));
        chs.to_raw_offset(&self.disk_format.layout()).unwrap_or(0) + self.sector_byte_cursor
    }

    fn read_sector(&mut self, sector_id: u8) -> Result<(), DiskImageError> {
        self.sector_id_cursor = sector_id;
        self.sector_byte_cursor = 0;
        self.sector_buffer = self
            .disk
            .read(NullContext::default())
            .unwrap()
            .read_sector_basic(self.track_cursor, SectorIdQuery::from(self.chsn()), None)?
            .into_boxed_slice();

        log::trace!("read_sector(): Reading sector: {}", self.chsn());

        // If the result is less or more than expected, extend or trim as necessary.
        #[allow(clippy::comparison_chain)]
        if self.sector_buffer.len() < self.sector_size {
            let mut new_sector_buffer = vec![0; self.sector_size].into_boxed_slice();
            new_sector_buffer[..self.sector_buffer.len()].copy_from_slice(&self.sector_buffer);
            self.sector_buffer = new_sector_buffer;
        }
        else if self.sector_buffer.len() > self.sector_size {
            let mut new_sector_buffer = vec![0; self.sector_size].into_boxed_slice();
            new_sector_buffer.copy_from_slice(&self.sector_buffer[..self.sector_size]);
            self.sector_buffer = new_sector_buffer;
        }

        // New sector means not at EOD
        self.eod = false;
        // New sector is clean.
        self.sector_dirty = false;
        Ok(())
    }

    /// Write the current sector buffer to the disk image if it is dirty.
    /// This function must be called before changing the track cursor.
    fn commit_sector(&mut self) -> Result<(), DiskImageError> {
        if self.sector_dirty {
            self.disk.write(NullContext::default()).unwrap().write_sector_basic(
                self.track_cursor,
                SectorIdQuery::from(self.chsn()),
                None,
                &self.sector_buffer,
            )?;
        }
        self.sector_dirty = false;
        Ok(())
    }

    fn next_sector(&mut self) -> Result<(), DiskImageError> {
        self.sector_id_cursor += 1;
        if self.sector_id_cursor > self.spt {
            // Standard sector ids are 1-indexed.
            self.sector_id_cursor = 1;
            self.eod = !self.track_cursor.seek_next_track(self.disk_format);
            log::trace!("next_sector(): Seek to new track: {}", self.track_cursor);
        }

        // Commit the sector if needed.
        self.commit_sector()?;

        if !self.eod {
            self.read_sector(self.sector_id_cursor)?;
        }

        Ok(())
    }
}

impl Read for StandardSectorView {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bytes_read = 0;
        let mut buf_cursor = 0;

        if self.eod {
            return Ok(0);
        }

        while !self.eod && (buf_cursor < buf.len()) {
            if self.sector_byte_cursor < self.sector_buffer.len() {
                buf[buf_cursor] = self.sector_buffer[self.sector_byte_cursor];
                self.sector_byte_cursor += 1;
                buf_cursor += 1;
                bytes_read += 1;
            }
            else {
                // We've reached the end of the current buffered sector, so we need to read the next sector.
                self.next_sector()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            }
        }
        Ok(bytes_read)
    }
}

impl Write for StandardSectorView {
    fn write(&mut self, buf: &[u8]) -> crate::io::Result<usize> {
        let mut bytes_written = 0;
        let mut buf_cursor = 0;

        if self.eod {
            return Ok(0);
        }

        while !self.eod && (buf_cursor < buf.len()) {
            if self.sector_byte_cursor < self.sector_buffer.len() {
                self.sector_buffer[self.sector_byte_cursor] = buf[buf_cursor];
                self.sector_byte_cursor += 1;
                buf_cursor += 1;
                bytes_written += 1;
                self.sector_dirty = true;
            }
            else {
                // Move to the next sector.
                self.next_sector()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            }
        }
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.commit_sector()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }
}

impl Seek for StandardSectorView {
    fn seek(&mut self, pos: crate::io::SeekFrom) -> crate::io::Result<u64> {
        let new_offset = match pos {
            std::io::SeekFrom::Start(offset) => {
                // Seek from start. We can directly calculate the CHS from the offset.
                offset as usize
            }
            std::io::SeekFrom::End(offset) => {
                // Get the total size and the signed offset from the end.
                self.disk_format.disk_size().saturating_add_signed(offset as isize)
            }
            std::io::SeekFrom::Current(offset) => {
                // Get the current offset and the signed offset from the current position.
                let current_offset = self.offset();
                current_offset.saturating_add_signed(offset as isize)
            }
        };

        // Very noisy.
        //log::trace!("seek(): Seeking to offset: {}", new_offset);
        self.seek_to_offset(new_offset)?;
        Ok(new_offset as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use std::sync::{Arc, RwLock};

    fn create_view() -> StandardSectorView {
        let disk_image = create_test_disk_image();
        let format = StandardFormat::PcFloppy360;
        StandardSectorView::new(disk_image, format).unwrap()
    }

    fn create_test_disk_image() -> Arc<RwLock<DiskImage>> {
        // Create a mock DiskImage for testing purposes
        let disk = ImageBuilder::new()
            .with_standard_format(StandardFormat::PcFloppy360)
            .with_formatted(true)
            .with_resolution(TrackDataResolution::BitStream)
            .build()
            .unwrap();

        DiskImage::into_arc(disk)
    }

    #[test]
    fn test_new_standard_sector_view() {
        _ = create_view();
    }

    #[test]
    fn test_read_sector() {
        let mut sector_view = create_view();
        let result = sector_view.read_sector(1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_sector() {
        let mut sector_view = create_view();
        let data = vec![0u8; sector_view.format().sector_size()];
        sector_view.seek(std::io::SeekFrom::Start(512)).unwrap();
        let result = sector_view.write(&data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), data.len());

        // Read sector back
        let mut read_data = vec![0u8; sector_view.format().sector_size()];
        sector_view.seek(std::io::SeekFrom::Start(512)).unwrap();
        let read_result = sector_view.read(&mut read_data);
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), data.len());
    }

    #[test]
    fn test_seek_sector() {
        let mut sector_view = create_view();

        for sector in 0..sector_view.format().chs().total_sectors() {
            let offset = sector * sector_view.format().sector_size();
            let result = sector_view.seek(std::io::SeekFrom::Start(offset as u64));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), offset as u64);
        }
    }

    #[test]
    fn test_seek_logic() {
        let offset = 2560; // Sector 5

        let mut sector_view = create_view();
        sector_view.seek_to_offset(offset).unwrap();

        let chs = DiskChs::from(sector_view.chsn());
        assert_eq!(DiskChs::new(0, 0, 6), chs);
    }
}
