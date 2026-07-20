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

#[cfg(feature = "serde")]
use serde;

use std::fmt::{self, Display, Formatter};
use thiserror::Error;

pub mod date_time;
#[cfg(feature = "fat")]
pub mod fat;
pub mod file_tree;
pub mod native;

pub use date_time::FsDateTime;
pub use file_tree::{FileEntry, FileNameType, FileTreeNode};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileSystemType {
    Fat12,
    Fat16,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FileSystemArchive {
    Zip,
    Tar,
}

impl Display for FileSystemArchive {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemArchive::Zip => write!(f, "Zip"),
            FileSystemArchive::Tar => write!(f, "Tar"),
        }
    }
}

impl FileSystemArchive {
    pub fn ext(&self) -> &str {
        match self {
            FileSystemArchive::Zip => ".zip",
            FileSystemArchive::Tar => ".tar",
        }
    }
}

/// [FileSystemError] is the error type for FileSystem implementations.
#[derive(Clone, Debug, Error)]
pub enum FileSystemError {
    #[error("An IO error occurred reading or writing the disk image: {0}")]
    IoError(String),
    #[error("The filesystem is not mounted")]
    NotMountedError,
    #[error("The filesystem is empty")]
    EmptyFileSystem,
    #[error("An error occurred mounting the file system: {0}")]
    MountError(String),
    #[error("An error occurred reading a file: {0}")]
    ReadError(String),
    #[error("An error occurred writing a file: {0}")]
    WriteError(String),
    #[error("An archive error occurred: {0}")]
    ArchiveError(String),
    #[error("The requested path was not found: {0}")]
    PathNotFound(String),
    #[error("Feature {0} option required but not compiled.")]
    FeatureError(String),
    #[error("A cycle was detected in the file system. Cyclical symlinks are not supported.")]
    CycleError,
    #[error("A filesystem object was detected that was not a file or directory: {0}")]
    UnsupportedFileObject(String),
}

impl From<crate::io::Error> for FileSystemError {
    fn from(e: crate::io::Error) -> Self {
        FileSystemError::IoError(e.to_string())
    }
}

#[cfg(feature = "zip")]
impl From<zip::result::ZipError> for FileSystemError {
    fn from(e: zip::result::ZipError) -> Self {
        FileSystemError::ArchiveError(e.to_string())
    }
}
