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

#[cfg(feature = "zip")]
use crate::io::Write;

#[cfg(feature = "tar")]
use crate::io::Cursor;

use crate::{
    disk_lock::{DiskLock, LockContext, NonTrackingDiskLock},
    file_system::{
        file_tree::{FileEntry, FileEntryType, FileNameType, FileTreeNode},
        FileSystemArchive,
        FileSystemError,
    },
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
    ///
    ///
    pub fn mount<L, C>(disk_lock: L, lock_context: C, format: Option<StandardFormat>) -> Result<Self, FileSystemError>
    where
        L: DiskLock<DiskImage, C> + Into<NonTrackingDiskLock<DiskImage>>,
        C: LockContext,
    {
        log::debug!(
            "FatFileSystem::mount(): Attempting to lock disk image for writing with {} references...",
            disk_lock.strong_count()
        );

        // If a format was not provided, attempt to auto-detect the format
        let format = match format {
            Some(f) => Some(f),
            None => disk_lock.read(lock_context).unwrap().closest_format(true),
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
            Some(Self::build_file_tree_recursive(None, &root_dir, &mut path_stack))
        }
        else {
            None
        }
    }

    pub fn build_file_tree_recursive<IO: ReadWriteSeek, TP: TimeProvider, OCC: OemCpConverter>(
        dir_entry: Option<&fluxfox_fat::DirEntry<IO, TP, OCC>>,
        dir: &Dir<IO, TP, OCC>,
        path_stack: &mut Vec<String>,
    ) -> FileTreeNode {
        let mut children = Vec::new();
        if let Some(dir_entry) = dir_entry {
            path_stack.push(dir_entry.short_file_name());
        }

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
                children.push(Self::build_file_tree_recursive(Some(&entry), &sub_dir, path_stack));
            }
            else if entry.is_file() {
                // log::debug!(
                //     "adding file: {} modified date: {}",
                //     entry_name,
                //     FsDateTime::from(entry.modified())
                // );
                children.push(FileTreeNode::File(FileEntry {
                    e_type: FileEntryType::File,
                    short_name: entry_name,
                    long_name: Some(entry.file_name()),
                    size: entry_size,
                    path: full_path,
                    created: None, // Created date was not implemented by DOS. Added in NT + later
                    modified: Some(entry.modified().into()),
                }));
            }
        }

        let node = FileTreeNode::Directory {
            dfe: FileEntry {
                e_type: FileEntryType::Directory,
                short_name: dir_entry.map(|e| e.short_file_name()).unwrap_or_default(),
                long_name: dir_entry.map(|e| Some(e.file_name())).unwrap_or_else(|| None),
                path: if path_stack.len() < 2 {
                    "/".to_string()
                }
                else {
                    path_stack.join("/")
                },
                size: 0, // Directory size can be calculated if needed
                created: None,
                modified: dir_entry.map(|e| e.modified().into()),
            },
            children,
        };

        // Pop the current directory name from the path stack
        path_stack.pop();

        node
    }

    #[cfg(any(feature = "zip", feature = "tar"))]
    pub fn root_as_archive(&mut self, archive_type: FileSystemArchive) -> Result<Vec<u8>, FileSystemError> {
        let root_node;
        if let Some(fat) = &self.fat {
            let root_dir = fat.root_dir();
            root_node = Self::build_file_tree_recursive(None, &root_dir, &mut Vec::new());
        }
        else {
            return Err(FileSystemError::MountError("Filesystem not mounted".to_string()));
        }
        self.node_as_archive(&root_node, true, FileNameType::Short, archive_type)
    }

    #[cfg(any(feature = "zip", feature = "tar"))]
    pub fn path_as_archive(
        &mut self,
        path: &str,
        recursive: bool,
        archive_type: FileSystemArchive,
    ) -> Result<Vec<u8>, FileSystemError> {
        let root_node;
        if let Some(fat) = &self.fat {
            let root_dir = fat.root_dir();
            root_node = Self::build_file_tree_recursive(None, &root_dir, &mut Vec::new());
        }
        else {
            return Err(FileSystemError::MountError("Filesystem not mounted".to_string()));
        }

        // Resolve the path to the node
        if let Some(node) = root_node.node(path) {
            self.node_as_archive(node, recursive, FileNameType::Short, archive_type)
        }
        else {
            Err(FileSystemError::PathNotFound(path.to_string()))
        }
    }

    #[cfg(any(feature = "zip", feature = "tar"))]
    pub fn node_as_archive(
        &mut self,
        node: &FileTreeNode,
        recursive: bool,
        name_type: FileNameType,
        archive_type: FileSystemArchive,
    ) -> Result<Vec<u8>, FileSystemError> {
        let archive_data = match archive_type {
            FileSystemArchive::Zip => {
                #[cfg(feature = "zip")]
                {
                    self.node_as_zip(node, recursive, name_type)?
                }
                #[cfg(not(feature = "zip"))]
                {
                    return Err(FileSystemError::FeatureError("zip".to_string()));
                }
            }
            FileSystemArchive::Tar => {
                #[cfg(feature = "tar")]
                {
                    self.node_as_tar(node, recursive, name_type)?
                }
                #[cfg(not(feature = "tar"))]
                {
                    return Err(FileSystemError::FeatureError("tar".to_string()));
                }
            }
        };
        Ok(archive_data)
    }

    #[cfg(feature = "zip")]
    pub fn node_as_zip(
        &mut self,
        node: &FileTreeNode,
        recursive: bool,
        name_type: FileNameType,
    ) -> Result<Vec<u8>, FileSystemError> {
        log::debug!("node_as_zip(): Building zip archive from node: {}", node);
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();

        let file_list = node.file_paths(recursive, name_type);
        if file_list.is_empty() {
            return Err(FileSystemError::EmptyFileSystem);
        }

        for file_path in file_list {
            log::debug!("node_as_zip(): Adding file to zip: {}", file_path);
            let file_data = self.read_file(&file_path)?;
            writer.start_file_from_path(file_path, options)?;
            writer.write_all(&file_data)?;
        }

        let zip_data = writer.finish()?;
        Ok(zip_data.into_inner())
    }

    #[cfg(feature = "tar")]
    pub fn node_as_tar(
        &mut self,
        node: &FileTreeNode,
        recursive: bool,
        name_type: FileNameType,
    ) -> Result<Vec<u8>, FileSystemError> {
        log::debug!("node_as_tar(): Building tarfile archive from node: {}", node);

        let mut builder = tar::Builder::new(Vec::new());

        let file_list = node.file_paths(recursive, name_type);
        if file_list.is_empty() {
            return Err(FileSystemError::EmptyFileSystem);
        }

        for file_path in file_list {
            let mut header = tar::Header::new_gnu();
            log::debug!("node_as_tar(): Adding file to tarfile: {}", file_path);
            let file_data = self.read_file(&file_path)?;

            header.set_size(file_data.len() as u64);
            header.set_cksum();
            // Remove the leading slash from the file path so that the tar is relative
            builder.append_data(&mut header, file_path.trim_start_matches('/'), Cursor::new(file_data))?;
        }

        builder.finish()?;
        let tar_data = builder.into_inner()?;
        Ok(tar_data)
    }
}
