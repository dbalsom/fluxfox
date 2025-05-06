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

use crate::{
    disk_lock::{DiskLock, NonTrackingDiskLock, NullContext},
    file_system,
    file_system::{fat::fat_fs::FatFileSystem, FileSystemError, FileSystemType, FileTreeNode},
    types::{DiskImageFlags, TrackDataResolution},
    DiskImage,
    DiskImageError,
    StandardFormat,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

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

    /// Set whether the [DiskImage] to be built should be formatted as the specified [FileSystemType],
    /// containing files from the specified path.
    /// # Arguments:
    /// * `path` - The path to the directory containing the files to be added to the [DiskImage].
    /// * `filesystem` - The [FileSystemType] to use for the [DiskImage].
    /// * `recursive` - If `true`, files will be added recursively from the specified path, creating
    ///      subdirectories as necessary. If `false`, only files in the specified directory will be
    ///      added in the root directory of the [DiskImage].
    /// * `must_fit` - Whether the files must fit on the disk image. If false, files will be added
    ///      to the disk image until it is full. If true, an error will be returned if the
    ///      files do not fit on the disk image.
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
        self.recursive = recursive;
        self.must_fit = must_fit;
        self
    }

    /// Set whether the [DiskImage] to be built should be formatted as the specified [FileSystemType],
    /// containing files from the specified archive. The archive must be in a format supported and
    /// enabled by the correct feature flag.
    /// # Arguments:
    /// * `path` - The path to the directory containing the archive to be added to the [DiskImage].
    /// * `filesystem` - The [FileSystemType] to use for the [DiskImage].
    /// * `recursive` - If `true`, files will be added recursively from the specified path, creating
    ///      subdirectories as necessary. If `false`, only files in the specified directory will be
    ///      added in the root directory of the [DiskImage].
    /// * `must_fit` - Whether the files must fit on the disk image. If false, files will be added
    ///      to the disk image until it is full. If true, an error will be returned if the
    ///      files do not fit on the disk image.
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

    /// Build the [`DiskImage`] using the specified parameters.
    pub fn build(self) -> Result<DiskImage, DiskImageError> {
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
                None,
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
                    disk_image = Self::inject_files_from_path_fat(&path, disk_image, self.recursive, self.must_fit)?;
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

        // Do post-load processing as normal
        //disk_image.post_load_process();

        // Clear dirty flag
        disk_image.clear_flag(DiskImageFlags::DIRTY);

        Ok(disk_image)
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

    fn inject_files_from_path_fat(
        path: impl AsRef<Path>,
        mut disk_image: DiskImage,
        recursive: bool,
        must_fit: bool,
    ) -> Result<DiskImage, DiskImageError> {
        let path = path.as_ref();
        let base_path = path.to_path_buf().to_string_lossy().to_string();

        // Get the list of files to add to the disk image, honoring the 'recursive' flag
        let files = file_system::native::build_file_tree(path).map_err(|e| DiskImageError::FilesystemError(e))?;

        // Mount the filesystem
        let arc = disk_image.into_arc();
        let lock = NonTrackingDiskLock::new(arc);

        let mut fs = FatFileSystem::mount(lock.clone(), NullContext::default(), None)
            .map_err(|e| DiskImageError::FilesystemError(e))?;

        // match files.iter().try_for_each(|filename| {
        //     let relative_filename = filename.trim_start_matches(&base_path);
        //
        //     log::debug!(
        //         "ImageBuilder::inject_files_from_path(): Adding file {:?} to disk image at {}",
        //         filename,
        //         relative_filename
        //     );
        //
        //     fs.create_file(filename, None, None)
        //         .map_err(|e| DiskImageError::FilesystemError(e))?;
        //
        //     let data = fs::read(filename).map_err(|e| DiskImageError::IoError(e.to_string()))?;
        //
        //     match fs.write_file(relative_filename, &data) {
        //         Ok(_) => Ok(()),
        //         Err(e) => {
        //             if must_fit {
        //                 Err(DiskImageError::FilesystemError(e))
        //             }
        //             else {
        //                 Ok(())
        //             }
        //         }
        //     }
        // }) {
        //     Ok(_) => {
        //         // There's no good way to get the disk image back out of the lock, so we'll just clone it
        //         disk_image = match lock.read(NullContext::default()) {
        //             Ok(disk_image) => disk_image.clone(),
        //             Err(_) => {
        //                 log::error!("ImageBuilder::inject_files_from_path(): Failed to get disk image from lock");
        //                 return Err(DiskImageError::ParameterError);
        //             }
        //         };
        //
        //         Ok(disk_image)
        //     }
        //     Err(e) => Err(e),
        // }

        Self::build_fat(&files, &mut fs, &HashSet::new()).map_err(|e| DiskImageError::FilesystemError(e))?;

        disk_image = match lock.read(NullContext::default()) {
            Ok(disk_image) => disk_image.clone(),
            Err(_) => {
                log::error!("ImageBuilder::inject_files_from_path(): Failed to get disk image from lock");
                return Err(DiskImageError::ParameterError);
            }
        };

        Ok(disk_image)
    }

    fn build_fat(
        dir_node: &FileTreeNode,
        fs: &mut FatFileSystem,
        visited: &HashSet<String>,
    ) -> Result<(), FileSystemError> {
        for (i, entry) in dir_node.children().iter().enumerate() {
            log::debug!("Processing child #{}, {}", i, entry.path());

            match entry {
                FileTreeNode::File(file) => {
                    let path = file.path();
                    if visited.contains(path) {
                        log::debug!("Skipping previously installed file: {:?}", path);
                        continue;
                    }
                    log::debug!("Adding file {:?}", path);
                    let data = fs::read(file.path())?;
                    fs.create_file(path, None, None)?;
                    fs.write_file(path, &data)?;
                }
                FileTreeNode::Directory { dfe: dir, children: _ } => {
                    let path = dir.path();
                    if path != "/" {
                        if visited.contains(path) {
                            log::debug!("Skipping previously installed directory: {:?}", path);
                            continue;
                        }
                        log::debug!("Adding directory {:?}", path);
                        fs.create_dir(path)?;
                    }

                    Self::build_fat(entry, fs, visited)?;
                }
            }
        }
        Ok(())
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
