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

use crate::chs::DiskCh;
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, ReadSeek, ReadWriteSeek, Write};

use crate::trackdata::TrackData;
use crate::{
    DiskDataEncoding, DiskDataRate, DiskDataResolution, DiskDensity, DiskImage, DiskImageError, DiskImageFormat,
    FoxHashSet, LoadingCallback, DEFAULT_SECTOR_SIZE,
};
use binrw::meta::WriteEndian;
use binrw::{binrw, BinRead, BinWrite};

pub struct PriFormat;
pub const MAXIMUM_CHUNK_SIZE: usize = 0x100000; // Reasonable 1MB limit for chunk sizes.

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PriChunkHeader {
    pub id: [u8; 4],
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
    pub version: u16,
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
pub struct PriWeakMask {
    pub bit_offset: u32,
}

#[binrw]
#[brw(big)]
pub struct PriAlternateClock {
    pub bit_offset: u32,
    pub new_clock: u32,
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

pub(crate) fn pri_crc(buf: &[u8]) -> u32 {
    let mut crc = 0;
    for i in 0..buf.len() {
        crc ^= ((buf[i] & 0xff) as u32) << 24;

        for _j in 0..8 {
            if crc & 0x80000000 != 0 {
                crc = (crc << 1) ^ 0x1edc6f41;
            } else {
                crc <<= 1;
            }
        }
    }
    crc & 0xffffffff
}

/// Return slice bounds for the weak bit mask.
pub(crate) fn pri_weak_bounds(buf: &[u8]) -> (usize, usize) {
    let mut start = 0;
    let mut end = 0;

    for i in 0..buf.len() {
        if buf[i] != 0 {
            start = i;
            break;
        }
    }

    for i in (0..buf.len()).rev() {
        if buf[i] != 0 {
            end = i;
            break;
        }
    }

    (start, end)
}

impl PriFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
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
    pub(crate) fn can_write(image: &DiskImage) -> ParserWriteCompatibility {
        if let Some(resolution) = image.resolution {
            if !matches!(resolution, DiskDataResolution::BitStream) {
                return ParserWriteCompatibility::Incompatible;
            }
        } else {
            return ParserWriteCompatibility::Incompatible;
        }

        if PriFormat::capabilities().contains(image.required_caps()) {
            ParserWriteCompatibility::Ok
        } else {
            ParserWriteCompatibility::DataLoss
        }
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PriChunk, DiskImageError> {
        let chunk_pos = image.stream_position()?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PriChunkHeader::read(&mut image)?;

        if let Ok(id) = std::str::from_utf8(&chunk_header.id) {
            log::trace!("Chunk ID: {} Size: {}", id, chunk_header.size);
        } else {
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

        let crc_calc = pri_crc(&buffer);
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
            id: *chunk_str,
            size: data_buf.get_ref().len() as u32,
        };

        log::trace!("Writing chunk: {:?} size: {}", chunk_type, data_buf.get_ref().len());
        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(data_buf.get_ref())?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pri_crc(chunk_buf.get_ref());

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
            id: *chunk_str,
            size: text.len() as u32,
        };

        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(text.as_bytes())?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pri_crc(chunk_buf.get_ref());

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
            id: *chunk_str,
            size: data.len() as u32,
        };

        chunk_header.write(&mut chunk_buf)?;

        chunk_buf.write_all(data)?;

        // Calculate CRC for chunk, over header and data bytes.
        let crc_calc = pri_crc(chunk_buf.get_ref());

        // Write the CRC to the chunk.
        let chunk_crc = PriChunkCrc { crc: crc_calc };
        chunk_crc.write(&mut chunk_buf)?;

        // Write the chunk buffer to the image.
        image.write_all(chunk_buf.get_ref())?;

        Ok(())
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut image: RWS,
        _callback: Option<LoadingCallback>,
    ) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();
        disk_image.set_source_format(DiskImageFormat::PceBitstreamImage);

        // Seek to start of image.
        image.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PriFormat::read_chunk(&mut image)?;
        // File header must be first chunk.
        if chunk.chunk_type != PriChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PriHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PRI file header. Format version: {}", file_header.version);

        let mut comment_string = String::new();
        let mut current_ch = DiskCh::default();
        let current_crc_error = false;

        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut cylinders_seen: FoxHashSet<u16> = FoxHashSet::new();

        let mut default_bit_clock = 0;
        let mut current_bit_clock = 0;
        let mut expected_data_size = 0;
        let mut track_header = PriTrackHeader::default();

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
                    current_ch = ch;
                }
                PriChunkType::AlternateBitClock => {
                    let alt_clock = PriAlternateClock::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;

                    if alt_clock.new_clock == 0 {
                        current_bit_clock = default_bit_clock;
                    } else {
                        let new_bit_clock =
                            ((alt_clock.new_clock as f64 / u16::MAX as f64) * default_bit_clock as f64) as u32;

                        current_bit_clock = new_bit_clock;
                    }
                    log::trace!(
                        "Alternate bit clock. Bit offset: {} New clock: {}",
                        alt_clock.bit_offset,
                        current_bit_clock
                    );
                }
                PriChunkType::TrackData => {
                    log::trace!(
                        "Track data chunk: {} size: {} expected size: {} crc_error: {}",
                        current_ch,
                        chunk.size,
                        expected_data_size,
                        current_crc_error
                    );

                    // Set the global disk data rate once.
                    if disk_data_rate.is_none() {
                        disk_data_rate = Some(DiskDataRate::from(current_bit_clock));
                    }

                    disk_image.add_track_bitstream(
                        DiskDataEncoding::Mfm,
                        DiskDataRate::from(current_bit_clock),
                        current_ch,
                        current_bit_clock,
                        Some(track_header.bit_length as usize),
                        &chunk.data,
                        None,
                    )?;
                }
                PriChunkType::WeakMask => {
                    let weak_mask = PriWeakMask::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;
                    log::trace!(
                        "Weak mask chunk. Size: {} Bit offset: {}",
                        chunk.size,
                        weak_mask.bit_offset
                    );
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

            chunk = PriFormat::read_chunk(&mut image)?;
        }

        log::trace!("Comment: {}", comment_string);

        let head_ct = heads_seen.len() as u8;
        let cylinder_ct = cylinders_seen.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((cylinder_ct, head_ct)),
            data_rate: disk_data_rate.unwrap(),
            data_encoding: DiskDataEncoding::Mfm,
            density: DiskDensity::from(disk_data_rate.unwrap()),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: None,
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(image: &DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        if matches!(image.resolution(), DiskDataResolution::BitStream) {
            log::trace!("Saving PRI image...");
        } else {
            log::error!("Unsupported image resolution.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        // Write the file header chunk. Version remains at 0 for now.
        let file_header = PriHeader {
            version: 0,
            reserved: 0,
        };
        PriFormat::write_chunk(output, PriChunkType::FileHeader, &file_header)?;

        // Write any comments present in the image to a TEXT chunk.
        image
            .get_comment()
            .map(|comment| PriFormat::write_text(output, comment));

        // Iterate through tracks and write track headers and data.
        for track in image.track_iter() {
            if let TrackData::BitStream {
                encoding,
                data_rate,
                data_clock,
                cylinder,
                head,
                data,
                sector_ids,
                ..
            } = track
            {
                log::trace!(
                    "Track c:{} h:{} sectors: {} encoding: {:?} data_rate: {:?} bit length: {}",
                    cylinder,
                    head,
                    sector_ids.len(),
                    encoding,
                    data_rate,
                    data.len(),
                );

                // Write the track header.
                let track_header = PriTrackHeader {
                    cylinder: *cylinder as u32,
                    head: *head as u32,
                    bit_length: data.len() as u32,
                    clock_rate: *data_clock,
                };
                PriFormat::write_chunk(output, PriChunkType::TrackHeader, &track_header)?;

                // Write the track data.
                let track_data = data.data();
                PriFormat::write_chunk(output, PriChunkType::TrackData, &track_data)?;

                // Write the weak mask, if any bits are set in the weak bit mask.
                if data.weak_mask().any() {
                    // At least one bit is set in the weak bit mask, so let's export it.
                    let weak_data = data.weak_data();

                    // Optimization: PRI supports supplying a bit offset for the weak bit mask.
                    // Determine the slice of the weak mask that contains the first and last
                    // set bits.
                    let (slice_start, slice_end) = pri_weak_bounds(&weak_data);
                    let weak_header = PriWeakMask {
                        bit_offset: (slice_start * 8) as u32,
                    };

                    // Create a buffer for our weak mask.
                    let mut weak_buffer = Cursor::new(Vec::new());

                    // Write the weak mask header.
                    weak_header.write(&mut weak_buffer)?;

                    // Write the weak mask data.
                    weak_buffer.write_all(&weak_data[slice_start..slice_end])?;

                    PriFormat::write_chunk_raw(output, PriChunkType::WeakMask, weak_buffer.get_ref())?;
                }
            } else {
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
