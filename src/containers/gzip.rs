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
use crate::{
    containers::archive::ArchiveInfo,
    io::{Read, ReadSeek, SeekFrom},
};

use crate::containers::archive::{FileArchiveError, FileArchiveType};
use flate2::read::GzDecoder;

// Only support deflate-based gzips
const GZIP_SIGNATURE: &[u8; 3] = b"\x1F\x8B\x08";
const MAX_FILE_SIZE: u64 = 100_000_000;

/// Return true if the provided image is a ZIP file.
#[allow(dead_code)]
pub struct GZipFileEntry {
    pub name: String,
    pub size: u64,
}
#[allow(dead_code)]
pub struct GZipFileListing {
    pub files: Vec<GZipFileEntry>,
    pub total_size: u64,
}

pub fn detect<T: ReadSeek>(image_io: &mut T) -> bool {
    let mut buf = [0u8; 3];
    image_io.seek(SeekFrom::Start(0)).ok();
    if image_io.read_exact(&mut buf).is_err() {
        return false;
    }
    buf == *GZIP_SIGNATURE
}

// pub struct ArchiveInfo {
//     pub archive_type: FileArchiveType,
//     pub file_count:   usize,
//     pub total_size:   u64,
// }
pub fn info<T: ReadSeek>(image_io: &mut T) -> Result<ArchiveInfo, FileArchiveError> {
    if let Some(header) = GzDecoder::new(image_io).header() {
        log::debug!("Gzip::info(): GZIP header: {:?}", header);
    }

    Ok(ArchiveInfo {
        archive_type: FileArchiveType::Gzip,
        file_count:   1,
        total_size:   0, // Not sure if there's a way to get the total size of the archive
    })
}

/// Reads and decompresses a GZIP file, returning its contents as a byte vector.
pub fn extract<T: ReadSeek>(image_io: &mut T) -> Result<Vec<u8>, FileArchiveError> {
    image_io
        .seek(SeekFrom::Start(0))
        .map_err(|e| FileArchiveError::IoError(e.to_string()))?;

    let mut decoder = GzDecoder::new(image_io);
    let mut decompressed_data = Vec::new();

    // Read and decompress the data
    decoder
        .read_to_end(&mut decompressed_data)
        .map_err(|e| FileArchiveError::IoError(e.to_string()))?;

    // Sanity check on the decompressed data size
    if decompressed_data.len() as u64 > MAX_FILE_SIZE {
        return Err(FileArchiveError::IoError("Decompressed file too large".to_string()));
    }

    Ok(decompressed_data)
}

/// Returns the name of the file inside the GZIP archive, if present.
#[allow(dead_code)]
pub fn filename<T: ReadSeek>(image_io: &mut T) -> Result<Option<String>, FileArchiveError> {
    image_io
        .seek(SeekFrom::Start(0))
        .map_err(|e| FileArchiveError::IoError(e.to_string()))?;

    let decoder = GzDecoder::new(image_io);

    if let Some(header) = decoder.header() {
        if let Some(filename_bytes) = header.filename() {
            return String::from_utf8(filename_bytes.to_vec())
                .map(Some)
                .map_err(|_| FileArchiveError::IoError("Failed to parse filename".to_string()));
        }
    }

    Ok(None)
}
