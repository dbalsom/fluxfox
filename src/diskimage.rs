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

//! The `diskimage` module defines the `DiskImage` struct which serves as the main
//! interface to fluxfox. A `DiskImage` represents the single disk image as read
//! from a disk image file, or created new as a specified format.

use crate::{bitstream::mfm::MfmCodec, track::bitstream::BitStreamTrack};

use std::{
    fmt::Display,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use bit_vec::BitVec;
use bitflags::bitflags;
use sha1_smol::Digest;

use crate::{
    bitstream::{fm::FmCodec, TrackDataStream},
    boot_sector::BootSector,
    chs::{DiskCh, DiskChs, DiskChsn},
    containers::{
        zip::{extract_file, extract_first_file},
        DiskImageContainer,
    },
    detect::detect_image_format,
    file_parsers::{filter_writable, formats_from_caps, kryoflux::KfxFormat, FormatCaps, ImageParser},
    io::ReadSeek,
    standard_format::StandardFormat,
    structure_parsers::{system34::System34Standard, DiskStructureMetadata},
    track::{fluxstream::FluxStreamTrack, metasector::MetaSectorTrack, DiskTrack, Track, TrackConsistency},
    util,
    DiskDataEncoding,
    DiskDataRate,
    DiskDataResolution,
    DiskDensity,
    DiskImageError,
    DiskRpm,
    FoxHashMap,
    FoxHashSet,
    LoadingCallback,
    LoadingStatus,
};

pub(crate) const DEFAULT_BOOT_SECTOR: &[u8] = include_bytes!("../resources/bootsector.bin");

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

/// A DiskSelection enumeration is used to select a disk image by either index or path when dealing
/// with containers that contain multiple disk images.
#[derive(Clone, Debug)]
pub enum DiskSelection {
    /// Specify a disk image by index into a list of normally sorted path names within the container.
    Index(usize),
    /// Specify a disk image by path within the container.
    Path(PathBuf),
}

impl Display for DiskSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskSelection::Index(idx) => write!(f, "(Index: {})", idx),
            DiskSelection::Path(path) => write!(f, "(Path: {})", path.display()),
        }
    }
}

/// `DiskImageFileFormat` is an enumeration listing the various disk image file formats that can be
/// read or written by FluxFox.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum DiskImageFileFormat {
    /// A raw sector image. Typically, has extensions IMG, IMA, DSK.
    RawSectorImage,
    /// An ImageDisk sector image. Typically has extension IMD.
    ImageDisk,
    /// A PCE sector image. Typically, has extension PSI.
    PceSectorImage,
    /// A PCE bitstream image. Typically, has extension PRI,
    PceBitstreamImage,
    /// A PCE flux stream image. Typically, has extension PFI.
    PceFluxImage,
    /// An MFM bitstream image. Typically, has extension MFM.
    MfmBitstreamImage,
    /// A TeleDisk sector image. Typically, has extension TD0.
    TeleDisk,
    /// A Kryoflux flux stream image. Typically, has extension RAW.
    KryofluxStream,
    /// An HFEv1 bitstream image. Typically, has extension HFE.
    HfeImage,
    /// An 86F bitstream image. Typically, has extension 86F.
    F86Image,
    /// A TransCopy bitstream image. Typically, has extension TC.
    TransCopyImage,
    /// A SuperCard Pro flux stream image. Typically, has extension SCP.
    SuperCardPro,
    /// A MAME floppy image. Typically, has extension MFI.
    MameFloppyImage,
}

impl DiskImageFileFormat {
    /// Return the priority of the disk image format. Higher values are higher priority.
    /// Used to sort returned lists of disk image formats, hopefully returning the most desirable
    /// format first.
    pub fn priority(self) -> usize {
        match self {
            DiskImageFileFormat::KryofluxStream => 0,
            // Supported bytestream formats (low priority)
            DiskImageFileFormat::RawSectorImage => 1,
            DiskImageFileFormat::TeleDisk => 0,
            DiskImageFileFormat::ImageDisk => 0,

            DiskImageFileFormat::PceSectorImage => 1,
            // Supported bitstream formats (high priority)
            DiskImageFileFormat::TransCopyImage => 0,
            DiskImageFileFormat::MfmBitstreamImage => 0,
            DiskImageFileFormat::HfeImage => 0,
            DiskImageFileFormat::PceBitstreamImage => 7,
            DiskImageFileFormat::F86Image => 8,
            // Flux images (not supported for writes)
            DiskImageFileFormat::SuperCardPro => 0,
            DiskImageFileFormat::PceFluxImage => 0,
            DiskImageFileFormat::MameFloppyImage => 0,
        }
    }

    pub fn resolution(self) -> DiskDataResolution {
        match self {
            DiskImageFileFormat::RawSectorImage => DiskDataResolution::MetaSector,
            DiskImageFileFormat::ImageDisk => DiskDataResolution::MetaSector,
            DiskImageFileFormat::PceSectorImage => DiskDataResolution::MetaSector,
            DiskImageFileFormat::PceBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::MfmBitstreamImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::TeleDisk => DiskDataResolution::MetaSector,
            DiskImageFileFormat::KryofluxStream => DiskDataResolution::FluxStream,
            DiskImageFileFormat::HfeImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::F86Image => DiskDataResolution::BitStream,
            DiskImageFileFormat::TransCopyImage => DiskDataResolution::BitStream,
            DiskImageFileFormat::SuperCardPro => DiskDataResolution::FluxStream,
            DiskImageFileFormat::PceFluxImage => DiskDataResolution::FluxStream,
            DiskImageFileFormat::MameFloppyImage => DiskDataResolution::FluxStream,
        }
    }
}

impl Display for DiskImageFileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFileFormat::RawSectorImage => "Raw Sector".to_string(),
            DiskImageFileFormat::PceSectorImage => "PCE Sector".to_string(),
            DiskImageFileFormat::PceBitstreamImage => "PCE Bitstream".to_string(),
            DiskImageFileFormat::ImageDisk => "ImageDisk Sector".to_string(),
            DiskImageFileFormat::TeleDisk => "TeleDisk Sector".to_string(),
            DiskImageFileFormat::KryofluxStream => "Kryoflux Flux Stream".to_string(),
            DiskImageFileFormat::MfmBitstreamImage => "HxC MFM Bitstream".to_string(),
            DiskImageFileFormat::HfeImage => "HFEv1 Bitstream".to_string(),
            DiskImageFileFormat::F86Image => "86F Bitstream".to_string(),
            DiskImageFileFormat::TransCopyImage => "TransCopy Bitstream".to_string(),
            DiskImageFileFormat::SuperCardPro => "SuperCard Pro Flux".to_string(),
            DiskImageFileFormat::PceFluxImage => "PCE Flux Stream".to_string(),
            DiskImageFileFormat::MameFloppyImage => "MAME Floppy Image".to_string(),
        };
        write!(f, "{}", str)
    }
}

/// A `DiskFormat` enumeration describes the format of a disk image.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum DiskFormat {
    /// An unknown format. This is the default format for a disk image before a disk's format can
    /// be determined.
    Unknown,
    /// A non-standard disk format. This format is used for disk images that do not conform to a
    /// standard format, such a copy-protected titles that may have varying track lengths,
    /// non-consecutive sectors, or other non-standard features.
    Nonstandard(DiskChs),
    /// A standard disk format. This format is used for disk images that conform to a standard
    /// IBM PC format type, determined by a `StandardFormat` enum.
    Standard(StandardFormat),
}

/// A structure used to describe the parameters of a sector to be created on a `MetaSector`
/// resolution track.
#[derive(Default)]
pub struct SectorDescriptor {
    pub id_chsn: DiskChsn,
    pub data: Vec<u8>,
    pub weak_mask: Option<Vec<u8>>,
    pub hole_mask: Option<Vec<u8>>,
    pub address_crc_error: bool,
    pub data_crc_error: bool,
    pub deleted_mark: bool,
    pub missing_data: bool,
}

/// A structure to uniquely identify a specific sector on a track.
#[derive(Copy, Clone, Debug, Default)]
pub struct SectorCursor {
    /// The sector id. Either a `sector_idx` or `bit_offset` is required to discriminate between
    /// sectors with the same ID.
    pub id_chsn: DiskChsn,
    /// The physical sector index within the track, starting at 0.
    pub sector_idx: Option<usize>,
    /// The bit offset of the start of the sector header element.
    pub header_offset: Option<usize>,
    /// The bit offset of the start of the sector data element.
    pub data_offset: Option<usize>,
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

/// A `DiskDescriptor` structure describes the basic geometry and parameters of a disk image.
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

/// An enum that defines the scope of a sector operation.
#[derive(Copy, Clone, Debug)]
pub enum RwSectorScope {
    /// The operation will include the entire data element, including address marker and CRC bytes.
    DataElement,
    /// The operation will include only the sector data, excluding address marker and CRC bytes.
    DataOnly,
}

/// A `ScanSectorResult` structure contains the results of a scan sector operation.
#[derive(Debug, Default, Clone)]
pub struct ScanSectorResult {
    /// Whether the specified Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector was marked deleted.
    pub deleted_mark: bool,
    /// Whether the specified sector had a CRC error with the sector header.
    pub address_crc_error: bool,
    /// Whether the specified sector had a CRC error with the sector data.
    pub data_crc_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
}

/// A `ReadSectorResult` structure contains the results of a read sector operation.
#[derive(Clone)]
pub struct ReadSectorResult {
    /// The matching Sector ID as `DiskChsn`, or `None`.
    pub id_chsn: Option<DiskChsn>,
    /// Whether the specified Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector was marked deleted.
    pub deleted_mark: bool,
    /// Whether the specified sector had a CRC error with the sector header.
    pub address_crc_error: bool,
    /// Whether the specified sector had a CRC error with the sector data.
    pub data_crc_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
    /// The index of the start of sector data within `read_buf`.
    pub data_idx: usize,
    /// The length of sector data, starting from `data_idx`, within `read_buf`.
    pub data_len: usize,
    /// The data read for the sector, potentially including address mark and CRC bytes.
    /// Use the `data_idx` and `data_len` fields to isolate the sector data within this vector.
    pub read_buf: Vec<u8>,
}

/// A `ReadTrackResult` structure contains the results of a read track operation.
#[derive(Clone)]
pub struct ReadTrackResult {
    /// Whether no sectors were found reading the track.
    pub not_found: bool,
    /// Whether the track contained at least one sector with a deleted data mark.
    pub deleted_mark: bool,
    /// Whether the track contained at least one sector with a CRC error in the address mark.
    pub address_crc_error: bool,
    /// Whether the track contained at least one sector with a CRC error in the data.
    pub data_crc_error: bool,
    /// The total number of sectors read from the track.
    pub sectors_read: u16,
    /// The data read for the track.
    pub read_buf: Vec<u8>,
    /// The total number of bits read.
    pub read_len_bits: usize,
    /// The total number of bytes read.
    pub read_len_bytes: usize,
}

/// A `WriteSectorResult` structure contains the results of a write sector operation.
#[derive(Clone)]
pub struct WriteSectorResult {
    /// Whether a matching Sector ID was found.
    pub not_found: bool,
    /// Whether the specified Sector ID was found, but no corresponding sector data was found.
    pub no_dam: bool,
    /// Whether the specific sector header matching the Sector ID had a bad CRC.
    /// In this case, the write operation will have failed.
    pub address_crc_error: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub wrong_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a bad cylinder
    /// specifier was found.
    pub bad_cylinder: bool,
    /// Whether the specified sector ID was not matched, but a sector ID with a different head
    /// specifier was found.
    pub wrong_head: bool,
}

pub struct TrackRegion {
    pub start: usize,
    pub end:   usize,
}

pub struct BitStreamTrackParams<'a> {
    pub encoding: DiskDataEncoding,
    pub data_rate: DiskDataRate,
    pub rpm: Option<DiskRpm>,
    pub ch: DiskCh,
    pub bitcell_ct: Option<usize>,
    pub data: &'a [u8],
    pub weak: Option<&'a [u8]>,
    pub hole: Option<&'a [u8]>,
    pub detect_weak: bool,
}

#[derive(Default)]
pub(crate) struct SharedDiskContext {
    /// The number of write operations (WriteData or FormatTrack) operations performed on the disk image.
    /// This can be used to determine if the disk image has been modified since the last save.
    pub(crate) writes: u64,
}

/// A [`DiskImage`] represents the structure of a floppy disk. It contains a pool of track data
/// structures, which are indexed by a head vector which contains cylinder vectors.
///
/// A [`DiskImage`] can be created from a specified disk format using an ImageBuilder.
///
/// A [`DiskImage`] may be of two [`DiskDataResolution`] levels: ByteStream or BitStream. ByteStream images
/// are sourced from sector-based disk image formats, while BitStream images are sourced from
/// bitstream-based disk image formats.
pub struct DiskImage {
    // Flags that can be applied to a disk image.
    pub(crate) flags: DiskImageFlags,
    // The standard format of the disk image, if it adheres to one. (Nonstandard images will be None)
    pub(crate) standard_format: Option<StandardFormat>,
    // The image format the disk image was sourced from, if any
    pub(crate) source_format: Option<DiskImageFileFormat>,
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
    /// A shared context for the disk image, accessible by Tracks.
    pub(crate) shared: Arc<Mutex<SharedDiskContext>>,
}

impl Default for DiskImage {
    fn default() -> Self {
        Self {
            flags: DiskImageFlags::empty(),
            standard_format: None,
            source_format: None,
            resolution: Default::default(),
            descriptor: DiskDescriptor::default(),
            consistency: Default::default(),
            boot_sector: None,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            shared: Arc::new(Mutex::new(SharedDiskContext::default())),
        }
    }
}

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
            shared: Arc::new(Mutex::new(SharedDiskContext::default())),
        }
    }

    pub fn track_iter(&self) -> impl Iterator<Item = &DiskTrack> {
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

    pub fn track(&self, ch: DiskCh) -> Option<&DiskTrack> {
        self.track_map[ch.h() as usize]
            .get(ch.c() as usize)
            .and_then(|&track_idx| self.track_pool.get(track_idx))
    }

    pub fn track_mut(&mut self, ch: DiskCh) -> Option<&mut DiskTrack> {
        self.track_map[ch.h() as usize]
            .get(ch.c() as usize)
            .and_then(|&track_idx| self.track_pool.get_mut(track_idx))
    }

    pub fn track_by_idx(&self, track_idx: usize) -> Option<&DiskTrack> {
        self.track_pool.get(track_idx)
    }

    pub fn track_by_idx_mut(&mut self, track_idx: usize) -> Option<&mut DiskTrack> {
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

    pub fn load_from_file(
        file_path: PathBuf,
        disk_selection: Option<DiskSelection>,
        callback: Option<LoadingCallback>,
    ) -> Result<Self, DiskImageError> {
        let mut file_vec = std::fs::read(file_path.clone())?;
        let mut cursor = Cursor::new(&mut file_vec);
        let image = DiskImage::load(&mut cursor, Some(file_path), disk_selection, callback)?;

        Ok(image)
    }

    pub fn load<RS: ReadSeek>(
        image_io: &mut RS,
        image_path: Option<PathBuf>,
        disk_selection: Option<DiskSelection>,
        callback: Option<LoadingCallback>,
    ) -> Result<Self, DiskImageError> {
        let container = DiskImage::detect_format(image_io)?;

        match container {
            DiskImageContainer::Raw(format) => {
                let mut image = DiskImage::default();
                format.load_image(image_io, &mut image, None)?;
                image.post_load_process();
                Ok(image)
            }
            DiskImageContainer::Zip(format) => {
                #[cfg(feature = "zip")]
                {
                    let file_vec = extract_first_file(image_io)?;
                    let file_cursor = Cursor::new(file_vec);
                    let mut image = DiskImage::default();
                    format.load_image(file_cursor, &mut image, callback)?;
                    image.post_load_process();
                    Ok(image)
                }
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat);
                }
            }
            DiskImageContainer::ZippedKryofluxSet(disks) => {
                #[cfg(feature = "zip")]
                {
                    let disk_opt = match disk_selection {
                        Some(DiskSelection::Index(idx)) => disks.get(idx),
                        Some(DiskSelection::Path(ref path)) => disks.iter().find(|disk| disk.base_path == *path),
                        _ => {
                            if disks.len() == 1 {
                                disks.get(0)
                            }
                            else {
                                log::error!("Multiple disks found in Kryoflux set without a selection.");
                                return Err(DiskImageError::MultiDiskError(
                                    "No disk selection provided.".to_string(),
                                ));
                            }
                        }
                    };

                    if let Some(disk) = disk_opt {
                        // Create an empty image. We will loop through all the files in the set and
                        // append tracks to them as we go.
                        let mut image = DiskImage::default();
                        image.descriptor.geometry = disk.geometry;

                        if let Some(ref callback_fn) = callback {
                            // Let caller know to show a progress bar
                            callback_fn(LoadingStatus::ProgressSupport(true));
                        }

                        for (fi, file_path) in disk.file_set.iter().enumerate() {
                            let mut file_vec = extract_file(image_io, &file_path.clone())?;
                            let mut cursor = Cursor::new(&mut file_vec);
                            log::debug!("load(): Loading Kryoflux stream file from zip: {:?}", file_path);

                            // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                            // updating percentage complete as a fraction of files loaded.
                            match KfxFormat::load_image(&mut cursor, &mut image, None) {
                                Ok(_) => {}
                                Err(e) => {
                                    // It's okay to fail if we have already added the standard number of tracks to an image.
                                    log::error!("load(): Error loading Kryoflux stream file: {:?}", e);
                                    //return Err(e);
                                    break;
                                }
                            }

                            if let Some(ref callback_fn) = callback {
                                let completion = (fi + 1) as f64 / disk.file_set.len() as f64;
                                callback_fn(LoadingStatus::Progress(completion));
                            }
                        }

                        if let Some(callback_fn) = callback {
                            callback_fn(LoadingStatus::Complete);
                        }

                        image.post_load_process();
                        Ok(image)
                    }
                    else {
                        log::error!(
                            "Disk selection {} not found in Kryoflux set.",
                            disk_selection.clone().unwrap()
                        );
                        Err(DiskImageError::MultiDiskError(format!(
                            "Disk selection {} not found in set.",
                            disk_selection.unwrap()
                        )))
                    }
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
                    let mut image = DiskImage::default();
                    // Set the geometry of the disk image to the geometry of the Kryoflux set.
                    image.descriptor.geometry = set_ch;

                    for (fi, file_path) in file_set.iter().enumerate() {
                        // Reading the entire file in one go and wrapping in a cursor is much faster
                        // than a BufReader.
                        let mut file_vec = std::fs::read(file_path.clone())?;
                        let mut cursor = Cursor::new(&mut file_vec);

                        log::debug!("load(): Loading Kryoflux stream file: {:?}", file_path);

                        // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                        // updating percentage complete as a fraction of files loaded.
                        match KfxFormat::load_image(&mut cursor, &mut image, None) {
                            Ok(_) => {}
                            Err(e) => {
                                // It's okay to fail if we have already added the standard number of tracks to an image.
                                log::error!("load(): Error loading Kryoflux stream file: {:?}", e);
                                //return Err(e);
                                break;
                            }
                        }

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
                    image.post_load_process();
                    Ok(image)
                }
                else {
                    log::error!("Path parameter required when loading Kryoflux set.");
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
        self.shared.lock().unwrap().writes
    }

    pub fn source_format(&self) -> Option<DiskImageFileFormat> {
        self.source_format
    }

    pub fn set_source_format(&mut self, format: DiskImageFileFormat) {
        self.source_format = Some(format);
    }

    /// Return the resolution of the disk image, either ByteStream or BitStream.
    pub fn resolution(&self) -> DiskDataResolution {
        self.resolution.unwrap_or(DiskDataResolution::MetaSector)
    }

    /// Adds a new track to the disk image, of FluxStream resolution.
    /// Data of this resolution is sourced from FluxStream images such as Kryoflux, SCP or MFI.
    ///
    /// This function locks the disk image to `FluxStream` resolution and adds a new track with the specified
    /// data encoding, data rate, geometry, and data clock.
    ///
    /// # Parameters
    /// - `track`: A `FluxStreamTrack` the track to add. Unlike adding a bitstream or metasector track,
    ///            we must construct a FluxStreamTrack first before adding it.
    ///
    /// # Returns
    /// - `Ok(())` if the track was successfully added.
    /// - `Err(DiskImageError::SeekError)` if the head value in `ch` is greater than or equal to 2.
    /// - `Err(DiskImageError::ParameterError)` if the length of `data` and `weak` do not match.
    /// - `Err(DiskImageError::IncompatibleImage)` if the disk image is not compatible with `BitStream` resolution.
    pub fn add_track_fluxstream(
        &mut self,
        ch: DiskCh,
        mut track: FluxStreamTrack,
        clock_hint: Option<f64>,
        rpm_hint: Option<DiskRpm>,
    ) -> Result<&DiskTrack, DiskImageError> {
        let head = ch.h() as usize;
        if head >= 2 {
            return Err(DiskImageError::SeekError);
        }

        // Lock the disk image to BitStream resolution.
        match self.resolution {
            None => {
                self.resolution = Some(DiskDataResolution::FluxStream);
                log::debug!("add_track_fluxstream(): Disk resolution is now: {:?}", self.resolution);
            }
            Some(DiskDataResolution::FluxStream) => {}
            _ => {
                return {
                    log::error!(
                        "add_track_fluxstream(): Disk resolution is incompatible with FluxStream: {:?}",
                        self.resolution
                    );
                    Err(DiskImageError::IncompatibleImage)
                }
            }
        }

        track.set_ch(ch);
        track.set_shared(self.shared.clone());
        track.synthesize_revolutions(); // Create synthetic revolutions to increase chances of successful decoding.
        track.decode_revolutions(clock_hint, rpm_hint)?;
        track.analyze_revolutions();

        log::debug!(
            "add_track_fluxstream(): adding {:?} track {}",
            track.encoding(),
            track.ch(),
        );

        self.track_pool.push(Box::new(track));
        self.track_map[head].push(self.track_pool.len() - 1);

        // Consider adding a track to an image to be a single 'write' operation.
        self.incr_writes();

        Ok(self.track_pool.last().unwrap())
    }

    /// Adds a new track to the disk image, of BitStream resolution.
    /// Data of this resolution is sourced from BitStream images such as MFM, HFE or 86F.
    ///
    /// This function locks the disk image to `BitStream` resolution and adds a new track with the specified
    /// data encoding, data rate, geometry, and data clock.
    ///
    /// # Parameters
    /// - `params`: A `BitstreamTrackParams` struct describing the track to be added.
    ///
    /// # Returns
    /// - `Ok(())` if the track was successfully added.
    /// - `Err(DiskImageError::SeekError)` if the head value in `ch` is greater than or equal to 2.
    /// - `Err(DiskImageError::ParameterError)` if the length of `data` and `weak` do not match.
    /// - `Err(DiskImageError::IncompatibleImage)` if the disk image is not compatible with `BitStream` resolution.
    pub fn add_track_bitstream(&mut self, params: BitStreamTrackParams) -> Result<(), DiskImageError> {
        if params.ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
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
            params.encoding,
            params.ch,
            params.bitcell_ct.unwrap_or(params.data.len() * 8)
        );

        let head = params.ch.h() as usize;
        let new_track = BitStreamTrack::new(params, self.shared.clone())?;

        self.track_pool.push(Box::new(new_track));
        self.track_map[head].push(self.track_pool.len() - 1);

        // Consider adding a track to an image to be a single 'write' operation.
        self.incr_writes();

        Ok(())
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
    pub fn add_track_metasector(
        &mut self,
        encoding: DiskDataEncoding,
        data_rate: DiskDataRate,
        ch: DiskCh,
    ) -> Result<&mut DiskTrack, DiskImageError> {
        if ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        // Lock the disk image to ByteStream resolution.
        match self.resolution {
            None => self.resolution = Some(DiskDataResolution::MetaSector),
            Some(DiskDataResolution::MetaSector) => {}
            _ => return Err(DiskImageError::IncompatibleImage),
        }

        self.track_pool.push(Box::new(MetaSectorTrack {
            ch,
            encoding,
            data_rate,
            sectors: Vec::new(),
            shared: self.shared.clone(),
        }));
        self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);

        // Consider adding a track to an image to be a single 'write' operation.
        self.incr_writes();

        Ok(self.track_pool.last_mut().unwrap())
    }

    // TODO: Fix this, it doesn't handle nonconsecutive sectors
    #[allow(deprecated)]
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
        }
        else {
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

    /// Read the track specified by `ch`, decoding data. The data is returned within a
    /// ReadTrackResult struct, which crucially contains the exact length of the track data in bits.
    ///
    /// # Parameters
    /// - `ch`: The cylinder and head of the track to read.
    /// - `overdump`: An optional parameter to specify the number of bytes to read past the end of
    ///               the track. This is useful for examining track wrapping behavior.
    pub fn read_track(&mut self, ch: DiskCh, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_track(overdump)
    }

    /// Read the track specified by `ch`, without decoding. The data is returned within a
    /// ReadTrackResult struct, which crucially contains the exact length of the track data in bits.
    ///
    /// # Parameters
    /// - `ch`: The cylinder and head of the track to read.
    /// - `overdump`: An optional parameter to specify the number of bytes to read past the end of
    ///               the track. This is useful for examining track wrapping behavior.
    pub fn read_track_raw(&mut self, ch: DiskCh, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if ch.h() > 1 || ch.c() as usize >= self.track_map[ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[ch.h() as usize][ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_track_raw(overdump)
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

                self.track_pool.push(Box::new(BitStreamTrack {
                    encoding,
                    data_rate,
                    rpm: self.descriptor.rpm,
                    ch,
                    data: stream,
                    metadata: DiskStructureMetadata::default(),
                    sector_ids: Vec::new(),
                    shared: self.shared.clone(),
                }));

                new_track_index = self.track_pool.len() - 1;
                self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);
            }
            Some(DiskDataResolution::MetaSector) => {
                if self.track_map[ch.h() as usize].len() != ch.c() as usize {
                    log::error!("add_empty_track(): Can't create sparse track map.");
                    return Err(DiskImageError::ParameterError);
                }

                self.track_pool.push(Box::new(MetaSectorTrack {
                    encoding,
                    data_rate,
                    ch,
                    sectors: Vec::new(),
                    shared: self.shared.clone(),
                }));

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

        // Formatting can change disk layout. Update image consistency to ensure export support
        // is accurate.
        self.update_consistency();
        Ok(())
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
            }
            else {
                log::warn!("update_standard_boot_sector(): Failed to examine boot sector.");
            }
        }

        Ok(())
    }

    /// Called after loading a disk image to perform any post-load operations.
    pub(crate) fn post_load_process(&mut self) {
        // Set writes to 1.
        self.shared.lock().unwrap().writes = 1;

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
                }
                else if self.standard_format != Some(format) {
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
            }
            else if c > 40 {
                40
            }
            else {
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

        let mut consistent_size_map: FoxHashSet<u8> = FoxHashSet::new();

        let mut last_track_sector_size = 0;

        log::debug!("update_consistency(): Running consistency check...");
        for track_idx in self.track_idx_iter() {
            let td = &self.track_pool[track_idx];
            match td.get_track_consistency() {
                Ok(track_consistency) => {
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
                        all_consistency.sector_ct = track_consistency.sector_ct;
                        all_consistency.join(&track_consistency);
                    }
                }
                Err(_) => {
                    log::warn!("update_consistency(): Track {} has no consistency data.", track_idx);
                    continue;
                }
            };
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
        }
        else {
            self.consistency.consistent_track_length = Some(all_consistency.sector_ct as u32);
        }

        if variable_sector_size {
            log::debug!("update_consistency(): Variable sector sizes detected in tracks.");
            self.consistency.consistent_sector_size = None;
        }
        else {
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
                }
                else {
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

    /// Remove tracks with no sectors from the image.
    /// This should be called with caution as sometimes disk images contain empty tracks between
    /// valid tracks.
    pub fn remove_empty_tracks(&mut self) {
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
            for ti in head.iter() {
                let track = &mut self.track_pool[*ti];
                let mut track_ch = track.ch();

                if track_ch.c() != logical_cylinder {
                    log::trace!(
                        "remap_tracks(): Remapping track idx {}, head: {} from c:{} to c:{}",
                        ti,
                        head_idx,
                        track_ch.c(),
                        logical_cylinder
                    );

                    track_ch.set_c(logical_cylinder);
                    track.set_ch(track_ch);
                }
                logical_cylinder += 1;
            }
        }
    }

    pub fn dump_info<W: crate::io::Write>(&mut self, mut out: W) -> Result<(), crate::io::Error> {
        let disk_format_string = match self.standard_format {
            Some(format) => format.to_string(),
            None => "Non-standard".to_string(),
        };

        // let rpm_string = match self.descriptor.rpm {
        //     Some(rpm) => rpm.to_string(),
        //     None => "Unknown".to_string(),
        // };

        out.write_fmt(format_args!("Disk Format: {}\n", disk_format_string))?;
        out.write_fmt(format_args!("Geometry: {}\n", self.descriptor.geometry))?;
        out.write_fmt(format_args!("Density: {}\n", self.descriptor.density))?;
        //out.write_fmt(format_args!("RPM: {}\n", rpm_string))?;
        //out.write_fmt(format_args!("Volume Name: {:?}\n", self.volume_name))?;

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
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have bad data CRCs\n"))?;
        }

        if self.consistency.bad_address_crc {
            out.write_fmt(format_args!("Disk contains sectors with bad address CRCs\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have bad address CRCs\n"))?;
        }

        if self.consistency.deleted_data {
            out.write_fmt(format_args!("Disk contains sectors marked as deleted\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk are marked as deleted\n"))?;
        }

        if self.consistency.overlapped {
            out.write_fmt(format_args!("Disk contains sectors with overlapping data\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have overlapping data\n"))?;
        }

        if self.consistency.weak {
            out.write_fmt(format_args!("Disk contains tracks with weak bits\n"))?;
        }
        else {
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

    pub fn find_duplication_mark(&self) -> Option<(DiskCh, DiskChsn)> {
        for track in self.track_iter() {
            if let DiskDataEncoding::Fm = track.encoding() {
                //log::debug!("find_duplication_mark(): Found FM track at {}", track.ch());
                if let Some(sector) = track.get_sector_list().iter().take(1).next() {
                    log::debug!(
                        "find_duplication_mark(): first sector of FM track {}: {}",
                        track.ch(),
                        sector.chsn
                    );
                    return Some((track.ch(), sector.chsn));
                }
            }
        }

        None
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
            RwSectorScope::DataElement => &rsr.read_buf,
        };

        util::dump_slice(data_slice, 0, bytes_per_row, &mut out)
    }

    pub fn dump_sector_string(
        &mut self,
        phys_ch: DiskCh,
        id_chs: DiskChs,
        n: Option<u8>,
    ) -> Result<String, DiskImageError> {
        let rsr = self.read_sector(phys_ch, id_chs, n, RwSectorScope::DataOnly, true)?;

        Ok(util::dump_string(&rsr.read_buf))
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
    pub fn compatible_formats(&self, writable: bool) -> Vec<(DiskImageFileFormat, Vec<String>)> {
        let mut formats = formats_from_caps(self.consistency.image_caps);

        // Filter only writable formats if filtering is requested.
        if writable {
            let formats_alone: Vec<DiskImageFileFormat> = formats.iter().map(|f| f.0).collect();
            //log::debug!("compatible_formats(): got formats: {:?}", formats_alone);

            let filtered_formats = filter_writable(self, formats_alone);
            //log::debug!("compatible_formats(): filtered formats: {:?}", filtered_formats);
            formats.retain(|f| filtered_formats.contains(&f.0));
        }

        // Sort the formats by priority, highest first
        formats.sort_by(|a, b| b.0.priority().cmp(&a.0.priority()));

        formats
    }

    pub fn incr_writes(&mut self) {
        self.shared.lock().unwrap().writes += 1;
    }

    /// Return the last track in the track pool.
    /// This is useful for returning the last track created when performing an incremental file load,
    /// ie, to use the last track's data rate to prime the pll for the next track.
    /// It should not be used afterward as entries in the track pool can be orphaned by track map
    /// renumbering.
    #[allow(dead_code)]
    pub(crate) fn last_pool_track(&self) -> Option<&DiskTrack> {
        self.track_pool.last()
    }
}
