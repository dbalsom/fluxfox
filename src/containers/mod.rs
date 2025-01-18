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

//! An abstract conception of a 'container' for disk images. All disk images
//! have a container, which may be of several types of [DiskImageContainer]:
//!
//! * `ImageFile` - A transparent container abstraction over a disk image file existing as a single
//!                 file object on the native filesystem or within a known container.
//!                 All of fluxfox's disk image format parsers must eventually operate on Raw
//!                 `File`s.
//!
//! * `Archive  ` - A container abstraction over am archive format that may contain one or more disk
//!                 images. This may represent any of the known formats that a user may have placed
//!                 in a supported archive format, but also includes image 'standards' that are
//!                 compressed in some specific way:  
//!                 `IMZ` files are `zip` archives containing a single `IMA` file internally.  
//!                 `ADZ` files are simple `ADF` files compressed with `gzip`  
//!                 Archives may or may not be compressed - some formats like `tar` typically just
//!                 bundle files together.  
//!                 `Archive`s may be nested together, such as the common `tar.gz` scheme seen in
//!                 Linux and Unix systems.
//!
//! * `FileSet`   - A container abstraction over a grouping of related files that comprise a single
//!                 disk image. comprise several individual files, such as when
//!                 using the Kryoflux stream format, which stores an individual file for each
//!                 track imaged. A FileSet may also be nested within an `Archive` (which is
//!                 preferable for file handling)
//!                 Typically, if a user selects a file that is detected to belong to a `FileSet`,
//!                 the entire set will be resolved. This allows loading of Kryoflux stream images
//!                 by opening any file in the set. Obviously this is not possible with web targets,
//!                 drag-and-drop or other non-file-based input methods.
//!
//! FluxFox, with the appropriate feature flags enabled, can open archives an
//! attempt to detect the context - if there is a single disk image within the
//! archive, it will be extracted and treated as if it were any other disk image.
//!
//! If an archive contains multiple disk images, FluxFox will attempt to identify
//! all the disk images by path, and naturally sort them by path.
//!
//! A load operation without providing a discriminating path or index will result
//! in an error, as the container is unable to determine which disk image to load.
//!
//! A UI built around fluxfox could then display the list of detected disk images
//! in the archive and allow the user to re-try the operation with a specific image
//! specified.
//!
//! More abstractly, a container can also represent a uncompressed `glob` of disk
//! image files, such as a Kryoflux set, where the "container" is actually just
//! a vector of [Paths] to the track stream files that comprise the disk image.
//!
//! A `FileSet` container may also exist within an `FileArchive` if that format
//! supports multiple files (e.g. `zip` or `tar`).
//!
//! Containers may also be nested, as is frequently seen on linux with the
//! `tar.gz` nested container format. A `kryoflux_dump.tar.gz` would essentially
//! be three nested containers: A Kryoflux `FileSet` inside a `tar` archive inside
//! a `Gzip` archive.  

pub mod archive;
#[cfg(feature = "gzip")]
pub mod gzip;
#[cfg(feature = "zip")]
pub mod zip;

use std::{
    fmt::{Display, Formatter, Result},
    path::PathBuf,
};

use crate::containers::archive::FileArchiveType;

use crate::{DiskCh, DiskImageFileFormat};

#[derive(Clone, Debug)]
pub struct KryoFluxSet {
    pub base_path: PathBuf,
    pub file_set:  Vec<PathBuf>,
    pub geometry:  DiskCh,
}

#[derive(Clone, Debug)]
pub enum DiskImageContainer {
    /// During the process of loading a file container and analyzing it, sometimes we have the
    /// entire disk image file in memory as a vector. If we can determine this is the only relevant
    /// file in the container (or is a root level File container), we can return it directly as a
    /// `ResolvedFile` container. This is useful for handling single-file archives transparently.
    /// The final two [PathBuf] parameters specify the path to the file itself, and the path to any
    /// parent archive that might have contained it.
    /// Note that writing disk images back to nested [Archive]s is not supported.
    ResolvedFile(DiskImageFileFormat, Vec<u8>, Option<PathBuf>, Option<PathBuf>),
    /// A 'File' container represents no container. It is a single file either residing on the
    /// native filesystem or resolved from another container.
    /// It holds a [DiskImageFileFormat] and an optional [PathBuf] to the file. The path may only
    /// be present in certain contexts - receiving a file via drag and drop on the web may not have
    /// an associated [Path]. The [PathBuf] may also not represent a valid path on the native
    /// filesystem, such as when a file is resolved from an archive
    File(DiskImageFileFormat, Option<PathBuf>),
    /// An `Archive` container represents some archive type that may contain one or more files
    /// within it, optionally compressed. Examples of Archives include `tar`, `zip` and `gz`.
    /// An Archive stores a vector of [DiskImageContainer], allowing it to recursively hold files,
    /// file sets, or even other archives.
    Archive(FileArchiveType, Vec<(DiskImageContainer, PathBuf)>, Option<PathBuf>),
    /// A `FileSet` container represents a set of related files that comprise a single disk image.
    /// The primary example of this is the Kryoflux stream format, which stores an individual file
    /// for each track imaged. All files in a FileSet should belong to a specific [DiskImageFileFormat],
    /// which for all intents and purposes will be `KryofluxStream`.
    /// The final, optional [PathBuf] parameter represents the initial file used to resolve the set,
    /// if available.
    FileSet(DiskImageFileFormat, Vec<PathBuf>, Option<PathBuf>),
    /// A set of Kryoflux images zipped together.
    /// The outer vector represents the number of disks in the archive (the number of unique
    /// paths found to a kryoflux .*00.0.raw file). It stores a tuple, the first element of which is
    /// the path to the raw files, the second is a Vec containing the full path of all raws in that
    /// set, and the third is the geometry of that set as DiskCh.
    ZippedKryofluxSet(Vec<KryoFluxSet>),
    KryofluxSet,
}

impl Display for DiskImageContainer {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            DiskImageContainer::ResolvedFile(fmt, buf, _, _) => write!(f, "{:?} of {} bytes", fmt, buf.len()),
            DiskImageContainer::File(fmt, _) => write!(f, "{:?}", fmt),
            DiskImageContainer::Archive(archive_type, items, _) => {
                write!(f, "{:?} archive of {} items", archive_type, items.len())
            }
            DiskImageContainer::FileSet(fmt, items, _) => write!(f, "File set of {} {:?} images", items.len(), fmt),
            DiskImageContainer::ZippedKryofluxSet(_) => write!(f, "Zipped Kryoflux Image Set"),
            DiskImageContainer::KryofluxSet => write!(f, "Kryoflux Image Set"),
        }
    }
}
