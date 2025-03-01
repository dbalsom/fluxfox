/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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
use crate::{
    file_parsers::{FormatCaps, ParserReadOptions, ParserWriteCompatibility, ParserWriteOptions},
    io::{ReadSeek, ReadWriteSeek},
    source_map::{MapDump, OptionalSourceMap, SourceValue},
    types::{BitStreamTrackParams, DiskCh, DiskDescriptor, Platform, TrackDataEncoding, TrackDataRate, TrackDensity},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
};
use binrw::{binrw, BinRead};
use strum::IntoEnumIterator;

const fn reverse_bits(mut byte: u8) -> u8 {
    //byte = (byte >> 4) | (byte << 4);
    byte = byte.rotate_left(4);
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

impl From<(Platform, TrackDensity)> for HfeFloppyInterface {
    fn from(value: (Platform, TrackDensity)) -> Self {
        match value {
            (Platform::IbmPc, TrackDensity::Double) => HfeFloppyInterface::IbmPcDd,
            (Platform::IbmPc, TrackDensity::High) => HfeFloppyInterface::IbmPcHd,
            (Platform::IbmPc, TrackDensity::Extended) => HfeFloppyInterface::IbmPcEd,
            (Platform::Amiga, TrackDensity::Double) => HfeFloppyInterface::AmigaDd,
            (Platform::Amiga, TrackDensity::High) => HfeFloppyInterface::AmigaHd,
            _ => HfeFloppyInterface::Unknown,
        }
    }
}

impl TryFrom<HfeFloppyInterface> for Platform {
    type Error = ();
    fn try_from(value: HfeFloppyInterface) -> Result<Self, Self::Error> {
        match value {
            HfeFloppyInterface::IbmPcDd => Ok(Platform::IbmPc),
            HfeFloppyInterface::IbmPcHd => Ok(Platform::IbmPc),
            HfeFloppyInterface::AtariStDd => Err(()),
            HfeFloppyInterface::AtariStHd => Err(()),
            HfeFloppyInterface::AmigaDd => Ok(Platform::Amiga),
            HfeFloppyInterface::AmigaHd => Ok(Platform::Amiga),
            HfeFloppyInterface::CpcDd => Err(()),
            HfeFloppyInterface::GenericShugartDd => Err(()),
            HfeFloppyInterface::IbmPcEd => Ok(Platform::IbmPc),
            HfeFloppyInterface::Msx2Dd => Err(()),
            HfeFloppyInterface::C64Dd => Err(()),
            HfeFloppyInterface::EmuShugart => Err(()),
            HfeFloppyInterface::S950Dd => Err(()),
            HfeFloppyInterface::S950Hd => Err(()),
            HfeFloppyInterface::Disable => Err(()),
            HfeFloppyInterface::Unknown => Err(()),
        }
    }
}

impl From<HfeFloppyInterface> for TrackDensity {
    fn from(value: HfeFloppyInterface) -> Self {
        match value {
            HfeFloppyInterface::IbmPcDd => TrackDensity::Double,
            HfeFloppyInterface::IbmPcHd => TrackDensity::High,
            HfeFloppyInterface::AtariStDd => TrackDensity::Double,
            HfeFloppyInterface::AtariStHd => TrackDensity::High,
            HfeFloppyInterface::AmigaDd => TrackDensity::Double,
            HfeFloppyInterface::AmigaHd => TrackDensity::High,
            HfeFloppyInterface::CpcDd => TrackDensity::Double,
            HfeFloppyInterface::GenericShugartDd => TrackDensity::Double,
            HfeFloppyInterface::IbmPcEd => TrackDensity::Extended,
            HfeFloppyInterface::Msx2Dd => TrackDensity::Double,
            HfeFloppyInterface::C64Dd => TrackDensity::Double,
            HfeFloppyInterface::EmuShugart => TrackDensity::Double,
            HfeFloppyInterface::S950Dd => TrackDensity::Double,
            HfeFloppyInterface::S950Hd => TrackDensity::High,
            HfeFloppyInterface::Disable => TrackDensity::Double,
            HfeFloppyInterface::Unknown => TrackDensity::Double,
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
    rpm: u16,               // Rotation per minute (Not used by the emulator)
    interface_mode: u8,     // Floppy interface mode. (Please see the list above.)
    unused: u8,             // Reserved
    track_list_offset: u16, // Offset of the track list LUT in block of 512 bytes
    // (Ex: 1=0x200)
    write_allowed: u8, // The Floppy image is write protected ?
    // v1.1 addition – Set them to 0xFF if unused.
    single_step: u8,           // 0xFF : Single Step – 0x00 Double Step mode
    track0s0_alt_encoding: u8, // 0x00 : Use an alternate track_encoding for track 0 Side 0
    track0s0_encoding: u8,     // alternate track_encoding for track 0 Side 0
    track0s1_alt_encoding: u8, // 0x00 : Use an alternate track_encoding for track 0 Side 1
    track0s1_encoding: u8,     // alternate track_encoding for track 0 Side 1
}

impl MapDump for HfeFileHeader {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let signature_str = String::from_utf8_lossy(&self.signature).to_string();
        #[rustfmt::skip]
        map.add_child(parent, "HFE File Header", SourceValue::default())
            .add_child("signature", SourceValue::string(&signature_str))
            .add_sibling("format_revision", SourceValue::u8(self.format_revision))
            .add_sibling("number_of_tracks", SourceValue::u8(self.number_of_tracks))
            .add_sibling("number_of_sides", SourceValue::u8(self.number_of_sides))
            .add_sibling("track_encoding", SourceValue::u8(self.track_encoding))
            .add_sibling("bit_rate", SourceValue::u16(self.bit_rate))
            .add_sibling("rpm", SourceValue::u16(self.rpm))
            .add_sibling("interface_mode", SourceValue::u8(self.interface_mode))
            .add_sibling("unused", SourceValue::u8(self.unused))
            .add_sibling("track_list_offset", SourceValue::u16(self.track_list_offset))
            .add_sibling("write_allowed", SourceValue::u8(self.write_allowed))
            .add_sibling("single_step", SourceValue::u8(self.single_step))
            .add_sibling("track0s0_alt_encoding", SourceValue::u8(self.track0s0_alt_encoding))
            .add_sibling("track0s0_encoding", SourceValue::u8(self.track0s0_encoding))
            .add_sibling("track0s1_alt_encoding", SourceValue::u8(self.track0s1_alt_encoding))
            .add_sibling("track0s1_encoding", SourceValue::u8(self.track0s1_encoding));

        parent
    }
}

#[derive(Debug)]
#[binrw]
#[br(import(index: usize))]
#[brw(little)]
struct HfeTrackIndexEntry {
    #[bw(ignore)]
    #[br(calc = index)]
    index:  usize,
    offset: u16,
    len:    u16,
}

impl MapDump for HfeTrackIndexEntry {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        #[rustfmt::skip]
        map.add_child(parent,&format!("[{}] Track Index Entry", self.index), SourceValue::default())
            .add_child("offset", SourceValue::u16(self.offset))
            .add_sibling("len", SourceValue::u16(self.len));

        parent
    }
}

impl HfeFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["hfe"]
    }

    pub(crate) fn platforms() -> Vec<Platform> {
        // HFE images support a wide variety of platforms
        Platform::iter().collect()
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

    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::HfeImage);
        disk_image.assign_source_map(true);

        let image_len = read_buf.seek(std::io::SeekFrom::End(0))?;

        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let file_header = HfeFileHeader::read(&mut read_buf)?;
        if file_header.signature != "HXCPICFE".as_bytes() {
            log::error!("Invalid HFE signature");
            return Err(DiskImageError::UnknownFormat);
        }
        file_header.write_to_map(disk_image.source_map_mut(), 0);

        let hfe_floppy_interface = HfeFloppyInterface::from(file_header.interface_mode);
        let hfe_track_encoding = HfeFloppyEncoding::from(file_header.track_encoding);
        log::trace!(
            "Got HXE header. Cylinders: {} Heads: {} Encoding: {:?}",
            file_header.number_of_tracks,
            file_header.number_of_sides,
            hfe_track_encoding
        );
        let track_list_offset = file_header.track_list_offset as u64 * HFE_TRACK_OFFSET_BLOCK;
        read_buf.seek(std::io::SeekFrom::Start(track_list_offset))?;

        let mut track_index_vec = Vec::new();
        for ti in 0..file_header.number_of_tracks {
            let track_index_entry = HfeTrackIndexEntry::read_args(&mut read_buf, (ti as usize,))?;
            track_index_entry.write_to_map(disk_image.source_map_mut(), 0);
            if track_index_entry.len & 1 != 0 {
                log::error!("Track {} length cannot be odd, due to head interleave.", ti);
                return Err(DiskImageError::FormatParseError);
            }
            track_index_vec.push(track_index_entry);
        }

        for (ti, track) in track_index_vec.iter().enumerate() {
            let mut track_data: [Vec<u8>; 2] = [Vec::with_capacity(50 * 512), Vec::with_capacity(50 * 512)];
            let track_data_offset = track.offset as u64 * HFE_TRACK_OFFSET_BLOCK;
            read_buf.seek(std::io::SeekFrom::Start(track_data_offset))?;

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
            }
            else {
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
                }
                else {
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
                    read_buf.read_exact(&mut track_block_data)?;

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

            let params = BitStreamTrackParams {
                schema: None,
                encoding: TrackDataEncoding::Mfm,
                data_rate: TrackDataRate::from(file_header.bit_rate as u32 * 100),
                rpm: None,
                ch: DiskCh::from((ti as u16, 0)),
                bitcell_ct: None,
                data: &track_data[0],
                weak: None,
                hole: None,
                detect_weak: false,
            };

            disk_image.add_track_bitstream(&params)?;

            // And the track data for head 1, if sides > 1
            if file_header.number_of_sides > 1 {
                log::trace!(
                    "Adding bitstream track: C:{} H:{} Bitcells: {}",
                    ti,
                    1,
                    track_data[1].len() * 8
                );

                let params = BitStreamTrackParams {
                    schema: None,
                    encoding: TrackDataEncoding::Mfm,
                    data_rate: TrackDataRate::from(file_header.bit_rate as u32 * 100),
                    rpm: None,
                    ch: DiskCh::from((ti as u16, 1)),
                    bitcell_ct: None,
                    data: &track_data[1],
                    weak: None,
                    hole: None,
                    detect_weak: false,
                };

                disk_image.add_track_bitstream(&params)?;
            }
        }

        disk_image.descriptor = DiskDescriptor {
            // Can't trust HFE platform, so return empty list.
            platforms: None,
            geometry: DiskCh::from((file_header.number_of_tracks as u16, file_header.number_of_sides)),
            data_rate: TrackDataRate::from(file_header.bit_rate as u32 * 1000),
            density: TrackDensity::from(hfe_floppy_interface),
            data_encoding: TrackDataEncoding::Mfm,
            rpm: None,
            write_protect: Some(file_header.write_allowed == 0),
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        _image: &DiskImage,
        _opts: &ParserWriteOptions,
        _output: &mut RWS,
    ) -> Result<(), DiskImageError> {
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
        for (ti, table_item) in table.into_iter().enumerate() {
            assert_eq!(table_item, simple_reverse_bits(ti as u8), "Failed at index {}", ti);
        }
        println!("test_generate_reverse_table(): passed");
    }
}
