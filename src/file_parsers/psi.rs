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

    src/parsers/psi.rs

    A parser for the PSI disk image format.

    PSI format images are PCE Sector Images, an internal format used by the PCE emulator and
    devised by Hampa Hug.

    It is a chunk-based format similar to RIFF.

*/

use crate::chs::{DiskCh, DiskChs, DiskChsn};
use crate::diskimage::{DiskDescriptor, SectorDescriptor};
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{Cursor, ReadSeek, ReadWriteSeek};

use crate::{
    DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat, FoxHashMap, FoxHashSet,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

pub struct PsiFormat;
pub const MAXIMUM_CHUNK_SIZE: usize = 0x100000; // Reasonable 1MB limit for chunk sizes.

pub const SH_FLAG_COMPRESSED: u8 = 0b0001;
pub const SH_FLAG_ALTERNATE: u8 = 0b0010;
pub const SH_FLAG_CRC_ERROR: u8 = 0b0100;
pub const SH_IBM_FLAG_CRC_ERROR_ID: u8 = 0b0001;
pub const SH_IBM_FLAG_CRC_ERROR_DATA: u8 = 0b0010;
pub const SH_IBM_DELETED_DATA: u8 = 0b0100;
pub const SH_IBM_MISSING_DATA: u8 = 0b1000;

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PsiChunkHeader {
    pub id: [u8; 4],
    pub size: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PsiHeader {
    pub version: u16,
    pub sector_format: [u8; 2],
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PsiChunkCrc {
    pub crc: u32,
}

#[binrw]
#[brw(big)]
pub struct PsiSectorHeader {
    pub cylinder: u16,
    pub head: u8,
    pub sector: u8,
    pub size: u16,
    pub flags: u8,
    pub compressed_data: u8,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PsiChunkType {
    FileHeader,
    Text,
    SectorHeader,
    SectorData,
    WeakMask,
    IbmFmSectorHeader,
    IbmMfmSectorHeader,
    MacintoshSectorHeader,
    SectorPositionOffset,
    ClockRateAdjustment,
    End,
    Unknown,
}

pub struct PsiChunk {
    pub chunk_type: PsiChunkType,
    pub data: Vec<u8>,
}

pub(crate) fn psi_crc(buf: &[u8]) -> u32 {
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

pub(crate) fn decode_psi_sector_format(sector_format: [u8; 2]) -> Option<(DiskDataEncoding, DiskDensity)> {
    match sector_format {
        [0x00, 0x00] => Some((DiskDataEncoding::Fm, DiskDensity::Standard)),
        [0x01, 0x00] => Some((DiskDataEncoding::Fm, DiskDensity::Double)),
        [0x02, 0x00] => Some((DiskDataEncoding::Fm, DiskDensity::High)),
        [0x02, 0x01] => Some((DiskDataEncoding::Fm, DiskDensity::High)),
        [0x02, 0x02] => Some((DiskDataEncoding::Mfm, DiskDensity::Extended)),
        // TODO: What density are GCR disks? Are they all the same? PSI doesn't specify any variants.
        [0x03, 0x00] => Some((DiskDataEncoding::Gcr, DiskDensity::Double)),
        _ => None,
    }
}

impl PsiFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceSectorImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["psi"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = PsiChunkHeader::read_be(&mut image) {
            if file_header.id == "PSI ".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PsiChunk, DiskImageError> {
        let chunk_pos = image.stream_position()?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PsiChunkHeader::read(&mut image)?;

        if let Ok(id) = std::str::from_utf8(&chunk_header.id) {
            log::trace!("Chunk ID: {} Size: {}", id, chunk_header.size);
        } else {
            log::trace!("Chunk ID: {:?} Size: {}", chunk_header.id, chunk_header.size);
        }

        let chunk_type = match &chunk_header.id {
            b"PSI " => PsiChunkType::FileHeader,
            b"TEXT" => PsiChunkType::Text,
            b"END " => PsiChunkType::End,
            b"SECT" => PsiChunkType::SectorHeader,
            b"DATA" => PsiChunkType::SectorData,
            b"WEAK" => PsiChunkType::WeakMask,
            b"IBMF" => PsiChunkType::IbmFmSectorHeader,
            b"IMFM" => PsiChunkType::IbmMfmSectorHeader,
            b"MACG" => PsiChunkType::MacintoshSectorHeader,
            b"OFFS" => PsiChunkType::SectorPositionOffset,
            b"TIME" => PsiChunkType::ClockRateAdjustment,
            _ => {
                log::trace!("Unknown chunk type.");
                PsiChunkType::Unknown
            }
        };

        if chunk_header.size > MAXIMUM_CHUNK_SIZE as u32 {
            return Err(DiskImageError::FormatParseError);
        }

        let mut buffer = vec![0u8; chunk_header.size as usize + 8];

        //log::trace!("Seeking to chunk start...");
        image.seek(std::io::SeekFrom::Start(chunk_pos))?;
        image.read_exact(&mut buffer)?;

        let crc_calc = psi_crc(&buffer);
        let chunk_crc = PsiChunkCrc::read(&mut image)?;

        if chunk_crc.crc != crc_calc {
            return Err(DiskImageError::CrcError);
        }

        //log::trace!("CRC matched: {:04X} {:04X}", chunk_crc.crc, crc_calc);

        let chunk = PsiChunk {
            chunk_type,
            data: buffer[8..].to_vec(),
        };
        Ok(chunk)
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();
        disk_image.set_source_format(DiskImageFormat::PceSectorImage);

        // Seek to start of image.
        image.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PsiFormat::read_chunk(&mut image)?;
        // File header must be first chunk.
        if chunk.chunk_type != PsiChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PsiHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PSI file header. Format version: {}", file_header.version);

        let (default_encoding, disk_density) =
            decode_psi_sector_format(file_header.sector_format).ok_or(DiskImageError::FormatParseError)?;
        let mut comment_string = String::new();
        let mut current_chs = DiskChs::default();
        let mut current_crc_error = false;

        let mut track_set: FoxHashSet<DiskCh> = FoxHashSet::new();
        let mut sector_counts: FoxHashMap<u8, u32> = FoxHashMap::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut sectors_per_track = 0;

        while chunk.chunk_type != PsiChunkType::End {
            match chunk.chunk_type {
                PsiChunkType::FileHeader => {}
                PsiChunkType::SectorHeader => {
                    //log::trace!("Sector header chunk.");
                    let sector_header = PsiSectorHeader::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;
                    let chs = DiskChs::from((sector_header.cylinder, sector_header.head, sector_header.sector));
                    let ch = DiskCh::from((sector_header.cylinder, sector_header.head));

                    heads_seen.insert(sector_header.head);

                    if !track_set.contains(&ch) {
                        log::trace!("Adding track...");
                        disk_image.add_track_bytestream(default_encoding, DiskDataRate::from(disk_density), ch)?;
                        track_set.insert(ch);
                        log::trace!("Observing sector count: {}", sectors_per_track);
                        sector_counts
                            .entry(sectors_per_track)
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
                        sectors_per_track = 0;
                    }

                    if sector_header.flags & SH_FLAG_ALTERNATE != 0 {
                        log::trace!("Alternate sector data.");
                    }

                    current_crc_error = sector_header.flags & SH_FLAG_CRC_ERROR != 0;

                    // Write sector data immediately if compressed data is indicated (no sector data chunk follows)
                    if sector_header.flags & SH_FLAG_COMPRESSED != 0 {
                        log::trace!("Compressed sector data: {:02X}", sector_header.compressed_data);

                        let chunk_expand = vec![sector_header.compressed_data; sector_header.size as usize];

                        // Add this sector to track.
                        let sd = SectorDescriptor {
                            id: chs.s(),
                            cylinder_id: None,
                            head_id: None,
                            n: DiskChsn::bytes_to_n(chunk_expand.len()),
                            data: chunk_expand,
                            weak: None,
                            address_crc_error: false,
                            data_crc_error: current_crc_error,
                            deleted_mark: false,
                        };

                        disk_image.master_sector(chs, &sd)?;
                    }

                    current_chs = chs;
                    log::trace!(
                        "Sector CHS: {} size: {} crc_error: {}",
                        chs,
                        sector_header.size,
                        current_crc_error
                    );
                }
                PsiChunkType::SectorData => {
                    log::trace!("Sector data chunk: {} crc_error: {}", current_chs, current_crc_error);

                    // Add this sector to track.
                    let sd = SectorDescriptor {
                        id: current_chs.s(),
                        cylinder_id: None,
                        head_id: None,
                        n: DiskChsn::bytes_to_n(chunk.data.len()),
                        data: chunk.data,
                        weak: None,
                        address_crc_error: false,
                        data_crc_error: current_crc_error,
                        deleted_mark: false,
                    };

                    disk_image.master_sector(current_chs, &sd)?;

                    sectors_per_track += 1;
                }
                PsiChunkType::Text => {
                    // PSI docs:
                    // `If there are multiple TEXT chunks, their contents should be concatenated`
                    if let Ok(text) = std::str::from_utf8(&chunk.data) {
                        comment_string.push_str(text);
                    }
                }
                PsiChunkType::End => {
                    log::trace!("End chunk.");
                    break;
                }
                _ => {
                    log::warn!("Unhandled chunk type: {:?}", chunk.chunk_type);
                }
            }

            chunk = PsiFormat::read_chunk(&mut image)?;
        }

        let head_ct = heads_seen.len() as u8;
        let track_ct = track_set.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((track_ct / head_ct as u16, head_ct)),
            data_rate: Default::default(),
            data_encoding: DiskDataEncoding::Mfm,
            density: disk_density,
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
