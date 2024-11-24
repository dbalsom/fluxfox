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
use crate::{
    file_system::{FileEntry, FileEntryType, FileSystemError, FileTreeNode},
    io::{Cursor, Seek},
    DiskImage,
    DiskImageFileFormat,
    ImageParser,
};
use fluxfox_fat::{Dir, FileSystem, FsOptions, OemCpConverter, ReadWriteSeek, StdIoWrapper, TimeProvider};
use std::{
    io::Read,
    sync::{Arc, RwLock},
};

pub struct FatFileSystem {
    fat: Option<Arc<FileSystem<StdIoWrapper<Cursor<Vec<u8>>>>>>,
}

impl FatFileSystem {
    pub fn mount(disk_lock: Arc<RwLock<DiskImage>>) -> Result<Self, FileSystemError> {
        log::debug!(
            "FatFileSystem::mount(): Attempting to lock disk image for writing with {} references...",
            Arc::strong_count(&disk_lock)
        );
        let disk = &mut disk_lock.write().unwrap();

        // Create a buffer to hold the disk image
        let img_buf = Vec::new();
        let mut cursor = Cursor::new(img_buf);
        // Convert the disk image into a buffer
        match DiskImageFileFormat::RawSectorImage.save_image(disk, &mut cursor) {
            Ok(_) => {}
            Err(e) => return Err(FileSystemError::MountError(e.to_string())),
        }

        // Reset the cursor to the beginning of the buffer or the mount will fail
        cursor.seek(std::io::SeekFrom::Start(0)).unwrap();

        // Mount the filesystem
        let fat = match FileSystem::new(cursor, FsOptions::new()) {
            Ok(fs) => fs,
            Err(e) => return Err(FileSystemError::MountError(e.to_string())),
        };

        Ok(Self {
            fat: Some(Arc::new(fat)),
        })
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

            self.list_files_recursive(&dir, &mut files);
        }
        files
    }

    pub fn list_files_recursive<IO: ReadWriteSeek, TP: TimeProvider, OCC: OemCpConverter>(
        &self,
        dir: &Dir<IO, TP, OCC>,
        files: &mut Vec<String>,
    ) {
        for r in dir.iter() {
            if let Ok(entry) = r {
                if entry.is_dir() {
                    // Ignore the current and parent directory entries to avoid infinite recursion
                    if entry.short_file_name() == "." || entry.short_file_name() == ".." {
                        continue;
                    }
                    log::debug!("descending into dir: {}", entry.short_file_name());
                    let sub_dir = entry.to_dir();
                    self.list_files_recursive(&sub_dir, files);
                }
                else if entry.is_file() {
                    files.push(entry.short_file_name());
                }
            }
        }
    }

    pub fn build_file_tree_from_root(&self) -> Option<FileTreeNode> {
        if let Some(fat) = &self.fat {
            let root_dir = fat.root_dir();
            let mut path_stack = Vec::new();
            Some(self.build_file_tree_recursive("", &root_dir, &mut path_stack))
        }
        else {
            None
        }
    }

    pub fn build_file_tree_recursive<IO: ReadWriteSeek, TP: TimeProvider, OCC: OemCpConverter>(
        &self,
        dir_name: &str,
        dir: &Dir<IO, TP, OCC>,
        path_stack: &mut Vec<String>,
    ) -> FileTreeNode {
        let mut children = Vec::new();
        path_stack.push(dir_name.to_string());

        for r in dir.iter() {
            if let Ok(entry) = r {
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
                    children.push(self.build_file_tree_recursive(&entry_name, &sub_dir, path_stack));
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
