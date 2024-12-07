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

use binrw::{binrw, BinRead};

use crate::{
    file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility},
    io::{Cursor, ReadBytesExt, ReadSeek, ReadWriteSeek},
    track::fluxstream::FluxStreamTrack,
    types::{chs::DiskCh, DiskDataEncoding, DiskDensity, DiskDescriptor},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashSet,
    LoadingCallback,
    DEFAULT_SECTOR_SIZE,
};

pub struct PfiFormat;
pub const MAXIMUM_CHUNK_SIZE: usize = 0x1000000; // Reasonable 10MB limit for chunk sizes.

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiChunkHeader {
    pub id:   [u8; 4],
    pub size: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(big)]
pub struct PfiHeader {
    pub version:  u16,
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

#[derive(Default)]
pub struct TrackContext {
    phys_ch: Option<DiskCh>,
    clock_rate: Option<u32>,
    clock_period: f64,
    index_clocks: Vec<u32>,
}

#[derive(Default)]
pub struct PfiRevolution {
    transitions: Vec<f64>,
    index_time:  f64,
}

impl PfiFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceFluxImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["pfi"]
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
    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
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
            b"PFI " => PfiChunkType::FileHeader,
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
        disk_image.set_source_format(DiskImageFileFormat::PceFluxImage);

        // Seek to start of read_buf.
        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let mut chunk = PfiFormat::read_chunk(&mut read_buf)?;
        // File header must be first chunk.
        if chunk.chunk_type != PfiChunkType::FileHeader {
            return Err(DiskImageError::UnknownFormat);
        }

        let file_header =
            PfiHeader::read(&mut Cursor::new(&chunk.data)).map_err(|_| DiskImageError::FormatParseError)?;
        log::trace!("Read PFI file header. Format version: {}", file_header.version);

        let mut comment_string = String::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();
        let mut cylinders_seen: FoxHashSet<u16> = FoxHashSet::new();
        let disk_clock_rate = None;
        let mut track_header;

        let mut ctx = TrackContext::default();

        while chunk.chunk_type != PfiChunkType::End {
            match chunk.chunk_type {
                PfiChunkType::TrackHeader => {
                    track_header = PfiTrackHeader::read(&mut Cursor::new(&chunk.data))
                        .map_err(|_| DiskImageError::FormatParseError)?;

                    let ch = DiskCh::from((track_header.cylinder as u16, track_header.head as u8));

                    ctx.phys_ch = Some(ch);
                    ctx.clock_rate = Some(track_header.clock_rate);
                    ctx.clock_period = 1.0 / (track_header.clock_rate as f64);

                    log::trace!(
                        "Track header: {:?} Clock Rate: {:.04}Mhz Period: {:.04}us",
                        ch,
                        track_header.clock_rate as f64 / 1_000_000.0,
                        ctx.clock_period * 1_000_000.0
                    );
                    cylinders_seen.insert(track_header.cylinder as u16);
                    heads_seen.insert(track_header.head as u8);
                }
                PfiChunkType::Index => {
                    let index_entries = chunk.size / 4;
                    let mut index_list: Vec<u32> = Vec::with_capacity(index_entries as usize);

                    for i in 0..index_entries {
                        let index = u32::from_be_bytes([
                            chunk.data[i as usize * 4],
                            chunk.data[i as usize * 4 + 1],
                            chunk.data[i as usize * 4 + 2],
                            chunk.data[i as usize * 4 + 3],
                        ]);
                        index_list.push(index);
                    }

                    log::trace!("Index chunk with {} entries:", index_entries);
                    for idx in &index_list {
                        log::trace!("Index clock: {}", idx);
                    }

                    ctx.index_clocks = index_list;
                }
                PfiChunkType::TrackData => {
                    log::trace!(
                        "Track data chunk: {} size: {}",
                        ctx.phys_ch.unwrap_or_default(),
                        chunk.size,
                    );

                    let revolutions = PfiFormat::read_track_data(&chunk.data, &ctx.index_clocks, ctx.clock_period)?;
                    log::trace!("Read {} revolutions from track data.", revolutions.len());

                    let mut flux_track = FluxStreamTrack::new();

                    // Get last ch in image.
                    let next_ch = if disk_image.track_ch_iter().count() == 0 {
                        log::debug!("No tracks in image, starting at c:0 h:0");
                        DiskCh::new(0, 0)
                    }
                    else {
                        let mut last_ch = disk_image.track_ch_iter().last().unwrap_or(DiskCh::new(0, 0));
                        log::debug!("Previous track in image: {} heads: {}", last_ch, heads_seen.len());

                        last_ch.seek_next_track_unchecked(heads_seen.len() as u8);
                        log::debug!("Setting next track ch: {}", last_ch);
                        last_ch
                    };

                    for (ri, rev) in revolutions.iter().enumerate() {
                        log::trace!(
                            "Adding revolution {} with {} transitions and index time of {:.04}ms.",
                            ri,
                            rev.transitions.len(),
                            rev.index_time * 1_000.0
                        );

                        flux_track.add_revolution(next_ch, &rev.transitions, rev.index_time);
                    }

                    // Get hints from disk image if we aren't the first track.
                    let (clock_hint, rpm_hint) = if !disk_image.track_pool.is_empty() {
                        (
                            Some(disk_image.descriptor.density.base_clock(disk_image.descriptor.rpm)),
                            disk_image.descriptor.rpm,
                        )
                    }
                    else {
                        (None, None)
                    };

                    let data_rate = disk_image.data_rate();
                    let new_track = disk_image.add_track_fluxstream(next_ch, flux_track, clock_hint, rpm_hint)?;

                    let (new_density, new_rpm) = if new_track.sector_ct() == 0 {
                        log::warn!("Track did not decode any sectors. Not updating disk image descriptor.");
                        (disk_image.descriptor.density, disk_image.descriptor.rpm)
                    }
                    else {
                        let info = new_track.info();
                        log::debug!(
                            "Updating disk descriptor with density: {:?} and RPM: {:?}",
                            info.density,
                            info.rpm
                        );
                        (info.density.unwrap_or(disk_image.descriptor.density), info.rpm)
                    };

                    log::debug!("Track added.");

                    disk_image.descriptor = DiskDescriptor {
                        geometry: DiskCh::from((cylinders_seen.len() as u16, heads_seen.len() as u8)),
                        data_rate,
                        density: new_density,
                        data_encoding: DiskDataEncoding::Mfm,
                        default_sector_size: DEFAULT_SECTOR_SIZE,
                        rpm: new_rpm,
                        write_protect: Some(true),
                    };
                }
                PfiChunkType::Text => {
                    // PFI docs:
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

        let clock_rate = disk_clock_rate.unwrap_or_default();

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((cylinder_ct, head_ct)),
            data_rate: clock_rate,
            data_encoding: DiskDataEncoding::Mfm,
            density: DiskDensity::from(clock_rate),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: None,
        };

        Ok(())
    }

    /// Read PFI variable-length flux transitions and return a list of flux transition times
    /// in f64 seconds
    fn read_track_data(
        data: &[u8],
        index_times: &[u32],
        clock_period: f64,
    ) -> Result<Vec<PfiRevolution>, DiskImageError> {
        if index_times.is_empty() {
            log::error!("No index times found in track data.");
            return Err(DiskImageError::FormatParseError);
        }

        let mut revs: Vec<PfiRevolution> = Vec::with_capacity(5);

        let mut current_rev_idx = 0;
        let mut next_index = index_times[0];
        let mut current_rev = &mut PfiRevolution::default();

        let mut clocks = 0;
        let mut data_cursor = Cursor::new(data);

        let mut last_index_clock = 0;

        while let Ok(byte) = data_cursor.read_u8() {
            if clocks >= next_index {
                log::trace!("Reached next index position at clock: {}", clocks);
                current_rev_idx += 1;
                if current_rev_idx >= index_times.len() {
                    break;
                }

                current_rev.index_time = (clocks - last_index_clock) as f64 * clock_period;

                next_index = index_times[current_rev_idx];
                revs.push(PfiRevolution {
                    transitions: Vec::with_capacity(225_000),
                    index_time:  0.0,
                });

                current_rev = revs.last_mut().unwrap();
                last_index_clock = clocks;
            }

            match byte {
                0x00 => {
                    // Invalid
                    log::error!("Invalid 0x00 byte in flux stream.");
                    return Err(DiskImageError::FormatParseError);
                }
                0x01 => {
                    // XX YY
                    let xx = data_cursor.read_u8()?;
                    let yy = data_cursor.read_u8()?;
                    let time = (xx as u16) << 8 | yy as u16;
                    clocks += time as u32;
                    current_rev.transitions.push(time as f64 * clock_period);
                }
                0x02 => {
                    // XX YY ZZ
                    let xx = data_cursor.read_u8()?;
                    let yy = data_cursor.read_u8()?;
                    let zz = data_cursor.read_u8()?;
                    let time = (xx as u32) << 16 | (yy as u32) << 8 | zz as u32;
                    clocks += time;
                    current_rev.transitions.push(time as f64 * clock_period);
                }
                0x03 => {
                    // XX YY ZZ WW
                    let xx = data_cursor.read_u8()?;
                    let yy = data_cursor.read_u8()?;
                    let zz = data_cursor.read_u8()?;
                    let ww = data_cursor.read_u8()?;
                    let time = (xx as u32) << 24 | (yy as u32) << 16 | (zz as u32) << 8 | ww as u32;
                    clocks += time;
                    current_rev.transitions.push(time as f64 * clock_period);
                }
                0x04..0x08 => {
                    // 0(N-4) XX
                    let base = byte - 0x04;
                    let xx = data_cursor.read_u8()?;
                    let time = (base as u16) << 8 | xx as u16;
                    clocks += time as u32;
                    current_rev.transitions.push(time as f64 * clock_period);
                }
                _ => {
                    // Byte as literal clock count.
                    clocks += byte as u32;
                    current_rev.transitions.push(byte as f64 * clock_period);
                }
            }
        }

        current_rev.index_time = (clocks - last_index_clock) as f64 * clock_period;
        Ok(revs)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
