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
use crate::diskimage::{DiskDescriptor, SectorDescriptor};
use crate::file_parsers::compression::lzhuf::{expand, TD0_READ_OPTIONS};
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, Read, ReadBytesExt, ReadSeek, ReadWriteSeek, Seek};
use crate::{DiskCh, DiskDataEncoding, DiskDataRate, DiskDensity, FoxHashSet, LoadingCallback};
use crate::{DiskChsn, DiskImage, DiskImageError, DiskImageFormat};
use binrw::{binrw, BinRead};

//pub const SECTOR_DUPLICATED: u8 = 0b0000_0001;
pub const SECTOR_CRC_ERROR: u8 = 0b0000_0010;
pub const SECTOR_DELETED: u8 = 0b0000_0100;

pub const SECTOR_SKIPPED: u8 = 0b0001_0000;
pub const SECTOR_NO_DAM: u8 = 0b0010_0000;

// When would we see this set? How would a sector with no IDAM even be seen?
//pub const SECTOR_NO_IDAM: u8 = 0b0100_0000;

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

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct SectorDataHeader {
    pub len: u16,
    pub encoding: u8,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct RepeatedDataEntry {
    pub count: u16,
    pub data: [u8; 2],
}

pub struct Td0Format {}

fn td0_data_rate(rate: u8) -> DiskDataRate {
    match rate & 0x03 {
        0 => DiskDataRate::Rate250Kbps,
        1 => DiskDataRate::Rate300Kbps,
        2 => DiskDataRate::Rate500Kbps,
        _ => {
            log::warn!("TD0 Data Rate out of range: {} Assuming 300Kbps", rate);
            DiskDataRate::Rate300Kbps
        }
    }
}

/// Implement a 16-bit CRC for the TD0 format. The TD0 CRC is a simple polynomial CRC with a
/// polynomial of 0xA097.
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

/// Calculate the CRC of a block of data in a disk image, starting at the specified offset and
/// extending for the specified length. The stream position is restored to its original position
/// after CRC calculation.
fn calc_crc<RWS: ReadSeek>(image: &mut RWS, offset: u64, len: usize, input_crc: u16) -> Result<u16, DiskImageError> {
    let mut crc_data = vec![0u8; len];

    let saved_pos = image.stream_position()?;
    image.seek(std::io::SeekFrom::Start(offset))?;
    image.read_exact(&mut crc_data)?;
    image.seek(std::io::SeekFrom::Start(saved_pos))?;
    Ok(td0_crc(&crc_data, input_crc))
}

impl Td0Format {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::TeleDisk
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["td0"]
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
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFormat::TeleDisk);

        let mut image_data = Vec::new();

        read_buf.seek(std::io::SeekFrom::Start(0))?;
        read_buf.read_to_end(&mut image_data)?;

        if image_data.len() < 12 {
            log::trace!("Image is too small to be a Teledisk read_buf.");
            return Err(DiskImageError::UnknownFormat);
        }

        // Read first 10 bytes to calculate header CRC.
        let header_crc = td0_crc(&image_data[0..10], 0);

        read_buf.seek(std::io::SeekFrom::Start(0))?;
        let file_header = TelediskHeader::read(&mut read_buf)?;
        let detected = file_header.id == "TD".as_bytes() || file_header.id == "td".as_bytes();

        if !detected {
            return Err(DiskImageError::UnknownFormat);
        }

        let compressed = file_header.id == "td".as_bytes();
        let major_version = file_header.version / 10;
        let minor_version = file_header.version % 10;
        let has_comment_block = file_header.stepping & 0x80 != 0;

        let disk_data_rate = td0_data_rate(file_header.data_rate);

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

        // Decompress the read_buf data if necessary.
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
            image_data_ref = &mut decompression_buffer;
        }

        // From this point forward, we are working with the decompressed data.
        image_data_ref.seek(std::io::SeekFrom::Start(0))?;

        // Parse comment block if indicated.
        if has_comment_block {
            let comment_header = CommentHeader::read(&mut image_data_ref)?;
            let calculated_crc = calc_crc(
                &mut image_data_ref,
                2,
                COMMENT_HEADER_SIZE - 2 + comment_header.length as usize,
                0,
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
            image_data_ref.read_exact(&mut comment_data_block)?;

            // Comment black consists of nul-terminated strings. Convert nul terminators to newlines.
            for char in comment_data_block.iter_mut() {
                if *char == 0 {
                    *char = b'\n';
                }
            }

            let comment = String::from_utf8(comment_data_block).map_err(|_| DiskImageError::FormatParseError)?;
            log::trace!("Comment block data: {}", comment);
        }

        // Read tracks in
        let mut cylinder_set: FoxHashSet<u16> = FoxHashSet::new();

        let mut track_header_offset = image_data_ref.stream_position()?;
        while let Ok(track_header) = TrackHeader::read(&mut image_data_ref) {
            let calculated_track_header_crc = calc_crc(&mut image_data_ref, track_header_offset, 3, 0)?;
            log::trace!(
                "Read track header. c:{} h:{} Sectors: {} crc: {:02X} calculated: {:02X}",
                track_header.cylinder,
                track_header.head,
                track_header.sectors,
                track_header.crc,
                calculated_track_header_crc as u8
            );

            if track_header.sectors == 0xFF {
                // End of track headers.
                log::trace!("End of TeleDisk track headers.");
                break;
            }

            if track_header.crc != calculated_track_header_crc as u8 {
                return Err(DiskImageError::ImageCorruptError);
            }

            log::trace!("Adding track: c:{} h:{}...", track_header.cylinder, track_header.head);
            let new_track = disk_image.add_track_metasector(
                DiskDataEncoding::Mfm,
                disk_data_rate,
                DiskCh::from((track_header.cylinder as u16, track_header.head)),
            )?;
            cylinder_set.insert(track_header.cylinder as u16);

            for _s in 0..track_header.sectors {
                //let sector_header_offset = image_data_ref.stream_position()?;
                let sector_header = SectorHeader::read(&mut image_data_ref)?;
                log::trace!(
                    "Read sector header: c:{} h:{} sid:{} size:{} flags:{:02X} crc:{:02X}",
                    sector_header.cylinder,
                    sector_header.head,
                    sector_header.sector_id,
                    sector_header.sector_size,
                    sector_header.flags,
                    sector_header.crc
                );

                // The description of the sector header CRC in Dave Dunfield's TD0 notes is incorrect.
                // The CRC is calculated for the expanded data block, and does not include the
                // sector header or sector data header.

                // A Sector Data Header follows as long as neither of these two flags are not set.
                let have_sector_data = sector_header.flags & (SECTOR_NO_DAM | SECTOR_SKIPPED) == 0;
                let sector_size_bytes = DiskChsn::n_to_bytes(sector_header.sector_size);

                if have_sector_data {
                    // let sector_data_header_offset =
                    //     image_data_ref.stream_position()?;
                    let sector_data_header = SectorDataHeader::read(&mut image_data_ref)?;

                    log::trace!(
                        "Read sector data header. len:{} encoding:{}",
                        sector_data_header.len - 1,
                        sector_data_header.encoding
                    );

                    // 'len' field of sector data header includes the encoding byte.
                    let mut sector_data_vec = vec![0; sector_size_bytes];

                    match sector_data_header.encoding {
                        0 => {
                            // Raw data. 'len' bytes follow.
                            image_data_ref.read_exact(&mut sector_data_vec)?;
                        }
                        1 => {
                            // Repeated two-byte pattern.
                            Td0Format::td0_decompress_repeated_data(&mut image_data_ref, &mut sector_data_vec)?;
                        }
                        2 => {
                            // Run-length encoded data.
                            Td0Format::td0_decompress_rle_data(&mut image_data_ref, &mut sector_data_vec)?;
                        }
                        _ => {
                            log::error!("Unknown sector data encoding: {}", sector_data_header.encoding);
                            return Err(DiskImageError::FormatParseError);
                        }
                    }

                    // Calculate sector CRC from expanded data.
                    let data_crc = td0_crc(&sector_data_vec, 0);
                    log::trace!(
                        "Sector header crc: {:02X} Calculated data block crc: {:02X}",
                        sector_header.crc,
                        data_crc as u8
                    );

                    if sector_header.crc != data_crc as u8 {
                        return Err(DiskImageError::ImageCorruptError);
                    }

                    // Add this sector to track.
                    let sd = SectorDescriptor {
                        id_chsn: DiskChsn::new(
                            sector_header.cylinder as u16,
                            sector_header.head,
                            sector_header.sector_id,
                            DiskChsn::bytes_to_n(sector_data_vec.len()),
                        ),
                        data: sector_data_vec,
                        weak_mask: None,
                        hole_mask: None,
                        address_crc_error: false,
                        data_crc_error: sector_header.flags & SECTOR_CRC_ERROR != 0,
                        deleted_mark: sector_header.flags & SECTOR_DELETED != 0,
                        missing_data: false,
                    };

                    new_track.add_sector(&sd, false)?;
                }
            }

            // Update the track header offset for next track header crc calculation.
            track_header_offset = image_data_ref.stream_position()?;
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((cylinder_set.len() as u16, file_header.heads)),
            data_rate: disk_data_rate,
            data_encoding: DiskDataEncoding::Mfm,
            density: DiskDensity::from(disk_data_rate),
            default_sector_size: 512,
            rpm: None,
            write_protect: None,
        };

        Ok(())
    }

    pub fn td0_decompress_repeated_data<RWS: ReadSeek>(
        read_buf: &mut RWS,
        output: &mut [u8],
    ) -> Result<(), DiskImageError> {
        let data_len = output.len();
        let mut decoded_len = 0;
        while decoded_len < data_len {
            let entry = RepeatedDataEntry::read(read_buf)?;

            let count = entry.count as usize;

            for _ in 0..count {
                if decoded_len < (data_len - 1) {
                    output[decoded_len + 1] = entry.data[0];
                    output[decoded_len] = entry.data[1];
                    decoded_len += 2;
                }
                else {
                    return Err(DiskImageError::FormatParseError);
                }
            }
        }

        log::trace!("td0_decompress_repeated_data(): Decoded {} bytes", decoded_len);
        Ok(())
    }

    pub fn td0_decompress_rle_data<RWS: ReadSeek>(read_buf: &mut RWS, output: &mut [u8]) -> Result<(), DiskImageError> {
        let start_pos = read_buf.stream_position()?;
        //log::trace!("RLE data start pos: {:X}", start_pos);
        let data_len = output.len();
        let mut decoded_len = 0;
        let mut encoded_len = 0;

        while decoded_len < data_len {
            let entry_code = read_buf.read_u8()?;
            encoded_len += 1;

            if entry_code == 0 {
                // Literal data block. The next byte encodes a length, and `length` bytes are copied
                // to the output slice.
                let block_len = read_buf.read_u8()? as usize;
                read_buf.read_exact(&mut output[decoded_len..decoded_len + block_len])?;
                decoded_len += block_len;
                encoded_len += block_len;
            }
            else {
                // Run-length encoded block. The entry code byte encodes the length of the data pattern,

                let pattern_length = entry_code as usize * 2;
                let repeat_ct = read_buf.read_u8()?;
                let mut pattern_block = vec![0; pattern_length];
                read_buf.read_exact(&mut pattern_block)?;
                encoded_len += pattern_length + 1;

                for _ in 0..repeat_ct {
                    if decoded_len < data_len {
                        output[decoded_len..decoded_len + pattern_length].copy_from_slice(&pattern_block);
                        decoded_len += pattern_length;
                    }
                    else {
                        let data_pos = read_buf.stream_position()?;
                        log::trace!(
                            "td0_decompress_rle_data(): Output buffer overrun; input_offset: {} decoded_len: {}",
                            data_pos - start_pos,
                            decoded_len
                        );
                        return Err(DiskImageError::FormatParseError);
                    }
                }
            }
        }

        log::trace!(
            "td0_decompress_rle_data(): Decoded {}->{} bytes",
            encoded_len,
            decoded_len
        );

        //util::dump_slice(output, 16, 0, std::io::stdout())?;

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
