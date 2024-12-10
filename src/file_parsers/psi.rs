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

use crate::{
    file_parsers::{FormatCaps, ParserWriteCompatibility},
    io::{Cursor, ReadSeek, ReadWriteSeek},
    types::{AddSectorParams, DiskDescriptor},
};

use crate::{
    file_parsers::{ParserReadOptions, ParserWriteOptions},
    types::{
        chs::{DiskCh, DiskChs, DiskChsn},
        MetaSectorTrackParams,
        Platform,
        SectorAttributes,
        TrackDataEncoding,
        TrackDataRate,
        TrackDensity,
    },
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashMap,
    FoxHashSet,
    LoadingCallback,
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

#[derive(Default)]
pub struct SectorContext {
    phys_chs: Option<DiskChs>,
    phys_size: usize,
    ibm_chsn: Option<DiskChsn>,
    data_crc_error: bool,
    address_crc_error: bool,
    deleted: bool,
    no_dam: bool,
    alternate: bool,
    bit_offset: Option<u32>,
}

impl SectorContext {
    fn have_context(&self) -> bool {
        self.phys_chs.is_some()
    }

    fn reset(&mut self) {
        *self = SectorContext::default();
    }

    #[allow(dead_code)]
    fn phys_ch(&self) -> DiskCh {
        DiskCh::from(self.phys_chs.unwrap())
    }

    fn sid(&self) -> DiskChsn {
        self.ibm_chsn.unwrap_or(DiskChsn::new(
            self.phys_chs.unwrap().c(),
            self.phys_chs.unwrap().h(),
            self.phys_chs.unwrap().s(),
            DiskChsn::bytes_to_n(self.phys_size),
        ))
    }
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PsiChunkHeader {
    pub id:   [u8; 4],
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

#[binrw]
#[brw(big)]
pub struct PsiIbmSectorHeader {
    pub cylinder: u8,
    pub head: u8,
    pub sector: u8,
    pub n: u8,
    pub flags: u8,
    pub encoding: u8,
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
            }
            else {
                crc <<= 1;
            }
        }
    }
    crc & 0xffffffff
}

pub(crate) fn decode_psi_sector_format(sector_format: [u8; 2]) -> Option<(TrackDataEncoding, TrackDensity)> {
    match sector_format {
        [0x00, 0x00] => Some((TrackDataEncoding::Fm, TrackDensity::Standard)),
        [0x01, 0x00] => Some((TrackDataEncoding::Fm, TrackDensity::Double)),
        [0x02, 0x00] => Some((TrackDataEncoding::Fm, TrackDensity::High)),
        [0x02, 0x01] => Some((TrackDataEncoding::Fm, TrackDensity::High)),
        [0x02, 0x02] => Some((TrackDataEncoding::Mfm, TrackDensity::Extended)),
        // TODO: What density are GCR disks? Are they all the same? PSI doesn't specify any variants.
        [0x03, 0x00] => Some((TrackDataEncoding::Gcr, TrackDensity::Double)),
        _ => None,
    }
}

impl PsiFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceSectorImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub fn platforms() -> Vec<Platform> {
        // PSI images support both PC and Macintosh platforms.
        vec![Platform::IbmPc, Platform::Macintosh]
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

    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn read_chunk<RWS: ReadSeek>(mut image: RWS) -> Result<PsiChunk, DiskImageError> {
        let chunk_pos = image.stream_position()?;

        //log::trace!("Reading chunk header...");
        let chunk_header = PsiChunkHeader::read(&mut image)?;

        if let Ok(id) = std::str::from_utf8(&chunk_header.id) {
            log::trace!("Chunk ID: {} Size: {}", id, chunk_header.size);
        }
        else {
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
            b"IBMM" => PsiChunkType::IbmMfmSectorHeader,
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

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::PceSectorImage);

        // Seek to start of read_buf.
        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PsiFormat::read_chunk(&mut read_buf)?;
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

        let mut ctx = SectorContext::default();
        let mut track_set: FoxHashSet<DiskCh> = FoxHashSet::new();
        let mut sector_counts: FoxHashMap<u8, u32> = FoxHashMap::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut sectors_per_track = 0;

        let mut current_track = None;

        while chunk.chunk_type != PsiChunkType::End {
            match chunk.chunk_type {
                PsiChunkType::FileHeader => {}
                PsiChunkType::SectorHeader => {
                    //log::trace!("Sector header chunk.");
                    let sector_header = PsiSectorHeader::read(&mut Cursor::new(&chunk.data))?;
                    let chs = DiskChs::from((sector_header.cylinder, sector_header.head, sector_header.sector));
                    let ch = DiskCh::from((sector_header.cylinder, sector_header.head));

                    heads_seen.insert(sector_header.head);

                    if !track_set.contains(&ch) {
                        log::trace!("Adding track...");

                        let params = MetaSectorTrackParams {
                            ch,
                            data_rate: TrackDataRate::from(disk_density),
                            encoding: default_encoding,
                        };

                        let new_track = disk_image.add_track_metasector(&params)?;

                        current_track = Some(new_track);
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
                        ctx.alternate = true;
                    }
                    else {
                        ctx.alternate = false;
                    }

                    ctx.phys_chs = Some(chs);
                    ctx.phys_size = sector_header.size as usize;
                    ctx.data_crc_error = sector_header.flags & SH_FLAG_CRC_ERROR != 0;

                    // Write sector data immediately if compressed data is indicated (no sector data chunk follows)
                    if sector_header.flags & SH_FLAG_COMPRESSED != 0 {
                        log::trace!("Compressed sector data: {:02X}", sector_header.compressed_data);
                        let chunk_expand = vec![sector_header.compressed_data; sector_header.size as usize];

                        if let Some(ref mut track) = current_track {
                            // Add this sector to track.
                            let params = AddSectorParams {
                                id_chsn: DiskChsn::from((chs, DiskChsn::bytes_to_n(sector_header.size as usize))),
                                data: &chunk_expand,
                                weak_mask: None,
                                hole_mask: None,
                                attributes: SectorAttributes {
                                    address_crc_valid: true, // Compressed data cannot encode address CRC state.
                                    data_crc_valid: !ctx.data_crc_error,
                                    deleted_mark: false,
                                    no_dam: false,
                                },
                                alternate: ctx.alternate,
                                bit_index: ctx.bit_offset.map(|x| x as usize),
                            };

                            track.add_sector(&params)?;
                            ctx.reset();
                        }
                        else {
                            log::error!("Tried to add sector without a current track.");
                            return Err(DiskImageError::FormatParseError);
                        }
                    }

                    log::trace!(
                        "SECT chunk: Sector ID: {} size: {} data_crc_error: {}",
                        chs,
                        sector_header.size,
                        ctx.data_crc_error
                    );
                }
                PsiChunkType::SectorData => {
                    if !ctx.have_context() {
                        log::error!("Sector data chunk without a preceding sector header.");
                        return Err(DiskImageError::FormatParseError);
                    }

                    log::trace!(
                        "DATA chunk: {} crc_error: {}",
                        ctx.phys_chs.unwrap(),
                        ctx.data_crc_error
                    );

                    if ctx.phys_size != chunk.data.len() {
                        log::warn!(
                            "Sector data size mismatch. Header specified: {} SectorData specified: {}",
                            ctx.phys_size,
                            chunk.data.len()
                        );
                    }

                    if let Some(ref mut track) = current_track {
                        // Add this sector to track.
                        let params = AddSectorParams {
                            id_chsn: ctx.sid(),
                            data: &chunk.data,
                            weak_mask: None,
                            hole_mask: None,
                            attributes: SectorAttributes {
                                address_crc_valid: !ctx.address_crc_error,
                                data_crc_valid: !ctx.data_crc_error,
                                deleted_mark: ctx.deleted,
                                no_dam: ctx.no_dam,
                            },
                            alternate: ctx.alternate,
                            bit_index: ctx.bit_offset.map(|x| x as usize),
                        };

                        track.add_sector(&params)?;
                    }
                    else {
                        log::error!("Tried to add sector without a current track.");
                        return Err(DiskImageError::FormatParseError);
                    }
                    sectors_per_track += 1;
                    ctx.reset();
                }
                PsiChunkType::Text => {
                    // PSI docs:
                    // `If there are multiple TEXT chunks, their contents should be concatenated`
                    if let Ok(text) = std::str::from_utf8(&chunk.data) {
                        comment_string.push_str(text);
                    }
                }
                PsiChunkType::SectorPositionOffset => {
                    let offset = u32::from_be_bytes([chunk.data[0], chunk.data[1], chunk.data[2], chunk.data[3]]);
                    ctx.bit_offset = Some(offset);
                    log::trace!("Sector position offset: {}", offset);
                }
                PsiChunkType::IbmMfmSectorHeader => {
                    let ibm_header = PsiIbmSectorHeader::read(&mut Cursor::new(&chunk.data))?;

                    if ctx.ibm_chsn.is_some() {
                        log::warn!("Duplicate IBM sector header or context not reset");
                    }

                    ctx.ibm_chsn = Some(DiskChsn::from((
                        ibm_header.cylinder as u16,
                        ibm_header.head,
                        ibm_header.sector,
                        ibm_header.n,
                    )));

                    ctx.data_crc_error = ibm_header.flags & SH_IBM_FLAG_CRC_ERROR_DATA != 0;
                    ctx.address_crc_error = ibm_header.flags & SH_IBM_FLAG_CRC_ERROR_ID != 0;
                    ctx.deleted = ibm_header.flags & SH_IBM_DELETED_DATA != 0;
                    ctx.no_dam = ibm_header.flags & SH_IBM_MISSING_DATA != 0;
                }
                PsiChunkType::End => {
                    log::trace!("End chunk.");
                    break;
                }
                _ => {
                    log::warn!("Unhandled chunk type: {:?}", chunk.chunk_type);
                }
            }

            chunk = PsiFormat::read_chunk(&mut read_buf)?;
        }

        let head_ct = heads_seen.len() as u8;
        let track_ct = track_set.len() as u16;
        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((track_ct / head_ct as u16, head_ct)),
            data_rate: Default::default(),
            data_encoding: TrackDataEncoding::Mfm,
            density: disk_density,
            rpm: None,
            write_protect: None,
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
