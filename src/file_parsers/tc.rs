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

    src/parsers/tc.rs

    A parser for the TransCopy (.TC) disk image format.

    TransCopy images are bitstream-level images produced by the TransCopy
    utility bundled with Central Point Software's Copy II PC Option Board.

    Documentation of this format helpfully provided by NewRisingSun.
    https://www.robcraig.com/wiki/transcopy-version-5-x-format/

    TransCopy images do not have a separate weak bit mask. Instead, weak bits
    can be detected by an invalid sequence of 0's in the MFM bitstream.

    fluxfox will attempt to detect weak bits when adding tracks to the image,
    if a weak bit mask is not provided.

    The padding between tracks is not just on 256 byte boundaries. It is a bit
    unusual, but we don't write to TC yet so don't have to handle whatever
    scheme it is using. Track data is padded so that it does not pass a 64k
    boundary. This was apparently done so to make it easier for TransCopy to
    handle DMA transfer of track data.
*/

use crate::{
    file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility},
    io::{ReadSeek, ReadWriteSeek},
};

use crate::{
    types::{BitStreamTrackParams, DiskDescriptor},
    DiskCh,
    DiskDataEncoding,
    DiskDataRate,
    DiskDensity,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    DiskRpm,
    LoadingCallback,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

// Disk Type Constants
// All types are listed here, but fluxfox will initially only support PC-specific formats

// PCE tools generate TC's with the 'UNKNOWN' disk type, which is unfortunate.
// We normally use this disk type to set the data rate and RPM. So we'll have to come up with an
// alternate method for determining these values.
pub const TC_DISK_TYPE_UNKNOWN: u8 = 0xFF;
pub const TC_DISK_TYPE_MFM_HD: u8 = 0x02;
pub const TC_DISK_TYPE_MFM_DD_360: u8 = 0x03;
//pub const TC_DISK_TYPE_GCR_APPLEII: u8 = 0x04;
//pub const TC_DISK_TYPE_FM_SD: u8 = 0x05;
//pub const TC_DISK_TYPE_GCR_COMMODORE: u8 = 0x06;
pub const TC_DISK_TYPE_MFM_DD: u8 = 0x07;
//pub const TC_DISK_TYPE_AMIGA: u8 = 0x08;
//pub const TC_DISK_TYPE_FM_ATARI: u8 = 0x0C;

// Track flags. We don't use these yet, but they're here for reference.
//pub const TC_FLAG_KEEP_TRACK_LENGTH: u16 = 0b0000_0000_0000_0001;
//pub const TC_FLAG_COPY_ACROSS_INDEX: u16 = 0b0000_0000_0000_0010;

// I suppose this flag was some hint to TransCopy when writing a track. We will always create a
// weak bit mask when detecting weak bits.
//pub const TC_FLAG_COPY_WEAK_BITS: u16 = 0b0000_0000_0000_0100;
//pub const TC_FLAG_VERIFY_WRITE: u16 = 0b0000_0000_0000_1000;
//pub const TC_FLAG_TOLERANCE_ADJUST: u16 = 0b0000_0000_0100_0000;

// This flag indicates no address marks on a track. We'll find that out for ourselves when we add
// the track, so it's not really that important.
//pub const TC_FLAG_NO_ADDRESS_MARKS: u16 = 0b0000_0000_1000_0000;
//pub const TC_FLAG_UNKNOWN: u16 = 0b1000_0000_0000_0000;

// These values are used to represent empty entries in corresponding tables.
pub const TC_EMPTY_TRACK_SKEW: u16 = 0x1111;
pub const TC_EMPTY_TRACK_DATA: u16 = 0x3333;
pub const TC_EMPTY_TRACK_FLAGS: u16 = 0x4444;

#[derive(Debug)]
#[binrw]
#[brw(big)]
struct TCFileHeader {
    id: [u8; 2],        // Magic number: 0x5A 0xA5
    comment0: [u8; 32], // First comment line, zero-terminated
    comment1: [u8; 32], // Second comment line, zero-terminated
    padding: [u8; 190], // Unused, filled with random memory data
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
struct TCDiskInfo {
    disk_type: u8,
    starting_c: u8,
    ending_c: u8,
    num_sides: u8,
    cylinder_increment: u8,
    #[br(little)]
    track_skews: [u16; 256],
    #[br(big)]
    track_offsets: [u16; 256],
    #[br(little)]
    track_sizes: [u16; 256],
    #[br(little)]
    track_flags: [u16; 256],
}

/// Convert one of the TC header comment fields into a String.
fn tc_read_comment(raw_comment: &[u8]) -> String {
    let comment_end_pos = raw_comment.iter().position(|&c| c == 0).unwrap_or(raw_comment.len());

    String::from(std::str::from_utf8(&raw_comment[..comment_end_pos]).unwrap_or_default())
}

fn tc_parse_disk_type(disk_type: u8) -> Result<(DiskDataEncoding, DiskDataRate, DiskRpm), DiskImageError> {
    let (encoding, data_rate, disk_rpm) = match disk_type {
        // Return a default for UNKNOWN, as PCE tools generate TC's with this disk type.
        TC_DISK_TYPE_UNKNOWN => (DiskDataEncoding::Mfm, DiskDataRate::Rate250Kbps(1.0), DiskRpm::Rpm300),
        TC_DISK_TYPE_MFM_HD => (DiskDataEncoding::Mfm, DiskDataRate::Rate500Kbps(1.0), DiskRpm::Rpm300),
        TC_DISK_TYPE_MFM_DD_360 => (DiskDataEncoding::Mfm, DiskDataRate::Rate500Kbps(1.0), DiskRpm::Rpm360),
        TC_DISK_TYPE_MFM_DD => (DiskDataEncoding::Mfm, DiskDataRate::Rate250Kbps(1.0), DiskRpm::Rpm300),
        _ => return Err(DiskImageError::UnsupportedFormat),
    };

    Ok((encoding, data_rate, disk_rpm))
}

pub struct TCFormat {}

impl TCFormat {
    pub fn extensions() -> Vec<&'static str> {
        vec!["tc"]
    }

    pub fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_TRACK_ENCODING | FormatCaps::CAP_ENCODING_FM | FormatCaps::CAP_ENCODING_MFM
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }
        let header = if let Ok(header) = TCFileHeader::read(&mut image) {
            header
        }
        else {
            return false;
        };

        header.id[0] == 0x5A && header.id[1] == 0xA5
    }

    pub fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::TransCopyImage);

        let disk_image_size = read_buf.seek(std::io::SeekFrom::End(0))?;

        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let header = if let Ok(header) = TCFileHeader::read(&mut read_buf) {
            header
        }
        else {
            return Err(DiskImageError::UnsupportedFormat);
        };

        if header.id[0] != 0x5A || header.id[1] != 0xA5 {
            log::error!("Invalid TransCopy header id: {:?}", header.id);
            return Err(DiskImageError::UnsupportedFormat);
        }

        log::trace!("load_image(): Got TransCopy read_buf.");

        // Read comment arrays, and turn into a string with newlines.
        let comment0_string = tc_read_comment(&header.comment0);
        let comment1_string = tc_read_comment(&header.comment1);

        let comment_string = format!("{}\n{}", comment0_string, comment1_string);
        log::trace!("Read comment: {}", comment_string);

        let disk_info = if let Ok(di) = TCDiskInfo::read(&mut read_buf) {
            di
        }
        else {
            return Err(DiskImageError::FormatParseError);
        };

        // Only support PC disk types for now
        if ![
            TC_DISK_TYPE_UNKNOWN,
            TC_DISK_TYPE_MFM_HD,
            TC_DISK_TYPE_MFM_DD_360,
            TC_DISK_TYPE_MFM_DD,
        ]
        .contains(&disk_info.disk_type)
        {
            log::error!("Unsupported disk type: {:02X}", disk_info.disk_type);
            return Err(DiskImageError::IncompatibleImage);
        }

        let (disk_encoding, disk_data_rate, disk_rpm) = tc_parse_disk_type(disk_info.disk_type)?;

        log::trace!("Disk encoding: {:?}", disk_encoding);
        log::trace!("Starting cylinder: {}", disk_info.starting_c);
        log::trace!("Ending cylinder: {}", disk_info.ending_c);
        log::trace!("Number of sides: {}", disk_info.num_sides);
        log::trace!("Cylinder increment: {}", disk_info.cylinder_increment);

        if disk_info.starting_c != 0 {
            // I don't know if we'll ever encounter images like this, but for now let's require starting at 0.
            log::error!("Unsupported starting cylinder: {}", disk_info.starting_c);
            return Err(DiskImageError::IncompatibleImage);
        }

        if disk_info.cylinder_increment != 1 {
            // Similarly, I am not sure why the track increment would ever be anything other than 1.
            log::error!("Unsupported cylinder increment: {}", disk_info.cylinder_increment);
            return Err(DiskImageError::IncompatibleImage);
        }

        let raw_track_skew_ct = disk_info
            .track_skews
            .iter()
            .take_while(|&v| *v != TC_EMPTY_TRACK_SKEW)
            .count();
        let raw_track_start_ct = disk_info.track_offsets.iter().take_while(|&v| *v != 0).count();
        let raw_track_data_ct = disk_info
            .track_sizes
            .iter()
            .take_while(|&v| *v != TC_EMPTY_TRACK_DATA)
            .count();
        let raw_track_flag_ct = disk_info
            .track_flags
            .iter()
            .take_while(|&v| *v != TC_EMPTY_TRACK_FLAGS)
            .count();

        log::trace!(
            "Raw track data counts: Skews {} Starts: {} Sizes: {} Flags: {}",
            raw_track_skew_ct,
            raw_track_start_ct,
            raw_track_data_ct,
            raw_track_flag_ct,
        );

        if raw_track_skew_ct != raw_track_data_ct
            || raw_track_start_ct != raw_track_data_ct
            || raw_track_flag_ct != raw_track_data_ct
        {
            log::error!("Mismatched track data counts");
            return Err(DiskImageError::IncompatibleImage);
        }

        // Limit tracks to pairs of sides
        let raw_track_data_ct = raw_track_data_ct & !0x01;
        let mut last_track_data_offset = 0;
        for i in 0..raw_track_data_ct {
            let track_offset = (disk_info.track_offsets[i] as u64) << 8;
            let track_size = disk_info.track_sizes[i] as u64;

            let adj_track_size = if track_size % 256 == 0 {
                track_size
            }
            else {
                ((track_size >> 8) + 1) << 8
            };

            if track_offset == 0 || track_size == 0 {
                log::error!("Invalid track offset or size: {} {}", track_offset, track_size);
                return Err(DiskImageError::IncompatibleImage);
            }

            if track_offset + track_size > disk_image_size {
                log::error!("Track data extends beyond end of read_buf");
                return Err(DiskImageError::IncompatibleImage);
            }

            last_track_data_offset = track_offset + adj_track_size;

            log::trace!(
                "Track {}: RawOffset:{} Byte Offset: {} Size: {} File size: {} Calculated next offset: {}",
                i,
                disk_info.track_offsets[i],
                track_offset,
                track_size,
                adj_track_size,
                last_track_data_offset
            );
        }

        let remaining_image = disk_image_size.saturating_sub(last_track_data_offset);

        log::trace!("Remaining data in image: {}", remaining_image);
        log::trace!(
            "Remaining data per track: {}",
            remaining_image / raw_track_data_ct as u64
        );
        log::trace!(
            "Last track offset: {}",
            disk_info.track_offsets[raw_track_data_ct - 1] << 8
        );
        log::trace!("Lack track size: {}", disk_info.track_sizes[raw_track_data_ct - 1]);
        log::trace!(
            "End of track data: {}",
            ((disk_info.track_offsets[raw_track_data_ct - 1] as u64) << 8)
                + disk_info.track_sizes[raw_track_data_ct - 1] as u64
        );

        // Read the tracks
        let mut head_n = 0;
        let track_shift = match disk_info.num_sides {
            1 => 0,
            2 => 1,
            _ => {
                log::error!("Unsupported number of sides: {}", disk_info.num_sides);
                return Err(DiskImageError::IncompatibleImage);
            }
        };
        for i in 0..raw_track_data_ct {
            let cylinder_n = (i >> track_shift) as u16;
            let track_offset = (disk_info.track_offsets[i] as u64) << 8;
            let track_size = disk_info.track_sizes[i] as u64;

            let mut track_data_vec = vec![0; track_size as usize];
            read_buf.seek(std::io::SeekFrom::Start(track_offset))?;
            read_buf.read_exact(&mut track_data_vec)?;

            log::trace!(
                "Adding {:?} encoded track: {}",
                disk_encoding,
                DiskCh::from((cylinder_n, head_n))
            );

            let params = BitStreamTrackParams {
                encoding: disk_encoding,
                data_rate: disk_data_rate,
                rpm: Some(disk_rpm),
                ch: DiskCh::new(cylinder_n, head_n),
                bitcell_ct: None,
                data: &track_data_vec,
                weak: None,
                hole: None,
                detect_weak: true, // flux2tc encodes weak bits as runs of MFM 0 bits
            };
            disk_image.add_track_bitstream(params)?;

            head_n += 1;
            if head_n == disk_info.num_sides {
                head_n = 0;
            }
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((
                (raw_track_data_ct / disk_info.num_sides as usize) as u16,
                disk_info.num_sides,
            )),
            data_rate: disk_data_rate,
            data_encoding: disk_encoding,
            density: DiskDensity::from(disk_data_rate),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: Some(disk_rpm),
            write_protect: None,
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
