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

    src/containers/zip.rs

    Code to handle a ZIP file container. Some disk images such as IMZ and
    Kryoflux raw dumps are stored in ZIP files. This module provides the
    utilities to handle these files.

*/
use crate::io::ReadSeek;
use crate::DiskImageError;
use std::io::Read;
use std::path::Path;
//const ZIP_SIGNATURE: &[u8; 4] = b"PK\x03\x04";

/// Return true if the provided image is a ZIP file.
#[allow(dead_code)]
pub struct ZipFileEntry {
    pub name: String,
    pub size: u64,
}
#[allow(dead_code)]
pub struct ZipFileListing {
    pub files: Vec<ZipFileEntry>,
    pub total_size: u64,
}

pub fn detect_zip<T: ReadSeek>(image_io: &mut T) -> (bool, usize, u64) {
    match zip::ZipArchive::new(image_io) {
        Ok(zip) => (true, zip.len(), zip.decompressed_size().unwrap_or(0) as u64),
        Err(_) => (false, 0, 0),
    }
}

#[allow(dead_code)]
pub fn file_ct<T: ReadSeek>(image_io: &mut T) -> Result<usize, DiskImageError> {
    let zip = zip::ZipArchive::new(image_io).map_err(|_| DiskImageError::FormatParseError)?;
    Ok(zip.len())
}

pub fn file_listing<T: ReadSeek>(image_io: &mut T) -> Result<ZipFileListing, DiskImageError> {
    let mut zip = zip::ZipArchive::new(image_io).map_err(|_| DiskImageError::FormatParseError)?;
    let mut files = Vec::new();
    let mut total_size = 0;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|_| DiskImageError::FormatParseError)?;
        if let Some(file_name) = file.enclosed_name() {
            files.push(ZipFileEntry {
                name: file_name.to_string_lossy().to_string(),
                size: file.size(),
            });
            total_size += file.size();
        }
    }

    Ok(ZipFileListing { files, total_size })
}

pub fn extract_file<T: ReadSeek>(image_io: &mut T, file_name: &Path) -> Result<Vec<u8>, DiskImageError> {
    let mut zip = zip::ZipArchive::new(image_io).map_err(|_| DiskImageError::FormatParseError)?;

    let mut file = zip
        .by_name(&file_name.to_string_lossy())
        .map_err(|_| DiskImageError::FormatParseError)?;

    // Sanity check, is file < 100MB? Let's not zip-bomb ourselves.
    if file.size() > 100_000_000 {
        return Err(DiskImageError::IoError("File too large".to_string()));
    }

    let mut file_buf = Vec::new();
    file.read_to_end(&mut file_buf)?;
    Ok(file_buf)
}

pub fn extract_first_file<T: ReadSeek>(image_io: &mut T) -> Result<Vec<u8>, DiskImageError> {
    let mut zip = zip::ZipArchive::new(image_io).map_err(|_| DiskImageError::FormatParseError)?;

    // No files in zip? Nothing we can do with that.
    if zip.is_empty() {
        return Err(DiskImageError::FormatParseError);
    }

    // Get the first file in the zip.
    let mut file = zip.by_index(0).map_err(|_| DiskImageError::FormatParseError)?;

    // Sanity check, is file < 100MB? Let's not zip-bomb ourselves.
    if file.size() > 100_000_000 {
        return Err(DiskImageError::IoError("File too large".to_string()));
    }

    // Read the entire first file.
    let mut file_buf = Vec::new();
    file.read_to_end(&mut file_buf)?;

    Ok(file_buf)
}
