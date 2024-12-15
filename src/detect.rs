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
    containers::{
        archive::{FileArchiveType, StatelessFileArchive},
        DiskImageContainer,
    },
    file_parsers::ImageFormatParser,
    io::ReadSeek,
    types::{chs::DiskChs, standard_format::StandardFormat},
    util::natural_sort,
    DiskImageError,
    DiskImageFileFormat,
};
use std::path::PathBuf;

#[cfg(feature = "zip")]
use crate::{containers::KryoFluxSet, file_parsers::kryoflux::KfxFormat};

use strum::IntoEnumIterator;

/// Attempt to detect the container format of an input stream implementing [Read] + [Seek]. If the
/// format cannot be determined, `[DiskImageError::UnknownFormat]` is returned.
///
/// If at least one archive format feature is enabled, we can look into and identify images in
/// archives such as `zip`, `gzip`, and `tar`.
///
/// Most common of these are WinImage's "Compressed Disk Image" format, `IMZ`, which is simply
/// a `zip` file containing a single raw sector image with `IMA` extension. Similarly, `ADZ` files
/// are Amiga `ADF` images compressed with gzip.
///
/// However, we can extend this to support simple archive containers of other file image formats.
/// if an archive contains a single file, it will be assumed to be a disk image of some sort.
///
/// Given MartyPC's feature set, it can either mount the files within a zip as a FAT filesystem,
/// or it can let fluxfox mount the image inside the zip instead. Choosing which is the correct
/// behavior desired may not be trivial; so we assume a compressed disk image will only contain
/// one file. You could override loading by adding a second dummy file.
///
/// The exception to this are Kryoflux images which span multiple files, but these  are easily
/// differentiated due to their large size and regular naming conventions.
///
/// The smallest Kryoflux set I have seen is of a 160K disk, 5_741_334 bytes uncompressed.
/// Therefore, I assume a cutoff point of 5MB for Kryoflux sets. This may need to be tweaked
/// to support even older, lower capacity disks in the future.
///
pub fn detect_container_format<T: ReadSeek>(
    image_io: &mut T,
    path: Option<PathBuf>,
) -> Result<DiskImageContainer, DiskImageError> {
    #[cfg(any(feature = "zip", feature = "gzip", feature = "tar"))]
    {
        // First of all, is the input file an archive?
        if let Some(archive) = FileArchiveType::detect_archive_type(image_io) {
            log::debug!("Archive detected: {:?}", archive);

            // Get the archive info
            let a_info = archive.info(image_io)?;

            if a_info.file_count > 0 {
                log::debug!(
                    "{:?} archive file detected with {} files, total size: {}",
                    a_info.archive_type,
                    a_info.file_count,
                    a_info.total_size
                );
            }

            // If there's only one file, we can assume it should be a disk image
            if a_info.file_count == 1 {
                let (file_buf, file_path) = archive.extract_first_file(image_io)?;

                // Wrap buffer in Cursor, and send it through all our format detectors.
                let mut file_io = std::io::Cursor::new(file_buf);
                for format in DiskImageFileFormat::iter() {
                    if format.detect(&mut file_io) {
                        // If we made a detection, we can return this single file as a ResolvedFile
                        // container. The caller doesn't even need to know it was in an archive.
                        return Ok(DiskImageContainer::ResolvedFile(
                            format,
                            file_io.into_inner(),
                            Some(file_path),
                            path,
                        ));
                    }
                }

                return Err(DiskImageError::UnknownFormat);
            }
            else if a_info.total_size > 5_000_000 {
                // Multiple files in the zip, of at least 5MB - this assumes a Kryoflux set

                let file_listing = archive.file_listing(image_io)?;
                let path_vec: Vec<PathBuf> = file_listing
                    .files
                    .iter()
                    .map(|entry| PathBuf::from(&entry.name))
                    .collect();

                // Get all files that end in "00.0.raw" - should match the first file of any Kryoflux set.
                let mut raw_files: Vec<_> = path_vec
                    .iter()
                    .filter(|&path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.ends_with("00.0.raw"))
                    })
                    .collect();

                // Sort the matches using alphabetic natural sort. This is intended to match the first disk if
                // a zip archive has multiple disks in it.
                raw_files.sort_by(|a: &&PathBuf, b: &&PathBuf| natural_sort(a, b));
                log::debug!("Raw files: {:?}", raw_files);

                let mut set_vec = Vec::new();
                for file in raw_files {
                    log::debug!("Found .raw file in archive: {:?}", file);
                    let kryo_set = KfxFormat::expand_kryoflux_set(file.clone(), Some(path_vec.clone()))?;
                    log::debug!(
                        "Expanded to Kryoflux set of {} files, geometry: {}",
                        kryo_set.0.len(),
                        kryo_set.1
                    );

                    let path_to_set = file.parent().unwrap_or(&PathBuf::new()).to_path_buf();

                    set_vec.push(KryoFluxSet {
                        base_path: path_to_set.clone(),
                        file_set:  kryo_set.0,
                        geometry:  kryo_set.1,
                    });
                }

                if !set_vec.is_empty() {
                    for (si, set) in set_vec.iter().enumerate() {
                        log::debug!(
                            "Found Kryoflux set in archive at idx {}, path : {}",
                            si,
                            set.base_path.display()
                        );
                    }

                    return Ok(DiskImageContainer::ZippedKryofluxSet(set_vec));
                }
            }
        }
    }

    // Format is not an archive.
    for format in DiskImageFileFormat::iter() {
        if format.detect(&mut *image_io) {
            // If this a Kryoflux stream file, we need to resolve the set of files it belongs to.
            if let DiskImageFileFormat::KryofluxStream = format {
                return Ok(DiskImageContainer::KryofluxSet);
            }
            // Otherwise this must just be a plain File container.
            return Ok(DiskImageContainer::File(format, path));
        }
    }
    Err(DiskImageError::UnknownFormat)
}

/// Attempt to return a DiskChs structure representing the geometry of a disk image from the size of a raw sector image.
/// Returns None if the size does not match a known raw disk image size.
pub fn chs_from_raw_size(size: usize) -> Option<DiskChs> {
    match StandardFormat::try_from(size) {
        Ok(fmt) => Some(fmt.chs()),
        Err(_) => None,
    }
}
