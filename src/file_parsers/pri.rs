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

use crate::chs::{DiskCh, DiskChs};
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, ReadSeek, ReadWriteSeek};

use crate::{
    DiskDataEncoding, DiskDataRate, DiskImage, DiskImageError, DiskImageFormat, FoxHashSet, DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

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

impl PriFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["pri"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = PriChunkHeader::read_be(&mut image) {
            if &file_header.id == "PRI ".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        // TODO: Determine what data representations would lead to data loss for PSI.
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PriChunk, DiskImageError> {
        let chunk_pos = image.stream_position().map_err(|_| DiskImageError::IoError)?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PriChunkHeader::read(&mut image).map_err(|_| DiskImageError::IoError)?;

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
        image
            .seek(std::io::SeekFrom::Start(chunk_pos))
            .map_err(|_| DiskImageError::IoError)?;
        image.read_exact(&mut buffer).map_err(|_| DiskImageError::IoError)?;

        let crc_calc = pri_crc(&buffer);
        let chunk_crc = PriChunkCrc::read(&mut image).map_err(|_| DiskImageError::IoError)?;

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

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        // Seek to start of image.
        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| DiskImageError::IoError)?;

        let mut chunk = PriFormat::read_chunk(&mut image)?;
        // File header must be first chunk.
        if chunk.chunk_type != PriChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PriHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PRI file header. Format version: {}", file_header.version);

        let mut comment_string = String::new();
        let current_chs = DiskChs::default();
        let current_crc_error = false;

        let track_set: FoxHashSet<DiskCh> = FoxHashSet::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();

        let mut default_bit_clock = 0;
        let mut current_bit_clock = 0;
        let mut expected_data_size = 0;

        while chunk.chunk_type != PriChunkType::End {
            match chunk.chunk_type {
                PriChunkType::TrackHeader => {
                    let track_header = PriTrackHeader::read(&mut Cursor::new(&chunk.data))
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
                    heads_seen.insert(track_header.head as u8);
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
                        current_chs,
                        chunk.size,
                        expected_data_size,
                        current_crc_error
                    );

                    disk_image.add_track_bitstream(
                        DiskDataEncoding::Mfm,
                        DiskDataRate::from(current_bit_clock),
                        current_chs.into(),
                        current_bit_clock,
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

        let head_ct = heads_seen.len() as u16;
        let track_ct = track_set.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((track_ct / head_ct, head_ct as u8)),
            data_rate: Default::default(),
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: None,
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
