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

    src/boot_sector/bpb.rs

    Routines for reading and modifying the BIOS Parameter block.
    This structure was present from DOS 2.0 onwards, although it was expanded
    with almost every DOS release. The BPB is used to encode metadata about the
    diskette media type and filesystem.

    When creating disk images with a supplied boot sector template, we must
    be able to patch the BPB values as appropriate for the specified floppy
    image format, or the disk will not be bootable.
*/
use crate::diskimage::StandardFormat;
use crate::io::{ReadSeek, ReadWriteSeek, SeekFrom, Write};
use crate::DiskImageError;
use binrw::{binrw, BinRead, BinWrite};

// Offset of the bios parameter block in the boot sector.
pub const BPB_OFFSET: u64 = 0x0B;

#[derive(Debug, Default)]
#[binrw]
#[brw(little)]
pub(crate) struct BiosParameterBlock2 {
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
    pub(crate) reserved_sectors: u16,
    pub(crate) number_of_fats: u8,
    pub(crate) root_entries: u16,
    pub(crate) total_sectors: u16,
    pub(crate) media_descriptor: u8,
    pub(crate) sectors_per_fat: u16,
}

impl BiosParameterBlock2 {
    /// Perform a sanity check on the BPB parameters. This functio should return true if a valid
    /// BPB is present for any standard floppy disk format from 160K to 2.88MB.
    ///
    pub fn is_valid(&self) -> bool {
        // TODO: Make more robust by validating against the media descriptor for specific values
        //       instead of ranges.
        if self.bytes_per_sector < 128 || self.bytes_per_sector > 4096 {
            return false;
        }
        if self.sectors_per_cluster > 2 {
            return false;
        }
        if self.number_of_fats == 0 || self.number_of_fats > 2 {
            return false;
        }
        if self.root_entries < 0x70 || self.root_entries > 0xF0 {
            return false;
        }
        if self.total_sectors < 320 || self.total_sectors > 5760 {
            return false;
        }
        if self.sectors_per_fat < 1 || self.sectors_per_fat > 9 {
            return false;
        }
        true
    }
}

impl TryFrom<&BiosParameterBlock2> for StandardFormat {
    type Error = &'static str;

    fn try_from(bpb: &BiosParameterBlock2) -> Result<Self, Self::Error> {
        let mut best_match = None;

        match bpb.total_sectors {
            320 => best_match = Some(StandardFormat::PcFloppy160),
            360 => best_match = Some(StandardFormat::PcFloppy180),
            640 => best_match = Some(StandardFormat::PcFloppy320),
            720 => best_match = Some(StandardFormat::PcFloppy360),
            1440 => best_match = Some(StandardFormat::PcFloppy720),
            1200 => best_match = Some(StandardFormat::PcFloppy1200),
            2880 => best_match = Some(StandardFormat::PcFloppy1440),
            5760 => best_match = Some(StandardFormat::PcFloppy2880),
            _ => {}
        };

        if let Some(best_match) = best_match {
            return Ok(best_match);
        }

        match bpb.media_descriptor {
            0xFE => best_match = Some(StandardFormat::PcFloppy160),
            0xFC => best_match = Some(StandardFormat::PcFloppy180),
            0xFD => best_match = Some(StandardFormat::PcFloppy360),
            0xFF => best_match = Some(StandardFormat::PcFloppy320),
            0xF9 => best_match = Some(StandardFormat::PcFloppy1200),
            0xF0 => best_match = Some(StandardFormat::PcFloppy1440),
            _ => {}
        }

        if let Some(best_match) = best_match {
            return Ok(best_match);
        }

        Err("Invalid BPB")
    }
}

impl From<StandardFormat> for BiosParameterBlock2 {
    fn from(format: StandardFormat) -> Self {
        match format {
            StandardFormat::PcFloppy160 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0x70,
                total_sectors: 320,
                media_descriptor: 0xFE,
                sectors_per_fat: 1,
            },
            StandardFormat::PcFloppy180 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0x70,
                total_sectors: 360,
                media_descriptor: 0xFE,
                sectors_per_fat: 1,
            },
            StandardFormat::PcFloppy320 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0x70,
                total_sectors: 640,
                media_descriptor: 0xFF,
                sectors_per_fat: 1,
            },
            StandardFormat::PcFloppy360 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0x70,
                total_sectors: 720,
                media_descriptor: 0xFD,
                sectors_per_fat: 2,
            },
            StandardFormat::PcFloppy720 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0x70,
                total_sectors: 1440,
                media_descriptor: 0xFD,
                sectors_per_fat: 3,
            },
            StandardFormat::PcFloppy1200 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0xE0,
                total_sectors: 1200,
                media_descriptor: 0xF9,
                sectors_per_fat: 7,
            },
            StandardFormat::PcFloppy1440 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 1,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0xE0,
                total_sectors: 2880,
                media_descriptor: 0xF0,
                sectors_per_fat: 9,
            },
            StandardFormat::PcFloppy2880 => BiosParameterBlock2 {
                bytes_per_sector: 512,
                sectors_per_cluster: 1,
                reserved_sectors: 1,
                number_of_fats: 2,
                root_entries: 0xF0,
                total_sectors: 5760,
                media_descriptor: 0xF0,
                sectors_per_fat: 9,
            },
            _ => Default::default(),
        }
    }
}

/// BIOS Parameter Block extensions introduced in MS-DOS 3.0
#[derive(Debug, Default)]
#[binrw]
#[brw(little)]
pub(crate) struct BiosParameterBlock3 {
    pub(crate) sectors_per_track: u16,
    pub(crate) number_of_heads: u16,
    pub(crate) hidden_sectors: u32,
}

impl From<StandardFormat> for BiosParameterBlock3 {
    fn from(format: StandardFormat) -> Self {
        match format {
            StandardFormat::PcFloppy160 => BiosParameterBlock3 {
                sectors_per_track: 8,
                number_of_heads: 1,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy180 => BiosParameterBlock3 {
                sectors_per_track: 9,
                number_of_heads: 1,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy320 => BiosParameterBlock3 {
                sectors_per_track: 8,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy360 => BiosParameterBlock3 {
                sectors_per_track: 9,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy720 => BiosParameterBlock3 {
                sectors_per_track: 9,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy1200 => BiosParameterBlock3 {
                sectors_per_track: 15,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy1440 => BiosParameterBlock3 {
                sectors_per_track: 18,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            StandardFormat::PcFloppy2880 => BiosParameterBlock3 {
                sectors_per_track: 36,
                number_of_heads: 2,
                hidden_sectors: 0,
            },
            _ => Default::default(),
        }
    }
}

pub struct BootSector {
    pub(crate) bpb2: BiosParameterBlock2,
    pub(crate) bpb3: BiosParameterBlock3,
    pub(crate) marker: [u8; 2],
}

impl BootSector {
    pub fn new<T: ReadSeek>(buffer: &mut T) -> Result<Self, DiskImageError> {
        buffer
            .seek(SeekFrom::Start(BPB_OFFSET))
            .map_err(|_e| DiskImageError::IoError)?;
        let bpb2 = BiosParameterBlock2::read(buffer).map_err(|_e| DiskImageError::IoError)?;
        let bpb3 = BiosParameterBlock3::read(buffer).map_err(|_e| DiskImageError::IoError)?;

        buffer.seek(SeekFrom::End(-2)).unwrap();
        let mut marker = [0; 2];
        buffer.read_exact(&mut marker).unwrap();

        Ok(BootSector { bpb2, bpb3, marker })
    }

    pub(crate) fn has_valid_bpb(&self) -> bool {
        self.bpb2.is_valid()
    }

    /// Write a new BPB to the provided sector buffer based on the specified StandardFormat.
    /// StandardFormat must not be Invalid!
    pub(crate) fn write_new_bpb<T: ReadWriteSeek>(
        &self,
        buffer: &mut T,
        format: StandardFormat,
    ) -> Result<(), DiskImageError> {
        if format == StandardFormat::Invalid {
            return Err(DiskImageError::IncompatibleImage);
        }

        buffer
            .seek(SeekFrom::Start(BPB_OFFSET))
            .map_err(|_e| DiskImageError::IoError)?;

        let bpb2 = BiosParameterBlock2::from(format);
        let bpb3 = BiosParameterBlock3::from(format);

        bpb2.write(buffer).map_err(|_e| DiskImageError::IoError)?;
        bpb3.write(buffer).map_err(|_e| DiskImageError::IoError)?;

        Ok(())
    }

    /// Attempt to correlate the current Bios Parameter Block with a StandardFormat.
    /// If the BPB is invalid, or no match is found, return IncompatibleImage.
    pub(crate) fn get_standard_format(&self) -> Result<StandardFormat, DiskImageError> {
        StandardFormat::try_from(&self.bpb2).map_err(|_e| DiskImageError::IncompatibleImage)
    }

    /// Dump the BPB values to a Write implementor for debugging purposes.
    pub(crate) fn dump_bpb<T: Write>(&self, buffer: &mut T) -> Result<(), crate::io::Error> {
        writeln!(buffer, "BIOS Parameter Block v2.0:")?;
        writeln!(buffer, "\tBytes per sector: {}", self.bpb2.bytes_per_sector)?;
        writeln!(buffer, "\tSectors per cluster: {}", self.bpb2.sectors_per_cluster)?;
        writeln!(buffer, "\tReserved sectors: {}", self.bpb2.reserved_sectors)?;
        writeln!(buffer, "\tNumber of FATs: {}", self.bpb2.number_of_fats)?;
        writeln!(buffer, "\tRoot entries: {}", self.bpb2.root_entries)?;
        writeln!(buffer, "\tTotal sectors: {}", self.bpb2.total_sectors)?;
        writeln!(buffer, "\tMedia descriptor: 0x{:02X}", self.bpb2.media_descriptor)?;
        writeln!(buffer, "\tSectors per FAT: {}", self.bpb2.sectors_per_fat)?;

        writeln!(buffer, "BIOS Parameter Block v3.0: {:?}", self.bpb3)?;
        writeln!(buffer, "\tSectors per track: {}", self.bpb3.sectors_per_track)?;
        writeln!(buffer, "\tNumber of heads: {}", self.bpb3.number_of_heads)?;
        writeln!(buffer, "\tHidden sectors: {}", self.bpb3.hidden_sectors)?;

        buffer.flush()?;
        Ok(())
    }
}
