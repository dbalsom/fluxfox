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
use std::sync::{Arc, RwLock};

use crate::{
    file_system::{FileEntry, FileEntryType, FileSystemError, FileTreeNode},
    io::{Read, Seek},
    sector_view::StandardSectorView,
    DiskImage,
    StandardFormat,
};

use fluxfox_fat::{Dir, FileSystem, FsOptions, OemCpConverter, ReadWriteSeek, StdIoWrapper, TimeProvider};

pub struct FatFileSystem {
    fat: Option<FileSystem<StdIoWrapper<StandardSectorView>>>,
}

impl FatFileSystem {
    /// Mount a FAT filesystem from a disk image.
    ///
    /// # Arguments
    /// - `disk_lock`: A reference-counted `RwLock` wrapping a `DiskImage` object.
    /// - `format`: An optional `StandardFormat` to use when mounting the filesystem. This can
    ///             be used to override auto-detection of the disk format.
    pub fn mount(disk_lock: Arc<RwLock<DiskImage>>, format: Option<StandardFormat>) -> Result<Self, FileSystemError> {
        log::debug!(
            "FatFileSystem::mount(): Attempting to lock disk image for writing with {} references...",
            Arc::strong_count(&disk_lock)
        );

        // If a format was not provided, attempt to auto-detect the format
        let format = match format {
            Some(f) => Some(f),
            None => disk_lock.read().unwrap().closest_format(true),
        };

        if format.is_none() {
            // Auto-detection failed. We can't mount the filesystem.
            return Err(FileSystemError::MountError(
                "Could not auto-detect disk format".to_string(),
            ));
        }

        // Move the arc into the view without cloning.
        let mut view = StandardSectorView::new(disk_lock, format.unwrap())
            .map_err(|e| FileSystemError::MountError(e.to_string()))?;

        // Reset the cursor to the beginning of the view or the mount will fail
        view.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| FileSystemError::MountError(e.to_string()))?;

        // Mount the filesystem
        let fat = match FileSystem::new(view, FsOptions::new()) {
            Ok(fs) => fs,
            Err(e) => return Err(FileSystemError::MountError(e.to_string())),
        };

        Ok(Self { fat: Some(fat) })
    }

    pub fn unmount(&mut self) {
        self.fat = None;
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FileSystemError> {
        if let Some(fat) = &self.fat {
            let mut file = fat
                .root_dir()
                .open_file(path)
                .map_err(|e| FileSystemError::ReadError(e.to_string()))?;
            let mut data = Vec::new();

            match file.read_to_end(&mut data) {
                Ok(_) => Ok(data),
                Err(e) => Err(FileSystemError::ReadError(e.to_string())),
            }
        }
        else {
            Err(FileSystemError::MountError("Filesystem not mounted".to_string()))
        }
    }

    pub fn list_all_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        if let Some(fat) = &self.fat {
            let dir = fat.root_dir();

            Self::list_files_recursive(&dir, &mut files);
        }
        files
    }

    pub fn list_files_recursive<IO: ReadWriteSeek, TP: TimeProvider, OCC: OemCpConverter>(
        dir: &Dir<IO, TP, OCC>,
        files: &mut Vec<String>,
    ) {
        for entry in dir.iter().flatten() {
            if entry.is_dir() {
                // Ignore the current and parent directory entries to avoid infinite recursion
                if entry.short_file_name() == "." || entry.short_file_name() == ".." {
                    continue;
                }
                log::debug!("descending into dir: {}", entry.short_file_name());
                let sub_dir = entry.to_dir();
                Self::list_files_recursive(&sub_dir, files);
            }
            else if entry.is_file() {
                files.push(entry.short_file_name());
            }
        }
    }

    pub fn build_file_tree_from_root(&self) -> Option<FileTreeNode> {
        if let Some(fat) = &self.fat {
            let root_dir = fat.root_dir();
            let mut path_stack = Vec::new();
            Some(Self::build_file_tree_recursive("", &root_dir, &mut path_stack))
        }
        else {
            None
        }
    }

    pub fn build_file_tree_recursive<IO: ReadWriteSeek, TP: TimeProvider, OCC: OemCpConverter>(
        dir_name: &str,
        dir: &Dir<IO, TP, OCC>,
        path_stack: &mut Vec<String>,
    ) -> FileTreeNode {
        let mut children = Vec::new();
        path_stack.push(dir_name.to_string());

        for entry in dir.iter().flatten() {
            let entry_name = entry.short_file_name();
            let entry_size = entry.len();
            let full_path = format!("{}/{}", path_stack.join("/"), entry_name);

            if entry.is_dir() {
                // Ignore the current and parent directory entries to avoid infinite recursion
                if entry_name == "." || entry_name == ".." {
                    continue;
                }
                log::debug!("descending into dir: {}", entry_name);
                let sub_dir = entry.to_dir();
                children.push(Self::build_file_tree_recursive(&entry_name, &sub_dir, path_stack));
            }
            else if entry.is_file() {
                children.push(FileTreeNode::File(FileEntry {
                    e_type: FileEntryType::File,
                    name:   entry_name,
                    size:   entry_size,
                    path:   full_path,
                }));
            }
        }

        let node = FileTreeNode::Directory {
            fs: FileEntry {
                e_type: FileEntryType::Directory,
                name:   dir_name.to_string(),
                path:   if path_stack.len() < 2 {
                    "/".to_string()
                }
                else {
                    path_stack.join("/")
                },
                size:   0, // Directory size can be calculated if needed
            },
            children,
        };

        // Pop the current directory name from the path stack
        path_stack.pop();

        node
    }
}
