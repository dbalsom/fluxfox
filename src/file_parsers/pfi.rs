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

    src/parsers/pfi.rs

    A parser for the PFI disk image format.

    PFI format images are PCE flux stream images, an internal format used by the PCE emulator and
    devised by Hampa Hug.

    It is a chunk-based format similar to RIFF.

*/

use crate::chs::DiskCh;
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, ReadSeek, ReadWriteSeek};

use crate::{
    DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat, FoxHashSet,
    LoadingCallback, DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

pub struct PfiFormat;
pub const MAXIMUM_CHUNK_SIZE: usize = 0x100000; // Reasonable 1MB limit for chunk sizes.

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiChunkHeader {
    pub id: [u8; 4],
    pub size: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiChunkFooter {
    pub id: [u8; 4],
    pub size: u32,
    pub footer: u32,
}

/// We use the Default implementation to set the special CRC value for the footer.
impl Default for PfiChunkFooter {
    fn default() -> Self {
        PfiChunkFooter {
            id: *b"END ",
            size: 0,
            footer: 0x3d64af78,
        }
    }
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiHeader {
    pub version: u16,
    pub reserved: u16,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiChunkCrc {
    pub crc: u32,
}

#[derive(Default, Debug)]
#[binrw]
#[brw(big)]
pub struct PfiTrackHeader {
    pub cylinder: u32,
    pub head: u32,
    pub clock_rate: u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PfiChunkType {
    FileHeader,
    Text,
    TrackHeader,
    Index,
    TrackData,
    End,
    Unknown,
}

pub struct PfiChunk {
    pub chunk_type: PfiChunkType,
    pub size: u32,
    pub data: Vec<u8>,
}

pub(crate) fn pfi_crc(buf: &[u8]) -> u32 {
    let mut crc = 0;
    for i in 0..buf.len() {
        crc ^= ((buf[i] & 0xff) as u32) << 24;

        for _j in 0..8 {
            if crc & 0x80000000 != 0 {
                crc = (crc << 1) ^ 0x1edc6f41;
            }
            else {
                crc <<= 1;
            }
        }
    }
    crc & 0xffffffff
}

impl PfiFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        //vec!["pfi"]
        Vec::new()
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = PfiChunkHeader::read_be(&mut image) {
            if file_header.id == "PFI ".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    /// Return the compatibility of the image with the parser.
    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PfiChunk, DiskImageError> {
        let chunk_pos = image.stream_position()?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PfiChunkHeader::read(&mut image)?;

        if let Ok(id) = std::str::from_utf8(&chunk_header.id) {
            log::trace!("Chunk ID: {} Size: {}", id, chunk_header.size);
        }
        else {
            log::trace!("Chunk ID: {:?} Size: {}", chunk_header.id, chunk_header.size);
        }

        let chunk_type = match &chunk_header.id {
            b"Pfi " => PfiChunkType::FileHeader,
            b"TEXT" => PfiChunkType::Text,
            b"END " => PfiChunkType::End,
            b"TRAK" => PfiChunkType::TrackHeader,
            b"INDX" => PfiChunkType::Index,
            b"DATA" => PfiChunkType::TrackData,
            _ => {
                log::trace!("Unknown chunk type.");
                PfiChunkType::Unknown
            }
        };

        if chunk_header.size > MAXIMUM_CHUNK_SIZE as u32 {
            return Err(DiskImageError::FormatParseError);
        }

        let mut buffer = vec![0u8; chunk_header.size as usize + 8];

        //log::trace!("Seeking to chunk start...");
        image.seek(std::io::SeekFrom::Start(chunk_pos))?;
        image.read_exact(&mut buffer)?;

        let crc_calc = pfi_crc(&buffer);
        let chunk_crc = PfiChunkCrc::read(&mut image)?;

        if chunk_crc.crc != crc_calc {
            return Err(DiskImageError::CrcError);
        }

        //log::trace!("CRC matched: {:04X} {:04X}", chunk_crc.crc, crc_calc);

        let chunk = PfiChunk {
            chunk_type,
            size: chunk_header.size,
            data: buffer[8..].to_vec(),
        };
        Ok(chunk)
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFormat::PceBitstreamImage);

        // Seek to start of read_buf.
        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PfiFormat::read_chunk(&mut read_buf)?;
        // File header must be first chunk.
        if chunk.chunk_type != PfiChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PfiHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PRI file header. Format version: {}", file_header.version);

        let mut comment_string = String::new();
        let mut current_ch = DiskCh::default();
        let current_crc_error = false;

        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut cylinders_seen: FoxHashSet<u16> = FoxHashSet::new();

        let mut disk_clock_rate = None;
        let mut current_track_clock = 0;
        let mut track_header;

        let mut index_list: Vec<u32> = Vec::new();

        while chunk.chunk_type != PfiChunkType::End {
            match chunk.chunk_type {
                PfiChunkType::TrackHeader => {
                    track_header = PfiTrackHeader::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;

                    let ch = DiskCh::from((track_header.cylinder as u16, track_header.head as u8));
                    log::trace!("Track header: {:?} Clock Rate: {}", ch, track_header.clock_rate);

                    current_track_clock = track_header.clock_rate;
                    cylinders_seen.insert(track_header.cylinder as u16);
                    heads_seen.insert(track_header.head as u8);
                    current_ch = ch;
                }
                PfiChunkType::Index => {
                    let index_entries = chunk.size / 4;
                    log::trace!("Index chunk with {} entries", index_entries);

                    for i in 0..index_entries {
                        let index = u32::from_be_bytes([
                            chunk.data[i as usize * 4],
                            chunk.data[i as usize * 4 + 1],
                            chunk.data[i as usize * 4 + 2],
                            chunk.data[i as usize * 4 + 3],
                        ]);
                        index_list.push(index);
                    }
                }
                PfiChunkType::TrackData => {
                    log::trace!(
                        "Track data chunk: {} size: {}  crc_error: {}",
                        current_ch,
                        chunk.size,
                        current_crc_error
                    );

                    // Set the global disk data rate once.
                    if disk_clock_rate.is_none() {
                        disk_clock_rate = Some(DiskDataRate::from(current_track_clock));
                    }

                    // disk_image.add_track_bitstream(
                    //     DiskDataEncoding::Mfm,
                    //     DiskDataRate::from(current_bit_clock),
                    //     current_ch,
                    //     current_bit_clock,
                    //     Some(track_header.bit_length as usize),
                    //     &chunk.data,
                    //     None,
                    // )?;
                }
                PfiChunkType::Text => {
                    // PSI docs:
                    // `If there are multiple TEXT chunks, their contents should be concatenated`
                    if let Ok(text) = std::str::from_utf8(&chunk.data) {
                        comment_string.push_str(text);
                    }
                }
                PfiChunkType::End => {
                    log::trace!("End chunk.");
                    break;
                }
                _ => {
                    log::trace!("Chunk type: {:?}", chunk.chunk_type);
                }
            }

            chunk = PfiFormat::read_chunk(&mut read_buf)?;
        }

        log::trace!("Comment: {}", comment_string);

        let head_ct = heads_seen.len() as u8;
        let cylinder_ct = cylinders_seen.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((cylinder_ct, head_ct)),
            data_rate: disk_clock_rate.unwrap(),
            data_encoding: DiskDataEncoding::Mfm,
            density: DiskDensity::from(disk_clock_rate.unwrap()),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: None,
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
