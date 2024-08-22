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

    src/parsers/f86.rs

    A parser for the 86f disk image format. (F is prepended due to inability to
    start identifiers with numbers in Rust.)

    86f format images are an internal bitstream-level format used by the 86Box emulator.

*/
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::{DiskCh, DiskDataEncoding, DiskDataRate, DiskImage, DiskImageError, DiskRpm, DEFAULT_SECTOR_SIZE};
use binrw::{binrw, BinRead};
use std::mem::size_of;

pub const F86_DISK_HAS_SURFACE_DESC: u16 = 0b0000_0001;
pub const F86_DISK_HOLE_MASK: u16 = 0b0000_0110;
pub const F86_DISK_SIDES: u16 = 0b0000_1000;
pub const F86_DISK_WRITE_PROTECT: u16 = 0b0001_0000;
pub const F86_DISK_RPM_SLOWDOWN: u16 = 0b0110_0000;
pub const F86_DISK_BITCELL_MODE: u16 = 0b1000_0000;
pub const F86_DISK_TYPE: u16 = 0b0000_0001_0000_0000;
pub const F86_DISK_REVERSE_ENDIAN: u16 = 0b0000_1000_0000_0000;
pub const F86_DISK_SPEEDUP_FLAG: u16 = 0b0001_0000_0000_0000;

#[derive(Debug)]
#[binrw]
#[brw(little)]
struct FileHeader {
    id: [u8; 4],       // “86BF”
    minor_version: u8, // 0C (12)
    major_version: u8, // 02 (2) -> 2.12
    flags: u16,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
struct TrackHeader {
    flags: u16,
    index_hole: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
struct TrackHeaderBitCells {
    flags: u16,
    bit_cells: u32,
    index_hole: u32,
}

#[derive(Debug)]
enum F86TimeShift {
    ZeroPercent,
    SlowOnePercent,
    SlowOneAndAHalfPercent,
    SlowTwoPercent,
    FastOnePercent,
    FastOneAndAHalfPercent,
    FastTwoPercent,
}

fn f86_disk_time_shift(flags: u16) -> F86TimeShift {
    match ((flags >> 5) & 0x03, flags & F86_DISK_SPEEDUP_FLAG != 0) {
        (0b00, _) => F86TimeShift::ZeroPercent,
        (0b01, false) => F86TimeShift::SlowOnePercent,
        (0b10, false) => F86TimeShift::SlowOneAndAHalfPercent,
        (0b11, false) => F86TimeShift::SlowTwoPercent,
        (0b01, true) => F86TimeShift::FastOnePercent,
        (0b10, true) => F86TimeShift::FastOneAndAHalfPercent,
        (0b11, true) => F86TimeShift::FastTwoPercent,
        _ => unreachable!(),
    }
}

fn f86_track_data_rate(flags: u16) -> Option<DiskDataRate> {
    match flags & 0x07 {
        0b000 => Some(DiskDataRate::Rate500Kbps),
        0b001 => Some(DiskDataRate::Rate300Kbps),
        0b010 => Some(DiskDataRate::Rate250Kbps),
        0b011 => Some(DiskDataRate::Rate125Kbps),
        _ => None,
    }
}

fn f86_track_encoding(flags: u16) -> Option<DiskDataEncoding> {
    match (flags >> 3) & 0x03 {
        0b00 => Some(DiskDataEncoding::Fm),
        0b01 => Some(DiskDataEncoding::Mfm),
        0b11 => Some(DiskDataEncoding::Gcr),
        _ => None,
    }
}

fn f86_track_rpm(flags: u16) -> Option<DiskRpm> {
    match (flags >> 5) & 0x07 {
        0b000 => Some(DiskRpm::Rpm300),
        0b001 => Some(DiskRpm::Rpm360),
        _ => None,
    }
}

pub struct F86Format {}

impl F86Format {
    pub fn extensions() -> Vec<&'static str> {
        vec!["86f"]
    }

    pub fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }
        let header = if let Ok(header) = FileHeader::read(&mut image) {
            header
        } else {
            return false;
        };

        header.id == "86BF".as_bytes() && header.minor_version == 0x0C && header.major_version == 0x02
    }

    pub fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;

        let header = FileHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;

        let has_surface_desc = header.flags & F86_DISK_HAS_SURFACE_DESC != 0;
        let hole = (header.flags & F86_DISK_HOLE_MASK) >> 1;
        let heads = if header.flags & F86_DISK_SIDES != 0 { 2 } else { 1 };
        let _image_data_rate = match hole {
            0 => DiskDataRate::Rate250Kbps,
            1 => DiskDataRate::Rate500Kbps,
            2 => DiskDataRate::Rate1000Kbps,
            3 => {
                log::warn!("Unsupported hole size: {}", hole);
                return Err(DiskImageError::UnsupportedFormat);
            }
            _ => unreachable!(),
        };

        let extra_bitcell_mode = header.flags & F86_DISK_BITCELL_MODE != 0;
        let disk_sides = if header.flags & F86_DISK_SIDES != 0 { 2 } else { 1 };

        if has_surface_desc {
            log::trace!("Image has surface description.");
        }

        /*        if extra_bitcell_mode {
            log::warn!("Extra bitcell mode not implemented.");
            return Err(DiskImageError::UnsupportedFormat);
        }*/

        let time_shift = f86_disk_time_shift(header.flags);
        log::trace!("Time shift: {:?}", time_shift);
        let absolute_bitcell_count = if matches!(time_shift, F86TimeShift::ZeroPercent) && extra_bitcell_mode {
            log::trace!("Extra bitcell count is an absolute count.");
            true
        } else {
            log::error!("Unsupported time shift: {:?}", time_shift);
            return Err(DiskImageError::UnsupportedFormat);
        };

        // A table of track offsets immediately follows the header. We can calculate the number of
        // tracks from the offset of the first track - the header size, giving us the number of
        // offsets in the table.

        let mut track_offsets: Vec<(u32, usize)> = Vec::new();
        let mut first_offset_buf = [0u8; 4];
        image
            .read_exact(&mut first_offset_buf)
            .map_err(|_| DiskImageError::IoError)?;
        let first_offset = u32::from_le_bytes(first_offset_buf);

        let num_tracks = (first_offset as usize - size_of::<FileHeader>()) / 4;
        log::trace!("Track offset table has {} entries", num_tracks);

        track_offsets.push((first_offset, 0));

        // Read the rest of the track offsets now that we know how many there are
        for _ in 1..num_tracks {
            let mut offset_buf = [0u8; 4];
            image.read_exact(&mut offset_buf).map_err(|_| DiskImageError::IoError)?;
            let offset = u32::from_le_bytes(offset_buf);

            if offset == 0 {
                break;
            }

            // Adjust size of previous track offset
            if let Some((prev_offset, prev_size)) = track_offsets.last_mut() {
                log::trace!("Track offset: {} - {}", offset, *prev_offset);
                *prev_size = (offset - *prev_offset) as usize;
            }

            track_offsets.push((offset, 0));
        }

        // Patch up the size of the last track
        if let Some((prev_offset, prev_size)) = track_offsets.last_mut() {
            let stream_len = image
                .seek(std::io::SeekFrom::End(0))
                .map_err(|_| DiskImageError::IoError)?;
            *prev_size = (stream_len - *prev_offset as u64) as usize;
        }

        log::trace!("Read {} track offsets from table.", track_offsets.len());

        let mut head_n = 0;
        let mut cylinder_n = 0;

        for (track_offset, track_entry_len) in track_offsets {
            image
                .seek(std::io::SeekFrom::Start(track_offset as u64))
                .map_err(|_| DiskImageError::IoError)?;

            let (track_flags, extra_bitcells) = match extra_bitcell_mode {
                true => {
                    let track_header = TrackHeaderBitCells::read(&mut image).map_err(|_| DiskImageError::IoError)?;
                    log::trace!("Read track header with extra bitcells: {:?}", track_header);
                    (track_header.flags, Some(track_header.bit_cells))
                }
                false => {
                    let track_header = TrackHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;
                    log::trace!("Read track header: {:?}", track_header);
                    (track_header.flags, None)
                }
            };

            let track_encoding = match f86_track_encoding(track_flags) {
                Some(enc) => enc,
                None => {
                    log::error!("Unsupported data encoding: {:04X}", track_flags);
                    return Err(DiskImageError::UnsupportedFormat);
                }
            };

            let track_data_rate = match f86_track_data_rate(track_flags) {
                Some(rate) => rate,
                None => {
                    log::error!("Unsupported data rate: {:04X}", track_flags);
                    return Err(DiskImageError::UnsupportedFormat);
                }
            };

            // Read the track data
            let track_data_size = track_entry_len
                - match extra_bitcell_mode {
                    true => 10, //size_of::<TrackHeaderBitCells>(),
                    false => 6, //size_of::<TrackHeader>(),
                };

            let track_data_length = if has_surface_desc {
                track_data_size / 2
            } else {
                track_data_size
            };

            log::trace!("Track data length: {}", track_data_length);

            if absolute_bitcell_count {
                if let Some(absolute_count) = extra_bitcells {
                    log::trace!(
                        "Absolute bitcell count specifies: {} bytes. Data length is: {}",
                        absolute_count / 8,
                        track_data_length
                    );
                    if (absolute_count / 8) as usize != track_data_length {
                        log::error!("Absolute bitcell count does not match data length.");
                        return Err(DiskImageError::UnsupportedFormat);
                    }
                }
            }

            let track_data_vec = {
                let mut track_data = vec![0u8; track_data_length];
                image.read_exact(&mut track_data).map_err(|_| DiskImageError::IoError)?;
                track_data
            };

            log::trace!(
                "Adding {:?} encoded track: {}",
                track_encoding,
                DiskCh::from((cylinder_n, head_n))
            );
            disk_image.add_track_bitstream(
                track_encoding,
                track_data_rate,
                DiskCh::from((cylinder_n, head_n)),
                track_data_rate.into(),
                &track_data_vec,
                None,
            )?;

            head_n += 1;
            if head_n == disk_sides {
                cylinder_n += 1;
                head_n = 0;
            }
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((cylinder_n, heads as u8)),
            data_rate: Default::default(),
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: Some(header.flags & F86_DISK_WRITE_PROTECT != 0),
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
