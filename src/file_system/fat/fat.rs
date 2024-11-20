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
    file_system::FileSystemError,
    io::{Cursor, Seek},
    DiskImage,
    DiskImageFileFormat,
    ImageParser,
};
use fluxfox_fat::{Dir, DirEntry, FileSystem, FsOptions, OemCpConverter, ReadWriteSeek, StdIoWrapper, TimeProvider};
use std::sync::Arc;

pub struct FatFileSystem {
    fat: Option<Arc<FileSystem<StdIoWrapper<Cursor<Vec<u8>>>>>>,
}

impl FatFileSystem {
    pub fn mount(disk: &mut DiskImage) -> Result<Self, FileSystemError> {
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
                    let sub_dir = entry.to_dir();
                    self.list_files_recursive(&sub_dir, files);
                }
                else if entry.is_file() {
                    files.push(entry.short_file_name());
                }
            }
        }
    }
}
