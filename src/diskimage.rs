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
*/
use crate::bitstream::mfm::MfmCodec;
use crate::bitstream::raw::RawCodec;
use crate::bitstream::TrackDataStream;
use crate::boot_sector::BootSector;
use crate::chs::{DiskCh, DiskChs, DiskChsn};
use crate::containers::zip::extract_first_file;
use crate::containers::DiskImageContainer;
use crate::detect::detect_image_format;
use crate::file_parsers::ImageParser;
use crate::io::ReadSeek;
use crate::standard_format::StandardFormat;
use crate::structure_parsers::system34::{System34Element, System34Parser};
use crate::structure_parsers::{DiskStructureElement, DiskStructureMetadata, DiskStructureParser};
use crate::trackdata::TrackData;
use crate::{
    util, DiskDataEncoding, DiskDataRate, DiskDataResolution, DiskDensity, DiskImageError, DiskRpm, EncodingPhase,
    FoxHashMap, DEFAULT_SECTOR_SIZE,
};
use bit_vec::BitVec;
use sha1_smol::Digest;
use std::fmt::Display;
use std::io::Cursor;

/// An enumeration describing the type of disk image.
#[derive(Copy, Clone, Debug)]
pub enum DiskImageFormat {
    RawSectorImage,
    ImageDisk,
    PceSectorImage,
    PceBitstreamImage,
    MfmBitstreamImage,
    TeleDisk,
    KryofluxStream,
    HfeImage,
    F86Image, // 86F
    TransCopyImage,
}

impl DiskImageFormat {
    pub fn resolution(self) -> DiskDataResolution {
        match self {
            DiskImageFormat::RawSectorImage => DiskDataResolution::ByteStream,
            DiskImageFormat::ImageDisk => DiskDataResolution::ByteStream,
            DiskImageFormat::PceSectorImage => DiskDataResolution::ByteStream,
            DiskImageFormat::PceBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFormat::MfmBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFormat::TeleDisk => DiskDataResolution::ByteStream,
            DiskImageFormat::KryofluxStream => DiskDataResolution::FluxStream,
            DiskImageFormat::HfeImage => DiskDataResolution::BitStream,
            DiskImageFormat::F86Image => DiskDataResolution::BitStream,
            DiskImageFormat::TransCopyImage => DiskDataResolution::BitStream,
        }
    }
}

impl Display for DiskImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFormat::RawSectorImage => "Raw Sector Image".to_string(),
            DiskImageFormat::PceSectorImage => "PCE Sector Image".to_string(),
            DiskImageFormat::PceBitstreamImage => "PCE Bitstream Image".to_string(),
            DiskImageFormat::ImageDisk => "ImageDisk".to_string(),
            DiskImageFormat::TeleDisk => "TeleDisk".to_string(),
            DiskImageFormat::KryofluxStream => "Kryoflux Stream".to_string(),
            DiskImageFormat::MfmBitstreamImage => "HxC MFM Bitstream Image".to_string(),
            DiskImageFormat::HfeImage => "HFEv1 Bitstream Image".to_string(),
            DiskImageFormat::F86Image => "86F Bitstream Image".to_string(),
            DiskImageFormat::TransCopyImage => "TransCopy Bitstream Image".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum DiskFormat {
    Unknown,
    Nonstandard(DiskChs),
    Standard(StandardFormat),
}

#[derive(Default)]
pub(crate) struct SectorDescriptor {
    pub id: u8,
    pub cylinder_id: Option<u16>,
    pub head_id: Option<u8>,
    pub n: u8,
    pub data: Vec<u8>,
    pub weak: Option<Vec<u8>>,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub deleted_mark: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct SectorMapEntry {
    pub chsn: DiskChsn,
    pub address_crc_valid: bool,
    pub data_crc_valid: bool,
    pub deleted_mark: bool,
}

/// A DiskConsistency structure maintains information about the consistency of a disk image.
#[derive(Default)]
pub struct DiskConsistency {
    // A field to hold image format capability flags that this image requires in order to be represented.
    pub image_caps: u64,
    /// Whether the disk image contains weak bits.
    pub weak: bool,
    /// Whether the disk image contains deleted sectors.
    pub deleted: bool,
    /// Whether the disk image contains sectors with bad address mark CRCs
    pub bad_address_crc: bool,
    /// Whether the disk image contains sectors with bad data CRCs
    pub bad_data_crc: bool,
    /// Whether the disk image contains overlapped sectors
    pub overlapped: bool,
    /// The sector size if the disk image has consistent sector sizes, otherwise None.
    pub consistent_sector_size: Option<u32>,
    /// The track length in sectors if the disk image has consistent track lengths, otherwise None.
    pub consistent_track_length: Option<u8>,
}

/// Per-track format settings. In most cases, this will not change per-track. Some formats encode
/// this per-track, so we store it here.
pub struct TrackFormat {
    pub data_encoding: DiskDataEncoding,
    pub data_sync: Option<EncodingPhase>,
    pub data_rate: DiskDataRate,
}

pub struct TrackSectorIndex {
    pub sector_id: u8,
    pub cylinder_id: u16,
    pub head_id: u8,
    pub t_idx: usize,
    pub n: u8,
    pub len: usize,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub deleted_mark: bool,
}

/// A Disk Track is a circular region of the disk surface in which a number of sectors are stored.
/// Certain disk operations can be performed on an entire track, such as reading and formatting.
pub struct DiskTrack {
    /// A track comprises a vector of indices into the DiskImage sector pool.
    pub format: TrackFormat,
    pub data: TrackData,
}

impl DiskTrack {
    pub fn get_sector_count(&self) -> usize {
        self.data.get_sector_ct()
    }

    pub fn has_sector_id(&self, id: u8) -> bool {
        self.data.has_sector_id(id)
    }

    pub fn get_sector_list(&self) -> Vec<SectorMapEntry> {
        self.data.get_sector_list()
    }

    pub fn metadata(&self) -> Option<&DiskStructureMetadata> {
        self.data.metadata()
    }

    pub fn get_hash(&self) -> Digest {
        self.data.get_hash()
    }

    pub fn has_weak_bits(&self) -> bool {
        self.data.has_weak_bits()
    }
}

#[derive(Copy, Clone, Default)]
pub struct DiskDescriptor {
    /// The basic geometry of the disk. Not all tracks present need to conform to the specified sector count (s).
    pub geometry: DiskCh,
    /// The "default" sector size of the disk. Larger or smaller sectors may still be present in the disk image.
    pub default_sector_size: usize,
    /// The default data encoding used. The disk may still contain tracks in different encodings.
    pub data_encoding: DiskDataEncoding,
    /// The density of the disk
    pub density: DiskDensity,
    /// The data rate of the disk
    pub data_rate: DiskDataRate,
    /// The rotation rate of the disk. If not provided, this can be determined from other parameters.
    pub rpm: Option<DiskRpm>,
    /// Whether the disk image should be considered read-only (None if image did not define this flag)
    pub write_protect: Option<bool>,
}

#[derive(Copy, Clone, Debug)]
pub enum RwSectorScope {
    DataBlock,
    DataOnly,
}

#[derive(Clone)]
pub struct ReadSectorResult {
    pub data_idx: usize,
    pub data_len: usize,
    pub read_buf: Vec<u8>,
    pub deleted_mark: bool,
    pub not_found: bool,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub wrong_cylinder: bool,
    pub wrong_head: bool,
}

#[derive(Clone)]
pub struct ReadTrackResult {
    pub not_found: bool,
    pub sectors_read: u16,
    pub read_buf: Vec<u8>,
    pub deleted_mark: bool,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
}

#[derive(Clone)]
pub struct WriteSectorResult {
    pub not_found: bool,
    pub address_crc_error: bool,
    pub wrong_cylinder: bool,
    pub wrong_head: bool,
}

pub struct TrackRegion {
    pub start: usize,
    pub end: usize,
}

/// A DiskImage represents an image of a floppy disk in memory. It comprises a pool of sectors, and an ordered
/// list of tracks that reference sectors in the pool.
/// Sectors may be variable length due to various copy protection schemes.
#[derive(Default)]
pub struct DiskImage {
    // The standard format of the disk image, if it adheres to one. (Nonstandard images will be None)
    pub(crate) standard_format: Option<StandardFormat>,
    // The image format the disk image was sourced from, if any
    pub(crate) source_format: Option<DiskImageFormat>,
    // The data-level resolution of the disk image. Set on first write operation to an empty disk image.
    pub(crate) resolution: Option<DiskDataResolution>,
    // A DiskDescriptor describing this image with more thorough parameters.
    pub(crate) descriptor: DiskDescriptor,
    // A structure containing information about the disks internal consistency. Used to construct image_caps.
    pub(crate) consistency: DiskConsistency,
    // The boot sector of the disk image, if successfully parsed.
    pub(crate) boot_sector: Option<BootSector>,
    // The volume name of the disk image, if any.
    pub(crate) volume_name: Option<String>,
    // An ASCII comment embedded in the disk image, if any.
    pub(crate) comment: Option<String>,
    /// A pool of track data structures, potentially in any order.
    pub(crate) track_pool: Vec<DiskTrack>,
    /// An array of vectors containing indices into the track pool. The first index is the head
    /// number, the second is the cylinder number.
    pub(crate) track_map: [Vec<usize>; 2],
}

// impl Default for DiskImage {
//     fn default() -> Self {
//         Self {
//             standard_format: None,
//             descriptor: DiskDescriptor::default(),
//             source_format: None,
//             resolution: Default::default(),
//             consistency: Default::default(),
//             boot_sector: None,
//             volume_name: None,
//             comment: None,
//             track_pool: Vec::new(),
//             track_map: [Vec::new(), Vec::new()],
//             sector_map: [Vec::new(), Vec::new()],
//         }
//     }
// }

impl DiskImage {
    pub fn detect_format<RS: ReadSeek>(mut image: &mut RS) -> Result<DiskImageContainer, DiskImageError> {
        detect_image_format(&mut image)
    }

    pub fn new(disk_format: StandardFormat) -> Self {
        Self {
            standard_format: Some(disk_format),
            descriptor: disk_format.get_descriptor(),
            source_format: None,
            resolution: None,
            consistency: DiskConsistency {
                image_caps: 0,
                weak: false,
                deleted: false,
                bad_address_crc: false,
                bad_data_crc: false,
                overlapped: false,
                consistent_sector_size: Some(DEFAULT_SECTOR_SIZE as u32),
                consistent_track_length: Some(disk_format.get_chs().s()),
            },

            boot_sector: None,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
        }
    }

    pub fn load<RS: ReadSeek>(image_io: &mut RS) -> Result<Self, DiskImageError> {
        let container = DiskImage::detect_format(image_io)?;

        match container {
            DiskImageContainer::Raw(format) => {
                let mut image = format.load_image(image_io)?;
                image.post_load_process();
                Ok(image)
            }
            DiskImageContainer::Zip(format) => {
                #[cfg(feature = "zip")]
                {
                    let file_vec = extract_first_file(image_io)?;
                    let file_cursor = std::io::Cursor::new(file_vec);
                    let mut image = format.load_image(file_cursor)?;
                    image.post_load_process();
                    Ok(image)
                }
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat);
                }
            }
        }
    }

    pub fn set_volume_name(&mut self, name: String) {
        self.volume_name = Some(name);
    }

    pub fn volume_name(&self) -> Option<&str> {
        self.volume_name.as_deref()
    }

    pub fn set_data_rate(&mut self, rate: DiskDataRate) {
        self.descriptor.data_rate = rate;
    }

    pub fn data_rate(&self) -> DiskDataRate {
        self.descriptor.data_rate
    }

    pub fn set_data_encoding(&mut self, encoding: DiskDataEncoding) {
        self.descriptor.data_encoding = encoding;
    }

    pub fn data_encoding(&self) -> DiskDataEncoding {
        self.descriptor.data_encoding
    }

    pub fn set_image_format(&mut self, format: DiskDescriptor) {
        self.descriptor = format;
    }

    pub fn image_format(&self) -> DiskDescriptor {
        self.descriptor
    }

    pub fn geometry(&self) -> DiskCh {
        self.descriptor.geometry
    }

    pub fn heads(&self) -> u8 {
        self.descriptor.geometry.h()
    }

    pub fn tracks(&self) -> u16 {
        self.descriptor.geometry.c()
    }

    pub fn source_format(&self) -> Option<DiskImageFormat> {
        self.source_format
    }

    pub fn set_source_format(&mut self, format: DiskImageFormat) {
        self.source_format = Some(format);
    }

    /// Return the resolution of the disk image, either ByteStream or BitStream.
    pub fn resolution(&self) -> DiskDataResolution {
        self.resolution.unwrap_or(DiskDataResolution::ByteStream)
    }

    /// Adds a new track to the disk image, of ByteStream resolution.
    /// Data of this resolution is typically sourced from sector-based image formats.
    ///
    /// This function locks the disk image to `ByteStream` resolution and adds a new track with the
    /// specified data encoding, data rate, and geometry.
    ///
    /// # Parameters
    /// - `data_encoding`: The encoding used for the track data.
    /// - `data_rate`: The data rate of the track.
    /// - `ch`: The geometry of the track (cylinder and head).
    ///
    /// # Returns
    /// - `Ok(())` if the track was successfully added.
    /// - `Err(DiskImageError::SeekError)` if the head value in `ch` is greater than or equal to 2.
    /// - `Err(DiskImageError::IncompatibleImage)` if the disk image is not compatible with `ByteStream` resolution.
    pub fn add_track_bytestream(
        &mut self,
        data_encoding: DiskDataEncoding,
        data_rate: DiskDataRate,
        ch: DiskCh,
    ) -> Result<(), DiskImageError> {
        if ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        // Lock the disk image to ByteStream resolution.
        match self.resolution {
            None => self.resolution = Some(DiskDataResolution::ByteStream),
            Some(DiskDataResolution::ByteStream) => {}
            _ => return Err(DiskImageError::IncompatibleImage),
        }

        let format = TrackFormat {
            data_encoding,
            data_sync: None,
            data_rate,
        };
        //self.tracks[ch.h() as usize].push(DiskTrack {
        self.track_pool.push(DiskTrack {
            format,
            data: TrackData::ByteStream {
                cylinder: ch.c(),
                head: ch.h(),
                sectors: Vec::new(),
                data: Vec::new(),
                weak_mask: Vec::new(),
            },
        });
        self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);

        Ok(())
    }

    /// Adds a new track to the disk image, of BitStream resolution.
    /// Data of this resolution is sourced from BitStream images such as MFM, HFE or 86F.
    ///
    /// This function locks the disk image to `BitStream` resolution and adds a new track with the specified
    /// data encoding, data rate, geometry, and data clock.
    ///
    /// # Parameters
    /// - `data_encoding`: The encoding used for the track data.
    /// - `data_rate`: The data rate of the track.
    /// - `ch`: The geometry of the track (cylinder and head).
    /// - `data_clock`: The clock rate of the bit stream data.
    /// - `data`: A slice containing the bit stream data.
    /// - `weak`: An optional slice containing the weak bit mask.
    ///
    /// # Returns
    /// - `Ok(())` if the track was successfully added.
    /// - `Err(DiskImageError::SeekError)` if the head value in `ch` is greater than or equal to 2.
    /// - `Err(DiskImageError::ParameterError)` if the length of `data` and `weak` do not match.
    /// - `Err(DiskImageError::IncompatibleImage)` if the disk image is not compatible with `BitStream` resolution.
    pub fn add_track_bitstream(
        &mut self,
        data_encoding: DiskDataEncoding,
        data_rate: DiskDataRate,
        ch: DiskCh,
        data_clock: u32,
        bitcell_ct: Option<usize>,
        data: &[u8],
        weak: Option<&[u8]>,
    ) -> Result<(), DiskImageError> {
        if ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        if weak.is_some() && (data.len() != weak.unwrap().len()) {
            return Err(DiskImageError::ParameterError);
        }

        // Lock the disk image to BitStream resolution.
        match self.resolution {
            None => self.resolution = Some(DiskDataResolution::BitStream),
            Some(DiskDataResolution::BitStream) => {}
            _ => return Err(DiskImageError::IncompatibleImage),
        }

        let data = BitVec::from_bytes(data);
        let weak_bitvec_opt = weak.map(BitVec::from_bytes);

        log::trace!("add_track_bitstream(): Encoding is {:?}", data_encoding);
        let (mut data_stream, markers) = match data_encoding {
            DiskDataEncoding::Mfm => {
                let mut codec;

                // If a weak bit mask was provided by the file format, we will honor it.
                // Otherwise, we will try to detect weak bits from the MFM stream.
                if weak_bitvec_opt.is_some() {
                    codec = MfmCodec::new(data, bitcell_ct, weak_bitvec_opt);
                } else {
                    codec = MfmCodec::new(data, bitcell_ct, None);
                    // let weak_regions = codec.detect_weak_bits(9);
                    // log::trace!(
                    //     "add_track_bitstream(): Detected {} weak bit regions",
                    //     weak_regions.len()
                    // );
                    let weak_bitvec = codec.create_weak_bit_mask(MfmCodec::WEAK_BIT_RUN);
                    _ = codec.set_weak_mask(weak_bitvec);
                }

                let mut data_stream = TrackDataStream::Mfm(codec);
                let markers = System34Parser::scan_track_markers(&mut data_stream);

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut().unwrap());

                data_stream.set_track_padding();

                (data_stream, markers)
            }
            DiskDataEncoding::Fm => {
                // TODO: Handle FM encoding sync
                (TrackDataStream::Raw(RawCodec::new(data, weak_bitvec_opt)), Vec::new())
            }
            _ => (TrackDataStream::Raw(RawCodec::new(data, weak_bitvec_opt)), Vec::new()),
        };

        let format = TrackFormat {
            data_encoding,
            data_sync: data_stream.get_sync(),
            data_rate,
        };

        let metadata = DiskStructureMetadata::new(System34Parser::scan_track_metadata(&mut data_stream, markers));
        let sector_ids = metadata.get_sector_ids();
        if sector_ids.is_empty() {
            log::warn!(
                "add_track_bitstream(): No sectors ids found in track {} metadata.",
                ch.c()
            );
        }

        let sector_offsets = metadata
            .items
            .iter()
            .filter_map(|i| {
                if let DiskStructureElement::System34(System34Element::Data { .. }) = i.elem_type {
                    //log::trace!("Got Data element, returning start address: {}", i.start);
                    Some(i.start)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        log::trace!(
            "add_track_bitstream(): Retrieved {} sector bitstream offsets from metadata.",
            sector_offsets.len()
        );

        self.track_pool.push(DiskTrack {
            format,
            data: TrackData::BitStream {
                cylinder: ch.c(),
                head: ch.h(),
                data_clock,
                data: data_stream,
                metadata,
                sector_ids,
            },
        });
        self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);

        Ok(())
    }

    /// Masters a new sector to a track in the disk image, essentially 'formatting' a new sector,
    /// This function is only valid for tracks with `ByteStream` resolution.
    ///
    /// # Parameters
    /// - `chs`: The geometry of the sector (cylinder, head, and sector).
    /// - `sd`: A reference to a `SectorDescriptor` containing the sector data and metadata.
    ///
    /// # Returns
    /// - `Ok(())` if the sector was successfully mastered.
    /// - `Err(DiskImageError::SeekError)` if the head value in `chs` is greater than 1 or the track map does not contain the specified cylinder.
    /// - `Err(DiskImageError::UnsupportedFormat)` if the track data is not of `ByteStream` resolution.
    pub(crate) fn master_sector(&mut self, chs: DiskChs, sd: &SectorDescriptor) -> Result<(), DiskImageError> {
        if chs.h() > 1 || self.track_map[chs.h() as usize].len() < chs.c() as usize {
            return Err(DiskImageError::SeekError);
        }

        if !matches!(self.resolution, Some(DiskDataResolution::ByteStream)) {
            return Err(DiskImageError::UnsupportedFormat);
        }

        // Create an empty weak bit mask if none is provided.
        let weak_buf_vec = match &sd.weak {
            Some(weak_buf) => weak_buf.to_vec(),
            None => vec![0; sd.data.len()],
        };

        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &mut self.track_pool[ti];

        match track.data {
            TrackData::ByteStream {
                ref mut sectors,
                ref mut data,
                ref mut weak_mask,
                ..
            } => {
                sectors.push(TrackSectorIndex {
                    sector_id: sd.id,
                    cylinder_id: sd.cylinder_id.unwrap_or(chs.c()),
                    head_id: sd.head_id.unwrap_or(chs.h()),
                    n: sd.n,
                    t_idx: data.len(),
                    len: sd.data.len(),
                    address_crc_error: sd.address_crc_error,
                    data_crc_error: sd.data_crc_error,
                    deleted_mark: sd.deleted_mark,
                });
                data.extend(&sd.data);
                weak_mask.extend(weak_buf_vec);
            }
            TrackData::BitStream { .. } => {
                return Err(DiskImageError::UnsupportedFormat);
            }
        }

        Ok(())
    }

    // TODO: Fix this, it doesn't handle nonconsecutive sectors
    pub fn next_sector_on_track(&self, chs: DiskChs) -> Option<DiskChs> {
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];
        let s = track.get_sector_count();

        // Get the track geometry
        let geom_chs = DiskChs::from((self.geometry(), s as u8));
        let next_sector = geom_chs.get_next_sector(&geom_chs);

        // Return the next sector as long as it is on the same track.
        if next_sector.c() == chs.c() {
            Some(next_sector)
        } else {
            None
        }
    }

    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    pub fn read_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &mut self.track_pool[ti];

        track.data.read_sector(chs, n, scope, debug)
    }

    pub fn write_sector(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        data: &[u8],
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &mut self.track_pool[ti];

        log::trace!("TrackData::write_sector(): data len is now: {}", data.len());
        track.data.write_sector(chs, n, data, scope, debug)
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    pub fn read_all_sectors(&mut self, ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.data.read_all_sectors(ch, n, eot)
    }

    pub fn read_track(&mut self, ch: DiskCh) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.data.read_track(ch)
    }

    pub fn is_id_valid(&self, chs: DiskChs) -> bool {
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return false;
        }
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];

        match &track.data {
            TrackData::BitStream { .. } => return track.has_sector_id(chs.s()),
            TrackData::ByteStream { sectors, .. } => {
                for si in sectors {
                    if si.sector_id == chs.s() {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return None;
        }
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];

        track.data.get_next_id(chs)
    }

    pub(crate) fn read_boot_sector(&mut self) -> Result<Vec<u8>, DiskImageError> {
        if self.track_map.is_empty() || self.track_map[0].is_empty() {
            return Err(DiskImageError::IncompatibleImage);
        }
        let ti = self.track_map[0][0];
        let track = &mut self.track_pool[ti];

        match track
            .data
            .read_sector(DiskChs::new(0, 0, 1), None, RwSectorScope::DataOnly, true)
        {
            Ok(result) => Ok(result.read_buf),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn write_boot_sector(&mut self, buf: &[u8]) -> Result<(), DiskImageError> {
        self.write_sector(DiskChs::new(0, 0, 1), Some(2), buf, RwSectorScope::DataOnly, false)?;
        Ok(())
    }

    pub(crate) fn parse_boot_sector(&mut self, buf: &[u8]) -> Result<(), DiskImageError> {
        let mut cursor = Cursor::new(buf);
        let bpb = BootSector::new(&mut cursor)?;
        self.boot_sector = Some(bpb);
        Ok(())
    }

    pub fn update_standard_boot_sector(&mut self, format: StandardFormat) -> Result<(), DiskImageError> {
        if let Ok(buf) = &mut self.read_boot_sector() {
            if self.parse_boot_sector(buf).is_ok() {
                if let Some(bpb) = &mut self.boot_sector {
                    bpb.update_bpb_from_format(format)?;
                    let mut cursor = Cursor::new(buf);
                    bpb.write_bpb_to_buffer(&mut cursor)?;
                    self.write_boot_sector(cursor.into_inner())?;
                }
            } else {
                log::warn!("update_standard_boot_sector(): Failed to examine boot sector.");
            }
        }

        Ok(())
    }

    /// Called after loading a disk image to perform any post-load operations.
    pub(crate) fn post_load_process(&mut self) {
        // Normalize the disk image
        self.normalize();

        // Examine the boot sector if present. Use this to determine if this image is a standard
        // format disk image (but do not rely on this as the sole method of determining the disk
        // format)
        match self.read_boot_sector() {
            Ok(buf) => _ = self.parse_boot_sector(&buf),
            Err(e) => {
                log::error!("post_load_process(): Failed to read boot sector: {:?}", e);
            }
        }

        if let Some(boot_sector) = &self.boot_sector {
            if let Ok(format) = boot_sector.get_standard_format() {
                log::trace!(
                    "post_load_process(): Boot sector of standard format detected: {:?}",
                    format
                );

                if self.standard_format.is_none() {
                    self.standard_format = Some(format);
                } else if self.standard_format != Some(format) {
                    log::warn!("post_load_process(): Boot sector format does not match image format.");
                }
            }
        }
    }

    /// Retrieve the DOS boot sector of the disk image, if present.
    pub fn boot_sector(&self) -> Option<&BootSector> {
        self.boot_sector.as_ref()
    }

    pub fn get_track_ct(&self, head: usize) -> usize {
        self.track_map[head].len()
    }

    /// Normalize a disk image by detecting and correcting typical image issues.
    /// This includes:
    /// 40 track images encoded as 80 tracks with empty tracks
    /// Single-sided images encoded as double-sided images with empty tracks
    /// 40 track images encoded as 80 tracks with duplicate tracks
    pub(crate) fn normalize(&mut self) {
        // Detect empty tracks (can be created by HxC when exporting to IMD, etc.)
        let mut empty_tracks = Vec::new();

        let mut track_ct = 0;
        for (head_idx, head) in self.track_map.iter().enumerate() {
            for (track_idx, track) in head.iter().enumerate() {
                track_ct += 1;
                if self.track_pool[*track].get_sector_count() == 0 {
                    empty_tracks.push((head_idx, track_idx));
                }
            }
        }

        // Remove empty tracks
        log::trace!(
            "normalize(): Detected {}/{} empty tracks.",
            empty_tracks.len(),
            track_ct
        );
        if track_ct > 50 && empty_tracks.len() >= track_ct / 2 {
            log::warn!("normalize(): Image is wide track image stored as narrow tracks, odd tracks empty. Removing odd tracks.");
            self.remove_empty_tracks();
            self.descriptor.geometry.set_c(self.get_track_ct(0) as u16);
        }

        // Remove duplicate tracks (created by 86f, etc.)
        let duplicate_track_ct = self.detect_duplicate_tracks(0);
        log::trace!(
            "normalize(): Head {}: Detected {}/{} duplicate tracks.",
            0,
            duplicate_track_ct,
            self.track_map[0].len()
        );
        if self.track_map[0].len() > 50 && duplicate_track_ct >= self.track_map[0].len() / 2 {
            log::warn!(
                "normalize(): Image is wide track image stored as narrow tracks, odd tracks duplicated. Removing odd tracks."
            );
            self.remove_odd_tracks();
            self.descriptor.geometry.set_c(self.get_track_ct(0) as u16);
        }
    }

    pub(crate) fn detect_duplicate_tracks(&mut self, head: usize) -> usize {
        let mut duplicate_ct = 0;

        // Iterate through each pair of tracks and see if the 2nd track is a duplicate of the first.
        for track_pair in self.track_map[head].chunks_exact(2) {
            let track0_hash = self.track_pool[track_pair[0]].get_hash();
            let track1_hash = self.track_pool[track_pair[1]].get_hash();

            if track0_hash == track1_hash {
                duplicate_ct += 1;
            }
        }

        duplicate_ct
    }

    /// Remove all odd tracks from image. This is useful for handling images that store 40 track
    /// images as 80 tracks, with each track duplicated (86f)
    pub(crate) fn remove_odd_tracks(&mut self) {
        let mut odd_tracks = vec![Vec::new(); 2];

        for (head_idx, track_map) in self.track_map.iter().enumerate() {
            for (track_no, _track_idx) in track_map.iter().enumerate() {
                if track_no % 2 != 0 {
                    //log::warn!("odd track: c:{}, h:{}", track_no, head_idx);
                    odd_tracks[head_idx].push(track_no);
                }
            }
        }

        for (head_idx, tracks) in odd_tracks.iter_mut().enumerate() {
            tracks.sort_by(|a, b| b.cmp(a));
            for track_no in tracks {
                //log::warn!("removing track {}", track_no);
                self.track_map[head_idx].remove(*track_no);
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn remove_duplicate_tracks(&mut self) {
        let mut track_hashes: FoxHashMap<Digest, u32> = FoxHashMap::new();
        let mut duplicate_tracks = vec![Vec::new(); 2];

        for (head_idx, head) in self.track_map.iter().enumerate() {
            for (track_idx, track) in head.iter().enumerate() {
                let track_entry_opt = track_hashes.get(&self.track_pool[*track].get_hash());
                if track_entry_opt.is_some() {
                    duplicate_tracks[head_idx].push(track_idx);
                } else {
                    track_hashes.insert(self.track_pool[*track].get_hash(), 1);
                }
            }
        }

        log::trace!(
            "Head 0: Detected {}/{} duplicate tracks.",
            duplicate_tracks[0].len(),
            self.track_map[0].len()
        );

        log::trace!(
            "Head 1: Detected {}/{} duplicate tracks.",
            duplicate_tracks[1].len(),
            self.track_map[1].len()
        );

        for (head_idx, empty_head) in duplicate_tracks.iter_mut().enumerate() {
            empty_head.sort_by(|a, b| b.cmp(a));
            for track_idx in empty_head {
                //let pool_idx = self.track_map[head_idx][*track_idx];
                self.track_map[head_idx].remove(*track_idx);
            }
        }

        // Now we could remove the duplicate tracks from the track pool, but we'd have to re-index
        // every other track as the pool indices change. It's not that terrible to have deleted
        // tracks hanging out in memory. They will be removed when we re-export the image.
    }

    /// Remove empty tracks from the disk image. In some cases, 40 cylinder images are stored or
    /// encoded as 80 cylinders. These may either encode as empty or duplicate tracks. The former
    /// can be handled here by re-indexing the track map to remove the empty tracks.
    pub(crate) fn remove_empty_tracks(&mut self) {
        let mut empty_tracks = vec![Vec::new(); 2];
        for (head_idx, head) in self.track_map.iter().enumerate() {
            for (track_idx, track) in head.iter().enumerate() {
                if self.track_pool[*track].get_sector_count() == 0 {
                    empty_tracks[head_idx].push(track_idx);
                }
            }
        }

        let mut pool_indices = Vec::new();
        // Sort empty track indices in descending order and then remove them in said order from the
        // track map.
        for (head_idx, empty_head) in empty_tracks.iter_mut().enumerate() {
            empty_head.sort_by(|a, b| b.cmp(a));
            for track_idx in empty_head {
                let pool_idx = self.track_map[head_idx][*track_idx];
                pool_indices.push(pool_idx);
                self.track_map[head_idx].remove(*track_idx);
            }
        }

        // Now we could remove the empty tracks from the track pool, but we'd have to re-index
        // every other track as the pool indices change. It's not that terrible to have deleted
        // tracks hanging out in memory. They will be removed when we re-export the image.
    }

    pub fn dump_info<W: crate::io::Write>(&mut self, mut out: W) -> Result<(), crate::io::Error> {
        out.write_fmt(format_args!("Disk Format: {:?}\n", self.standard_format))?;
        out.write_fmt(format_args!("Geometry: {}\n", self.descriptor.geometry))?;
        out.write_fmt(format_args!("Volume Name: {:?}\n", self.volume_name))?;

        if let Some(comment) = &self.comment {
            out.write_fmt(format_args!("Comment: {:?}\n", comment))?;
        }

        out.write_fmt(format_args!("Data Rate: {}\n", self.descriptor.data_rate))?;
        out.write_fmt(format_args!("Data Encoding: {}\n", self.descriptor.data_encoding))?;
        Ok(())
    }

    pub fn get_sector_map(&self) -> Vec<Vec<Vec<SectorMapEntry>>> {
        let mut head_map = Vec::new();

        let geom = self.geometry();
        //log::trace!("get_sector_map(): Geometry is {}", geom);

        for head in 0..geom.h() {
            let mut track_map = Vec::new();

            for track_idx in &self.track_map[head as usize] {
                let track = &self.track_pool[*track_idx];
                track_map.push(track.get_sector_list());
            }

            head_map.push(track_map);
        }

        head_map
    }

    pub fn dump_sector_map<W: crate::io::Write>(&self, mut out: W) -> Result<(), crate::io::Error> {
        let head_map = self.get_sector_map();

        for (head_idx, head) in head_map.iter().enumerate() {
            out.write_fmt(format_args!("Head {}\n", head_idx))?;
            for (track_idx, track) in head.iter().enumerate() {
                out.write_fmt(format_args!("\tTrack {}\n", track_idx))?;
                for sector in track {
                    out.write_fmt(format_args!(
                        "\t\t{} address_crc_valid: {} data_crc_valid: {} deleted: {}\n",
                        sector.chsn, sector.address_crc_valid, sector.data_crc_valid, sector.deleted_mark
                    ))?;
                }
            }
        }

        Ok(())
    }

    pub fn dump_sector_hex<W: crate::io::Write>(
        &mut self,
        chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        bytes_per_row: usize,
        mut out: W,
    ) -> Result<(), DiskImageError> {
        let rsr = self.read_sector(chs, n, scope, true)?;

        let data_slice = match scope {
            RwSectorScope::DataOnly => &rsr.read_buf[rsr.data_idx..rsr.data_idx + rsr.data_len],
            RwSectorScope::DataBlock => &rsr.read_buf,
        };

        util::dump_slice(data_slice, 0, bytes_per_row, &mut out)
    }

    pub fn has_weak_bits(&self) -> bool {
        for track in &self.track_pool {
            if track.has_weak_bits() {
                return true;
            }
        }
        false
    }
}
