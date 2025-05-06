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
*/

//! File format parser for the MOOF disk image format.
//! MOOF images are intended to store Macintosh disk images.
//! The format was developed by the author of the Applesauce project.
//! https://applesaucefdc.com/moof-reference/

use crate::{
    file_parsers::{bitstream_flags, FormatCaps},
    format_ms,
    io::ReadSeek,
    platform::Platform,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashMap,
    LoadingCallback,
    LoadingStatus,
    ParserWriteCompatibility,
};

use crate::{
    file_parsers::{
        r#as::{crc::applesauce_crc32, flux::decode_as_flux},
        ParserReadOptions,
        ParserWriteOptions,
    },
    io::ReadWriteSeek,
    prelude::{DiskCh, TrackDataEncoding, TrackDataRate, TrackDataResolution, TrackDensity},
    source_map::{MapDump, OptionalSourceMap, SourceValue},
    track::fluxstream::FluxStreamTrack,
    types::{BitStreamTrackParams, DiskDescriptor, DiskRpm, FluxStreamTrackParams},
};
use binrw::{binrw, BinRead};

pub const MOOF_MAGIC: &str = "MOOF";
pub const MAX_TRACKS: u8 = 160;

#[derive(Debug)]
pub enum MoofDiskType {
    SsDdGcr400K,
    DsDsGcr800K,
    DsHdMfm144M,
    Twiggy,
    Unknown,
}

impl MoofDiskType {
    pub fn heads(&self) -> u8 {
        match self {
            MoofDiskType::SsDdGcr400K => 1,
            MoofDiskType::DsDsGcr800K => 2,
            MoofDiskType::DsHdMfm144M => 2,
            // The Twiggy drive was a bizarre thing with two heads, but not directly opposing each
            // other. See http://www.brouhaha.com/~eric/retrocomputing/lisa/twiggy.html
            MoofDiskType::Twiggy => 2,
            // Just guess
            MoofDiskType::Unknown => 2,
        }
    }
}

impl TryFrom<&MoofDiskType> for TrackDataEncoding {
    type Error = String;
    fn try_from(value: &MoofDiskType) -> Result<Self, Self::Error> {
        match value {
            MoofDiskType::SsDdGcr400K => Ok(TrackDataEncoding::Gcr),
            MoofDiskType::DsDsGcr800K => Ok(TrackDataEncoding::Gcr),
            MoofDiskType::DsHdMfm144M => Ok(TrackDataEncoding::Mfm),
            MoofDiskType::Twiggy => Ok(TrackDataEncoding::Gcr),
            MoofDiskType::Unknown => Err("Unknown MOOF disk type".to_string()),
        }
    }
}

impl TryFrom<&MoofDiskType> for TrackDensity {
    type Error = String;
    fn try_from(value: &MoofDiskType) -> Result<Self, Self::Error> {
        match value {
            MoofDiskType::SsDdGcr400K => Ok(TrackDensity::Standard),
            MoofDiskType::DsDsGcr800K => Ok(TrackDensity::Double),
            MoofDiskType::DsHdMfm144M => Ok(TrackDensity::High),
            MoofDiskType::Twiggy => Ok(TrackDensity::Double),
            MoofDiskType::Unknown => Err("Unknown MOOF disk type".to_string()),
        }
    }
}

#[binrw]
#[brw(little)]
pub struct MoofHeader {
    magic: [u8; 4],
    data_check: u32,
    crc: u32,
}

#[binrw]
#[br(little)]
pub struct MoofChunkHeader {
    id:   [u8; 4],
    size: u32,
}

pub enum MoofChunk {
    Info(InfoChunk),
    TMap(TMapChunk),
    Trks(TrksChunk),
    Flux(FluxChunk),
    Meta(String),
    Unknown,
}

#[derive(BinRead)]
#[br(little)]
pub struct InfoChunk {
    info_version: u8,

    #[br(map = |x: u8| match x {
        1 => MoofDiskType::SsDdGcr400K,
        2 => MoofDiskType::DsDsGcr800K,
        3 => MoofDiskType::DsHdMfm144M,
        4 => MoofDiskType::Twiggy,
        _ => MoofDiskType::Unknown,
    })]
    disk_type: MoofDiskType,
    write_protected: u8,
    synchronized: u8,
    optimal_bit_timing: u8,

    #[br(map = |x: [u8; 32]| String::from_utf8_lossy(&x).trim_end().to_string())]
    creator: String, // 32-byte UTF-8 string padded with spaces

    _padding: u8, // Always 0

    largest_track: u16,
    flux_block: u16,
    largest_flux_track: u16,
}

impl MapDump for InfoChunk {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let record = map.add_child(parent, "[INFO] Chunk", SourceValue::default());
        let record_idx = record.index();
        record
            .add_child("info_version", SourceValue::u8(self.info_version))
            .add_sibling("disk_type", SourceValue::string(&format!("{:?}", self.disk_type)))
            .add_sibling("write_protected", SourceValue::u8(self.write_protected))
            .add_sibling("synchronized", SourceValue::u8(self.synchronized))
            .add_sibling("optimal_bit_timing", SourceValue::u8(self.optimal_bit_timing))
            .add_sibling("creator", SourceValue::string(&self.creator.clone()))
            .add_sibling("largest_track", SourceValue::u32(self.largest_track as u32))
            .add_sibling(
                "flux_block",
                SourceValue::u32(self.flux_block as u32).comment(if self.has_flux_block() {
                    "Flux block present"
                }
                else {
                    "No flux block"
                }),
            )
            .add_sibling("largest_flux_track", SourceValue::u32(self.largest_flux_track as u32));
        record_idx
    }
}

impl InfoChunk {
    pub fn has_flux_block(&self) -> bool {
        self.flux_block > 0 && self.largest_flux_track > 0
    }
}

#[binrw]
#[br(little)]
pub struct TMapChunk {
    pub(crate) track_map: [u8; 160],
}

impl MapDump for TMapChunk {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let entry = map.add_child(parent, "[TMAP] Chunk", SourceValue::default());
        let entry_idx = entry.index();
        for (i, track_pair) in self.track_map.chunks_exact(2).enumerate() {
            map.add_child(entry_idx, &format!("[{}] TMap Entry", i), SourceValue::default())
                .add_child("head0", SourceValue::u8(track_pair[0]).bad_if(track_pair[0] == 0xFF))
                .add_sibling("head1", SourceValue::u8(track_pair[1]).bad_if(track_pair[1] == 0xFF));
        }
        entry_idx
    }
}

#[binrw]
#[br(little)]
pub struct FluxChunk {
    pub(crate) track_map: [u8; 160],
}

impl MapDump for FluxChunk {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let entry = map.add_child(parent, "[FLUX] Chunk", SourceValue::default());
        let entry_idx = entry.index();
        for (i, track_pair) in self.track_map.chunks_exact(2).enumerate() {
            map.add_child(entry_idx, &format!("[{}] Flux Map Entry", i), SourceValue::default())
                .add_child("head0", SourceValue::u8(track_pair[0]).bad_if(track_pair[0] == 0xFF))
                .add_sibling("head1", SourceValue::u8(track_pair[1]).bad_if(track_pair[1] == 0xFF));
        }
        entry_idx
    }
}

#[derive(BinRead)]
#[br(little)]
pub struct Trk {
    #[br(map = |x: u16| {x as u64 * 512})]
    starting_block: u64,
    block_ct: u16,
    bit_ct: u32,
}

impl MapDump for Trk {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        map.add_child(
            parent,
            "starting_block",
            SourceValue::u32((self.starting_block / 512) as u32),
        )
        .add_sibling("block_ct", SourceValue::u32(self.block_ct as u32))
        .add_sibling("bit_ct", SourceValue::u32(self.bit_ct));
        0
    }
}

#[derive(BinRead)]
#[br(little)]
pub struct TrksChunk {
    trks: [Trk; 160],
}

impl MapDump for TrksChunk {
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize {
        let entry = map.add_child(parent, "[TRKS] Chunk", SourceValue::default());
        let entry_idx = entry.index();
        for (i, trk) in self.trks.iter().enumerate() {
            let trk_entry = map.add_child(entry_idx, &format!("[{}] Track Entry", i), SourceValue::default());
            let trk_entry_idx = trk_entry.index();
            trk.write_to_map(map, trk_entry_idx);
        }
        entry_idx
    }
}

pub struct MoofFormat;

impl MoofFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::MoofImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub fn platforms() -> Vec<Platform> {
        // MOOF in theory could support other formats, but is primarily used for Macintosh disk
        // images.
        vec![Platform::Macintosh]
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["moof"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = MoofHeader::read(&mut image) {
            if file_header.magic == MOOF_MAGIC.as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
        ParserWriteCompatibility::Incompatible
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut reader: RWS,
        disk_image: &mut DiskImage,
        _opts: &ParserReadOptions,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::MoofImage);
        disk_image.assign_source_map(true);

        // Advertise progress support
        if let Some(ref callback_fn) = callback {
            callback_fn(LoadingStatus::ProgressSupport);
        }

        // Get image size
        let image_size = reader.seek(std::io::SeekFrom::End(0))?;
        log::debug!("Image size: {} bytes", image_size);

        _ = reader.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = MoofHeader::read(&mut reader) {
            if file_header.magic != MOOF_MAGIC.as_bytes() {
                return Err(DiskImageError::ImageCorruptError(
                    "MOOF magic bytes not found".to_string(),
                ));
            }

            let rewind_pos = reader.seek(std::io::SeekFrom::Current(0))?;
            let mut crc_buf = Vec::with_capacity(image_size as usize);
            reader.read_to_end(&mut crc_buf)?;

            let crc = applesauce_crc32(&crc_buf, 0);
            log::debug!("Header CRC: {:0X?} Calculated CRC: {:0X?}", file_header.crc, crc);
            reader.seek(std::io::SeekFrom::Start(rewind_pos))?;

            if file_header.crc != crc {
                return Err(DiskImageError::ImageCorruptError("Header CRC mismatch".to_string()));
            }
        }

        let mut info_chunk_opt = None;
        let mut tmap_chunk_opt = None;
        let mut trks_chunk_opt = None;
        let mut flux_chunk_opt = None;

        log::debug!("Reading chunks...");
        let mut more_chunks = true;
        while more_chunks {
            let chunk_opt = match Self::read_chunk(&mut reader, image_size) {
                Ok(chunk_opt) => chunk_opt,
                Err(e) => {
                    log::error!("Error reading MOOF chunk: {}", e);
                    break;
                }
            };

            if let Some(chunk) = chunk_opt {
                match chunk {
                    MoofChunk::Info(info_chunk) => {
                        log::debug!(
                            "Got Info Chunk: version: {} Disk Type: {:?} Creator: {}",
                            info_chunk.info_version,
                            info_chunk.disk_type,
                            info_chunk.creator
                        );

                        if info_chunk.flux_block != 0 {
                            log::debug!("Flux block is present: {}", info_chunk.flux_block);
                        }

                        if info_chunk.info_version != 1 {
                            log::error!("Unsupported MOOF Info Chunk version: {}", info_chunk.info_version);
                            return Err(DiskImageError::IncompatibleImage(
                                "Unsupported MOOF Info Chunk version".to_string(),
                            ));
                        }
                        info_chunk.write_to_map(disk_image.source_map_mut(), 0);
                        info_chunk_opt = Some(info_chunk);
                    }
                    MoofChunk::TMap(tmap_chunk) => {
                        log::debug!("Got Track Map Chunk");
                        tmap_chunk.write_to_map(disk_image.source_map_mut(), 0);
                        tmap_chunk_opt = Some(tmap_chunk);
                    }
                    MoofChunk::Trks(trks_chunk) => {
                        log::debug!("Got Tracks Chunk");
                        trks_chunk.write_to_map(disk_image.source_map_mut(), 0);
                        trks_chunk_opt = Some(trks_chunk);
                    }
                    MoofChunk::Flux(flux_chunk) => {
                        log::debug!("Got Flux Chunk");
                        flux_chunk.write_to_map(disk_image.source_map_mut(), 0);
                        flux_chunk_opt = Some(flux_chunk);
                    }
                    MoofChunk::Meta(meta_str) => {
                        let meta_map = Self::parse_meta(&meta_str);

                        log::debug!("Metadata KV pairs:");

                        let mut cursor =
                            disk_image
                                .source_map_mut()
                                .add_child(0, "[META] Chunk", SourceValue::default());
                        for (i, (key, value)) in meta_map.iter().enumerate() {
                            if i == 0 {
                                cursor = cursor.add_child(key, SourceValue::string(value));
                            }
                            else {
                                cursor = cursor.add_sibling(key, SourceValue::string(value));
                            }
                            log::debug!("{}: {}", key, value);
                        }
                    }
                    MoofChunk::Unknown => {
                        log::debug!("Got Unknown Chunk");
                    }
                }
            }
            else {
                log::debug!("No more chunks found in MOOF image");
                more_chunks = false;
            }
        }

        if info_chunk_opt.is_none() {
            log::error!("Missing Info chunk");
            return Err(DiskImageError::ImageCorruptError("Missing Info chunk".to_string()));
        }

        let info_chunk = info_chunk_opt.unwrap();

        // Enable multi-resolution support if necessary
        if info_chunk.has_flux_block() {
            disk_image.set_multires(true);
        }

        let disk_heads = info_chunk.disk_type.heads();
        let disk_encoding = match TrackDataEncoding::try_from(&info_chunk.disk_type) {
            Ok(disk_encoding) => disk_encoding,
            Err(e) => {
                log::error!("Error converting MOOF disk type to TrackDataEncoding: {}", e);
                return Err(DiskImageError::IncompatibleImage(
                    "Error converting MOOF disk type to TrackDataEncoding".to_string(),
                ));
            }
        };

        let disk_density = match TrackDensity::try_from(&info_chunk.disk_type) {
            Ok(disk_density) => disk_density,
            Err(e) => {
                log::error!("Error converting MOOF disk type to TrackDensity: {}", e);
                return Err(DiskImageError::IncompatibleImage(
                    "Error converting MOOF disk type to TrackDensity".to_string(),
                ));
            }
        };

        let mut ch_iter = DiskCh::new(160, disk_heads).iter();

        if let (Some(tmap), Some(trks)) = (tmap_chunk_opt, trks_chunk_opt) {
            log::debug!("Track Map:");

            // Fluxfox should be able to deduplicate empty tracks, but we can save effort by skipping
            // empty tracks here.
            for (i, track_pair) in tmap.track_map.chunks_exact(2).enumerate() {
                log::debug!("\tMap Entry {}: h0: Trk {} h1: Trk {}", i, track_pair[0], track_pair[1]);

                for (head, trk_idx) in track_pair.iter().take(disk_heads as usize).enumerate() {
                    if let Some(ref callback_fn) = callback {
                        let progress = ((i * 2) + head) as f64 / MAX_TRACKS as f64;
                        callback_fn(LoadingStatus::Progress(progress));
                    }

                    if *trk_idx != 0xFF {
                        if trk_idx >= &MAX_TRACKS {
                            log::error!("Invalid track index: {}", trk_idx);
                            return Err(DiskImageError::ImageCorruptError(
                                "Invalid track index in TMAP chunk".to_string(),
                            ));
                        }

                        let ch = ch_iter.next().unwrap();
                        let trk_entry = &trks.trks[*trk_idx as usize];
                        Self::add_bitstream_track(&mut reader, disk_image, image_size, ch, disk_encoding, trk_entry)?;
                    }
                    else {
                        let ch = ch_iter.next().unwrap();

                        let mut add_empty_track = false;
                        // If we have a flux chunk we can check if this track has flux data
                        if let Some(flux_chunk) = &flux_chunk_opt {
                            let flux_idx = flux_chunk.track_map[(i * 2) + head];
                            if flux_idx < MAX_TRACKS {
                                let flux_entry = &trks.trks[flux_idx as usize];
                                log::debug!("\t\tFlux Track Index: {}", flux_idx);
                                Self::add_fluxstream_track(
                                    &mut reader,
                                    disk_image,
                                    image_size,
                                    ch,
                                    disk_encoding,
                                    flux_entry,
                                )?;
                            }
                            else {
                                log::debug!("\t\t(no flux)");
                                add_empty_track = true;
                            }
                        }

                        if add_empty_track {
                            disk_image.add_empty_track(
                                ch,
                                disk_encoding,
                                Some(TrackDataResolution::BitStream),
                                TrackDataRate::from(disk_density),
                                0,
                                None,
                            )?;
                        }
                    }
                }
            }
        }
        else {
            log::error!("Missing Track Map or Tracks chunk");
            return Err(DiskImageError::ImageCorruptError(
                "Missing Track Map or Tracks chunk".to_string(),
            ));
        }

        let geometry = DiskCh::new(ch_iter.next().unwrap().c(), disk_heads);

        let desc = DiskDescriptor {
            platforms: Some(vec![Platform::Macintosh]),
            geometry,
            data_encoding: disk_encoding,
            density: disk_density,
            data_rate: TrackDataRate::from(disk_density),
            rpm: None,
            write_protect: Some(info_chunk.write_protected != 0),
        };

        disk_image.descriptor = desc;

        Ok(())
    }

    fn add_bitstream_track<RWS: ReadSeek>(
        mut reader: RWS,
        disk: &mut DiskImage,
        _image_size: u64,
        ch: DiskCh,
        encoding: TrackDataEncoding,
        track: &Trk,
    ) -> Result<(), DiskImageError> {
        log::debug!(
            "add_bitstream_track(): Track: {} Starting block: {} Blocks: {} ({} bytes) Bitcells: {}",
            ch,
            track.starting_block,
            track.block_ct,
            track.block_ct as usize * 512,
            track.bit_ct
        );

        // Seek to the start of the track data block (This was converted to byte offset on read)
        reader.seek(std::io::SeekFrom::Start(track.starting_block))?;

        // Read in the track data.
        let mut read_vec = vec![0u8; track.block_ct as usize * 512];
        reader.read_exact(&mut read_vec)?;

        // Create the bitstream track parameters
        let params = BitStreamTrackParams {
            schema: None,
            ch,
            encoding,
            data_rate: Default::default(),
            rpm: None,
            bitcell_ct: Some(track.bit_ct as usize),
            data: &read_vec,
            weak: None,
            hole: None,
            detect_weak: false,
        };

        disk.add_track_bitstream(&params)?;

        Ok(())
    }

    fn add_fluxstream_track<RWS: ReadSeek>(
        mut reader: RWS,
        disk: &mut DiskImage,
        _image_size: u64,
        ch: DiskCh,
        _encoding: TrackDataEncoding,
        track: &Trk,
    ) -> Result<(), DiskImageError> {
        log::debug!(
            "add_fluxstream_track(): Track: {} Starting block: {} Blocks: {} ({} bytes) Fts: {}",
            ch,
            track.starting_block,
            track.block_ct,
            track.block_ct as usize * 512,
            track.bit_ct
        );

        // Seek to the start of the track data block (This was converted to byte offset on read)
        reader.seek(std::io::SeekFrom::Start(track.starting_block))?;

        // Read in the track data.
        let mut read_vec = vec![0u8; track.block_ct as usize * 512];
        reader.read_exact(&mut read_vec)?;

        // Decode the flux data
        let (fluxes, rev_time) = decode_as_flux(&read_vec);

        log::warn!(
            "Decoded {} flux transitions, index time: {}",
            fluxes.len(),
            format_ms!(rev_time)
        );

        // Create a fluxstream track
        let mut flux_track = FluxStreamTrack::new();

        // TODO: calculate the Zoned RPM here for Gcr disks
        flux_track.add_revolution(ch, &fluxes, DiskRpm::Rpm300(1.0).index_time_ms());

        let params = FluxStreamTrackParams {
            ch,
            schema: None,
            encoding: None,
            clock: None,
            rpm: None,
        };

        let new_track = disk.add_track_fluxstream(flux_track, &params)?;
        let info = new_track.info();

        log::debug!(
            "Added {} track {} containing {} bits to image...",
            ch,
            info.encoding,
            info.bit_length,
        );

        Ok(())
    }

    fn read_chunk<RWS: ReadSeek>(mut reader: RWS, image_size: u64) -> Result<Option<MoofChunk>, DiskImageError> {
        // Any bytes left in the stream?

        let offset = reader.seek(std::io::SeekFrom::Current(0))?;
        log::debug!("At file offset: {}", offset);

        if image_size == offset {
            log::debug!("No bytes left in reader!");
            return Ok(None);
        }

        // Read in the chunk header
        let chunk_header = MoofChunkHeader::read(&mut reader)?;
        log::debug!("Read chunk header: {:0X?}", chunk_header.id);

        // Save chunk data offset to advance unknown chunks
        let chunk_offset = reader.seek(std::io::SeekFrom::Current(0))?;

        let chunk = match &chunk_header.id {
            b"INFO" => {
                let info_chunk = InfoChunk::read(&mut reader)?;
                MoofChunk::Info(info_chunk)
            }
            b"TMAP" => {
                let tmap_chunk = TMapChunk::read(&mut reader)?;
                MoofChunk::TMap(tmap_chunk)
            }
            b"TRKS" => {
                let trks_chunk = TrksChunk::read(&mut reader)?;
                MoofChunk::Trks(trks_chunk)
            }
            b"FLUX" => {
                let flux_chunk = FluxChunk::read(&mut reader)?;
                MoofChunk::Flux(flux_chunk)
            }
            b"META" => {
                // Metadata chunk is just a string
                let mut meta_data = vec![0u8; chunk_header.size as usize];
                reader.read_exact(&mut meta_data)?;
                let meta_str = String::from_utf8_lossy(&meta_data).trim_end().to_string();
                MoofChunk::Meta(meta_str)
            }
            _ => {
                log::warn!("Unknown MOOF chunk: {:0X?}", chunk_header.id);
                MoofChunk::Unknown
            }
        };

        // Seek to next chunk
        reader.seek(std::io::SeekFrom::Start(chunk_offset + chunk_header.size as u64))?;

        Ok(Some(chunk))
    }

    fn parse_meta(meta_str: &str) -> FoxHashMap<String, String> {
        let mut meta_map = FoxHashMap::new();
        for line in meta_str.lines() {
            let mut parts = line.splitn(2, '\t');
            let key = parts.next().unwrap_or("").trim();
            let value = parts.next().unwrap_or("").trim();
            meta_map.insert(key.to_string(), value.to_string());
        }
        meta_map
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        _image: &DiskImage,
        _opts: &ParserWriteOptions,
        _output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
