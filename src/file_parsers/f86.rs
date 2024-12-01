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

use crate::{
    file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility},
    io::{ReadSeek, ReadWriteSeek},
    types::{BitStreamTrackParams, DiskDescriptor, DiskImageFlags},
};

use crate::{
    track::bitstream::BitStreamTrack,
    DiskCh,
    DiskDataEncoding,
    DiskDataRate,
    DiskDataResolution,
    DiskDensity,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    DiskRpm,
    LoadingCallback,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead, BinWrite};
use std::mem::size_of;

pub const F86_TRACK_TABLE_LEN_PER_HEAD: usize = 256;
pub const F86_TRACK_SIZE_BYTES: usize = 25000;

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

impl Default for FileHeader {
    fn default() -> Self {
        Self {
            id: *b"86BF",
            minor_version: 0x0C,
            major_version: 0x02,
            flags: 0,
        }
    }
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
    bit_cells: i32,
    index_hole: u32,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
enum F86TimeShift {
    ZeroPercent,
    SlowOnePercent,
    SlowOneAndAHalfPercent,
    SlowTwoPercent,
    FastOnePercent,
    FastOneAndAHalfPercent,
    FastTwoPercent,
}

impl F86TimeShift {
    pub(crate) fn adjust(&self, value: f64) -> f64 {
        value
            * match self {
                F86TimeShift::ZeroPercent => 1.0,
                F86TimeShift::SlowOnePercent => 1.01,
                F86TimeShift::SlowOneAndAHalfPercent => 1.01,
                F86TimeShift::SlowTwoPercent => 1.02,
                F86TimeShift::FastOnePercent => 1.0 / 1.01,
                F86TimeShift::FastOneAndAHalfPercent => 1.0 / 1.015,
                F86TimeShift::FastTwoPercent => 1.0 / 1.02,
            }
    }
}

#[derive(Debug)]
enum F86Endian {
    Little,
    Big,
}

#[derive(Debug)]
enum F86Density {
    Double,
    High,
    Extended,
    ExtendedPlus,
}

impl F86Density {
    fn track_length_words(&self, time_shift: F86TimeShift) -> usize {
        match self {
            F86Density::Double | F86Density::High => match time_shift {
                F86TimeShift::SlowTwoPercent => 12750,
                F86TimeShift::SlowOneAndAHalfPercent => 12687,
                F86TimeShift::SlowOnePercent => 12625,
                F86TimeShift::ZeroPercent => 12500,
                F86TimeShift::FastOnePercent => 12376,
                F86TimeShift::FastOneAndAHalfPercent => 12315,
                F86TimeShift::FastTwoPercent => 12254,
            },
            F86Density::Extended => match time_shift {
                F86TimeShift::SlowTwoPercent => 25250,
                F86TimeShift::SlowOneAndAHalfPercent => 25375,
                F86TimeShift::SlowOnePercent => 25250,
                F86TimeShift::ZeroPercent => 24752,
                F86TimeShift::FastOnePercent => 24630,
                F86TimeShift::FastOneAndAHalfPercent => 12315,
                F86TimeShift::FastTwoPercent => 12254,
            },
            F86Density::ExtendedPlus => match time_shift {
                F86TimeShift::SlowTwoPercent => 51000,
                F86TimeShift::SlowOneAndAHalfPercent => 50750,
                F86TimeShift::SlowOnePercent => 50500,
                F86TimeShift::ZeroPercent => 50000,
                F86TimeShift::FastOnePercent => 49504,
                F86TimeShift::FastOneAndAHalfPercent => 49261,
                F86TimeShift::FastTwoPercent => 49019,
            },
        }
    }
}

fn f86_disk_time_shift(flags: u16) -> F86TimeShift {
    match ((flags & F86_DISK_RPM_SLOWDOWN) >> 5, flags & F86_DISK_SPEEDUP_FLAG != 0) {
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

fn f86_disk_density(flags: u16) -> F86Density {
    match (flags & F86_DISK_HOLE_MASK) >> 1 {
        0b00 => F86Density::Double,
        0b01 => F86Density::High,
        0b10 => F86Density::Extended,
        0b11 => F86Density::ExtendedPlus,
        _ => unreachable!(),
    }
}

fn f86_track_data_rate(flags: u16) -> Option<DiskDataRate> {
    match flags & 0x07 {
        0b000 => Some(DiskDataRate::Rate500Kbps(1.0)),
        0b001 => Some(DiskDataRate::Rate300Kbps(1.0)),
        0b010 => Some(DiskDataRate::Rate250Kbps(1.0)),
        0b011 => Some(DiskDataRate::Rate1000Kbps(1.0)),
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

fn f86_weak_to_weak(bit_data: &mut [u8], weak_data: &[u8]) {
    for (byte, &weak_byte) in bit_data.iter_mut().zip(weak_data.iter()) {
        *byte |= weak_byte;
    }
}

fn f86_weak_to_holes(bit_data: &mut [u8], weak_data: &[u8]) {
    for (byte, &weak_byte) in bit_data.iter_mut().zip(weak_data.iter()) {
        *byte &= !weak_byte;
    }
}

/// Equivalent of 86Box's `common_get_raw_size()` function.
fn f86_track_bit_length(
    encoding: DiskDataEncoding,
    data_rate: DiskDataRate,
    rpm: DiskRpm,
    time_shift: F86TimeShift,
    extra_bitcells: i32,
) -> usize {
    let mut size = 100000.0;
    let mut rate = u32::from(data_rate) as f64 / 1000.0;
    if matches!(encoding, DiskDataEncoding::Fm) {
        rate /= 2.0;
    }

    size = (size / 250.0) * rate;
    size = (size * 300.0) / f64::from(rpm);
    size = time_shift.adjust(size);
    //log::debug!("f86_track_bit_length: rate: {}, rpm: {} size: {}", rate, rpm, size);
    (size as usize).saturating_add_signed(extra_bitcells as isize)
}

pub struct F86Format {}

impl F86Format {
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
        let header = if let Ok(header) = FileHeader::read(&mut image) {
            header
        }
        else {
            return false;
        };

        header.id == "86BF".as_bytes() && header.minor_version == 0x0C && header.major_version == 0x02
    }

    pub fn can_write(image: &DiskImage) -> ParserWriteCompatibility {
        if let Some(resolution) = image.resolution {
            if !matches!(resolution, DiskDataResolution::BitStream) {
                return ParserWriteCompatibility::Incompatible;
            }
        }
        else {
            return ParserWriteCompatibility::Incompatible;
        }

        // 86f images can encode about everything we can store for a bitstream format
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::F86Image);

        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let header = FileHeader::read(&mut read_buf)?;

        let has_surface_desc = header.flags & F86_DISK_HAS_SURFACE_DESC != 0;
        if has_surface_desc {
            log::trace!("Image has surface description.");
        }

        log::debug!(
            "bitcell flags: {},{},{},{}",
            header.flags >> 12 & 0x01,
            header.flags >> 7 & 0x01,
            header.flags >> 6 & 0x01,
            header.flags >> 5 & 0x01
        );

        let hole = f86_disk_density(header.flags);
        let heads = if header.flags & F86_DISK_SIDES != 0 { 2 } else { 1 };
        let (image_data_rate, image_density) = match hole {
            F86Density::Double => (DiskDataRate::Rate250Kbps(1.0), DiskDensity::Double),
            F86Density::High => (DiskDataRate::Rate500Kbps(1.0), DiskDensity::High),
            F86Density::Extended | F86Density::ExtendedPlus => {
                log::error!("Extended density images not supported.");
                return Err(DiskImageError::UnsupportedFormat);
            }
        };
        log::trace!("Image data rate: {:?} density: {:?}", image_data_rate, image_density);

        if header.flags & F86_DISK_TYPE != 0 {
            log::error!("Images with Zoned RPM unsupported.");
            return Err(DiskImageError::UnsupportedFormat);
        }
        let extra_bitcell_mode = header.flags & F86_DISK_BITCELL_MODE != 0;
        let disk_sides = if header.flags & F86_DISK_SIDES != 0 { 2 } else { 1 };
        let disk_data_endian = if header.flags & F86_DISK_REVERSE_ENDIAN != 0 {
            F86Endian::Big
        }
        else {
            F86Endian::Little
        };

        if matches!(disk_data_endian, F86Endian::Big) {
            log::warn!("Big-endian 86f images are not supported.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        /*        if extra_bitcell_mode {
            log::warn!("Extra bitcell mode not implemented.");
            return Err(DiskImageError::UnsupportedFormat);
        }*/

        let time_shift = f86_disk_time_shift(header.flags);
        log::debug!("Time shift: {:?}", time_shift);
        let absolute_bitcell_count = if matches!(time_shift, F86TimeShift::ZeroPercent)
            && (header.flags & F86_DISK_SPEEDUP_FLAG) != 0
            && extra_bitcell_mode
        {
            log::trace!("Extra bitcell count is an absolute count.");
            true
        }
        else {
            false
        };

        // A table of track offsets immediately follows the header. We can calculate the number of
        // tracks from the offset of the first track - the header size, giving us the number of
        // offsets in the table.

        let mut track_offsets: Vec<(u32, usize)> = Vec::new();
        let mut first_offset_buf = [0u8; 4];
        read_buf.read_exact(&mut first_offset_buf)?;
        let first_offset = u32::from_le_bytes(first_offset_buf);

        let num_tracks = (first_offset as usize - size_of::<FileHeader>()) / 4;
        log::trace!("Track offset table has {} entries", num_tracks);

        track_offsets.push((first_offset, 0));

        // Read the rest of the track offsets now that we know how many there are
        for _ in 1..num_tracks {
            let mut offset_buf = [0u8; 4];
            read_buf.read_exact(&mut offset_buf)?;
            let offset = u32::from_le_bytes(offset_buf);

            if offset == 0 {
                break;
            }

            // Adjust size of previous track offset
            if let Some((prev_offset, prev_size)) = track_offsets.last_mut() {
                log::trace!("Track offset: {} - {}", *prev_offset, offset);
                *prev_size = (offset - *prev_offset) as usize;
            }

            track_offsets.push((offset, 0));
        }

        // Patch up the size of the last track
        if let Some((prev_offset, prev_size)) = track_offsets.last_mut() {
            let stream_len = read_buf.seek(std::io::SeekFrom::End(0))?;
            *prev_size = (stream_len - *prev_offset as u64) as usize;
        }

        log::trace!("Read {} track offsets from table.", track_offsets.len());

        let mut head_n = 0;
        let mut cylinder_n = 0;

        let mut disk_rpm: Option<DiskRpm> = None;

        for (track_offset, track_entry_len) in track_offsets {
            read_buf.seek(std::io::SeekFrom::Start(track_offset as u64))?;

            let (track_flags, extra_bitcells, index_pos) = match extra_bitcell_mode {
                true => {
                    let track_header = TrackHeaderBitCells::read(&mut read_buf)?;
                    log::trace!("Read track header with extra bitcells: {:?}", track_header);
                    (
                        track_header.flags,
                        Some(track_header.bit_cells),
                        track_header.index_hole,
                    )
                }
                false => {
                    let track_header = TrackHeader::read(&mut read_buf)?;
                    log::trace!("Read track header: {:?}", track_header);
                    (track_header.flags, None, track_header.index_hole)
                }
            };

            log::debug!("Index position: {}", index_pos);

            let track_rpm = match f86_track_rpm(track_flags) {
                Some(rpm) => rpm,
                None => {
                    log::error!("Unsupported RPM: {:04X}", track_flags);
                    return Err(DiskImageError::UnsupportedFormat);
                }
            };
            if disk_rpm.is_none() {
                disk_rpm = Some(track_rpm);
            }
            else if disk_rpm != Some(track_rpm) {
                log::error!("Inconsistent RPMs in disk read_buf.");
                return Err(DiskImageError::UnsupportedFormat);
            }

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
            let raw_track_size = track_entry_len
                - match extra_bitcell_mode {
                    true => 10, //size_of::<TrackHeaderBitCells>(),
                    false => 6, //size_of::<TrackHeader>(),
                };

            if raw_track_size & 0x01 != 0 {
                log::error!("Invalid 86f: Track data size is not word-aligned.");
                return Err(DiskImageError::ImageCorruptError);
            }

            let raw_track_data_size = if has_surface_desc {
                log::debug!("Track has surface description, halving data size.");
                raw_track_size / 2
            }
            else {
                raw_track_size
            };

            log::debug!(
                "Track raw data size: {} ({} words) Extra bitcells: {}",
                raw_track_data_size,
                raw_track_data_size / 2,
                extra_bitcells.unwrap_or(0)
            );

            let mut read_length_bytes = raw_track_data_size;

            // Calculate the expected track length in words from the density and time shift.
            let mut read_length_expected_words = hole.track_length_words(time_shift);

            // Adjust track length in words for extra bitcells.
            let adjusted_read_length_words = if let Some(bitcells) = extra_bitcells {
                let track_bitcells = (read_length_expected_words * 16).saturating_add_signed(bitcells as isize);
                if track_bitcells % 16 != 0 {
                    (track_bitcells / 16) + 1
                }
                else {
                    track_bitcells / 16
                }
            }
            else {
                read_length_expected_words
            };

            log::debug!(
                "Base track word length: {} Adjusted track word length: {}",
                read_length_expected_words,
                adjusted_read_length_words,
            );

            read_length_expected_words = adjusted_read_length_words;

            let bitcell_ct = if absolute_bitcell_count {
                // An absolute bitcell count overrides the calculated track length.
                if let Some(absolute_count) = extra_bitcells {
                    let absolute_data_len =
                        ((absolute_count / 8) + if (absolute_count % 8) != 0 { 1 } else { 0 }) as usize;

                    log::trace!(
                        "Absolute bitcell count ({}) specifies: {} bytes. Raw data length is: {}",
                        absolute_count,
                        absolute_data_len,
                        raw_track_data_size
                    );

                    if absolute_data_len > raw_track_data_size {
                        log::error!(
                            "Data length calculated from absolute bitcell count is greater than track data length: {} > {}",
                            absolute_data_len,
                            raw_track_data_size
                        );
                        return Err(DiskImageError::ImageCorruptError);
                    }

                    read_length_bytes = absolute_data_len;
                    absolute_count as usize
                }
                else {
                    log::error!("Absolute bitcell count flag set, but no count provided.");
                    return Err(DiskImageError::ImageCorruptError);
                }
            }
            else {
                if raw_track_data_size < read_length_expected_words * 2 {
                    log::error!(
                        "Track data length is less than expected: {} < {}",
                        read_length_bytes,
                        read_length_expected_words * 2
                    );
                    return Err(DiskImageError::ImageCorruptError);
                }
                else if raw_track_data_size > read_length_expected_words * 2 {
                    log::warn!(
                        "Track data length is greater than expected: {} > {}",
                        read_length_bytes,
                        read_length_expected_words * 2
                    );

                    // We'll truncate the data to the expected length.
                    read_length_bytes = read_length_expected_words * 2;
                }

                // Calculate the bitcell count from track parameters. It may be less than the
                // track data length, especially for DD images.

                let calculated_bitcell_ct = f86_track_bit_length(
                    track_encoding,
                    track_data_rate,
                    track_rpm,
                    time_shift,
                    extra_bitcells.unwrap_or(0),
                );

                log::debug!(
                    "Calculated bitcell count: {} Track data length: {} bits",
                    calculated_bitcell_ct,
                    read_length_bytes * 16
                );

                calculated_bitcell_ct
            };

            log::debug!(
                "Data read length: {} ({} words)",
                read_length_bytes,
                read_length_bytes / 2
            );

            let track_data_vec = {
                let mut track_data = vec![0u8; read_length_bytes];
                read_buf.read_exact(&mut track_data)?;
                track_data
            };

            let surface_data_vec = if has_surface_desc {
                let mut surface_data = vec![0u8; read_length_bytes];
                read_buf.read_exact(&mut surface_data)?;
                Some(surface_data)
            }
            else {
                None
            };

            let mut track_weak_vec = vec![0u8; read_length_bytes];
            let mut track_hole_vec = vec![0u8; read_length_bytes];

            let (track_weak_opt, track_hole_opt) = if let Some(surface_data) = surface_data_vec {
                let mut have_weak = false;
                let mut have_hole = false;
                for (((weak, hole), surface), data) in track_weak_vec
                    .iter_mut()
                    .zip(track_hole_vec.iter_mut())
                    .zip(surface_data.iter())
                    .zip(track_data_vec.iter())
                {
                    // Weak bits are set when surface bit is 1 and data bit is 1.
                    *weak = *surface & *data;
                    // Hole bits are set when surface bit is 1 and data bit is 0.
                    *hole = *surface & !*data;

                    have_weak |= *weak != 0;
                    have_hole |= *hole != 0;
                }

                let weak_opt = have_weak.then_some(track_weak_vec.as_slice());
                let hole_opt = have_hole.then_some(track_hole_vec.as_slice());

                (weak_opt, hole_opt)
            }
            else {
                (None, None)
            };

            log::debug!(
                "Adding {:?} encoded track: {}",
                track_encoding,
                DiskCh::from((cylinder_n, head_n))
            );

            let params = BitStreamTrackParams {
                encoding: track_encoding,
                data_rate: track_data_rate,
                rpm: disk_rpm,
                ch: DiskCh::from((cylinder_n, head_n)),
                bitcell_ct: Some(bitcell_ct),
                data: &track_data_vec,
                weak: track_weak_opt,
                hole: track_hole_opt,
                detect_weak: false,
            };

            disk_image.add_track_bitstream(params)?;

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
            density: image_density,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: disk_rpm,
            write_protect: Some(header.flags & F86_DISK_WRITE_PROTECT != 0),
        };

        Ok(())
    }

    /// Write a disk read_buf in 86F format.
    /// We always emit 86f images with absolute bitcell counts - this is easier to handle.
    /// Without specifying an absolute bitcell count, there is a formula to use to calculate the
    /// number of words to write per track. Due to the variety of formats we import, we cannot
    /// guarantee a specific bitcell length.
    ///
    /// When writing track data, the size must be rounded to the nearest word (2 bytes).
    pub fn save_image<RWS: ReadWriteSeek>(image: &DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        if matches!(image.resolution(), DiskDataResolution::BitStream) {
            log::trace!("Saving 86f image...");
        }
        else {
            log::error!("Unsupported image resolution.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let mut disk_flags = 0;

        let mut has_surface_description = false;
        let has_weak_bits = image.has_weak_bits();
        if has_weak_bits {
            // We'll need to include a surface descriptor.
            log::trace!("Image has weak/hole bits.");
            has_surface_description = true;
            disk_flags |= F86_DISK_HAS_SURFACE_DESC;
        }
        else {
            log::trace!("Image has no weak/hole bits.");
        }

        disk_flags |= match image.descriptor.density {
            DiskDensity::Double => 0,
            DiskDensity::High => 0b01 << 1,
            DiskDensity::Extended => 0b10 << 1,
            _ => {
                log::error!("Unsupported disk density: {:?}", image.descriptor.density);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        disk_flags |= match image.descriptor.geometry.h() {
            1 => 0,
            2 => F86_DISK_SIDES,
            _ => {
                log::error!("Unsupported number of heads: {}", image.descriptor.geometry.h());
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        // We don't support the RPM slowdown feature.

        // We always want to specify an absolute bitcell count, so set bits 7 and 12.
        let use_absolute_bit_count = true;
        disk_flags |= F86_DISK_BITCELL_MODE;
        disk_flags |= F86_DISK_SPEEDUP_FLAG;

        if image.descriptor.write_protect.unwrap_or(false) {
            disk_flags |= F86_DISK_WRITE_PROTECT;
        }

        let f86_header = FileHeader {
            flags: disk_flags,
            ..Default::default()
        };

        // Write header to output.
        output.seek(std::io::SeekFrom::Start(0))?;
        f86_header.write(output)?;

        log::trace!("Image geometry: {}", image.descriptor.geometry);
        if image.descriptor.geometry.c() as usize > image.track_map[0].len()
            || image.descriptor.geometry.c() as usize > image.track_map[1].len()
        {
            log::error!(
                "Image geometry does not match track maps: {}: {},{}",
                image.descriptor.geometry.c(),
                image.track_map[0].len(),
                image.track_map[1].len()
            );
            return Err(DiskImageError::UnsupportedFormat);
        }

        let double_tracks = if image.descriptor.geometry.c() < 80 {
            log::trace!("Writing double tracks due to 40 track image.");
            true
        }
        else {
            false
        };

        let heads = image.descriptor.geometry.h() as usize;

        let track_entries = if double_tracks {
            image.descriptor.geometry.c() as usize * 2 * heads
        }
        else {
            image.descriptor.geometry.c() as usize * heads
        };

        log::trace!("Writing {} track entries.", track_entries);

        let mut track_offsets = vec![0u32; F86_TRACK_TABLE_LEN_PER_HEAD * heads];

        let offset_table_pos = output.stream_position()?;

        // Write track offsets to output.
        for offset in &track_offsets {
            output.write_all(&offset.to_le_bytes())?;
        }

        // We shouldn't need to change track flags per track, so set them now.
        let mut track_flags = 0;
        log::trace!("Setting data rate: {:?}", image.descriptor.data_rate);
        track_flags |= match image.descriptor.data_rate {
            DiskDataRate::Rate500Kbps(_) => 0b000,
            DiskDataRate::Rate300Kbps(_) => 0b001,
            DiskDataRate::Rate250Kbps(_) => 0b010,
            DiskDataRate::Rate1000Kbps(_) => 0b011,
            _ => {
                log::error!("Unsupported data rate: {:?}", image.descriptor.data_rate);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        log::trace!("Setting data encoding: {:?}", image.descriptor.data_encoding);
        track_flags |= match image.descriptor.data_encoding {
            DiskDataEncoding::Fm => 0b00 << 3,
            DiskDataEncoding::Mfm => 0b01 << 3,
            DiskDataEncoding::Gcr => 0b11 << 3,
        };

        log::trace!("Setting RPM: {:?}", image.descriptor.rpm);
        track_flags |= image.descriptor.rpm.map_or(0, |rpm| match rpm {
            DiskRpm::Rpm300 => 0b000 << 5,
            DiskRpm::Rpm360 => 0b001 << 5,
        });

        let mut c = 0;
        let mut h = 0;
        let mut track_copy = 0;

        for (i, offset) in track_offsets.iter_mut().take(track_entries).enumerate() {
            *offset = output.stream_position()? as u32;
            log::trace!("Writing track entry {}, c: {} h: {}, offset: {}", i, c, h, *offset);

            let ti = image.track_map[h][c as usize];

            if let Some(track) = image.track_pool[ti].as_any().downcast_ref::<BitStreamTrack>() {
                let absolute_bit_count = track.data.len();
                //log::trace!("Absolute bit count: {}", absolute_bit_count);

                let mut bit_data = track.data.data();
                let mut weak_data = track.data.weak_data();

                if has_surface_description && (bit_data.len() != weak_data.len()) {
                    log::error!("Bitstream and weak data lengths do not match.");
                    return Err(DiskImageError::UnsupportedFormat);
                }

                if !use_absolute_bit_count {
                    if bit_data.len() < F86_TRACK_SIZE_BYTES {
                        bit_data.resize(F86_TRACK_SIZE_BYTES, 0);
                    }
                    if weak_data.len() < F86_TRACK_SIZE_BYTES {
                        weak_data.resize(F86_TRACK_SIZE_BYTES, 0);
                    }
                }
                else {
                    // Pad to a word boundary
                    if bit_data.len() % 2 != 0 {
                        bit_data.push(0);
                        weak_data.push(0);
                    }
                }

                if image.has_flag(DiskImageFlags::PROLOK) && c == 39 && h == 0 {
                    log::debug!(
                        "PROLOK: Converting {} weak bits to holes.",
                        track.data.weak_data().len()
                    );
                    f86_weak_to_holes(&mut bit_data, &weak_data);
                }
                else {
                    f86_weak_to_weak(&mut bit_data, &weak_data);
                }

                log::trace!(
                    "Track has {} bitcells. Bytestream length: {}, Weak data length: {}",
                    absolute_bit_count,
                    bit_data.len(),
                    weak_data.len()
                );

                let track_header = TrackHeaderBitCells {
                    flags: track_flags,
                    bit_cells: absolute_bit_count as i32,
                    index_hole: 0,
                };

                let th_pos = output.stream_position()?;
                track_header.write(output)?;

                let after_th_pos = output.stream_position()?;
                let th_size = after_th_pos - th_pos;
                assert_eq!(th_size, 10);
                output.write_all(&bit_data)?;

                if has_surface_description {
                    output.write_all(&weak_data)?;
                }

                h += 1;
                if h == heads {
                    h = 0;

                    if double_tracks {
                        track_copy += 1;
                        if track_copy == 2 {
                            track_copy = 0;
                            c += 1;
                        }
                    }
                    else {
                        c += 1;
                    }
                }
            }
            else {
                return Err(DiskImageError::UnsupportedFormat);
            }
        }

        // Now we have to go back and patch up the offsets
        output.seek(std::io::SeekFrom::Start(offset_table_pos))?;

        log::trace!("Writing track offsets...");
        for offset in track_offsets.iter() {
            //log::trace!("Writing track offset {}: {:X} ({})", i, offset, offset);
            output.write_all(&offset.to_le_bytes())?;
        }

        // Perform post-write verification

        // Seek to the end in case the caller wants to write more data.
        output.seek(std::io::SeekFrom::End(0))?;

        Ok(())
    }
}
