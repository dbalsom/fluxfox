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

//! The `diskimage` module defines the [DiskImage] struct which serves as the main interface to
//! fluxfox. A [DiskImage] represents a single floppy disk, consisting of a collection of [Track]s
//!
//! ## Creating a [DiskImage]  
//! A [DiskImage] should not be created directly. Instead, use an [ImageBuilder] to create a new
//! disk image with specified parameters.

use crate::{bitstream::mfm::MfmCodec, track::bitstream::BitStreamTrack, DiskImageFileFormat, SectorMapEntry};

use bit_vec::BitVec;
use sha1_smol::Digest;
use std::{
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    bitstream::{fm::FmCodec, TrackDataStream},
    boot_sector::BootSector,
    containers::DiskImageContainer,
    detect::detect_image_format,
    file_parsers::{
        filter_writable,
        formats_from_caps,
        kryoflux::KfxFormat,
        FormatCaps,
        ImageFormatParser,
        ParserReadOptions,
    },
    io::ReadSeek,
    track::{fluxstream::FluxStreamTrack, metasector::MetaSectorTrack, DiskTrack, Track, TrackAnalysis},
    track_schema::{system34::System34Standard, TrackMetadata, TrackSchema},
    types::{
        chs::*,
        standard_format::StandardFormat,
        BitStreamTrackParams,
        DiskAnalysis,
        DiskDataResolution,
        DiskDescriptor,
        DiskImageFlags,
        DiskSelection,
        ReadSectorResult,
        ReadTrackResult,
        RwScope,
        SharedDiskContext,
        TrackDataEncoding,
        TrackDataRate,
        WriteSectorResult,
    },
    util,
    DiskImageError,
    FoxHashMap,
    FoxHashSet,
    LoadingCallback,
    LoadingStatus,
};

#[cfg(feature = "zip")]
use crate::containers::zip::{extract_file, extract_first_file};
use crate::types::{FluxStreamTrackParams, MetaSectorTrackParams};

pub(crate) const DEFAULT_BOOT_SECTOR: &[u8] = include_bytes!("../resources/bootsector.bin");

/// A [`DiskImage`] represents the structure of a floppy disk. It contains a pool of track data
/// structures, which are indexed by head and cylinder.
///
/// A [`DiskImage`] can be created from a specified disk format using an [ImageBuilder].
///
/// A [`DiskImage`] may be any of the defined [`DiskDataResolution`] levels:
/// * `MetaSector`: These images are sourced from sector-based disk image formats such as
///                 `IMG`, `IMD`, `TD0`, `ADF`, or `PSI`.
/// * `BitStream` : These images are sourced from file formats such as `HFE`, `MFM`, `86F`, or `PRI`.
/// * `FluxStream`: These images are sourced from flux-based formats such as `Kryoflux`, `SCP`, or
///                 `MFI`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    pub(crate) analysis: DiskAnalysis,
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
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) shared: Option<Arc<Mutex<SharedDiskContext>>>,
}

impl Default for DiskImage {
    fn default() -> Self {
        Self {
            flags: DiskImageFlags::empty(),
            standard_format: None,
            source_format: None,
            resolution: Default::default(),
            descriptor: DiskDescriptor::default(),
            analysis: Default::default(),
            boot_sector: None,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            shared: Some(Arc::new(Mutex::new(SharedDiskContext::default()))),
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
            descriptor: disk_format.descriptor(),
            source_format: None,
            resolution: None,
            analysis: DiskAnalysis {
                image_caps: Default::default(),
                weak: false,
                deleted_data: false,
                no_dam: false,
                address_error: false,
                data_error: false,
                overlapped: false,
                consistent_sector_size: Some(2),
                consistent_track_length: Some(disk_format.chs().s() as u32),
            },
            boot_sector: None,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            shared: Some(Arc::new(Mutex::new(SharedDiskContext::default()))),
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
        self.analysis.image_caps
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

        // TODO: DiskImage should probably not concern itself with archives or disk sets...
        //       We should probably move most of this into an ImageLoader interface similar to
        //       ImageBuilder
        match container {
            DiskImageContainer::Raw(format) => {
                let mut image = DiskImage::default();
                format.load_image(image_io, &mut image, &ParserReadOptions::default(), callback)?;
                image.post_load_process();
                Ok(image)
            }
            DiskImageContainer::Zip(format) => {
                #[cfg(feature = "zip")]
                {
                    let file_vec = extract_first_file(image_io)?;
                    let file_cursor = Cursor::new(file_vec);
                    let mut image = DiskImage::default();
                    format.load_image(file_cursor, &mut image, &ParserReadOptions::default(), callback)?;
                    image.post_load_process();
                    Ok(image)
                }
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat)
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
                                disks.first()
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
                            callback_fn(LoadingStatus::ProgressSupport);
                        }

                        for (fi, file_path) in disk.file_set.iter().enumerate() {
                            let mut file_vec = extract_file(image_io, &file_path.clone())?;
                            let mut cursor = Cursor::new(&mut file_vec);
                            log::debug!("load(): Loading Kryoflux stream file from zip: {:?}", file_path);

                            // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                            // updating percentage complete as a fraction of files loaded.
                            match KfxFormat::load_image(&mut cursor, &mut image, &ParserReadOptions::default(), None) {
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
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat)
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
                        match KfxFormat::load_image(&mut cursor, &mut image, &ParserReadOptions::default(), None) {
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

    #[cfg(feature = "async")]
    pub async fn load_async<RS: ReadSeek>(
        image_io: &mut RS,
        _image_path: Option<PathBuf>,
        disk_selection: Option<DiskSelection>,
        callback: Option<LoadingCallback>,
    ) -> Result<Self, DiskImageError> {
        let container = DiskImage::detect_format(image_io)?;

        match container {
            DiskImageContainer::Raw(format) => {
                let mut image = DiskImage::default();
                format.load_image(image_io, &mut image, &ParserReadOptions::default(), callback)?;
                image.post_load_process();
                Ok(image)
            }
            DiskImageContainer::Zip(format) => {
                #[cfg(feature = "zip")]
                {
                    let file_vec = extract_first_file(image_io)?;
                    let file_cursor = Cursor::new(file_vec);
                    let mut image = DiskImage::default();
                    format.load_image(file_cursor, &mut image, &ParserReadOptions::default(), callback)?;
                    image.post_load_process();
                    Ok(image)
                }
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat)
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
                                disks.first()
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
                            callback_fn(LoadingStatus::ProgressSupport);
                        }

                        let image_arc = Arc::new(Mutex::new(image));

                        for (fi, file_path) in disk.file_set.iter().enumerate() {
                            let file_vec = extract_file(image_io, &file_path.clone())?;
                            let cursor = Cursor::new(file_vec);
                            log::debug!("load(): Loading Kryoflux stream file from zip: {:?}", file_path);

                            // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                            // updating percentage complete as a fraction of files loaded.

                            let kfx_format = DiskImageFileFormat::KryofluxStream;
                            kfx_format
                                .load_image_async(cursor, image_arc.clone(), &ParserReadOptions::default(), None)
                                .await?;
                            //KfxFormat::load_image_async(&mut cursor, &mut image, None).await?;

                            if let Some(ref callback_fn) = callback {
                                let completion = (fi + 1) as f64 / disk.file_set.len() as f64;
                                callback_fn(LoadingStatus::Progress(completion));
                            }
                        }

                        if let Some(callback_fn) = callback {
                            callback_fn(LoadingStatus::Complete);
                        }

                        // Unwrap image from Arc
                        let mut image = Arc::try_unwrap(image_arc)
                            .map_err(|_| DiskImageError::SyncError("Failed to unwrap image from Arc".to_string()))?
                            .into_inner()
                            .map_err(|_| DiskImageError::SyncError("Failed to unlock image from Mutex".to_string()))?;

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
                #[cfg(not(feature = "zip"))]
                {
                    Err(DiskImageError::UnknownFormat)
                }
            }
            #[cfg(feature = "wasm")]
            DiskImageContainer::KryofluxSet => Err(DiskImageError::UnsupportedFormat),
            #[cfg(feature = "tokio-async")]
            DiskImageContainer::KryofluxSet => {
                if let Some(image_path) = _image_path {
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
                        let mut file_vec = tokio::fs::read(file_path.clone()).await?;
                        let mut cursor = Cursor::new(&mut file_vec);

                        log::debug!("load(): Loading Kryoflux stream file: {:?}", file_path);

                        // We won't give the callback to the kryoflux loader - instead we will call it here ourselves
                        // updating percentage complete as a fraction of files loaded.
                        match KfxFormat::load_image(&mut cursor, &mut image, &ParserReadOptions::default(), None) {
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

    /// Retrieve any comment that was saved in the source disk image, or subsequently added
    /// with `set_comment`.
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    /// Set a comment for the disk image. This comment will be saved with the disk image, if the
    /// output image format supports comments. Note that saving a comment may be a lossy operation
    /// due to character encoding and length limitations.
    pub fn set_comment(&mut self, comment: String) {
        self.comment = Some(comment);
    }

    pub fn set_data_rate(&mut self, rate: TrackDataRate) {
        self.descriptor.data_rate = rate;
    }

    pub fn data_rate(&self) -> TrackDataRate {
        self.descriptor.data_rate
    }

    pub fn set_data_encoding(&mut self, encoding: TrackDataEncoding) {
        self.descriptor.data_encoding = encoding;
    }

    pub fn data_encoding(&self) -> TrackDataEncoding {
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
        if let Some(shared) = &self.shared {
            shared.lock().unwrap().writes
        }
        else {
            0
        }
    }

    pub fn source_format(&self) -> Option<DiskImageFileFormat> {
        self.source_format
    }

    pub fn set_source_format(&mut self, format: DiskImageFileFormat) {
        self.source_format = Some(format);
    }

    /// Return the resolution of the disk image, either MetaSector or BitStream.
    pub fn resolution(&self) -> DiskDataResolution {
        self.resolution.unwrap_or(DiskDataResolution::MetaSector)
    }

    /// Adds a new `FluxStream` resolution track to the disk image.
    /// Data of this resolution is typically sourced from flux image formats such as Kryoflux, SCP
    /// or MFI.
    ///
    /// This function locks the disk image to `FluxStream` resolution.
    ///
    /// # Parameters
    /// - `ch`   : a `DiskCh` specifying the physical cylinder and head of the new track.
    ///            This value must be the next available track in the image.
    /// - `track`: The `FluxStreamTrack` to add. Unlike adding a BitStream or Metasector
    ///            resolution track, we must construct a `FluxStreamTrack` first before adding it.
    ///
    /// # Returns
    /// - `Ok(&mut DiskTrack)` if the track was successfully added, providing a reference to the newly added track.
    /// - `Err(DiskImageError::SeekError)` if the `ch` parameter is out of range or invalid.
    /// - `Err(DiskImageError::ParameterError)` if the length of `data` and `weak` do not match.
    /// - `Err(DiskImageError::IncompatibleImage)` if the current `DiskImage` is not compatible with `FluxStream` resolution.
    pub fn add_track_fluxstream(
        &mut self,
        mut track: FluxStreamTrack,
        params: &FluxStreamTrackParams,
    ) -> Result<&mut DiskTrack, DiskImageError> {
        let head = params.ch.h() as usize;
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

        track.set_ch(params.ch);
        track.set_shared(self.shared.clone().expect("Shared context not found."));
        track.synthesize_revolutions(); // Create synthetic revolutions to increase chances of successful decoding.
        track.decode_revolutions(params.clock, params.rpm)?;
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

        Ok(self.track_pool.last_mut().unwrap())
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
    pub fn add_track_bitstream(&mut self, params: &BitStreamTrackParams) -> Result<&mut DiskTrack, DiskImageError> {
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
        let new_track = BitStreamTrack::new(params, self.shared.clone().expect("Shared context not found"))?;

        self.track_pool.push(Box::new(new_track));
        self.track_map[head].push(self.track_pool.len() - 1);

        // Consider adding a track to an image to be a single 'write' operation.
        self.incr_writes();

        Ok(self.track_pool.last_mut().unwrap())
    }

    /// Adds a new track to the disk image, of `MetaSector` resolution.
    /// Data of this resolution is typically sourced from sector-based image formats such as IMG
    /// or ADF.
    ///
    /// This function locks the disk image to `MetaSector` resolution and adds a new track with the
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
    /// - `Err(DiskImageError::IncompatibleImage)` if the disk image is not compatible with `MetaSector` resolution.
    pub fn add_track_metasector(&mut self, params: &MetaSectorTrackParams) -> Result<&mut DiskTrack, DiskImageError> {
        if params.ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        // Lock the disk image to MetaSector resolution.
        match self.resolution {
            None => self.resolution = Some(DiskDataResolution::MetaSector),
            Some(DiskDataResolution::MetaSector) => {}
            _ => return Err(DiskImageError::IncompatibleImage),
        }

        self.track_pool.push(Box::new(MetaSectorTrack {
            ch: params.ch,
            encoding: params.encoding,
            schema: Some(TrackSchema::System34),
            data_rate: params.data_rate,
            sectors: Vec::new(),
            shared: self.shared.clone().expect("Shared context not found"),
        }));
        self.track_map[params.ch.h() as usize].push(self.track_pool.len() - 1);

        // Consider adding a track to an image to be a single 'write' operation.
        self.incr_writes();

        Ok(self.track_pool.last_mut().unwrap())
    }

    // // TODO: Fix this, it doesn't handle nonconsecutive sectors
    // #[allow(deprecated)]
    // pub fn next_sector_on_track(&self, chs: DiskChs) -> Option<DiskChs> {
    //     let ti = self.track_map[chs.h() as usize][chs.c() as usize];
    //     let track = &self.track_pool[ti];
    //     let s = track.sector_ct();
    //
    //     // Get the track geometry
    //     let geom_chs = DiskChs::from((self.geometry(), s as u8));
    //     let next_sector = geom_chs.next_sector(&geom_chs);
    //
    //     // Return the next sector as long as it is on the same track.
    //     if next_sector.c() == chs.c() {
    //         Some(next_sector)
    //     }
    //     else {
    //         None
    //     }
    // }

    /// Read the sector data from the sector at the physical location 'phys_ch' with the sector ID
    /// values specified by 'id_chs'.
    /// The data is returned within a ReadSectorResult struct which also sets some convenience
    /// metadata flags which are needed when handling MetaSector images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    pub fn read_sector(
        &mut self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];

        track.read_sector(id, n, offset, scope, debug)
    }

    /// A simplified version of read_sector() which only returns the sector data as a Vec<u8>,
    /// or an `DiskImageError` if the sector could not be read.
    pub fn read_sector_basic(
        &self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        offset: Option<usize>,
    ) -> Result<Vec<u8>, DiskImageError> {
        // Check that the head and cylinder are within the bounds of the track map.
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }
        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &self.track_pool[ti];
        let rsr = track.read_sector(id, id.n(), offset, RwScope::DataOnly, false)?;

        if rsr.not_found || rsr.address_crc_error || rsr.no_dam {
            return Err(DiskImageError::IdError);
        }
        Ok(rsr.read_buf[rsr.data_range].to_vec())
    }

    pub fn write_sector(
        &mut self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        offset: Option<usize>,
        data: &[u8],
        scope: RwScope,
        deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];
        track.write_sector(id, offset, data, scope, deleted, debug)
    }

    pub fn write_sector_basic(
        &mut self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        offset: Option<usize>,
        data: &[u8],
    ) -> Result<(), DiskImageError> {
        if phys_ch.h() > 1 || phys_ch.c() as usize >= self.track_map[phys_ch.h() as usize].len() {
            log::debug!(
                "write_sector_basic(): Seek error: track map for head {} has {} tracks",
                phys_ch.h(),
                self.track_map[phys_ch.h() as usize].len()
            );
            return Err(DiskImageError::SeekError);
        }

        let ti = self.track_map[phys_ch.h() as usize][phys_ch.c() as usize];
        let track = &mut self.track_pool[ti];
        let wsr = track.write_sector(id, offset, data, RwScope::DataOnly, false, false)?;

        if wsr.not_found || wsr.address_crc_error || wsr.no_dam {
            return Err(DiskImageError::IdError);
        }
        Ok(())
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling MetaSector images.
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

        track.read(overdump)
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

        track.read_raw(overdump)
    }

    pub fn add_empty_track(
        &mut self,
        ch: DiskCh,
        encoding: TrackDataEncoding,
        data_rate: TrackDataRate,
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
                    TrackDataEncoding::Mfm => {
                        Box::new(MfmCodec::new(BitVec::from_fn(bitcells, |i| i % 2 == 0), None, None))
                    }
                    TrackDataEncoding::Fm => {
                        Box::new(FmCodec::new(BitVec::from_fn(bitcells, |i| i % 2 == 0), None, None))
                    }
                    _ => return Err(DiskImageError::UnsupportedFormat),
                };

                // TODO: Add an empty track with a schema of None and set it on format
                self.track_pool.push(Box::new(BitStreamTrack {
                    encoding,
                    data_rate,
                    rpm: self.descriptor.rpm,
                    ch,
                    data: stream,
                    metadata: TrackMetadata::default(),
                    schema: Some(TrackSchema::System34),
                    shared: Some(self.shared.clone().expect("Shared context not found")),
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
                    schema: Some(TrackSchema::System34),
                    data_rate,
                    ch,
                    sectors: Vec::new(),
                    shared: self.shared.clone().expect("Shared context not found"),
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
        self.analyze();
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
        let chsn = format.chsn();
        let encoding = format.encoding();
        let data_rate = format.data_rate();
        let bitcell_size = format.bitcell_ct();

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

                let gap3 = format.gap3();
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

        track.next_id(chs)
    }

    pub(crate) fn read_boot_sector(&mut self) -> Result<Vec<u8>, DiskImageError> {
        if self.track_map.is_empty() || self.track_map[0].is_empty() {
            return Err(DiskImageError::IncompatibleImage);
        }
        let ti = self.track_map[0][0];
        let track = &mut self.track_pool[ti];

        match track.read_sector(DiskChsnQuery::new(0, 0, 1, 2), None, None, RwScope::DataOnly, true) {
            Ok(result) => Ok(result.read_buf[result.data_range].to_vec()),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn write_boot_sector(&mut self, buf: &[u8]) -> Result<(), DiskImageError> {
        self.write_sector(
            DiskCh::new(0, 0),
            DiskChsnQuery::new(0, 0, 1, 2),
            None,
            buf,
            RwScope::DataOnly,
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
        if let Some(shared) = &self.shared {
            shared.lock().unwrap().writes = 1;
        }

        // Normalize the disk image
        self.normalize();

        // Set the DiskAnalysis
        self.analyze();

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
            if let Some(format) = boot_sector.standard_format() {
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

    pub fn track_ct(&self, head: usize) -> usize {
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
            self.descriptor.geometry.set_c(self.track_ct(0) as u16);
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
                self.descriptor.geometry.set_c(self.track_ct(0) as u16);
            }
        }

        if removed_odd {
            // Renumber tracks.
            self.remap_tracks();
        }
    }

    /// Update a [DiskImage]'s [DiskAnalysis] struct to reflect the current state of the image.
    /// This function should be called after any changes to a track.
    pub(crate) fn analyze(&mut self) {
        let mut spt: FoxHashSet<usize> = FoxHashSet::new();

        let mut all_consistency: TrackAnalysis = Default::default();
        let mut variable_sector_size = false;

        let mut consistent_size_map: FoxHashSet<u8> = FoxHashSet::new();

        let mut last_track_sector_size = 0;

        log::debug!("analyze(): Running consistency check...");
        for track_idx in self.track_idx_iter() {
            let td = &self.track_pool[track_idx];
            match td.analysis() {
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
                    log::warn!("analyze(): Track {} has no analysis data.", track_idx);
                    continue;
                }
            };
        }

        self.analysis.set_track_analysis(&all_consistency);
        if consistent_size_map.len() > 1 {
            self.analysis.consistent_sector_size = None;
        }

        if spt.len() > 1 {
            log::debug!(
                "update_consistency(): Inconsistent sector counts detected in tracks: {:?}",
                spt
            );
            self.analysis.consistent_track_length = None;
        }
        else {
            self.analysis.consistent_track_length = Some(all_consistency.sector_ct as u32);
        }

        if variable_sector_size {
            log::debug!("update_consistency(): Variable sector sizes detected in tracks.");
            self.analysis.consistent_sector_size = None;
        }
        else {
            self.analysis.consistent_sector_size = Some(last_track_sector_size);
        }

        let mut new_caps = self.analysis.image_caps;

        new_caps.set(
            FormatCaps::CAP_VARIABLE_SPT,
            self.analysis.consistent_track_length.is_none(),
        );
        new_caps.set(
            FormatCaps::CAP_VARIABLE_SSPT,
            self.analysis.consistent_sector_size.is_none(),
        );
        new_caps.set(FormatCaps::CAP_DATA_CRC, self.analysis.data_error);
        new_caps.set(FormatCaps::CAP_ADDRESS_CRC, self.analysis.address_error);
        new_caps.set(FormatCaps::CAP_DATA_DELETED, self.analysis.deleted_data);
        new_caps.set(FormatCaps::CAP_NO_DAM, self.analysis.no_dam);

        log::debug!("update_consistency(): Image capabilities: {:?}", new_caps);
        self.analysis.image_caps = new_caps;
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
            let track0_sectors = self.track_pool[track_pair[0]].sector_ct();
            let track0_hash = self.track_pool[track_pair[0]].hash();
            let track1_hash = self.track_pool[track_pair[1]].hash();

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
            let track0_sectors = self.track_pool[track_pair[0]].sector_ct();
            let track1_sectors = self.track_pool[track_pair[1]].sector_ct();

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
                let track_entry_opt = track_hashes.get(&self.track_pool[*track].hash());
                if track_entry_opt.is_some() {
                    duplicate_tracks[head_idx].push(track_idx);
                }
                else {
                    track_hashes.insert(self.track_pool[*track].hash(), 1);
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
                if self.track_pool[*track].sector_ct() == 0 {
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

    pub fn dump_analysis<W: crate::io::Write>(&mut self, mut out: W) -> Result<(), crate::io::Error> {
        if self.analysis.data_error {
            out.write_fmt(format_args!("Disk contains sectors with bad data CRCs\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have bad data CRCs\n"))?;
        }

        if self.analysis.address_error {
            out.write_fmt(format_args!("Disk contains sectors with bad address CRCs\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have bad address CRCs\n"))?;
        }

        if self.analysis.deleted_data {
            out.write_fmt(format_args!("Disk contains sectors marked as deleted\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk are marked as deleted\n"))?;
        }

        if self.analysis.overlapped {
            out.write_fmt(format_args!("Disk contains sectors with overlapping data\n"))?;
        }
        else {
            out.write_fmt(format_args!("No sectors on disk have overlapping data\n"))?;
        }

        if self.analysis.weak {
            out.write_fmt(format_args!("Disk contains tracks with weak bits\n"))?;
        }
        else {
            out.write_fmt(format_args!("No tracks on disk have weak bits\n"))?;
        }

        match self.analysis.consistent_track_length {
            Some(sector_ct) => {
                out.write_fmt(format_args!("All tracks on disk have {} sectors\n", sector_ct))?;
            }
            None => {
                out.write_fmt(format_args!("Disk contains tracks with variable sector counts\n"))?;
            }
        }

        match self.analysis.consistent_sector_size {
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

    pub fn sector_map(&self) -> Vec<Vec<Vec<SectorMapEntry>>> {
        let mut head_map = Vec::new();

        let geom = self.geometry();
        //log::trace!("get_sector_map(): Geometry is {}", geom);

        for head in 0..geom.h() {
            let mut track_map = Vec::new();

            for track_idx in &self.track_map[head as usize] {
                let track = &self.track_pool[*track_idx];
                track_map.push(track.sector_list());
            }

            head_map.push(track_map);
        }

        head_map
    }

    pub fn find_duplication_mark(&self) -> Option<(DiskCh, DiskChsn)> {
        for track in self.track_iter() {
            if let TrackDataEncoding::Fm = track.encoding() {
                //log::debug!("find_duplication_mark(): Found FM track at {}", track.ch());
                if let Some(sector) = track.sector_list().iter().take(1).next() {
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
        let head_map = self.sector_map();

        for (head_idx, head) in head_map.iter().enumerate() {
            out.write_fmt(format_args!("Head {}\n", head_idx))?;
            for (track_idx, track) in head.iter().enumerate() {
                out.write_fmt(format_args!("\tTrack {}\n", track_idx))?;
                for sector in track {
                    out.write_fmt(format_args!(
                        "\t\t{} address_crc_valid: {} data_crc_valid: {} deleted: {}\n",
                        sector.chsn,
                        !sector.attributes.address_error,
                        !sector.attributes.data_error,
                        sector.attributes.no_dam
                    ))?;
                }
            }
        }

        Ok(())
    }

    pub fn dump_sector_hex<W: crate::io::Write>(
        &mut self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwScope,
        bytes_per_row: usize,
        mut out: W,
    ) -> Result<(), DiskImageError> {
        let rsr = self.read_sector(phys_ch, id, n, offset, scope, true)?;

        let data_slice = match scope {
            RwScope::DataOnly => &rsr.read_buf[rsr.data_range],
            RwScope::EntireElement => &rsr.read_buf,
            _ => return Err(DiskImageError::ParameterError),
        };

        util::dump_slice(data_slice, 0, bytes_per_row, 1, &mut out)
    }

    pub fn dump_sector_string(
        &mut self,
        phys_ch: DiskCh,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
    ) -> Result<String, DiskImageError> {
        let rsr = self.read_sector(phys_ch, id, n, offset, RwScope::DataOnly, true)?;

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
        let mut formats = formats_from_caps(self.analysis.image_caps);

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

    /// Attempt to determine the [StandardFormat] that most closely conforms to the disk image.
    /// This function is used for exporting to IMG and for constructing a [StandardSectorView].
    ///
    /// The guess is based off the following criteria:
    /// - The number of heads in the disk image
    /// - The media descriptor in the BPB of the boot sector, if present and valid.
    /// - The various parameters of the BPB (sectors per track, heads, etc.)
    /// - The disk image consistency flags.
    /// - The most frequent values of sector size and sectors per track on the disk.
    ///
    /// [DiskImage::update_consistency] should be called before calling this function.
    ///
    /// # Arguments
    /// - `trust_bpb`: If true, the function will trust the BPB to determine the disk format if
    ///                there is disagreement between the BPB and the disk consistency. This is useful
    ///                for creating a [StandardSectorView] to be used to read a FAT filesystem
    ///                as DOS will use the BPB to determine the disk format.
    ///                This flag has no effect if a BPB is not present or does not specify a valid
    ///                format.
    ///
    /// # Returns
    /// - `Some(StandardFormat)` if a format is found that closely matches the disk image, ignoring
    ///   non-standard tracks and sectors.
    /// - `None` if no format is found that closely matches the disk image, or the image data
    ///          is too inconsistent to determine a format.
    pub fn closest_format(&self, trust_bpb: bool) -> Option<StandardFormat> {
        let mut bpb_format = None;

        // Get the format from the boot sector if present.
        if let Some(boot_sector) = &self.boot_sector {
            if let Some(format) = boot_sector.standard_format() {
                bpb_format = Some(format);
            }
        }

        let mut consistency_format = None;

        // Check if the disk image is consistent enough to determine a format.
        if let Some(spt) = self.analysis.consistent_track_length {
            if let Some(cylinders) = StandardFormat::normalized_track_ct(self.track_ct(0)) {
                // Construct a DiskChs from the consistent parameters.
                let chs = DiskChs::new(cylinders as u16, self.heads(), spt as u8);
                // See if it matches a StandardFormat.

                if let Ok(format) = StandardFormat::try_from(&chs) {
                    consistency_format = Some(format);
                }
            }
        }
        else {
            log::debug!("closest_format(): Found inconsistent spt.");
        }

        if (bpb_format.is_some() || consistency_format.is_some()) && bpb_format != consistency_format {
            log::warn!(
                "closest_format(): BPB format {:?} and consistency format {:?} disagree.",
                bpb_format,
                consistency_format
            );

            if trust_bpb && bpb_format.is_some() {
                log::debug!("closest_format(): Trusting BPB format.");
                return bpb_format;
            }
            else if consistency_format.is_some() {
                log::debug!("closest_format(): Falling back to consistency-determined format.");
                return consistency_format;
            }
        }

        // TODO: If disk is not consistent, try to determine the the format of the 'normal' tracks
        //       and return that format.
        bpb_format.or(consistency_format)
    }

    pub(crate) fn incr_writes(&mut self) {
        if let Some(shared) = &self.shared {
            shared.lock().unwrap().writes += 1;
        }
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

    /// Consume the `DiskImage` and return an `Arc<Mutex<DiskImage>>`.
    pub fn into_arc(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}
