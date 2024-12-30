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

    src/parsers/pri.rs

    A parser for the PRI disk image format.

    PRI format images are PCE bitstream images, an internal format used by the PCE emulator and
    devised by Hampa Hug.

    It is a chunk-based format similar to RIFF.

*/

use crate::{
    file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility},
    io::{Cursor, ReadSeek, ReadWriteSeek, Write},
    types::{BitStreamTrackParams, DiskDescriptor},
};

use crate::{
    file_parsers::{pce::crc::pce_crc, ParserReadOptions, ParserWriteOptions},
    track::bitstream::BitStreamTrack,
    types::{chs::DiskCh, Platform, TrackDataEncoding, TrackDataRate, TrackDataResolution, TrackDensity},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashSet,
    LoadingCallback,
};
use binrw::{binrw, meta::WriteEndian, BinRead, BinWrite};

pub struct PriFormat;
pub const MAXIMUM_CHUNK_SIZE: usize = 0x100000; // Reasonable 1MB limit for chunk sizes.

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PriChunkHeader {
    pub id:   [u8; 4],
    pub size: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PriChunkFooter {
    pub id: [u8; 4],
    pub size: u32,
    pub footer: u32,
}

/// We use the Default implementation to set the special CRC value for the footer.
impl Default for PriChunkFooter {
    fn default() -> Self {
        PriChunkFooter {
            id: *b"END ",
            size: 0,
            footer: 0x3d64af78,
        }
    }
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PriHeader {
    pub version:  u16,
    pub reserved: u16,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PriChunkCrc {
    pub crc: u32,
}

#[derive(Default, Debug)]
#[binrw]
#[brw(big)]
pub struct PriTrackHeader {
    pub cylinder: u32,
    pub head: u32,
    pub bit_length: u32,
    pub clock_rate: u32,
}

#[binrw]
#[brw(big)]
pub struct PriWeakMaskEntry {
    pub bit_offset: u32,
    pub bit_mask:   u32,
}

#[binrw]
#[brw(big)]
pub struct PriAlternateClock {
    pub bit_offset: u32,
    pub new_clock:  u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PriChunkType {
    FileHeader,
    Text,
    TrackHeader,
    TrackData,
    WeakMask,
    AlternateBitClock,
    End,
    Unknown,
}

pub struct PriChunk {
    pub chunk_type: PriChunkType,
    pub size: u32,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct TrackContext {
    phys_ch:   DiskCh,
    bit_clock: u32,
}

impl PriFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub fn platforms() -> Vec<Platform> {
        // PRI images should in theory support any platform that can be represented as bitstream
        // tracks. PCE itself only supports PC and Macintosh platforms, however, so for now we'll
        // limit it to those.
        vec![Platform::IbmPc, Platform::Macintosh]
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["pri"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = PriChunkHeader::read_be(&mut image) {
            if file_header.id == "PRI ".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    /// Return the compatibility of the image with the parser.
    pub(crate) fn can_write(image: Option<&DiskImage>) -> ParserWriteCompatibility {
        image
            .map(|image| {
                if (image.resolution.len() > 1) || !image.resolution.contains(&TrackDataResolution::BitStream) {
                    // PRI images can't store multiple resolutions, and must store bitstream data
                    return ParserWriteCompatibility::Incompatible;
                }

                if PriFormat::capabilities().contains(image.required_caps()) {
                    ParserWriteCompatibility::Ok
                }
                else {
                    ParserWriteCompatibility::DataLoss
                }
            })
            .unwrap_or(ParserWriteCompatibility::Ok)
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PriChunk, DiskImageError> {
        let chunk_pos = image.stream_position()?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PriChunkHeader::read(&mut image)?;

        if let Ok(id) = std::str::from_utf8(&chunk_header.id) {
            log::trace!("Chunk ID: {} Size: {}", id, chunk_header.size);
        }
        else {
            log::trace!("Chunk ID: {:?} Size: {}", chunk_header.id, chunk_header.size);
        }

        let chunk_type = match &chunk_header.id {
            b"PRI " => PriChunkType::FileHeader,
            b"TEXT" => PriChunkType::Text,
            b"END " => PriChunkType::End,
            b"TRAK" => PriChunkType::TrackHeader,
            b"DATA" => PriChunkType::TrackData,
            b"WEAK" => PriChunkType::WeakMask,
            b"BCLK" => PriChunkType::AlternateBitClock,
            _ => {
                log::trace!("Unknown chunk type.");
                PriChunkType::Unknown
            }
        };

        if chunk_header.size > MAXIMUM_CHUNK_SIZE as u32 {
            return Err(DiskImageError::FormatParseError);
        }

        let mut buffer = vec![0u8; chunk_header.size as usize + 8];

        //log::trace!("Seeking to chunk start...");
        image.seek(std::io::SeekFrom::Start(chunk_pos))?;
        image.read_exact(&mut buffer)?;

        let crc_calc = pce_crc(&buffer);
        let chunk_crc = PriChunkCrc::read(&mut image)?;

        if chunk_crc.crc != crc_calc {
            return Err(DiskImageError::CrcError);
        }

        //log::trace!("CRC matched: {:04X} {:04X}", chunk_crc.crc, crc_calc);

        let chunk = PriChunk {
            chunk_type,
            size: chunk_header.size,
            data: buffer[8..].to_vec(),
        };
        Ok(chunk)
    }

    pub(crate) fn write_chunk<RWS: ReadWriteSeek, T: BinWrite + WriteEndian>(
        image: &mut RWS,
        chunk_type: PriChunkType,
        data: &T,
    ) -> Result<(), DiskImageError>
    where
        for<'a> <T as BinWrite>::Args<'a>: Default,
    {
        // Create a chunk buffer Cursor to write our chunk data into.
        let mut chunk_buf = Cursor::new(Vec::new());

        let chunk_str = match chunk_type {
            PriChunkType::FileHeader => b"PRI ",
            PriChunkType::Text => b"TEXT",
            PriChunkType::End => b"END ",
            PriChunkType::TrackHeader => b"TRAK",
            PriChunkType::TrackData => b"DATA",
            PriChunkType::WeakMask => b"WEAK",
            PriChunkType::AlternateBitClock => b"BCLK",
            PriChunkType::Unknown => b"UNKN",
        };

        // Serialize the data to a buffer, so we can set the length in the chunk header.
        let mut data_buf = Cursor::new(Vec::new());
        data.write(&mut data_buf)?;

        let chunk_header = PriChunkHeader {
            id:   *chunk_str,
            size: data_buf.get_ref().len() as u32,
        };

        log::trace!("Writing chunk: {:?} size: {}", chunk_type, data_buf.get_ref().len());
        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(data_buf.get_ref())?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pce_crc(chunk_buf.get_ref());

        // Write the CRC to the chunk.
        let chunk_crc = PriChunkCrc { crc: crc_calc };
        chunk_crc.write(&mut chunk_buf)?;

        // Write the chunk buffer to the image.
        image.write_all(chunk_buf.get_ref())?;

        Ok(())
    }

    /// We use a separate function to write text chunks, as str does not implement BinWrite.
    pub(crate) fn write_text<RWS: ReadWriteSeek>(image: &mut RWS, text: &str) -> Result<(), DiskImageError> {
        // Create a chunk buffer Cursor to write our chunk data into.
        let mut chunk_buf = Cursor::new(Vec::new());

        if text.len() > 1000 {
            panic!("Text chunk too large.");
        }

        let chunk_str = b"TEXT";
        let chunk_header = PriChunkHeader {
            id:   *chunk_str,
            size: text.len() as u32,
        };

        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(text.as_bytes())?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pce_crc(chunk_buf.get_ref());

        // Write the CRC to the chunk.
        let chunk_crc = PriChunkCrc { crc: crc_calc };
        chunk_crc.write(&mut chunk_buf)?;

        // Write the chunk buffer to the image.
        image.write_all(chunk_buf.get_ref())?;

        Ok(())
    }

    /// We use a separate function to write raw data chunks, as Vec or &[u8] does not implement BinWrite.
    pub(crate) fn write_chunk_raw<RWS: ReadWriteSeek>(
        image: &mut RWS,
        chunk_type: PriChunkType,
        data: &[u8],
    ) -> Result<(), DiskImageError> {
        // Create a chunk buffer Cursor to write our chunk data into.
        let mut chunk_buf = Cursor::new(Vec::new());

        let chunk_str = match chunk_type {
            PriChunkType::FileHeader => b"PRI ",
            PriChunkType::Text => b"TEXT",
            PriChunkType::End => b"END ",
            PriChunkType::TrackHeader => b"TRAK",
            PriChunkType::TrackData => b"DATA",
            PriChunkType::WeakMask => b"WEAK",
            PriChunkType::AlternateBitClock => b"BCLK",
            PriChunkType::Unknown => b"UNKN",
        };

        let chunk_header = PriChunkHeader {
            id:   *chunk_str,
            size: data.len() as u32,
        };

        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(data)?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pce_crc(chunk_buf.get_ref());

        // Write the CRC to the chunk.
        let chunk_crc = PriChunkCrc { crc: crc_calc };
        chunk_crc.write(&mut chunk_buf)?;

        // Write the chunk buffer to the image.
        image.write_all(chunk_buf.get_ref())?;

        Ok(())
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::PceBitstreamImage);

        // Seek to start of read_buf.
        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PriFormat::read_chunk(&mut read_buf)?;
        // File header must be first chunk.
        if chunk.chunk_type != PriChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PriHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PRI file header. Format version: {}", file_header.version);

        let mut comment_string = String::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut cylinders_seen: FoxHashSet<u16> = FoxHashSet::new();
        let mut default_bit_clock = 0;
        let mut expected_data_size = 0;
        let mut track_header = PriTrackHeader::default();

        let mut ctx = TrackContext::default();
        let mut disk_data_rate = None;

        while chunk.chunk_type != PriChunkType::End {
            match chunk.chunk_type {
                PriChunkType::TrackHeader => {
                    track_header = PriTrackHeader::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;

                    let ch = DiskCh::from((track_header.cylinder as u16, track_header.head as u8));
                    log::trace!(
                        "Track header: {:?} Bitcells: {} Clock Rate: {}",
                        ch,
                        track_header.bit_length,
                        track_header.clock_rate
                    );

                    expected_data_size =
                        track_header.bit_length as usize / 8 + if track_header.bit_length % 8 != 0 { 1 } else { 0 };

                    default_bit_clock = track_header.clock_rate;
                    cylinders_seen.insert(track_header.cylinder as u16);
                    heads_seen.insert(track_header.head as u8);
                    ctx.phys_ch = ch;
                }
                PriChunkType::AlternateBitClock => {
                    let alt_clock = PriAlternateClock::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;

                    if alt_clock.new_clock == 0 {
                        ctx.bit_clock = default_bit_clock;
                    }
                    else {
                        let new_bit_clock =
                            ((alt_clock.new_clock as f64 / u16::MAX as f64) * default_bit_clock as f64) as u32;

                        ctx.bit_clock = new_bit_clock;
                    }
                    log::trace!(
                        "Alternate bit clock. Bit offset: {} New clock: {}",
                        alt_clock.bit_offset,
                        ctx.bit_clock
                    );
                }
                PriChunkType::TrackData => {
                    log::trace!(
                        "Track data chunk: {} size: {} expected size: {}",
                        ctx.phys_ch,
                        chunk.size,
                        expected_data_size
                    );

                    // Set the global disk data rate once.
                    if disk_data_rate.is_none() {
                        disk_data_rate = Some(TrackDataRate::from(ctx.bit_clock));
                    }

                    let params = BitStreamTrackParams {
                        schema: None,
                        encoding: TrackDataEncoding::Mfm,
                        data_rate: TrackDataRate::from(ctx.bit_clock),
                        rpm: None,
                        ch: ctx.phys_ch,
                        bitcell_ct: Some(track_header.bit_length as usize),
                        data: &chunk.data,
                        weak: None,
                        hole: None,
                        detect_weak: false,
                    };

                    disk_image.add_track_bitstream(&params)?;
                }
                PriChunkType::WeakMask => {
                    let weak_table_len = chunk.size / 8;
                    if chunk.size % 8 != 0 {
                        log::error!("Weak mask chunk size is not a multiple of 8.");
                        return Err(DiskImageError::FormatParseError);
                    }

                    let mut cursor = Cursor::new(&chunk.data);

                    let track = disk_image
                        .track_mut(ctx.phys_ch)
                        .ok_or(DiskImageError::FormatParseError)?;
                    let bit_track = track
                        .as_any_mut()
                        .downcast_mut::<BitStreamTrack>()
                        .ok_or(DiskImageError::FormatParseError)?;

                    for _i in 0..weak_table_len {
                        let weak_mask =
                            PriWeakMaskEntry::read(&mut cursor).map_err(|_| DiskImageError::FormatParseError)?;

                        log::trace!(
                            "Weak mask entry. Bit offset: {} Mask: {:08X}",
                            weak_mask.bit_offset,
                            weak_mask.bit_mask
                        );

                        bit_track.write_weak_mask_u32(weak_mask.bit_mask, weak_mask.bit_offset as usize);
                    }
                }
                PriChunkType::Text => {
                    // PSI docs:
                    // `If there are multiple TEXT chunks, their contents should be concatenated`
                    if let Ok(text) = std::str::from_utf8(&chunk.data) {
                        comment_string.push_str(text);
                    }
                }
                PriChunkType::End => {
                    log::trace!("End chunk.");
                    break;
                }
                _ => {
                    log::trace!("Chunk type: {:?}", chunk.chunk_type);
                }
            }

            chunk = PriFormat::read_chunk(&mut read_buf)?;
        }

        log::trace!("Comment: {}", comment_string);

        let head_ct = heads_seen.len() as u8;
        let cylinder_ct = cylinders_seen.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            platforms: None,
            geometry: DiskCh::from((cylinder_ct, head_ct)),
            data_rate: disk_data_rate.unwrap(),
            data_encoding: TrackDataEncoding::Mfm,
            density: TrackDensity::from(disk_data_rate.unwrap()),
            rpm: None,
            write_protect: None,
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        image: &DiskImage,
        _opts: &ParserWriteOptions,
        output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        if (image.resolution.len() > 1) || !image.resolution.contains(&TrackDataResolution::BitStream) {
            log::error!("Unsupported image resolution.");
            return Err(DiskImageError::UnsupportedFormat);
        }
        log::trace!("Saving PRI image...");

        // Write the file header chunk. Version remains at 0 for now.
        let file_header = PriHeader {
            version:  0,
            reserved: 0,
        };
        PriFormat::write_chunk(output, PriChunkType::FileHeader, &file_header)?;

        // Write any comments present in the image to a TEXT chunk.
        image.comment().map(|comment| PriFormat::write_text(output, comment));

        // Iterate through tracks and write track headers and data.
        for track in image.track_iter() {
            if let Some(track) = track.as_any().downcast_ref::<BitStreamTrack>() {
                log::trace!(
                    "Track {}: encoding: {:?} data_rate: {:?} bit length: {}",
                    track.ch,
                    track.encoding,
                    track.data_rate,
                    track.data.len(),
                );

                // Write the track header.
                let track_header = PriTrackHeader {
                    cylinder: track.ch.c() as u32,
                    head: track.ch.h() as u32,
                    bit_length: track.data.len() as u32,
                    clock_rate: track.data_rate.into(),
                };
                PriFormat::write_chunk(output, PriChunkType::TrackHeader, &track_header)?;

                // Write the track data.
                let track_data = track.data.data_copied();
                PriFormat::write_chunk(output, PriChunkType::TrackData, &track_data)?;

                if track.data.weak_mask().any() {
                    // At least one bit is set in the weak bit mask, so let's export it.
                    let weak_mask = track.data.weak_mask();

                    // Create a buffer for our weak mask table.
                    let mut weak_buffer = Cursor::new(Vec::new());

                    let mut mask_offset;
                    let mut bit_offset = 0;
                    let mut iter = weak_mask.iter();
                    while let Some(bit) = iter.next() {
                        bit_offset += 1;
                        if bit {
                            mask_offset = bit_offset;
                            // Start with a 1 in the MSB position of the shift register
                            let mut mask_u32: u32 = 1 << 31;

                            // Shift in the next 31 bits, if available
                            for pos in 1..32 {
                                if let Some(next_bit) = iter.next() {
                                    bit_offset += 1;
                                    mask_u32 |= (next_bit as u32) << (31 - pos);
                                }
                                else {
                                    break;
                                }
                            }

                            // Add an entry to the table.
                            PriWeakMaskEntry {
                                bit_offset: mask_offset,
                                bit_mask:   mask_u32,
                            }
                            .write_be(&mut weak_buffer)?;
                        }
                    }

                    PriFormat::write_chunk_raw(output, PriChunkType::WeakMask, weak_buffer.get_ref())?;
                }
            }
            else {
                unreachable!("Expected only BitStream variants");
            }
        }

        // Write the file-end chunk.
        log::trace!("Writing END chunk...");
        let end_chunk = PriChunkFooter::default();
        end_chunk.write(output)?;

        Ok(())
    }
}
