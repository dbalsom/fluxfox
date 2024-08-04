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

    src/parsers/td0.rs

    A parser for the Teledisk (TD0) disk image format.

    The proprietary format used by the Teledisk disk copying software, published by Sydex in the
    1980s. This utility was quite popular for early disk archival efforts, and many Teledisk images
    exist in the wild.

    Teledisk disk images can be optionally encoded with 'advanced compression' which is a form of
    LZHUF compression.

*/

use crate::file_parsers::compression::lzhuf::{expand, TD0_READ_OPTIONS};
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, Read, ReadSeek, ReadWriteSeek, Seek};
use crate::{DiskImage, DiskImageError, DiskImageFormat};
use binrw::{binrw, BinRead};

pub const SECTOR_DUPLICATED: u8 = 0b0000_0001;
pub const SECTOR_CRC_ERROR: u8 = 0b0000_0010;
pub const SECTOR_DELETED: u8 = 0b0000_0100;

pub const SECTOR_SKIPPED: u8 = 0b0001_0000;
pub const SECTOR_NO_DATA: u8 = 0b0010_0000;
pub const SECTOR_NO_ID: u8 = 0b0100_0000;

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct TelediskHeader {
    pub id: [u8; 2],
    pub sequence: u8,
    pub check_sequence: u8,
    pub version: u8,
    pub data_rate: u8,
    pub drive_type: u8,
    pub stepping: u8,
    pub allocation_flag: u8,
    pub heads: u8,
    pub crc: u16,
}

pub const COMMENT_HEADER_SIZE: usize = 10;
/// Teledisk comment block header
/// 'length' bytes of comment data line records follow the header, as nul-terminated strings.
#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct CommentHeader {
    pub crc: u16,
    pub length: u16,
    pub year: u8,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct TrackHeader {
    pub sectors: u8,
    pub cylinder: u8,
    pub head: u8,
    pub crc: u8,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct SectorHeader {
    pub cylinder: u8,
    pub head: u8,
    pub sector_id: u8,
    pub sector_size: u8,
    pub flags: u8,
    pub crc: u8,
}

pub struct Td0Format {}

fn td0_crc(data: &[u8], input_crc: u16) -> u16 {
    let mut crc = input_crc;

    for byte in data.iter() {
        crc ^= (*byte as u16) << 8;
        for _j in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0xA097 } else { 0 };
        }
    }
    crc
}

fn calc_crc<RWS: ReadSeek>(image: &mut RWS, offset: u64, len: usize) -> Result<u16, DiskImageError> {
    let mut crc_data = vec![0u8; len];

    let saved_pos = image.stream_position().map_err(|_| DiskImageError::IoError)?;
    image
        .seek(std::io::SeekFrom::Start(offset))
        .map_err(|_| DiskImageError::IoError)?;
    image.read_exact(&mut crc_data).map_err(|_| DiskImageError::IoError)?;
    image
        .seek(std::io::SeekFrom::Start(saved_pos))
        .map_err(|_| DiskImageError::IoError)?;
    Ok(td0_crc(&crc_data, 0))
}

impl Td0Format {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::TeleDisk
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = TelediskHeader::read(&mut image) {
            if file_header.id == "TD".as_bytes() || file_header.id == "td".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        // TODO: Determine what data representations would lead to data loss for IMD.
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let disk_image = DiskImage::default();
        let mut image_data = Vec::new();

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;
        image
            .read_to_end(&mut image_data)
            .map_err(|_| DiskImageError::IoError)?;

        if image_data.len() < 12 {
            log::trace!("Image is too small to be a Teledisk image.");
            return Err(DiskImageError::UnknownFormat);
        }

        // Read first 10 bytes to calculate header CRC.
        let header_crc = td0_crc(&image_data[0..10], 0);

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;
        let file_header = TelediskHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;
        let detected = file_header.id == "TD".as_bytes() || file_header.id == "td".as_bytes();

        if !detected {
            return Err(DiskImageError::UnknownFormat);
        }

        let compressed = file_header.id == "td".as_bytes();
        let major_version = file_header.version / 10;
        let minor_version = file_header.version % 10;
        let has_comment_block = file_header.stepping & 0x80 != 0;

        log::trace!(
            "Detected Teledisk Image, version {}.{}, compressed: {} comment_block: {}",
            major_version,
            minor_version,
            compressed,
            has_comment_block
        );

        log::trace!("Header CRC: {:04X} Calculated CRC: {:04X}", file_header.crc, header_crc,);
        if file_header.crc != header_crc {
            return Err(DiskImageError::ImageCorruptError);
        }

        // Decompress the image data if necessary.
        let mut compressed_data = Cursor::new(image_data.to_vec());
        let mut image_data_ref = &mut compressed_data;
        let mut decompression_buffer = Cursor::new(Vec::with_capacity(image_data.len() * 2));
        let mut decompression_length = 0;
        if compressed {
            (_, decompression_length) = expand(&mut compressed_data, &mut decompression_buffer, &TD0_READ_OPTIONS)
                .map_err(|_| DiskImageError::ImageCorruptError)?;
            log::trace!(
                "Decompressed {} bytes to {} bytes",
                image_data.len(),
                decompression_length
            );
            //log::trace!("Decompressed data: {:02X?}", &decompression_buffer.into_inner()[0..64]);
            image_data_ref = &mut decompression_buffer;
        }

        // From this point forward, we are working with the decompressed data.
        image_data_ref
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;

        // Parse comment block if indicated.
        if has_comment_block {
            let comment_header = CommentHeader::read(&mut image_data_ref).map_err(|_| DiskImageError::IoError)?;
            let calculated_crc = calc_crc(
                &mut image_data_ref,
                2,
                COMMENT_HEADER_SIZE - 2 + comment_header.length as usize,
            )?;

            if comment_header.crc != calculated_crc {
                return Err(DiskImageError::ImageCorruptError);
            }

            log::trace!(
                "Comment block header crc: {:04X} calculated_crc: {:04X}",
                comment_header.crc,
                calculated_crc
            );

            if comment_header.length as u64 > decompression_length.saturating_sub(COMMENT_HEADER_SIZE as u64) {
                return Err(DiskImageError::ImageCorruptError);
            }

            let mut comment_data_block = vec![0; comment_header.length as usize];
            image_data_ref
                .read_exact(&mut comment_data_block)
                .map_err(|_| DiskImageError::IoError)?;

            // Comment black consists of nul-terminated strings. Convert nul terminators to newlines.
            for char in comment_data_block.iter_mut() {
                if *char == 0 {
                    *char = b'\n';
                }
            }

            let comment = String::from_utf8(comment_data_block).map_err(|_| DiskImageError::IoError)?;
            log::trace!("Comment block data: {}", comment);
        }

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
