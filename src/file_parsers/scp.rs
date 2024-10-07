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

    src/parsers/scp.rs

    A parser for the SuperCardPro format.

    SCP format images encode raw flux information for each track of the disk.

*/
use crate::file_parsers::{bitstream_flags, FormatCaps};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::{DiskImage, DiskImageError, DiskImageFormat, DiskRpm, ParserWriteCompatibility, StandardFormat};
use binrw::binrw;
use binrw::{BinRead, BinReaderExt};

pub const BASE_CAPTURE_RES: u32 = 25;
pub const SCP_TRACK_COUNT: usize = 168;
pub const MAX_TRACK_NUMBER: usize = SCP_TRACK_COUNT - 1;

pub const SCP_FB_INDEX: u8 = 0b0000_0001;
pub const SCP_FB_TPI: u8 = 0b0000_0010;
pub const SCP_FB_RPM: u8 = 0b0000_0100;
pub const SCP_FB_TYPE: u8 = 0b0000_1000;
pub const SCP_FB_READONLY: u8 = 0b0001_0000;
pub const SCP_FB_FOOTER: u8 = 0b0010_0000;
pub const SCP_FB_EXTENDED_MODE: u8 = 0b0100_0000;
pub const SCP_NON_SCP_CAPTURE: u8 = 0b1000_0000;

#[derive(Debug)]
pub enum ScpDiskManufacturer {
    Cbm = 0x00,
    Atari = 0x10,
    Apple = 0x20,
    Pc = 0x30,
    Tandy = 0x40,
    TI = 0x50,
    Roland = 0x60,
    Amstrad = 0x70,
    Other = 0x80,
    TapeDrive = 0xE0,
    HardDrive = 0xF0,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpFileHeader {
    pub id: [u8; 3],
    pub version: u8,
    pub disk_type: u8,
    pub revolutions: u8,
    pub start_track: u8,
    pub end_track: u8,
    pub flags: u8,
    pub bit_cell_width: u8,
    pub heads: u8,
    pub resolution: u8,
    pub checksum: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackOffsetTable {
    pub track_offsets: [u32; 168],
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackHeader {
    pub id: [u8; 3],
    pub track_number: u8,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackRevolution {
    pub index_time: u32,
    pub length: u32,
    pub data_offset: u32,
}

fn scp_parse_version(version_byte: u8) -> (u8, u8) {
    let major = version_byte >> 4;
    let minor = version_byte & 0x0F;
    (major, minor)
}

fn scp_disk_type(type_byte: u8) -> Option<(ScpDiskManufacturer, Option<StandardFormat>)> {
    let manufacturer = match type_byte & 0xF0 {
        0x00 => ScpDiskManufacturer::Cbm,
        0x10 => ScpDiskManufacturer::Atari,
        0x20 => ScpDiskManufacturer::Apple,
        0x30 => ScpDiskManufacturer::Pc,
        0x40 => ScpDiskManufacturer::Tandy,
        0x50 => ScpDiskManufacturer::TI,
        0x60 => ScpDiskManufacturer::Roland,
        0x70 => ScpDiskManufacturer::Amstrad,
        0x80 => ScpDiskManufacturer::Other,
        0xE0 => ScpDiskManufacturer::TapeDrive,
        0xF0 => ScpDiskManufacturer::HardDrive,
        _ => return None,
    };

    let subtype = type_byte & 0x0F;
    let mut disk_format = None;

    match manufacturer {
        ScpDiskManufacturer::Pc => {
            disk_format = match subtype {
                0x00 => Some(StandardFormat::PcFloppy360),
                0x01 => Some(StandardFormat::PcFloppy720),
                0x02 => Some(StandardFormat::PcFloppy1200),
                0x03 => Some(StandardFormat::PcFloppy1440),
                _ => None,
            };
            Some((manufacturer, disk_format))
        }
        ScpDiskManufacturer::Tandy => {
            disk_format = match subtype {
                0x00 => None,
                0x01 => Some(StandardFormat::PcFloppy180),
                0x02 => None,
                0x03 => Some(StandardFormat::PcFloppy360),
                _ => None,
            };
            Some((manufacturer, disk_format))
        }

        _ => None,
    }
}

pub struct ScpFormat {}

impl ScpFormat {
    pub fn extensions() -> Vec<&'static str> {
        vec!["86f"]
    }

    pub fn capabilities() -> FormatCaps {
        bitstream_flags()
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }
        let header = if let Ok(header) = ScpFileHeader::read(&mut image) {
            header
        } else {
            return false;
        };

        header.id == "SCP".as_bytes()
    }

    pub fn can_write(image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();
        disk_image.set_source_format(DiskImageFormat::SuperCardPro);

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;

        let header = ScpFileHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;
        if header.id != "SCP".as_bytes() {
            return Err(DiskImageError::UnsupportedFormat);
        }
        log::trace!("Detected SCP file.");

        let (disk_manufacturer, disk_type) = match scp_disk_type(header.disk_type) {
            Some(dt) => {
                log::trace!("Detected disk type: {:?}", dt);
                dt
            }
            None => {
                log::error!("Unknown SCP disk type: {:02X}", header.disk_type);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        match disk_type.is_some() {
            true => {
                log::trace!(
                    "Have supported disk type. Manufacturer: {:?} Type: {:?}",
                    disk_manufacturer,
                    disk_type
                );
            }
            _ => {
                log::warn!(
                    "Unsupported SCP disk type. Manufacturer: {:?} Type: {:1X}",
                    disk_manufacturer,
                    header.disk_type & 0x0F
                );
                //return Err(DiskImageError::UnsupportedFormat);
            }
        }

        let mut disk_major_ver = 0;
        let mut disk_minor_ver = 0;

        // Handle various flags now.
        if header.flags & SCP_FB_FOOTER != 0 {
            log::trace!("Extension footer is present.");
        } else {
            log::trace!("Extension footer is NOT present.");
            (disk_major_ver, disk_minor_ver) = scp_parse_version(header.version);
            log::trace!(
                "SCP version {}.{} ({:02X})",
                disk_major_ver,
                disk_minor_ver,
                header.version
            );
        }

        let disk_rpm = if header.flags & SCP_FB_RPM != 0 {
            DiskRpm::Rpm300
        } else {
            DiskRpm::Rpm360
        };
        log::trace!("Disk RPM: {:?}", disk_rpm);

        let disk_readonly = header.flags & SCP_FB_READONLY == 0;
        log::trace!("Disk read-only flag: {}", disk_readonly);

        if header.flags & SCP_FB_EXTENDED_MODE != 0 {
            log::error!("Extended mode SCP images not supported.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let flux_normalized = header.flags & SCP_FB_TYPE != 0;
        log::trace!("Flux data normalization flag: {}", flux_normalized);

        if header.flags & SCP_NON_SCP_CAPTURE == 0 {
            log::trace!("SCP image was created by SuperCardPro device.");
        } else {
            log::trace!("SCP image was not created by SuperCardPro device.");
        }

        log::trace!("Disk contains {} revolutions per track.", header.revolutions);
        log::trace!(
            "Starting track: {} Ending track: {}",
            header.start_track,
            header.end_track
        );
        log::trace!(
            "Bit cell width: {}",
            if header.bit_cell_width == 0 {
                16
            } else {
                header.bit_cell_width
            }
        );
        if header.bit_cell_width != 0 {
            log::error!("Non-standard bit cell width not supported.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let disk_heads = match header.heads {
            0 => 2,
            1 => 1,
            2 => {
                log::error!("SCP images with just side 1 are not supported.");
                return Err(DiskImageError::UnsupportedFormat);
            }
            _ => {
                log::error!("Unsupported number of disk heads: {}", header.heads);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        let capture_resolution = BASE_CAPTURE_RES + (header.resolution as u32 * BASE_CAPTURE_RES);
        log::trace!("Capture resolution: {}ns", capture_resolution);

        if header.checksum == 0 {
            log::trace!("Image has CRC==0. Skipping CRC verification.");
        } else {
            log::trace!("Image CRC: {:08X}", header.checksum);
            log::trace!("Image CRC not verified.");
        }

        let mut track_offsets: Vec<u32> = Vec::new();

        for to in 0..SCP_TRACK_COUNT {
            let track_offset: u32 = image.read_le().map_err(|_| DiskImageError::IoError)?;
            if track_offset > 0 {
                //log::trace!("Track offset table entry {} : {:08X}", to, track_offset);
                track_offsets.push(track_offset);
            } else {
                break;
            }
        }
        log::trace!("Got {} track offsets.", track_offsets.len());

        for (ti, offset) in track_offsets.iter().enumerate() {
            // Seek to the track header.
            image
                .seek(std::io::SeekFrom::Start(*offset as u64))
                .map_err(|_| DiskImageError::IoError)?;

            // Read the track header.
            let track_header = ScpTrackHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;
            log::trace!("Track number: {} offset: {:08X}", track_header.track_number, offset);

            // Verify header.
            if track_header.id != "TRK".as_bytes() {
                log::error!("Expected track header signature, got: {:?}", track_header.id);
                return Err(DiskImageError::UnsupportedFormat);
            }

            // Read in revolutions.
            for r in 0..header.revolutions {
                let revolution = ScpTrackRevolution::read(&mut image).map_err(|_| DiskImageError::IoError)?;
                log::trace!(
                    "Revolution {} index time: {:08} length: {:08} flux offset: {:08}",
                    r,
                    revolution.index_time,
                    revolution.length,
                    offset + revolution.data_offset
                );
            }
        }

        log::trace!("Read {} valid track offsets.", track_offsets.len());

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(image: &DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
