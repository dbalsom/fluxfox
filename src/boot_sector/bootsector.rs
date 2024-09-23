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

    src/boot_sector/bootsector.rs

    Routines for reading and modifying boot sector data

*/

use crate::boot_sector::bpb::{BiosParameterBlock2, BiosParameterBlock3, BPB_OFFSET};
use crate::io::{Cursor, Read, ReadSeek, ReadWriteSeek, Seek, SeekFrom, Write};
use crate::{DiskImageError, StandardFormat};
use binrw::{binrw, BinRead, BinWrite};

pub struct BootSector {
    pub(crate) bpb2: BiosParameterBlock2,
    pub(crate) bpb3: BiosParameterBlock3,
    pub(crate) marker: [u8; 2],
    pub(crate) sector_buf: Cursor<[u8; 512]>,
}

#[binrw]
#[brw(big)]
pub struct CreatorString {
    bytes: [u8; 8],
}

impl BootSector {
    pub fn new<T: ReadSeek>(buffer: &mut T) -> Result<Self, DiskImageError> {
        let mut sector_buf = [0; 512];
        buffer.seek(SeekFrom::Start(0)).map_err(|_e| DiskImageError::IoError)?;
        // Save a copy of the boot sector internally.
        buffer
            .read_exact(&mut sector_buf)
            .map_err(|_e| DiskImageError::IoError)?;

        // Seek to and read the BPB. Currently, we only support versions 2 and 3.
        buffer
            .seek(SeekFrom::Start(BPB_OFFSET))
            .map_err(|_e| DiskImageError::IoError)?;

        let bpb2 = BiosParameterBlock2::read(buffer).map_err(|_e| DiskImageError::IoError)?;
        let bpb3 = BiosParameterBlock3::read(buffer).map_err(|_e| DiskImageError::IoError)?;

        // Seek to the end and check the marker.
        buffer.seek(SeekFrom::End(-2)).map_err(|_e| DiskImageError::IoError)?;
        let mut marker = [0; 2];
        buffer.read_exact(&mut marker).map_err(|_e| DiskImageError::IoError)?;

        Ok(BootSector {
            bpb2,
            bpb3,
            marker,
            sector_buf: Cursor::new(sector_buf),
        })
    }

    /// The default bootsector includes a creator string of 8 characters. This is by default the
    /// string "fluxfox ". This can be overridden to identify the application using fluxfox to
    /// create disk images. If your string is shorter than 8 characters, pad with spaces.
    pub(crate) fn set_creator(&mut self, creator: &[u8; 8]) -> Result<(), DiskImageError> {
        let creator_offset = 0x147;
        eprintln!(
            "Creator offset: {} into {} bytes",
            creator_offset,
            self.sector_buf.get_ref().len()
        );

        match self.sector_buf.seek(SeekFrom::Start(creator_offset)) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error seeking to creator offset: {:?}", e);
                return Err(DiskImageError::IoError);
            }
        }

        // self.sector_buf
        //     .seek(SeekFrom::Start(creator_offset))
        //     .map_err(|_e| DiskImageError::IoError)?;

        //let creator_string = CreatorString { creator: *creator };

        let creator_string = CreatorString::read(&mut self.sector_buf).map_err(|_e| DiskImageError::IoError)?;

        if creator_string.bytes != "fluxfox ".as_bytes() {
            // We can only set the creator if we're using the included boot sector, otherwise we'd overwrite some random data.
            return Err(DiskImageError::IncompatibleImage);
        }

        self.sector_buf
            .seek(SeekFrom::Start(creator_offset))
            .map_err(|_e| DiskImageError::IoError)?;

        let new_creator_string = CreatorString { bytes: *creator };
        new_creator_string
            .write(&mut self.sector_buf)
            .map_err(|_e| DiskImageError::IoError)?;
        Ok(())
    }

    pub(crate) fn has_valid_bpb(&self) -> bool {
        self.bpb2.is_valid()
    }

    pub(crate) fn update_bpb_from_format(&mut self, format: StandardFormat) -> Result<(), DiskImageError> {
        if format == StandardFormat::Invalid {
            return Err(DiskImageError::IncompatibleImage);
        }
        self.bpb2 = BiosParameterBlock2::from(format);
        self.bpb3 = BiosParameterBlock3::from(format);

        // Update the internal buffer.
        self.sector_buf
            .seek(SeekFrom::Start(BPB_OFFSET))
            .map_err(|_e| DiskImageError::IoError)?;

        self.bpb2
            .write(&mut self.sector_buf)
            .map_err(|_e| DiskImageError::IoError)?;
        self.bpb3
            .write(&mut self.sector_buf)
            .map_err(|_e| DiskImageError::IoError)?;

        Ok(())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 512] {
        self.sector_buf.get_ref()
    }

    /// Write a new BPB to the provided sector buffer based on the specified StandardFormat.
    /// StandardFormat must not be Invalid!
    pub(crate) fn write_bpb_to_buffer<T: ReadWriteSeek>(&mut self, buffer: &mut T) -> Result<(), DiskImageError> {
        buffer
            .seek(SeekFrom::Start(BPB_OFFSET))
            .map_err(|_e| DiskImageError::IoError)?;

        self.bpb2.write(buffer).map_err(|_e| DiskImageError::IoError)?;
        self.bpb3.write(buffer).map_err(|_e| DiskImageError::IoError)?;
        Ok(())
    }

    /// Attempt to correlate the current Bios Parameter Block with a StandardFormat.
    /// If the BPB is invalid, or no match is found, return IncompatibleImage.
    pub(crate) fn get_standard_format(&self) -> Result<StandardFormat, DiskImageError> {
        StandardFormat::try_from(&self.bpb2).map_err(|_e| DiskImageError::IncompatibleImage)
    }

    /// Dump the BPB values to a Write implementor for debugging purposes.
    pub fn dump_bpb<T: Write>(&self, buffer: &mut T) -> Result<(), crate::io::Error> {
        writeln!(buffer, "BIOS Parameter Block v2.0:")?;
        writeln!(buffer, "\tBytes per sector: {}", self.bpb2.bytes_per_sector)?;
        writeln!(buffer, "\tSectors per cluster: {}", self.bpb2.sectors_per_cluster)?;
        writeln!(buffer, "\tReserved sectors: {}", self.bpb2.reserved_sectors)?;
        writeln!(buffer, "\tNumber of FATs: {}", self.bpb2.number_of_fats)?;
        writeln!(buffer, "\tRoot entries: {}", self.bpb2.root_entries)?;
        writeln!(buffer, "\tTotal sectors: {}", self.bpb2.total_sectors)?;
        writeln!(buffer, "\tMedia descriptor: 0x{:02X}", self.bpb2.media_descriptor)?;
        writeln!(buffer, "\tSectors per FAT: {}", self.bpb2.sectors_per_fat)?;
        writeln!(buffer)?;
        writeln!(buffer, "BIOS Parameter Block v3.0:")?;
        writeln!(buffer, "\tSectors per track: {}", self.bpb3.sectors_per_track)?;
        writeln!(buffer, "\tNumber of heads: {}", self.bpb3.number_of_heads)?;
        writeln!(buffer, "\tHidden sectors: {}", self.bpb3.hidden_sectors)?;
        writeln!(buffer)?;
        writeln!(
            buffer,
            "Boot sector marker: 0x{:02X}{:02X}",
            self.marker[0], self.marker[1]
        )?;
        let fmt = self.get_standard_format();
        if fmt.is_err() {
            writeln!(buffer, "Standard disk format not detected.")?;
        } else {
            writeln!(
                buffer,
                "Best standard disk format guess: {:?}",
                self.get_standard_format().unwrap()
            )?;
        }

        buffer.flush()?;
        Ok(())
    }
}
