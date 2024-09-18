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

    src/parsers/hfe.rs

    A parser for the HFEv1 disk image format.

    HFE format images are an internal bitstream-level format used by the HxC disk emulator.

*/
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::{DiskCh, DiskDataEncoding, DiskDataRate, DiskImage, DiskImageError, DiskImageFormat, DEFAULT_SECTOR_SIZE};
use binrw::{binrw, BinRead};

const fn reverse_bits(mut byte: u8) -> u8 {
    byte = (byte >> 4) | (byte << 4);
    byte = ((byte & 0x33) << 2) | ((byte & 0xCC) >> 2);
    byte = ((byte & 0x55) << 1) | ((byte & 0xAA) >> 1);
    byte
}

const fn generate_reverse_table() -> [u8; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = reverse_bits(i as u8);
        i += 1;
    }
    table
}

const REVERSE_TABLE: [u8; 256] = generate_reverse_table();

pub const HFE_TRACK_OFFSET_BLOCK: u64 = 0x200;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum HfeFloppyInterface {
    IbmPcDd = 0x00,
    IbmPcHd = 0x01,
    AtariStDd = 0x02,
    AtariStHd = 0x03,
    AmigaDd = 0x04,
    AmigaHd = 0x05,
    CpcDd = 0x06,
    GenericShugartDd = 0x07,
    IbmPcEd = 0x08,
    Msx2Dd = 0x09,
    C64Dd = 0x0A,
    EmuShugart = 0x0B,
    S950Dd = 0x0C,
    S950Hd = 0x0D,
    Disable = 0xFE,
    Unknown = 0xFF,
}

impl From<u8> for HfeFloppyInterface {
    fn from(value: u8) -> Self {
        match value {
            0x00 => HfeFloppyInterface::IbmPcDd,
            0x01 => HfeFloppyInterface::IbmPcHd,
            0x02 => HfeFloppyInterface::AtariStDd,
            0x03 => HfeFloppyInterface::AtariStHd,
            0x04 => HfeFloppyInterface::AmigaDd,
            0x05 => HfeFloppyInterface::AmigaHd,
            0x06 => HfeFloppyInterface::CpcDd,
            0x07 => HfeFloppyInterface::GenericShugartDd,
            0x08 => HfeFloppyInterface::IbmPcEd,
            0x09 => HfeFloppyInterface::Msx2Dd,
            0x0A => HfeFloppyInterface::C64Dd,
            0x0B => HfeFloppyInterface::EmuShugart,
            0x0C => HfeFloppyInterface::S950Dd,
            0x0D => HfeFloppyInterface::S950Hd,
            0xFE => HfeFloppyInterface::Disable,
            _ => HfeFloppyInterface::Unknown,
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum HfeFloppyEncoding {
    IsoIbmMfm = 0x00,
    AmigaMfm = 0x01,
    IsoIbmFm = 0x02,
    EmuFm = 0x03,
    Unknown = 0xFF,
}

impl From<u8> for HfeFloppyEncoding {
    fn from(value: u8) -> Self {
        match value {
            0x00 => HfeFloppyEncoding::IsoIbmMfm,
            0x01 => HfeFloppyEncoding::AmigaMfm,
            0x02 => HfeFloppyEncoding::IsoIbmFm,
            0x03 => HfeFloppyEncoding::EmuFm,
            _ => HfeFloppyEncoding::Unknown,
        }
    }
}

pub struct HfeFormat {}

#[derive(Debug)]
#[binrw]
#[brw(little)]
struct HfeFileHeader {
    signature: [u8; 8],   // “HXCPICFE”
    format_revision: u8,  // Revision 0
    number_of_tracks: u8, // Number of track in the file
    number_of_sides: u8,  // Number of valid side (Not used by the emulator)
    track_encoding: u8,   // Track Encoding mode
    // (Used for the write support - Please see the list above)
    bit_rate: u16, // Bitrate in Kbit/s. Ex : 250=250000bits/s
    // Max value : 500
    rpm: u16,              // Rotation per minute (Not used by the emulator)
    interface_mode: u8,    // Floppy interface mode. (Please see the list above.)
    unused: u8,            // Reserved
    rack_list_offset: u16, // Offset of the track list LUT in block of 512 bytes
    // (Ex: 1=0x200)
    write_allowed: u8, // The Floppy image is write protected ?
    // v1.1 addition – Set them to 0xFF if unused.
    single_step: u8,          // 0xFF : Single Step – 0x00 Double Step mode
    track0s0_altencoding: u8, // 0x00 : Use an alternate track_encoding for track 0 Side 0
    track0s0_encoding: u8,    // alternate track_encoding for track 0 Side 0
    track0s1_altencoding: u8, // 0x00 : Use an alternate track_encoding for track 0 Side 1
    track0s1_encoding: u8,    // alternate track_encoding for track 0 Side 1
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
struct HfeTrackIndexEntry {
    offset: u16,
    len: u16,
}

impl HfeFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["hfe"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = HfeFileHeader::read(&mut image) {
            if file_header.signature == "HXCPICFE".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        // TODO: Determine what data representations would lead to data loss for PSI.
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        let image_len = image
            .seek(std::io::SeekFrom::End(0))
            .map_err(|_| DiskImageError::IoError)?;

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;

        let file_header = if let Ok(file_header) = HfeFileHeader::read(&mut image) {
            if file_header.signature == "HXCPICFE".as_bytes() {
                file_header
            } else {
                return Err(DiskImageError::UnknownFormat);
            }
        } else {
            return Err(DiskImageError::IoError);
        };

        let hfe_track_encoding = HfeFloppyEncoding::from(file_header.track_encoding);
        log::trace!(
            "Got HXE header. Cylinders: {} Heads: {} Encoding: {:?}",
            file_header.number_of_tracks,
            file_header.number_of_sides,
            hfe_track_encoding
        );
        let track_list_offset = file_header.rack_list_offset as u64 * HFE_TRACK_OFFSET_BLOCK;
        image
            .seek(std::io::SeekFrom::Start(track_list_offset))
            .map_err(|_| DiskImageError::IoError)?;

        let mut track_index_vec = Vec::new();
        for ti in 0..file_header.number_of_tracks {
            let track_index = HfeTrackIndexEntry::read(&mut image).map_err(|_| DiskImageError::IoError)?;
            log::trace!("Track index: {:?}", track_index);
            if track_index.len & 1 != 0 {
                log::error!("Track {} length cannot be odd, due to head interleave.", ti);
                return Err(DiskImageError::FormatParseError);
            }
            track_index_vec.push(track_index);
        }

        for (ti, track) in track_index_vec.iter().enumerate() {
            let mut track_data: [Vec<u8>; 2] = [Vec::with_capacity(50 * 512), Vec::with_capacity(50 * 512)];
            let track_data_offset = track.offset as u64 * HFE_TRACK_OFFSET_BLOCK;
            image
                .seek(std::io::SeekFrom::Start(track_data_offset))
                .map_err(|_| DiskImageError::IoError)?;

            // Use either the offset of the next data block od the end of the file to determine the
            // length of the current data block.
            let next_data = track_index_vec
                .get(ti + 1)
                .map(|ti| ti.offset as u64 * HFE_TRACK_OFFSET_BLOCK)
                .unwrap_or(image_len);

            let data_block_len = next_data - track_data_offset;
            let data_block_ct = data_block_len / 512;

            if data_block_len % 512 != 0 {
                log::warn!(
                    "Cylinder {} data length {} is not a multiple of 512 bytes",
                    ti,
                    track.len
                );
            } else {
                log::trace!(
                    "Cylinder {} data length {} contains {} 512 byte blocks.",
                    ti,
                    track.len,
                    data_block_ct
                );
            }

            let mut bytes_remaining = track.len as usize;
            let mut block_ct = 0;

            let mut last_block = false;
            while !last_block && bytes_remaining > 0 {
                // HFE always seems to store two heads?

                let block_data_size: usize = if bytes_remaining >= 512 {
                    256
                } else {
                    last_block = true;
                    bytes_remaining / 2
                };

                for head in 0..2 {
                    log::trace!(
                        "Reading track {} head {} block {} bytes_remaining: {}",
                        ti,
                        head,
                        block_ct,
                        bytes_remaining
                    );
                    // Read 256 bytes for the current head...
                    let mut track_block_data = vec![0; block_data_size];
                    image
                        .read_exact(&mut track_block_data)
                        .map_err(|_| DiskImageError::IoError)?;

                    // Reverse all the bits in each byte read.
                    for byte in track_block_data.iter_mut() {
                        *byte = REVERSE_TABLE[*byte as usize];
                    }

                    // Add to track data under the appropriate head no
                    track_data[head].extend_from_slice(&track_block_data);

                    bytes_remaining = match bytes_remaining.checked_sub(block_data_size) {
                        Some(bytes) => bytes,
                        None => {
                            log::error!(
                                "Track {}: Block: {} Head: {} Data underflow reading track data",
                                ti,
                                block_ct,
                                head
                            );
                            return Err(DiskImageError::FormatParseError);
                        }
                    }
                }
                block_ct += 1;
            }

            // We should have two full vectors of track data now.
            // Add the track data for head 0...
            log::trace!(
                "Adding bitstream track: C:{} H:{} Bitcells: {}",
                ti,
                0,
                track_data[0].len() * 8
            );

            disk_image.add_track_bitstream(
                DiskDataEncoding::Mfm,
                DiskDataRate::from(file_header.bit_rate as u32 * 100),
                DiskCh::from((ti as u16, 0)),
                file_header.bit_rate as u32 * 100,
                None,
                &track_data[0],
                None,
            )?;

            // And the track data for head 1.
            log::trace!(
                "Adding bitstream track: C:{} H:{} Bitcells: {}",
                ti,
                0,
                track_data[0].len() * 8
            );
            disk_image.add_track_bitstream(
                DiskDataEncoding::Mfm,
                DiskDataRate::from(file_header.bit_rate as u32 * 100),
                DiskCh::from((ti as u16, 1)),
                file_header.bit_rate as u32 * 100,
                None,
                &track_data[1],
                None,
            )?;
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((file_header.number_of_tracks as u16, file_header.number_of_sides)),
            data_rate: DiskDataRate::from(file_header.bit_rate as u32 * 100),
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: Some(file_header.write_allowed == 0),
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_reverse_bits(byte: u8) -> u8 {
        let mut reversed = 0;
        for i in 0..8 {
            reversed |= ((byte >> i) & 1) << (7 - i);
        }
        reversed
    }

    #[test]
    fn test_generate_reverse_table() {
        let table = generate_reverse_table();
        for i in 0..256 {
            assert_eq!(table[i], simple_reverse_bits(i as u8), "Failed at index {}", i);
        }
        println!("test_generate_reverse_table(): passed");
    }
}
