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

//! # fluxfox
//!
//! fluxfox is a library crate for reading, writing, and manipulating floppy disk images of the
//! kind used with vintage IBM Personal Computers and compatibles.
//!
//! fluxfox is primarily designed for emulator authors who may be writing a PC emulator and would
//! like to support disk images in a variety of formats, however it can be used for visualization,
//! dumping, editing, and other disk image tasks.
//!
//! fluxfox currently supports several different disk image formats, both modern and vintage, of
//! flux, bitstream and sector-based resolution.
//!
//! The main interface to fluxfox is via a [`DiskImage`] object, which can be created by loading
//! a disk image file, or by creating a new disk image from scratch.
//!
//! It is recommended to use the [`ImageBuilder`] interface to load or create a disk image.

mod bit_ring;
pub mod bitstream_codec;
pub mod boot_sector;
mod containers;
mod detect;
pub mod diskimage;
mod file_parsers;
pub mod image_builder;
pub mod io;
mod random;
pub mod track_schema;
pub mod util;

mod copy_protection;
pub mod file_system;
pub mod flux;
mod image_writer;
pub mod prelude;
mod range_check;
pub mod track;
pub mod types;

mod disk_schema;
mod image_loader;
mod platform;
mod scripting;
mod sector_view;
pub mod source_map;
mod tree_map;
#[cfg(feature = "viz")]
pub mod visualization;

use std::{hash::RandomState, sync::Arc};
use thiserror::Error;

pub const MAXIMUM_SECTOR_SIZE: usize = 8192;
pub const DEFAULT_SECTOR_SIZE: usize = 512;
pub const ASCII_EOF: u8 = 0x1A;
/// The maximum cylinder any drive can seek to or that we will ever see in an image.
/// This is used for setting safe capacities for vectors and other data structures and track-based
/// normalization logic.
/// This may need to be adjusted if we ever see a disk image with more than 85 cylinders.
pub const MAX_CYLINDER: usize = 85;

#[allow(unused)]
pub type FoxHashMap<K, V, S = RandomState> = std::collections::HashMap<K, V, S>;
#[allow(unused)]
type FoxHashSet<T, S = RandomState> = std::collections::HashSet<T, S>;

/// The status of a disk image loading operation, for file parsers that support progress reporting.
pub enum LoadingStatus {
    /// Emitted by file parsers that support progress updates. This is sent before any other task
    /// is performed, to allow the caller time to prepare and display a progress bar.
    ProgressSupport,
    /// Emitted by file parsers that support progress updates to inform the caller of the current progress.
    /// The value is a floating-point number between 0.0 and 1.0, where 1.0 represents full completion.
    /// Note: The value 1.0 is not guaranteed to be emitted.
    Progress(f64),
    /// Emitted by file parsers to inform the caller that the loading operation is complete.
    Complete,
    /// Emitted by file parsers to inform the caller that an error occurred during the loading operation.
    Error,
}

pub type LoadingCallback = Arc<dyn Fn(LoadingStatus) + Send + Sync>;

#[derive(Clone, Debug, Error)]
pub enum DiskImageError {
    #[error("An IO error occurred reading or writing the disk image: {0}")]
    IoError(String),
    #[error("A filesystem error occurred or path not found")]
    FsError,
    #[error("An error occurred reading or writing a file archive: {0}")]
    ArchiveError(FileArchiveError),
    #[error("Unknown disk image format")]
    UnknownFormat,
    #[error("Unsupported disk image format for requested operation")]
    UnsupportedFormat,
    #[error("The disk image is valid but contains incompatible disk information: {0}")]
    IncompatibleImage(String),
    #[error("The disk image format parser encountered an error")]
    FormatParseError,
    #[error("The disk image format parser reported the image was corrupt: {0}")]
    ImageCorruptError(String),
    #[error("The requested head or cylinder could not be found")]
    SeekError,
    #[error("An error occurred addressing the track bitstream")]
    BitstreamError,
    #[error("The requested sector ID could not be found")]
    IdError,
    #[error("The requested operation matched multiple sector IDs")]
    UniqueIdError,
    #[error("No sectors were found on the current track")]
    DataError,
    #[error("No schema is defined for the current track")]
    SchemaError,
    #[error("A CRC error was detected in the disk image")]
    CrcError,
    #[error("An invalid function parameter was supplied")]
    ParameterError,
    #[error("Write-protect status prevents writing to the disk image")]
    WriteProtectError,
    #[error("Flux track has not been resolved")]
    ResolveError,
    #[error("An error occurred reading a multi-disk archive: {0}")]
    MultiDiskError(String),
    #[error("An error occurred attempting to lock a resource: {0}")]
    SyncError(String),
    #[error("The disk image was not compatible with the requested platform")]
    PlatformMismatch,
    #[error("The disk image was not compatible with the requested format")]
    FormatMismatch,
}

// Manually implement `From<io::Error>` for `DiskImageError`
impl From<io::Error> for DiskImageError {
    fn from(err: io::Error) -> Self {
        DiskImageError::IoError(err.to_string()) // You could convert in a different way
    }
}

impl From<FileArchiveError> for DiskImageError {
    fn from(err: FileArchiveError) -> Self {
        DiskImageError::ArchiveError(err)
    }
}

// Manually implement `From<binrw::Error>` for `DiskImageError`
impl From<binrw::Error> for DiskImageError {
    fn from(err: binrw::Error) -> Self {
        DiskImageError::IoError(err.to_string()) // Again, you could convert differently
    }
}

#[derive(Debug, Error)]
pub enum DiskVisualizationError {
    #[error("An invalid parameter was supplied: {0}")]
    InvalidParameter(String),
    #[error("No compatible tracks were found to visualize")]
    NoTracks,
    #[error("The disk image is not a valid format for visualization")]
    InvalidImage,
    #[error("The supplied parameters do not produce a visible visualization")]
    NotVisible,
}

// Re-export tiny_skia for convenience
pub use crate::{
    diskimage::DiskImage,
    file_parsers::{format_from_ext, supported_extensions, ImageFormatParser, ParserWriteCompatibility},
    image_builder::ImageBuilder,
    image_writer::ImageWriter,
    types::{DiskImageFileFormat, SectorMapEntry},
};

use types::{DiskCh, DiskChs, DiskChsn, DiskChsnQuery};
// Re-export tiny_skia for convenience
use crate::containers::archive::FileArchiveError;
pub use types::standard_format::StandardFormat;

pub type SectorId = DiskChsn;
pub type SectorIdQuery = DiskChsnQuery;
pub type DiskSectorMap = Vec<Vec<Vec<SectorMapEntry>>>;
