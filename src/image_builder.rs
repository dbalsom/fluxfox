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

//! A module to implement the builder pattern for [DiskImage]. Due to the
//! complexity of the [DiskImage] object, it is not advisable to attempt to
//! create one directly.
//!
//! An [ImageBuilder] allows for creation of a [DiskImage] with the desired
//! parameters, at the desired [TrackDataResolution], optionally formatted.
//!
//! For IBM PC disk images, a creator tag can be specified which will be
//! displayed during boot if the disk is left in the drive.

#[cfg(feature = "fat")]
use crate::{
    disk_lock::{DiskLock, NonTrackingDiskLock, NullContext},
    file_system::fat::fat_fs::FatFileSystem,
};
use crate::{
    file_system::{self, FileSystemType},
    types::{DiskImageFlags, TrackDataResolution},
    DiskImage,
    DiskImageError,
    StandardFormat,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "zip")]
use std::io::Read;

const BOOT_SECTOR_SIZE: usize = 512;

/// Implements the Builder pattern for [DiskImage] objects.
/// [ImageBuilder] for creation of blank or pre-formatted [DiskImage]s.
#[derive(Default)]
pub struct ImageBuilder {
    /// Specify the [StandardFormat] to use for the [DiskImage] to be built.
    pub standard_format: Option<StandardFormat>,
    /// Specify the [DiskDataResolution] to use for the DiskImage to be built.
    pub resolution: Option<TrackDataResolution>,
    /// Specify the creator tag to display during boot.
    pub creator_tag: Option<[u8; 8]>,
    /// Specify whether the [DiskImage] should be formatted.
    pub formatted: bool,
    /// Specify whether the [DiskImage] should use the specified [FileSystemType].
    /// Required if `formatted` is true.
    pub filesystem: Option<FileSystemType>,
    /// Specify whether the [DiskImage] should be created from a directory of files.
    /// Mutually exclusive with `from_archive`.
    pub from_path: Option<PathBuf>,
    /// Specify whether the [DiskImage] should be created from an archive file.
    /// Mutually exclusive with `from_path`.
    pub from_archive: Option<PathBuf>,
    /// Specify whether we should attempt to create a bootable disk image if `from_path` or
    /// `from_archive` are specified.
    pub bootable: bool,
    /// An explicit boot sector to install and patch with the selected disk format's BPB.
    /// If omitted, a boot sector is discovered from the source root, or the bundled fox boot
    /// sector is used as a final fallback.
    pub boot_sector: Option<Vec<u8>>,
    /// Specify whether the files should be added recursively from the specified path
    /// or archive. If false, only files in the root directory will be added.
    pub recursive: bool,
    /// Specify whether the files must fit on the disk image. If false, files will be added
    /// to the disk image until it is full. If true, an error will be returned if the
    /// files do not fit on the disk image.
    pub must_fit: bool,
}

impl ImageBuilder {
    pub fn new() -> ImageBuilder {
        Default::default()
    }

    /// Set the [StandardFormat] to use for the [DiskImage] to be built.
    pub fn with_standard_format(mut self, standard_format: impl Into<StandardFormat>) -> ImageBuilder {
        self.standard_format = Some(standard_format.into());
        self
    }

    /// Set the [TrackDataResolution] to use for the [DiskImage] to be built.
    pub fn with_resolution(mut self, resolution: TrackDataResolution) -> ImageBuilder {
        self.resolution = Some(resolution);
        self
    }

    /// Set whether the [DiskImage] to be built should be formatted as the specified [FileSystemType].
    /// If this is not set, the [DiskImage] will be created as a blank image which must be formatted
    /// before it can be read in a disk drive or emulator.
    pub fn with_filesystem(mut self, filesystem: FileSystemType) -> ImageBuilder {
        self.filesystem = Some(filesystem);
        self.from_path = None;
        self.from_archive = None;
        self.formatted = true;
        self
    }

    /// Set whether the [DiskImage] to be built should be formatted using the default FAT12
    /// filesystem.
    pub fn with_formatted(mut self, formatted: bool) -> ImageBuilder {
        self.formatted = formatted;
        self.filesystem = formatted.then_some(FileSystemType::Fat12);
        self
    }

    /// Set whether the [DiskImage] to be built should be formatted as the specified [FileSystemType],
    /// containing files from the specified path.
    /// # Arguments:
    /// * `path` - The path to the directory containing the files to be added to the [DiskImage].
    /// * `filesystem` - The [FileSystemType] to use for the [DiskImage].
    /// * `recursive` - If `true`, files will be added recursively from the specified path, creating
    ///   subdirectories as necessary. If `false`, only files in the specified directory will be
    ///   added in the root directory of the [DiskImage].
    /// * `must_fit` - Whether the files must fit on the disk image. If false, files will be added
    ///   to the disk image until it is full. If true, an error will be returned if the
    ///   files do not fit on the disk image.
    ///
    /// A complete `IO.SYS`/`MSDOS.SYS` or `IBMIO.SYS`/`IBMDOS.SYS` pair in the source root is
    /// written first and marked read-only, hidden, and system. Incomplete or mixed pairs are
    /// rejected.
    ///
    /// A root-level `bootsector.bin` or `*_bootsector.bin` is installed as the image boot sector,
    /// patched with the selected format's BPB, and omitted from the FAT filesystem. If `bootable`
    /// is true, the absence of such a file (or an explicit [`ImageBuilder::with_bootsector`]
    /// override) is an error.
    pub fn with_filesystem_from_path(
        mut self,
        path: impl AsRef<Path>,
        filesystem: FileSystemType,
        bootable: bool,
        recursive: bool,
        must_fit: bool,
    ) -> ImageBuilder {
        self.filesystem = Some(filesystem);
        self.from_path = Some(path.as_ref().to_path_buf());
        self.from_archive = None;
        self.formatted = true;
        self.bootable = bootable;
        self.recursive = recursive;
        self.must_fit = must_fit;
        self
    }

    /// Set whether the [DiskImage] to be built should be formatted as the specified [FileSystemType],
    /// containing files from the specified ZIP archive. This requires the `zip` feature.
    /// # Arguments:
    /// * `path` - The path to the ZIP archive containing files to be added to the [DiskImage].
    /// * `filesystem` - The [FileSystemType] to use for the [DiskImage].
    /// * `recursive` - If `true`, files will be added recursively from the specified path, creating
    ///   subdirectories as necessary. If `false`, only files in the specified directory will be
    ///   added in the root directory of the [DiskImage].
    /// * `must_fit` - Whether the files must fit on the disk image. If false, files will be added
    ///   to the disk image until it is full. If true, an error will be returned if the
    ///   files do not fit on the disk image.
    ///
    /// A complete `IO.SYS`/`MSDOS.SYS` or `IBMIO.SYS`/`IBMDOS.SYS` pair in the archive root is
    /// written first and marked read-only, hidden, and system. Incomplete or mixed pairs are
    /// rejected.
    ///
    /// A root-level `bootsector.bin` or `*_bootsector.bin` is installed as the image boot sector,
    /// patched with the selected format's BPB, and omitted from the FAT filesystem.
    pub fn with_filesystem_from_archive(
        mut self,
        path: impl AsRef<Path>,
        filesystem: FileSystemType,
        recursive: bool,
        must_fit: bool,
    ) -> ImageBuilder {
        self.filesystem = Some(filesystem);
        self.from_archive = Some(path.as_ref().to_path_buf());
        self.from_path = None;
        self.formatted = true;
        self.recursive = recursive;
        self.must_fit = must_fit;
        self
    }

    /// Set the creator tag for the [`DiskImage`] to be built. This is only used if the [`DiskImage`]
    /// is to be formatted.
    pub fn with_creator_tag(mut self, creator_tag: &[u8]) -> ImageBuilder {
        let mut new_creator_tag = [0x20; 8];
        let max_len = creator_tag.len().min(8);
        new_creator_tag[..max_len].copy_from_slice(&creator_tag[..max_len]);

        self.creator_tag = Some(new_creator_tag);
        self
    }

    /// Install the supplied 512-byte boot sector and patch its BPB for the selected disk format.
    ///
    /// This explicit value takes precedence over any boot-sector binary in a source directory or
    /// ZIP archive. The length is validated by [`ImageBuilder::build`].
    pub fn with_bootsector(mut self, boot_sector: &[u8]) -> ImageBuilder {
        self.boot_sector = Some(boot_sector.to_vec());
        self.bootable = true;
        self
    }

    /// Build the [`DiskImage`] using the specified parameters.
    pub fn build(self) -> Result<DiskImage, DiskImageError> {
        if let Some(boot_sector) = self.boot_sector.as_deref() {
            Self::validate_boot_sector(boot_sector, "with_bootsector()")?;
        }
        if let Some(path) = self.from_path.as_ref() {
            if !path.is_dir() {
                return Err(DiskImageError::FilesystemError(
                    file_system::FileSystemError::PathNotFound(path.display().to_string()),
                ));
            }
        }
        if let Some(path) = self.from_archive.as_ref() {
            if !path.is_file() {
                return Err(DiskImageError::FilesystemError(
                    file_system::FileSystemError::PathNotFound(path.display().to_string()),
                ));
            }
        }
        if self.resolution.is_none() {
            log::error!("DiskDataResolution not set");
            return Err(DiskImageError::ParameterError);
        }

        if self.standard_format.is_some() {
            match self.resolution {
                Some(TrackDataResolution::BitStream) => self.build_bitstream(),
                Some(TrackDataResolution::MetaSector) => self.build_metasector(),
                _ => Err(DiskImageError::UnsupportedFormat),
            }
        }
        else {
            Err(DiskImageError::UnsupportedFormat)
        }
    }

    fn build_bitstream(self) -> Result<DiskImage, DiskImageError> {
        let format = self.standard_format.unwrap();
        let mut disk_image = DiskImage::create(format);
        disk_image.set_resolution(TrackDataResolution::BitStream);

        // An explicit sector wins. Otherwise discover one in the source root, falling back to the
        // bundled fox sector when bootability was not required.
        let discovered_boot_sector = if self.boot_sector.is_none() {
            if let Some(path) = self.from_path.as_deref() {
                Self::discover_boot_sector_from_directory(path)?
            }
            else if let Some(path) = self.from_archive.as_deref() {
                #[cfg(feature = "zip")]
                {
                    Self::discover_boot_sector_from_zip(path)?
                }
                #[cfg(not(feature = "zip"))]
                {
                    let _ = path;
                    None
                }
            }
            else {
                None
            }
        }
        else {
            None
        };
        let boot_sector = self.boot_sector.as_deref().or(discovered_boot_sector.as_deref());

        if self.bootable && boot_sector.is_none() {
            return Err(DiskImageError::FilesystemError(
                file_system::FileSystemError::InvalidBootSector(
                    "bootable image requested, but no bootsector.bin or *_bootsector.bin was found in the source root"
                        .to_string(),
                ),
            ));
        }

        let chsn = format.layout();
        let encoding = format.encoding();
        let data_rate = format.data_rate();
        let bitcell_size = format.bitcell_ct();

        log::debug!(
            "ImageBuilder::build_bitstream(): Building disk image with format {:?}",
            format
        );

        for ch in chsn.ch_iter() {
            disk_image.add_empty_track(
                ch,
                encoding,
                Some(TrackDataResolution::BitStream),
                data_rate,
                bitcell_size,
                Some(false),
            )?;
        }

        // Format the new disk image if required
        if self.formatted && self.filesystem.is_some() {
            log::debug!("ImageBuilder::build_bitstream(): Formatting disk image as {:?}", format);
            disk_image.format(
                format,
                TrackDataResolution::BitStream,
                self.filesystem.unwrap(),
                boot_sector,
                self.creator_tag.as_ref(),
            )?;
            disk_image.post_load_process();
        }

        // Sanity check - do we have the correct number of heads and tracks?
        if disk_image.track_map[0].len() != chsn.c() as usize {
            log::error!("ImageBuilder::build_bitstream(): Incorrect number of tracks in head 0 after format operation");
            return Err(DiskImageError::ParameterError);
        }

        if let Some(boot_sector) = disk_image.boot_sector() {
            log::debug!(
                "ImageBuilder::build_bitstream(): Boot sector found! {:#?}",
                boot_sector.bpb2
            );
        }

        // If we're building from a path, inject the files
        if let Some(path) = self.from_path {
            match self.filesystem {
                Some(FileSystemType::Fat12) => {
                    log::debug!(
                        "ImageBuilder::build_bitstream(): Injecting files from path {:?} into FAT12 filesystem",
                        path
                    );
                    #[cfg(feature = "fat")]
                    {
                        disk_image =
                            Self::inject_files_from_path_fat(&path, disk_image, self.recursive, self.must_fit)?;
                    }
                    #[cfg(not(feature = "fat"))]
                    {
                        let _ = (path, disk_image);
                        return Err(DiskImageError::FilesystemError(
                            crate::file_system::FileSystemError::FeatureError("fat".to_string()),
                        ));
                    }
                }
                None => {
                    log::error!("ImageBuilder::build_bitstream(): No filesystem specified for file injection!");
                    return Err(DiskImageError::ParameterError);
                }
                _ => {
                    log::error!("ImageBuilder::build_bitstream(): Unsupported filesystem type for file injection");
                    return Err(DiskImageError::UnsupportedFilesystem);
                }
            }
        }
        else if let Some(path) = self.from_archive {
            match self.filesystem {
                Some(FileSystemType::Fat12) => {
                    log::debug!(
                        "ImageBuilder::build_bitstream(): Injecting files from ZIP archive {:?} into FAT12 filesystem",
                        path
                    );
                    #[cfg(all(feature = "fat", feature = "zip"))]
                    {
                        disk_image = Self::inject_files_from_zip_fat(&path, disk_image, self.recursive, self.must_fit)?;
                    }
                    #[cfg(not(feature = "fat"))]
                    {
                        let _ = (path, disk_image);
                        return Err(DiskImageError::FilesystemError(
                            crate::file_system::FileSystemError::FeatureError("fat".to_string()),
                        ));
                    }
                    #[cfg(all(feature = "fat", not(feature = "zip")))]
                    {
                        let _ = (path, disk_image);
                        return Err(DiskImageError::FilesystemError(
                            crate::file_system::FileSystemError::FeatureError("zip".to_string()),
                        ));
                    }
                }
                None => {
                    log::error!("ImageBuilder::build_bitstream(): No filesystem specified for archive injection!");
                    return Err(DiskImageError::ParameterError);
                }
                _ => {
                    log::error!("ImageBuilder::build_bitstream(): Unsupported filesystem type for archive injection");
                    return Err(DiskImageError::UnsupportedFilesystem);
                }
            }
        }

        // Do post-load processing as normal
        //disk_image.post_load_process();

        // Clear dirty flag
        disk_image.clear_flag(DiskImageFlags::DIRTY);

        Ok(disk_image)
    }

    fn validate_boot_sector(boot_sector: &[u8], source: &str) -> Result<(), DiskImageError> {
        if boot_sector.len() != BOOT_SECTOR_SIZE {
            return Err(DiskImageError::FilesystemError(
                file_system::FileSystemError::InvalidBootSector(format!(
                    "{source} is {} bytes; expected {BOOT_SECTOR_SIZE}",
                    boot_sector.len()
                )),
            ));
        }
        Ok(())
    }

    fn select_boot_sector_candidate(mut candidates: Vec<(String, Vec<u8>)>) -> Result<Option<Vec<u8>>, DiskImageError> {
        candidates.sort_by(|(left, _), (right, _)| left.cmp(right));
        if candidates.len() > 1 {
            let names = candidates
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(DiskImageError::FilesystemError(
                file_system::FileSystemError::InvalidBootSector(format!(
                    "multiple boot-sector candidates were found: {names}"
                )),
            ));
        }

        let Some((name, bytes)) = candidates.pop()
        else {
            return Ok(None);
        };
        Self::validate_boot_sector(&bytes, &name)?;
        Ok(Some(bytes))
    }

    fn discover_boot_sector_from_directory(path: &Path) -> Result<Option<Vec<u8>>, DiskImageError> {
        let mut candidates = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().into_string().map_err(|name| {
                DiskImageError::FilesystemError(file_system::FileSystemError::UnsupportedFileObject(
                    name.to_string_lossy().into_owned(),
                ))
            })?;
            if file_system::is_boot_sector_filename(&name) {
                let size = entry.metadata()?.len();
                if size != BOOT_SECTOR_SIZE as u64 {
                    return Err(DiskImageError::FilesystemError(
                        file_system::FileSystemError::InvalidBootSector(format!(
                            "{name} is {size} bytes; expected {BOOT_SECTOR_SIZE}"
                        )),
                    ));
                }
                candidates.push((name, fs::read(entry.path())?));
            }
        }
        Self::select_boot_sector_candidate(candidates)
    }

    #[cfg(feature = "zip")]
    fn discover_boot_sector_from_zip(path: &Path) -> Result<Option<Vec<u8>>, DiskImageError> {
        let file = fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(file_system::FileSystemError::from)
            .map_err(DiskImageError::FilesystemError)?;
        let mut candidate_indices = Vec::new();

        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(file_system::FileSystemError::from)
                .map_err(DiskImageError::FilesystemError)?;
            let Some(enclosed_path) = entry.enclosed_name()
            else {
                continue;
            };
            if !entry.is_file() || enclosed_path.components().count() != 1 {
                continue;
            }
            let Some(name) = enclosed_path.file_name().and_then(|name| name.to_str())
            else {
                continue;
            };
            if file_system::is_boot_sector_filename(name) {
                candidate_indices.push((name.to_string(), index));
            }
        }

        let mut candidates = Vec::with_capacity(candidate_indices.len());
        for (name, index) in candidate_indices {
            let mut entry = archive
                .by_index(index)
                .map_err(file_system::FileSystemError::from)
                .map_err(DiskImageError::FilesystemError)?;
            if entry.size() != BOOT_SECTOR_SIZE as u64 {
                return Err(DiskImageError::FilesystemError(
                    file_system::FileSystemError::InvalidBootSector(format!(
                        "{name} is {} bytes; expected {BOOT_SECTOR_SIZE}",
                        entry.size()
                    )),
                ));
            }
            let mut bytes = Vec::with_capacity(BOOT_SECTOR_SIZE);
            entry
                .read_to_end(&mut bytes)
                .map_err(file_system::FileSystemError::from)
                .map_err(DiskImageError::FilesystemError)?;
            candidates.push((name, bytes));
        }
        Self::select_boot_sector_candidate(candidates)
    }

    fn build_metasector(self) -> Result<DiskImage, DiskImageError> {
        if self.formatted {
            log::error!("MetaSector formatting not yet implemented");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let mut disk_image = DiskImage::create(self.standard_format.unwrap());
        disk_image.set_resolution(TrackDataResolution::MetaSector);

        // Do post-load processing as normal
        disk_image.post_load_process();

        // Clear dirty flag
        disk_image.clear_flag(DiskImageFlags::DIRTY);

        Ok(disk_image)
    }

    #[cfg(feature = "fat")]
    fn inject_files_from_path_fat(
        path: impl AsRef<Path>,
        mut disk_image: DiskImage,
        recursive: bool,
        must_fit: bool,
    ) -> Result<DiskImage, DiskImageError> {
        let path = path.as_ref();

        // Get the list of files to add to the disk image, honoring the 'recursive' flag
        let files = file_system::native::build_file_tree_with_options(path, recursive)
            .map_err(DiskImageError::FilesystemError)?;

        // Mount the filesystem
        let arc = disk_image.into_arc();
        let lock = NonTrackingDiskLock::new(arc);

        let mut fs = FatFileSystem::mount(lock.clone(), NullContext::default(), None)
            .map_err(DiskImageError::FilesystemError)?;

        let report = fs
            .write_file_tree(path, &files, must_fit)
            .map_err(DiskImageError::FilesystemError)?;
        log::debug!(
            "ImageBuilder::inject_files_from_path_fat(): Wrote {} files, {} directories, and {} bytes (complete: {})",
            report.files_written,
            report.directories_written,
            report.bytes_written,
            report.complete
        );
        fs.unmount();

        disk_image = match lock.read(NullContext::default()) {
            Ok(disk_image) => disk_image.clone(),
            Err(_) => {
                log::error!("ImageBuilder::inject_files_from_path(): Failed to get disk image from lock");
                return Err(DiskImageError::ParameterError);
            }
        };

        disk_image.post_load_process();

        Ok(disk_image)
    }

    #[cfg(all(feature = "fat", feature = "zip"))]
    fn inject_files_from_zip_fat(
        path: impl AsRef<Path>,
        mut disk_image: DiskImage,
        recursive: bool,
        must_fit: bool,
    ) -> Result<DiskImage, DiskImageError> {
        let mut source =
            file_system::zip_source::ZipFileSource::open(path, recursive).map_err(DiskImageError::FilesystemError)?;
        let files = source.tree().clone();

        let arc = disk_image.into_arc();
        let lock = NonTrackingDiskLock::new(arc);
        let mut fs = FatFileSystem::mount(lock.clone(), NullContext::default(), None)
            .map_err(DiskImageError::FilesystemError)?;

        let report = fs
            .write_file_tree_with(&files, must_fit, |entry| source.read_file(entry))
            .map_err(DiskImageError::FilesystemError)?;
        log::debug!(
            "ImageBuilder::inject_files_from_zip_fat(): Wrote {} files, {} directories, and {} bytes (complete: {})",
            report.files_written,
            report.directories_written,
            report.bytes_written,
            report.complete
        );
        fs.unmount();

        disk_image = match lock.read(NullContext::default()) {
            Ok(disk_image) => disk_image.clone(),
            Err(_) => {
                log::error!("ImageBuilder::inject_files_from_zip_fat(): Failed to get disk image from lock");
                return Err(DiskImageError::ParameterError);
            }
        };

        disk_image.post_load_process();
        Ok(disk_image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StandardFormat, TrackDataResolution};

    #[test]
    fn test_with_resolution() {
        let resolution = TrackDataResolution::BitStream;
        let builder = ImageBuilder::new().with_resolution(resolution);
        assert_eq!(builder.resolution, Some(resolution));
    }

    #[test]
    fn test_with_filesystem() {
        let builder = ImageBuilder::new().with_filesystem(FileSystemType::Fat12);
        assert_eq!(builder.filesystem, Some(FileSystemType::Fat12));
        assert!(builder.formatted);
    }

    #[test]
    fn test_with_creator_tag() {
        let tag = b"CREATOR";
        let builder = ImageBuilder::new().with_creator_tag(tag);
        assert_eq!(builder.creator_tag, Some(*b"CREATOR "));
    }

    #[test]
    fn test_build_bitstream() {
        let format = StandardFormat::PcFloppy360;
        let builder = ImageBuilder::new()
            .with_standard_format(format)
            .with_resolution(TrackDataResolution::BitStream);
        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_bitstream_formatted() {
        let format = StandardFormat::PcFloppy360;
        let builder = ImageBuilder::new()
            .with_standard_format(format)
            .with_resolution(TrackDataResolution::BitStream)
            .with_filesystem(FileSystemType::Fat12);

        let result = builder.build();
        assert!(result.is_ok());

        let mut disk = result.unwrap();
        for sector in format.layout().chsn_iter() {
            assert!(disk.read_sector_basic(sector.ch(), sector.into(), None).is_ok());
        }

        let write_vec = vec![0x55; 512];
        for sector in format.layout().chsn_iter() {
            assert!(disk
                .write_sector_basic(sector.ch(), sector.into(), None, &write_vec)
                .is_ok());
        }
    }

    #[test]
    fn test_build_metasector() {
        let format = StandardFormat::PcFloppy360;
        let builder = ImageBuilder::new()
            .with_standard_format(format)
            .with_resolution(TrackDataResolution::MetaSector);
        let result = builder.build();
        assert!(result.is_ok());
    }
    /*
    // TODO: Enable these tests when we have implemented formatting for MetaSector disks
    #[test]
    fn test_build_metasector_formatted() {
        let format = StandardFormat::PcFloppy360;
        let builder = ImageBuilder::new()
            .with_standard_format(format)
            .with_resolution(DiskDataResolution::MetaSector)
            .with_formatted(true);
        let result = builder.build();
        assert!(result.is_ok());


        let mut disk = result.unwrap();
        for sector in format.chsn().iter() {
            assert!(disk.read_sector_basic(sector.ch(), sector.into(), None).is_ok());
        }

        let write_vec = vec![0x55; 512];
        for sector in format.chsn().iter() {
            assert!(disk
                .write_sector_basic(sector.ch(), sector.into(), None, &write_vec)
                .is_ok());
        }
    }*/
}
