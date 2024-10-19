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

use crate::bitstream::fm::FmCodec;
use crate::bitstream::TrackDataStream;
use crate::boot_sector::BootSector;
use crate::chs::{DiskCh, DiskChs, DiskChsn};
use crate::containers::zip::extract_first_file;
use crate::containers::DiskImageContainer;
use crate::detect::detect_image_format;
use crate::file_parsers::kryoflux::KfxFormat;
use crate::file_parsers::{filter_writable, formats_from_caps, FormatCaps, ImageParser};
use crate::io::ReadSeek;
use crate::standard_format::StandardFormat;
use crate::structure_parsers::system34::{System34Element, System34Parser, System34Standard};
use crate::structure_parsers::{DiskStructureElement, DiskStructureMetadata, DiskStructureParser};
use crate::trackdata::TrackData;
use crate::{
    util, DiskDataEncoding, DiskDataRate, DiskDataResolution, DiskDensity, DiskImageError, DiskRpm, FoxHashMap,
    FoxHashSet, LoadingCallback, LoadingStatus, TrackConsistency,
};
use bit_vec::BitVec;
use bitflags::bitflags;
use sha1_smol::Digest;
use std::fmt::Display;
use std::io::Cursor;
use std::path::PathBuf;

pub const DEFAULT_BOOT_SECTOR: &[u8] = include_bytes!("../resources/bootsector.bin");

bitflags! {
    /// Bit flags that can be applied to a disk image.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[rustfmt::skip]
    pub struct DiskImageFlags: u32 {
        #[doc = "Disk Image source specified image is read-only"]
        const READONLY      = 0b0000_0000_0000_0001;
        #[doc = "Disk Image has been written to since last save"]
        const DIRTY         = 0b0000_0000_0000_0010;
        #[doc = "Disk Image represents a PROLOK protected disk"]
        const PROLOK        = 0b0000_0000_0000_0100;
    }
}

/// An enumeration describing the type of disk image.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum DiskImageFormat {
    RawSectorImage,
    ImageDisk,
    PceSectorImage,    // PSI
    PceBitstreamImage, // PRI
    PceFluxImage,      // PFI
    MfmBitstreamImage,
    TeleDisk,
    KryofluxStream,
    HfeImage,
    F86Image, // 86F
    TransCopyImage,
    SuperCardPro,
    MameFloppyImage,
}

impl DiskImageFormat {
    /// Return the priority of the disk image format. Higher values are higher priority.
    /// Used to sort returned lists of disk image formats, hopefully returning the most desirable
    /// format first.
    pub fn priority(self) -> usize {
        match self {
            DiskImageFormat::KryofluxStream => 0,
            // Supported bytestream formats (low priority)
            DiskImageFormat::RawSectorImage => 1,
            DiskImageFormat::TeleDisk => 0,
            DiskImageFormat::ImageDisk => 0,

            DiskImageFormat::PceSectorImage => 1,
            // Supported bitstream formats (high priority)
            DiskImageFormat::TransCopyImage => 0,
            DiskImageFormat::MfmBitstreamImage => 0,
            DiskImageFormat::HfeImage => 0,
            DiskImageFormat::PceBitstreamImage => 7,
            DiskImageFormat::F86Image => 8,
            // Flux images (not supported for writes)
            DiskImageFormat::SuperCardPro => 0,
            DiskImageFormat::PceFluxImage => 0,
            DiskImageFormat::MameFloppyImage => 0,
        }
    }

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
            DiskImageFormat::SuperCardPro => DiskDataResolution::FluxStream,
            DiskImageFormat::PceFluxImage => DiskDataResolution::FluxStream,
            DiskImageFormat::MameFloppyImage => DiskDataResolution::FluxStream,
        }
    }
}

impl Display for DiskImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFormat::RawSectorImage => "Raw Sector".to_string(),
            DiskImageFormat::PceSectorImage => "PCE Sector".to_string(),
            DiskImageFormat::PceBitstreamImage => "PCE Bitstream".to_string(),
            DiskImageFormat::ImageDisk => "ImageDisk Sector".to_string(),
            DiskImageFormat::TeleDisk => "TeleDisk Sector".to_string(),
            DiskImageFormat::KryofluxStream => "Kryoflux Flux Stream".to_string(),
            DiskImageFormat::MfmBitstreamImage => "HxC MFM Bitstream".to_string(),
            DiskImageFormat::HfeImage => "HFEv1 Bitstream".to_string(),
            DiskImageFormat::F86Image => "86F Bitstream".to_string(),
            DiskImageFormat::TransCopyImage => "TransCopy Bitstream".to_string(),
            DiskImageFormat::SuperCardPro => "SuperCard Pro Flux".to_string(),
            DiskImageFormat::PceFluxImage => "PCE Flux Stream".to_string(),
            DiskImageFormat::MameFloppyImage => "MAME Floppy Image".to_string(),
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
    pub no_dam: bool,
}

/// A DiskConsistency structure maintains information about the consistency of a disk image.
#[derive(Default)]
pub struct DiskConsistency {
    // A field to hold image format capability flags that this image requires in order to be represented.
    pub image_caps: FormatCaps,
    /// Whether the disk image contains weak bits.
    pub weak: bool,
    /// Whether the disk image contains deleted sectors.
    pub deleted_data: bool,
    /// Whether the disk image contains sector IDAMs with no corresponding DAMS.
    pub no_dam: bool,
    /// Whether the disk image contains sectors with bad address mark CRCs
    pub bad_address_crc: bool,
    /// Whether the disk image contains sectors with bad data CRCs
    pub bad_data_crc: bool,
    /// Whether the disk image contains overlapped sectors
    pub overlapped: bool,
    /// The sector size if the disk image has consistent sector sizes, otherwise None.
    pub consistent_sector_size: Option<u8>,
    /// The track length in sectors if the disk image has consistent track lengths, otherwise None.
    pub consistent_track_length: Option<u32>,
}

impl DiskConsistency {
    pub fn set_track_consistency(&mut self, track_consistency: &TrackConsistency) {
        self.deleted_data = track_consistency.deleted_data;
        self.bad_address_crc = track_consistency.bad_address_crc;
        self.bad_data_crc = track_consistency.bad_data_crc;
        self.no_dam = track_consistency.no_dam;

        if track_consistency.consistent_sector_size.is_none() {
            self.consistent_sector_size = None;
        }
    }
}

pub struct TrackSectorIndex {
    pub id_chsn: DiskChsn,
    pub t_idx: usize,
    pub len: usize,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub deleted_mark: bool,
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

#[derive(Debug, Default, Clone)]
pub struct ScanSectorResult {
    pub not_found: bool,
    pub no_dam: bool,
    pub deleted_mark: bool,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub wrong_cylinder: bool,
    pub bad_cylinder: bool,
    pub wrong_head: bool,
}

#[derive(Clone)]
pub struct ReadSectorResult {
    pub id_chsn: Option<DiskChsn>,
    pub not_found: bool,
    pub no_dam: bool,
    pub deleted_mark: bool,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub wrong_cylinder: bool,
    pub bad_cylinder: bool,
    pub wrong_head: bool,
    pub data_idx: usize,
    pub data_len: usize,
    pub read_buf: Vec<u8>,
}

#[derive(Clone)]
pub struct ReadTrackResult {
    pub not_found: bool,
    pub deleted_mark: bool,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub sectors_read: u16,
    pub read_buf: Vec<u8>,
}

#[derive(Clone)]
pub struct WriteSectorResult {
    pub not_found: bool,
    pub no_dam: bool,
    pub address_crc_error: bool,
    pub wrong_cylinder: bool,
    pub bad_cylinder: bool,
    pub wrong_head: bool,
}

pub struct TrackRegion {
    pub start: usize,
    pub end: usize,
}

/// A [`DiskImage`] represents the structure of a floppy disk. It contains a pool of track data
/// structures, which are indexed by a head vector which contains cylinder vectors.
///
/// A [`DiskImage`] can be created from a specified disk format using an ImageBuilder.
///
/// A [`DiskImage`] may be of two [`DiskDataResolution`] levels: ByteStream or BitStream. ByteStream images
/// are sourced from sector-based disk image formats, while BitStream images are sourced from
/// bitstream-based disk image formats.
#[derive(Default)]
pub struct DiskImage {
    // Flags that can be applied to a disk image.
    pub(crate) flags: DiskImageFlags,
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
    pub(crate) track_pool: Vec<TrackData>,
    /// An array of vectors containing indices into the track pool. The first index is the head
    /// number, the second is the cylinder number.
    pub(crate) track_map: [Vec<usize>; 2],
    /// The number of write operations (WriteData or FormatTrack) operations performed on the disk image.
    /// This can be used to determine if the disk image has been modified since the last save.
    pub(crate) writes: u64,
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

    /// Create a new [`DiskImage`] with the specified disk format. This function should not be called
    /// directly - use an [`ImageBuilder]` if you wish to create a new [`DiskImage`] from a specified format.
    pub fn create(disk_format: StandardFormat) -> Self {
        Self {
            flags: DiskImageFlags::empty(),
            standard_format: Some(disk_format),
            descriptor: disk_format.get_descriptor(),
            source_format: None,
            resolution: None,
            consistency: DiskConsistency {
                image_caps: Default::default(),
                weak: false,
                deleted_data: false,
                no_dam: false,
                bad_address_crc: false,
                bad_data_crc: false,
                overlapped: false,
                consistent_sector_size: Some(2),
                consistent_track_length: Some(disk_format.get_chs().s() as u32),
            },
            boot_sector: None,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            writes: 0,
        }
    }

    pub fn track_iter(&self) -> impl Iterator<Item = &TrackData> {
        // Find the maximum number of tracks among all heads
        let max_tracks = self.track_map.iter().map(|tracks| tracks.len()).max().unwrap_or(0);

        (0..max_tracks).flat_map(move |track_idx| {
            self.track_map.iter().filter_map(move |head_tracks| {
                head_tracks
                    .get(track_idx)
                    .and_then(move |&track_index| self.track_pool.get(track_index))
            })
        })
    }

    pub fn track_idx_iter(&self) -> impl Iterator<Item = usize> + '_ {
        // Find the maximum number of tracks among all heads
        let max_tracks = self.track_map.iter().map(|tracks| tracks.len()).max().unwrap_or(0);

        (0..max_tracks).flat_map(move |track_idx| {
            self.track_map
                .iter()
                .filter_map(move |head_tracks| head_tracks.get(track_idx).copied())
        })
    }

    pub fn track_ch_iter(&self) -> impl Iterator<Item = DiskCh> + '_ {
        self.track_idx_iter()
            .map(move |track_idx| self.track_pool[track_idx].ch())
    }

    pub fn track(&self, ch: DiskCh) -> Option<&TrackData> {
        self.track_map[ch.h() as usize]
            .get(ch.c() as usize)
            .and_then(|&track_idx| self.track_pool.get(track_idx))
    }

    pub fn track_by_idx(&self, track_idx: usize) -> Option<&TrackData> {
        self.track_pool.get(track_idx)
    }

    pub fn track_by_idx_mut(&mut self, track_idx: usize) -> Option<&mut TrackData> {
        self.track_pool.get_mut(track_idx)
    }

    pub fn set_resolution(&mut self, resolution: DiskDataResolution) {
        self.resolution = Some(resolution);
    }

    pub fn set_flag(&mut self, flag: DiskImageFlags) {
        self.flags |= flag;
    }

    pub fn clear_flag(&mut self, flag: DiskImageFlags) {
        self.flags &= !flag;
    }

    pub fn has_flag(&self, flag: DiskImageFlags) -> bool {
        self.flags.contains(flag)
    }

    pub fn required_caps(&self) -> FormatCaps {
        self.consistency.image_caps
    }

    pub fn load_from_file(file_path: PathBuf, callback: Option<LoadingCallback>) -> Result<Self, DiskImageError> {
        let mut file = std::fs::File::open(file_path.clone())?;
        let mut buf_reader = std::io::BufReader::new(&mut file);
        let image = DiskImage::load(&mut buf_reader, Some(file_path), callback)?;

        Ok(image)
    }

    pub fn load<RS: ReadSeek>(
        image_io: &mut RS,
        image_path: Option<PathBuf>,
        callback: Option<LoadingCallback>,
    ) -> Result<Self, DiskImageError> {
        let container = DiskImage::detect_format(image_io)?;

        match container {
            DiskImageContainer::Raw(format) => {
                let mut image = format.load_image(image_io, None, None)?;
                image.post_load_process();
                Ok(image)
            }
            DiskImageContainer::Zip(format) => {
                #[cfg(feature = "zip")]
                {
                    let file_vec = extract_first_file(image_io)?;
                    let file_cursor = Cursor::new(file_vec);
                    let mut image = format.load_image(file_cursor, None, callback)?;
                    image.post_load_process();
                    Ok(image)
                }
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat);
                }
            }
            DiskImageContainer::KryofluxSet => {
                if let Some(image_path) = image_path {
                    let (file_set, set_ch) = KfxFormat::expand_kryoflux_set(image_path, None)?;

                    log::debug!(
                        "load(): Expanded Kryoflux set to {} files, ch: {}",
                        file_set.len(),
                        set_ch
                    );

                    // Create an empty image. We will loop through all the files in the set and
                    // append tracks to them as we go.
                    let mut build_image = DiskImage::default();
                    // Set the geometry of the disk image to the geometry of the Kryoflux set.
                    build_image.descriptor.geometry = set_ch;

                    for (fi, file_path) in file_set.iter().enumerate() {
                        let mut file = std::fs::File::open(file_path.clone())?;
                        let mut buf_reader = std::io::BufReader::new(&mut file);
                        log::debug!("load(): Loading Kryoflux stream file: {:?}", file_path);

                        // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                        // updating percentage complete as a fraction of files loaded.
                        build_image = KfxFormat::load_image(&mut buf_reader, Some(build_image), None)?;

                        if let Some(ref callback_fn) = callback {
                            let completion = (fi + 1) as f64 / file_set.len() as f64;
                            callback_fn(LoadingStatus::Progress(completion));
                        }
                    }

                    //let ch = DiskCh::new(build_image.track_map[0].len() as u16, build_image.track_map.len() as u8);
                    //build_image.descriptor.geometry = ch;

                    if let Some(callback_fn) = callback {
                        callback_fn(LoadingStatus::Complete);
                    }
                    Ok(build_image)
                } else {
                    Err(DiskImageError::ParameterError)
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

    pub fn get_comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    pub fn set_comment(&mut self, comment: String) {
        self.comment = Some(comment);
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

    pub fn tracks(&self, head: u8) -> u16 {
        self.track_map[head as usize].len() as u16
    }

    pub fn write_ct(&self) -> u64 {
        //log::trace!("write_ct(): writes: {}", self.writes);
        self.writes
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
        encoding: DiskDataEncoding,
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

        //self.tracks[ch.h() as usize].push(DiskTrack {
        self.track_pool.push(TrackData::ByteStream {
            encoding,
            data_rate,
            cylinder: ch.c(),
            head: ch.h(),
            sectors: Vec::new(),
            data: Vec::new(),
            weak_mask: Vec::new(),
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
        encoding: DiskDataEncoding,
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
        if data.is_empty() {
            log::error!("add_track_bitstream(): Data is empty.");
            return Err(DiskImageError::ParameterError);
        }
        if weak.is_some() && (data.len() != weak.unwrap().len()) {
            log::error!("add_track_bitstream(): Data and weak bit mask lengths do not match.");
            return Err(DiskImageError::ParameterError);
        }

        // Lock the disk image to BitStream resolution.
        match self.resolution {
            None => {
                self.resolution = Some(DiskDataResolution::BitStream);
                log::debug!("add_track_bitstream(): Disk resolution is now: {:?}", self.resolution);
            }
            Some(DiskDataResolution::BitStream) => {}
            _ => return Err(DiskImageError::IncompatibleImage),
        }

        log::debug!(
            "add_track_bitstream(): adding {:?} track {}, {} bits",
            encoding,
            ch,
            bitcell_ct.unwrap_or(data.len() * 8)
        );

        let data = BitVec::from_bytes(data);
        let weak_bitvec_opt = weak.map(BitVec::from_bytes);

        let (mut data_stream, markers) = match encoding {
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
                    if weak_bitvec.any() {
                        log::debug!(
                            "add_track_bitstream(): Detected {} weak bits in MFM bitstream.",
                            weak_bitvec.count_ones()
                        );
                    }
                    _ = codec.set_weak_mask(weak_bitvec);
                }

                let mut data_stream: TrackDataStream = Box::new(codec);
                let markers = System34Parser::scan_track_markers(&data_stream);
                if !markers.is_empty() {
                    log::debug!("First marker found at {}", markers[0].start);
                }

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut());

                data_stream.set_track_padding();

                (data_stream, markers)
            }
            DiskDataEncoding::Fm => {
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
                    if weak_bitvec.any() {
                        log::trace!(
                            "add_track_bitstream(): Detected {} weak bits in FM bitstream.",
                            weak_bitvec.count_ones()
                        );
                    }
                    _ = codec.set_weak_mask(weak_bitvec);
                }

                let mut data_stream: TrackDataStream = Box::new(codec);
                let markers = System34Parser::scan_track_markers(&data_stream);

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut());

                data_stream.set_track_padding();

                (data_stream, markers)
            }
            _ => {
                log::error!("add_track_bitstream(): Unsupported data encoding: {:?}", encoding);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        // let format = TrackFormat {
        //     data_encoding,
        //     data_sync: data_stream.get_sync(),
        //     data_rate,
        // };

        let metadata = DiskStructureMetadata::new(System34Parser::scan_track_metadata(&mut data_stream, markers));
        let sector_ids = metadata.get_sector_ids();
        if sector_ids.is_empty() {
            log::warn!(
                "add_track_bitstream(): No sector ids found in track {} metadata.",
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

        log::debug!(
            "add_track_bitstream(): Retrieved {} sector bitstream offsets from {} metadata entries.",
            sector_offsets.len(),
            metadata.items.len()
        );

        self.track_pool.push(TrackData::BitStream {
            encoding,
            data_rate,
            cylinder: ch.c(),
            head: ch.h(),
            data_clock,
            data: data_stream,
            metadata,
            sector_ids,
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

        match track {
            TrackData::ByteStream {
                ref mut sectors,
                ref mut data,
                ref mut weak_mask,
                ..
            } => {
                let id_chsn = DiskChsn::from((
                    sd.cylinder_id.unwrap_or(chs.c()),
                    sd.head_id.unwrap_or(chs.h()),
                    sd.id,
                    sd.n,
                ));

                sectors.push(TrackSectorIndex {
                    id_chsn,
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
        let s = track.get_sector_ct();

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

    /// Read the sector data from the sector at the physical location 'phys_ch' with the sector ID
    /// values specified by 'id_chs'.
    /// The data is returned within a ReadSectorResult struct which also sets some convenience
    /// metadata flags which are needed when handling ByteStream images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    pub fn read_sector(
        &mut self,
        phys_ch: DiskCh,
        id_chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_sector(id_chs, n, scope, debug)
    }

    pub fn write_sector(
        &mut self,
        phys_ch: DiskCh,
        id_chs: DiskChs,
        n: Option<u8>,
        data: &[u8],
        scope: RwSectorScope,
        deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];

        self.writes = self.writes.wrapping_add(1);
        track.write_sector(id_chs, n, data, scope, deleted, debug)
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling ByteStream images.
    /// Unlike read_sector(), the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    pub fn read_all_sectors(
        &mut self,
        phys_ch: DiskCh,
        id_ch: DiskCh,
        n: u8,
        eot: u8,
    ) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_all_sectors(id_ch, n, eot)
    }

    pub fn read_track(&mut self, ch: DiskCh) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_track(ch)
    }

    pub fn add_empty_track(
        &mut self,
        ch: DiskCh,
        encoding: DiskDataEncoding,
        data_rate: DiskDataRate,
        bitcells: usize,
    ) -> Result<usize, DiskImageError> {
        if ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        let bitcell_bytes = (bitcells + 7) / 8;
        let new_track_index;

        match self.resolution {
            Some(DiskDataResolution::BitStream) => {
                if self.track_map[ch.h() as usize].len() != ch.c() as usize {
                    log::error!("add_empty_track(): Can't create sparse track map.");
                    return Err(DiskImageError::ParameterError);
                }

                let stream: TrackDataStream = match encoding {
                    DiskDataEncoding::Mfm => Box::new(MfmCodec::new(BitVec::from_elem(bitcells, false), None, None)),
                    DiskDataEncoding::Fm => Box::new(FmCodec::new(BitVec::from_elem(bitcells, false), None, None)),
                    _ => return Err(DiskImageError::UnsupportedFormat),
                };

                self.track_pool.push(TrackData::BitStream {
                    encoding,
                    data_rate,
                    cylinder: ch.c(),
                    head: ch.h(),
                    data_clock: 0,
                    data: stream,
                    metadata: DiskStructureMetadata::default(),
                    sector_ids: Vec::new(),
                });

                new_track_index = self.track_pool.len() - 1;
                self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);
            }
            Some(DiskDataResolution::ByteStream) => {
                if self.track_map[ch.h() as usize].len() != ch.c() as usize {
                    log::error!("add_empty_track(): Can't create sparse track map.");
                    return Err(DiskImageError::ParameterError);
                }

                self.track_pool.push(TrackData::ByteStream {
                    encoding,
                    data_rate,
                    cylinder: ch.c(),
                    head: ch.h(),
                    sectors: Vec::new(),
                    data: vec![0; bitcell_bytes],
                    weak_mask: Vec::new(),
                });

                new_track_index = self.track_pool.len() - 1;
                self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);
            }
            _ => {
                log::error!(
                    "add_empty_track(): Disk image resolution not set: {:?}",
                    self.resolution
                );
                return Err(DiskImageError::IncompatibleImage);
            }
        }

        Ok(new_track_index)
    }

    pub fn format_track(
        &mut self,
        ch: DiskCh,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        sector_gap: usize,
    ) -> Result<(), DiskImageError> {
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        // TODO: How would we support other structures here?
        track.format(System34Standard::Iso, format_buffer, fill_pattern, sector_gap)?;

        self.writes = self.writes.wrapping_add(1);
        // Formatting can change disk layout. Update image consistency to ensure export support
        // is accurate.
        self.update_consistency();
        Ok(())
    }

    pub fn is_id_valid(&self, chs: DiskChs) -> bool {
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return false;
        }
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];

        match &track {
            TrackData::BitStream { .. } => return track.has_sector_id(chs.s()),
            TrackData::ByteStream { sectors, .. } => {
                for si in sectors {
                    if si.id_chsn.s() == chs.s() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Reset an image to an empty state.
    pub fn reset_image(&mut self) {
        self.track_pool.clear();
        self.track_map = [Vec::new(), Vec::new()];

        *self = DiskImage {
            flags: DiskImageFlags::empty(),
            standard_format: self.standard_format,
            descriptor: self.descriptor,
            source_format: self.source_format,
            resolution: self.resolution,
            ..Default::default()
        }
    }

    pub fn format(
        &mut self,
        format: StandardFormat,
        boot_sector: Option<&[u8]>,
        creator: Option<&[u8; 8]>,
    ) -> Result<(), DiskImageError> {
        let chsn = format.get_chsn();
        let encoding = format.get_encoding();
        let data_rate = format.get_data_rate();
        let bitcell_size = format.get_bitcell_ct();

        // Drop all previous data as we will be overwriting the entire disk.
        self.reset_image();

        // Attempt to load the boot sector if provided, or fall back to our built-in default.
        let boot_sector_buf = boot_sector.unwrap_or(DEFAULT_BOOT_SECTOR);

        // Create a BootSector object from the buffer
        let mut bs_cursor = Cursor::new(boot_sector_buf);
        let mut bootsector = BootSector::new(&mut bs_cursor)?;

        // Update the boot sector with the disk format
        bootsector.update_bpb_from_format(format)?;
        if let Some(creator) = creator {
            bootsector.set_creator(creator)?;
        }

        // Repopulate the image with empty tracks.
        for head in 0..chsn.h() {
            for cylinder in 0..chsn.c() {
                let ch = DiskCh::new(cylinder, head);
                self.add_empty_track(ch, encoding, data_rate, bitcell_size)?;
            }
        }

        // Format each track with the specified format
        for head in 0..chsn.h() {
            for cylinder in 0..chsn.c() {
                let ch = DiskCh::new(cylinder, head);

                // Build the format buffer we provide to format_track() that specifies the sector
                // layout parameters.
                let mut format_buffer = Vec::new();
                for s in 0..chsn.s() {
                    format_buffer.push(DiskChsn::new(ch.c(), ch.h(), s + 1, chsn.n()));
                }

                let gap3 = format.get_gap3();
                self.format_track(ch, format_buffer, &[0x00], gap3)?;
            }
        }

        // Write the boot sector to the disk image
        self.write_boot_sector(bootsector.as_bytes())?;
        self.writes = self.writes.wrapping_add(1);
        Ok(())
    }

    pub fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
        if chs.h() > 1 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return None;
        }
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];

        track.get_next_id(chs)
    }

    pub(crate) fn read_boot_sector(&mut self) -> Result<Vec<u8>, DiskImageError> {
        if self.track_map.is_empty() || self.track_map[0].is_empty() {
            return Err(DiskImageError::IncompatibleImage);
        }
        let ti = self.track_map[0][0];
        let track = &mut self.track_pool[ti];

        match track.read_sector(DiskChs::new(0, 0, 1), None, RwSectorScope::DataOnly, true) {
            Ok(result) => Ok(result.read_buf),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn write_boot_sector(&mut self, buf: &[u8]) -> Result<(), DiskImageError> {
        self.write_sector(
            DiskCh::new(0, 0),
            DiskChs::new(0, 0, 1),
            Some(2),
            buf,
            RwSectorScope::DataOnly,
            false,
            false,
        )?;
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
        // Set writes to 1.
        self.writes = 1;

        // Normalize the disk image
        self.normalize();

        // Determine disk consistency
        self.update_consistency();

        // Examine the boot sector if present. Use this to determine if this image is a standard
        // format disk image (but do not rely on this as the sole method of determining the disk
        // format)
        match self.read_boot_sector() {
            Ok(buf) => _ = self.parse_boot_sector(&buf),
            Err(e) => {
                log::warn!("post_load_process(): Failed to read boot sector: {:?}", e);
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
        let track_ct = self.track_idx_iter().count();
        let empty_odd_track_ct = self.detect_empty_odd_tracks(0);
        let mut removed_odd = false;

        fn normalize_cylinders(c: usize) -> usize {
            if c > 80 {
                80
            } else if c > 40 {
                40
            } else {
                c
            }
        }

        // Remove empty tracks
        log::trace!(
            "normalize(): Detected {}/{} empty odd tracks.",
            empty_odd_track_ct,
            track_ct
        );

        if track_ct > 50 && empty_odd_track_ct >= normalize_cylinders(track_ct) / 2 {
            log::warn!("normalize(): Image is wide track image stored as narrow tracks, odd tracks empty. Removing odd tracks.");
            self.remove_odd_tracks();
            removed_odd = true;
            self.descriptor.geometry.set_c(self.get_track_ct(0) as u16);
        }

        if !removed_odd {
            // Remove duplicate tracks (created by 86f, etc.)
            let duplicate_track_ct = self.detect_duplicate_odd_tracks(0);
            log::trace!(
                "normalize(): Head {}: Detected {}/{} duplicate tracks.",
                0,
                duplicate_track_ct,
                self.track_map[0].len()
            );
            if self.track_map[0].len() > 50 && duplicate_track_ct >= normalize_cylinders(self.track_map[0].len()) / 2 {
                log::warn!(
                    "normalize(): Image is wide track image stored as narrow tracks, odd tracks duplicated. Removing odd tracks."
                );
                self.remove_odd_tracks();
                removed_odd = true;
                self.descriptor.geometry.set_c(self.get_track_ct(0) as u16);
            }
        }

        if removed_odd {
            // Renumber tracks.
            self.remap_tracks();
        }
    }

    /// Update a DiskImage's DiskConsistency struct to reflect the current state of the image.
    /// This function should be called after any changes to a track.
    /// update_consistency takes a list of track indices that have been modified.
    pub(crate) fn update_consistency(&mut self) {
        let mut spt: FoxHashSet<usize> = FoxHashSet::new();

        let mut all_consistency: TrackConsistency = Default::default();
        let mut variable_sector_size = false;
        let mut track_sector_ct = 0;

        let mut consistent_size_map: FoxHashSet<u8> = FoxHashSet::new();

        let mut last_track_sector_size = 0;

        for track_idx in self.track_idx_iter() {
            let td = &self.track_pool[track_idx];
            let track_consistency = td.get_track_consistency();

            match track_consistency.consistent_sector_size {
                None => {
                    variable_sector_size = true;
                }
                Some(size) if size > 0 => {
                    last_track_sector_size = size;
                    consistent_size_map.insert(size);
                }
                _ => {}
            }

            // Don't count tracks with no sectors for purposes of sectors-per-track consistency.
            // Empty sectors can be dropped in the output format.
            if track_consistency.sector_ct > 0 {
                spt.insert(track_consistency.sector_ct);
                track_sector_ct = track_consistency.sector_ct;
                all_consistency.sector_ct = track_consistency.sector_ct;
                all_consistency.join(&track_consistency);
            }
        }

        self.consistency.set_track_consistency(&all_consistency);
        if consistent_size_map.len() > 1 {
            self.consistency.consistent_sector_size = None;
        }

        if spt.len() > 1 {
            log::debug!(
                "update_consistency(): Inconsistent sector counts detected in tracks: {:?}",
                spt
            );
            self.consistency.consistent_track_length = None;
        } else {
            self.consistency.consistent_track_length = Some(all_consistency.sector_ct as u32);
        }

        if variable_sector_size {
            log::debug!("update_consistency(): Variable sector sizes detected in tracks.");
            self.consistency.consistent_sector_size = None;
        } else {
            self.consistency.consistent_sector_size = Some(last_track_sector_size);
        }

        let mut new_caps = self.consistency.image_caps;

        new_caps.set(
            FormatCaps::CAP_VARIABLE_SPT,
            self.consistency.consistent_track_length.is_none(),
        );
        new_caps.set(
            FormatCaps::CAP_VARIABLE_SSPT,
            self.consistency.consistent_sector_size.is_none(),
        );
        new_caps.set(FormatCaps::CAP_DATA_CRC, self.consistency.bad_data_crc);
        new_caps.set(FormatCaps::CAP_ADDRESS_CRC, self.consistency.bad_address_crc);
        new_caps.set(FormatCaps::CAP_DATA_DELETED, self.consistency.deleted_data);
        new_caps.set(FormatCaps::CAP_NO_DAM, self.consistency.no_dam);

        log::debug!("update_consistency(): Image capabilities: {:?}", new_caps);
        self.consistency.image_caps = new_caps;
    }

    /// Some formats may encode a 40-track image as an 80-track image with each track duplicated.
    /// (86F, primarily). This function detects such duplicates.
    ///
    /// We can't just detect any duplicate tracks, as it is possible for duplicate tracks to exist
    /// without all odd tracks being duplicates of the previous track.
    pub(crate) fn detect_duplicate_odd_tracks(&mut self, head: usize) -> usize {
        let mut duplicate_ct = 0;

        // Iterate through each pair of tracks and see if the 2nd track is a duplicate of the first.
        for track_pair in self.track_map[head].chunks_exact(2) {
            let track0_sectors = self.track_pool[track_pair[0]].get_sector_ct();
            let track0_hash = self.track_pool[track_pair[0]].get_hash();
            let track1_hash = self.track_pool[track_pair[1]].get_hash();

            // Only count a track as duplicate if the even track in the pair has any data
            if (track0_sectors > 0) && track0_hash == track1_hash {
                duplicate_ct += 1;
            }
        }

        duplicate_ct
    }

    /// Some formats may encode a 40-track image as an 80-track image with each track duplicated.
    /// (HxC exporting IMD, etc.) This function detects and removes the empty tracks inserted after
    /// each valid track.
    ///
    /// We can't just detect and remove all empty tracks as we would end up removing tracks present
    /// in a blank disk image.
    pub(crate) fn detect_empty_odd_tracks(&mut self, head: usize) -> usize {
        let mut empty_ct = 0;

        // Iterate through each pair of tracks and see if the 2nd track is empty when the first
        // track is not.
        for track_pair in self.track_map[head].chunks_exact(2) {
            let track0_sectors = self.track_pool[track_pair[0]].get_sector_ct();
            let track1_sectors = self.track_pool[track_pair[1]].get_sector_ct();

            if track0_sectors > 0 && track1_sectors == 0 {
                empty_ct += 1;
            }
        }

        empty_ct
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
                if self.track_pool[*track].get_sector_ct() == 0 {
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

    /// Remap tracks sequentially after an operation has removed some tracks.
    pub(crate) fn remap_tracks(&mut self) {
        let mut logical_cylinder;
        log::trace!("remap_tracks(): Disk geometry is {}", self.geometry());
        for (head_idx, head) in self.track_map.iter().enumerate() {
            logical_cylinder = 0;
            for track in head.iter() {
                match self.track_pool[*track] {
                    TrackData::ByteStream { ref mut cylinder, .. } => {
                        if *cylinder != logical_cylinder as u16 {
                            log::trace!(
                                "remap_tracks(): Remapping track idx {}, head: {} from c:{} to c:{}",
                                track,
                                head_idx,
                                *cylinder,
                                logical_cylinder
                            );
                        }
                        *cylinder = logical_cylinder as u16;
                    }
                    TrackData::BitStream { ref mut cylinder, .. } => {
                        if *cylinder != logical_cylinder as u16 {
                            log::trace!(
                                "remap_tracks(): Remapping track idx {}, head: {} from c:{} to c:{}",
                                track,
                                head_idx,
                                *cylinder,
                                logical_cylinder
                            );
                        }
                        *cylinder = logical_cylinder as u16;
                    }
                }
                logical_cylinder += 1;
            }
        }
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

    pub fn dump_consistency<W: crate::io::Write>(&mut self, mut out: W) -> Result<(), crate::io::Error> {
        if self.consistency.bad_data_crc {
            out.write_fmt(format_args!("Disk contains sectors with bad data CRCs\n"))?;
        } else {
            out.write_fmt(format_args!("No sectors on disk have bad data CRCs\n"))?;
        }

        if self.consistency.bad_address_crc {
            out.write_fmt(format_args!("Disk contains sectors with bad address CRCs\n"))?;
        } else {
            out.write_fmt(format_args!("No sectors on disk have bad address CRCs\n"))?;
        }

        if self.consistency.deleted_data {
            out.write_fmt(format_args!("Disk contains sectors marked as deleted\n"))?;
        } else {
            out.write_fmt(format_args!("No sectors on disk are marked as deleted\n"))?;
        }

        if self.consistency.overlapped {
            out.write_fmt(format_args!("Disk contains sectors with overlapping data\n"))?;
        } else {
            out.write_fmt(format_args!("No sectors on disk have overlapping data\n"))?;
        }

        if self.consistency.weak {
            out.write_fmt(format_args!("Disk contains tracks with weak bits\n"))?;
        } else {
            out.write_fmt(format_args!("No tracks on disk have weak bits\n"))?;
        }

        match self.consistency.consistent_track_length {
            Some(sector_ct) => {
                out.write_fmt(format_args!("All tracks on disk have {} sectors\n", sector_ct))?;
            }
            None => {
                out.write_fmt(format_args!("Disk contains tracks with variable sector counts\n"))?;
            }
        }

        match self.consistency.consistent_sector_size {
            Some(sector_size) => {
                out.write_fmt(format_args!(
                    "All sectors on disk have a consistent size of {}\n",
                    sector_size
                ))?;
            }
            None => {
                out.write_fmt(format_args!("Disk contains tracks with variable sector sizes\n"))?;
            }
        }

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
        phys_ch: DiskCh,
        id_chs: DiskChs,
        n: Option<u8>,
        scope: RwSectorScope,
        bytes_per_row: usize,
        mut out: W,
    ) -> Result<(), DiskImageError> {
        let rsr = self.read_sector(phys_ch, id_chs, n, scope, true)?;

        let data_slice = match scope {
            RwSectorScope::DataOnly => &rsr.read_buf[rsr.data_idx..rsr.data_idx + rsr.data_len],
            RwSectorScope::DataBlock => &rsr.read_buf,
        };

        util::dump_slice(data_slice, 0, bytes_per_row, &mut out)
    }

    pub fn has_weak_bits(&self) -> bool {
        for track in self.track_iter() {
            if track.has_weak_bits() {
                return true;
            }
        }
        false
    }

    /// Return a list of tuples representing the disk image formats that are compatible with the
    /// current image. This does not mean that fluxfox supports writing to these formats, only that
    /// the image is compatible with them.
    ///
    /// The tuple contains the DiskImageFormat and a list of strings representing the format's
    /// typical file extensions.
    ///
    /// Arguments:
    /// - `writable`: If true, only return formats that are writable.
    pub fn compatible_formats(&self, writable: bool) -> Vec<(DiskImageFormat, Vec<String>)> {
        let mut formats = formats_from_caps(self.consistency.image_caps);

        // Filter only writable formats if filtering is requested.
        if writable {
            let formats_alone: Vec<DiskImageFormat> = formats.iter().map(|f| f.0).collect();
            //log::debug!("compatible_formats(): got formats: {:?}", formats_alone);

            let filtered_formats = filter_writable(self, formats_alone);
            //log::debug!("compatible_formats(): filtered formats: {:?}", filtered_formats);
            formats.retain(|f| filtered_formats.contains(&f.0));
        }

        // Sort the formats by priority, highest first
        formats.sort_by(|a, b| b.0.priority().cmp(&a.0.priority()));

        formats
    }
}
