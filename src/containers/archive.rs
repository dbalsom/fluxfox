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

#![cfg(any(feature = "zip", feature = "tar", feature = "gzip"))]

#[cfg(feature = "zip")]
use super::zip;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;

#[cfg(feature = "gzip")]
use super::gzip;

use crate::io::ReadSeek;
use thiserror::Error;

///! FluxFox, with the appropriate feature flags enabled, can open archives an
///! attempt to detect the context - if there is a single disk image within the
///! archive, it can be extracted and treated as if it were any other disk image.
///!
///! If an archive contains multiple disk images, FluxFox will attempt to identify
///! all the disk images by path, and naturally sort them by path.
///!
///! A load operation without providing a discriminating path or index will result
///! in an error, as the container is unable to determine which disk image to load.
///!
///! A UI built around fluxfox could then display the list of detected disk images
///! in the archive and allow the user to re-try the operation with a specific image
///! specified.

/// An error type for file archive operations, using thiserror for convenience.
#[derive(Clone, Debug, Error)]
pub enum FileArchiveError {
    #[error("An IO error occurred reading or writing the file archive: {0}")]
    IoError(String),
    #[error("A filename or path was not found during an operation on the archive: {0}")]
    PathError(String),
    #[error("The current archive does not support the requested operation: {0}")]
    UnsupportedOperation(String),
    #[error("The archive backend reported an unexpected error: {0}")]
    OtherError(String),
    #[error("No files were found in the archive")]
    EmptyArchive,
}

/// A list of supported archive types.
#[derive(Copy, Clone, Debug, strum::EnumIter)]
pub enum FileArchiveType {
    Zip,
    Tar,
    Gzip,
}

impl FileArchiveType {
    pub fn verb(&self) -> &str {
        match self {
            FileArchiveType::Zip => "Zipped",
            FileArchiveType::Tar => "Tarred",
            FileArchiveType::Gzip => "GZipped",
        }
    }
}

#[allow(dead_code)]
pub struct ArchiveFileEntry {
    pub name: String,
    pub size: u64,
}

#[allow(dead_code)]
pub struct ArchiveFileListing {
    pub files: Vec<ArchiveFileEntry>,
    pub total_size: u64,
}

pub struct ArchiveInfo {
    pub archive_type: FileArchiveType,
    pub file_count:   usize,
    pub total_size:   u64,
}

/// Define a simple interface trait for various file archive types to implement.
pub trait StatelessFileArchive {
    fn detect_archive_type<T: ReadSeek>(image_io: &mut T) -> Option<FileArchiveType> {
        FileArchiveType::iter().find(|&archive_type| archive_type.detect(image_io))
    }
    fn detect<T: ReadSeek>(&self, image_io: &mut T) -> bool;
    fn info<T: ReadSeek>(&self, image_io: &mut T) -> Result<ArchiveInfo, FileArchiveError>;
    #[allow(dead_code)]
    fn file_ct<T: ReadSeek>(&self, image_io: &mut T) -> Result<usize, FileArchiveError>;
    fn file_listing<T: ReadSeek>(&self, image_io: &mut T) -> Result<ArchiveFileListing, FileArchiveError>;
    #[allow(dead_code)]
    fn extract_file<T: ReadSeek>(&self, image_io: &mut T, file_name: &Path) -> Result<Vec<u8>, FileArchiveError>;
    fn extract_first_file<T: ReadSeek>(&self, image_io: &mut T) -> Result<(Vec<u8>, PathBuf), FileArchiveError>;
}

impl StatelessFileArchive for FileArchiveType {
    fn detect<T: ReadSeek>(&self, image_io: &mut T) -> bool {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::detect(image_io),
            FileArchiveType::Tar => false,
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => gzip::detect(image_io),
            _ => false,
        }
    }

    fn info<T: ReadSeek>(&self, image_io: &mut T) -> Result<ArchiveInfo, FileArchiveError> {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::info(image_io),
            FileArchiveType::Tar => todo!(),
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => gzip::info(image_io),
            _ => Err(FileArchiveError::UnsupportedOperation(
                "No archive enabled!".to_string(),
            )),
        }
    }

    fn file_ct<T: ReadSeek>(&self, image_io: &mut T) -> Result<usize, FileArchiveError> {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::file_ct(image_io),
            FileArchiveType::Tar => todo!(),
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => Ok(1),
            _ => Err(FileArchiveError::UnsupportedOperation(
                "No archive enabled!".to_string(),
            )),
        }
    }

    fn file_listing<T: ReadSeek>(&self, image_io: &mut T) -> Result<ArchiveFileListing, FileArchiveError> {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::file_listing(image_io),
            FileArchiveType::Tar => todo!(),
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => Ok(ArchiveFileListing {
                files: vec![ArchiveFileEntry {
                    name: "file".to_string(),
                    size: 0,
                }],
                total_size: 0,
            }),
            _ => Err(FileArchiveError::UnsupportedOperation(
                "No archive enabled!".to_string(),
            )),
        }
    }

    fn extract_file<T: ReadSeek>(&self, image_io: &mut T, file_name: &Path) -> Result<Vec<u8>, FileArchiveError> {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::extract_file(image_io, file_name),
            FileArchiveType::Tar => todo!(),
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => gzip::extract(image_io),
            _ => Err(FileArchiveError::UnsupportedOperation(
                "No archive enabled!".to_string(),
            )),
        }
    }

    fn extract_first_file<T: ReadSeek>(&self, image_io: &mut T) -> Result<(Vec<u8>, PathBuf), FileArchiveError> {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "zip")]
            FileArchiveType::Zip => zip::extract_first_file(image_io),
            FileArchiveType::Tar => todo!(),
            #[cfg(feature = "gzip")]
            FileArchiveType::Gzip => match gzip::extract(image_io) {
                Ok(data) => Ok((data, PathBuf::from("file"))),
                Err(e) => Err(e),
            },
            _ => Err(FileArchiveError::UnsupportedOperation(
                "No archive enabled!".to_string(),
            )),
        }
    }
}
