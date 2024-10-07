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
use crate::chs::{DiskChs, DiskChsn};
use crate::detect::chs_from_raw_size;
use crate::diskimage::{DiskDescriptor, DiskImage, RwSectorScope, SectorDescriptor};
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::structure_parsers::system34::System34Standard;
use crate::trackdata::TrackData;
use crate::util::get_length;
use crate::{
    DiskCh, DiskDataEncoding, DiskDataResolution, DiskDensity, DiskImageError, DiskImageFormat, StandardFormat,
    DEFAULT_SECTOR_SIZE,
};
use std::cmp::Ordering;

pub struct RawFormat;

impl RawFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::RawSectorImage
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["img", "ima", "dsk", "bin"]
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let raw_len = get_length(&mut image).map_or(0, |l| l as usize);
        chs_from_raw_size(raw_len).is_some()
    }

    pub(crate) fn can_write(image: &DiskImage) -> ParserWriteCompatibility {
        if !image.consistency.image_caps.is_empty() {
            // RAW sector images support no capability flags.
            log::warn!("RAW sector images do not support capability flags.");
            ParserWriteCompatibility::DataLoss
        } else {
            ParserWriteCompatibility::Ok
        }
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut raw: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();
        disk_image.set_source_format(DiskImageFormat::RawSectorImage);
        disk_image.set_resolution(DiskDataResolution::BitStream);

        // Assign the disk geometry or return error.
        let raw_len = get_length(&mut raw).map_err(|_e| DiskImageError::UnknownFormat)? as usize;

        let floppy_format = StandardFormat::from(raw_len);
        if floppy_format == StandardFormat::Invalid {
            return Err(DiskImageError::UnknownFormat);
        } else {
            log::trace!("Raw::load_image(): Detected format {}", floppy_format);
        }

        let disk_chs = floppy_format.get_chs();
        log::trace!("Raw::load_image(): Disk CHS: {}", disk_chs);
        let data_rate = floppy_format.get_data_rate();
        let data_encoding = floppy_format.get_encoding();
        let bitcell_ct = floppy_format.get_bitcell_ct();
        let rpm = floppy_format.get_rpm();
        let gap3 = floppy_format.get_gap3();

        let mut cursor_chs = DiskChs::default();

        raw.seek(std::io::SeekFrom::Start(0))
            .map_err(|_e| DiskImageError::IoError)?;

        let track_size = disk_chs.s() as usize * DEFAULT_SECTOR_SIZE;
        let track_ct = raw_len / track_size;

        if disk_chs.c() as usize * disk_chs.h() as usize != track_ct {
            log::error!("Raw::load_image(): Calculated track count does not match standard image.");
            return Err(DiskImageError::UnknownFormat);
        }

        let track_ct_overflow = raw_len % track_size;
        if track_ct_overflow != 0 {
            return Err(DiskImageError::UnknownFormat);
        }

        // Despite being a sector-based format, we convert to a bitstream based image by providing
        // the raw sector data to each track's format function.

        let mut sector_buffer = vec![0u8; DEFAULT_SECTOR_SIZE];

        // Insert sectors in order encountered.
        for c in 0..disk_chs.c() {
            for h in 0..disk_chs.h() {
                log::trace!("Raw::load_image(): Adding new track: c:{} h:{}", c, h);
                let new_track_idx =
                    disk_image.add_empty_track(DiskCh::new(c, h), data_encoding, data_rate, bitcell_ct)?;

                let mut format_buffer = Vec::with_capacity(disk_chs.s() as usize);
                let mut track_pattern = Vec::with_capacity(DEFAULT_SECTOR_SIZE * disk_chs.s() as usize);

                log::trace!("Raw::load_image(): Formatting track with {} sectors", disk_chs.s());
                for s in 1..disk_chs.s() + 1 {
                    let sector_chsn = DiskChsn::new(c, h, s, 2);

                    raw.read_exact(&mut sector_buffer)
                        .map_err(|_e| DiskImageError::IoError)?;

                    //log::warn!("Raw::load_image(): Sector data: {:X?}", sector_buffer);

                    track_pattern.extend(sector_buffer.clone());
                    format_buffer.push(sector_chsn);
                }

                let td = disk_image
                    .get_track_mut(new_track_idx)
                    .ok_or(DiskImageError::FormatParseError)?;

                //log::warn!("Raw::load_image(): Track pattern: {:X?}", track_pattern);

                td.format(System34Standard::Ibm, format_buffer, &track_pattern, gap3)?;
            }
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: disk_chs.into(),
            data_rate,
            data_encoding,
            density: DiskDensity::from(data_rate),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: Some(rpm),
            write_protect: None,
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(image: &mut DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        // Clamp track count to 40 or 80 for a standard disk image. We may read in more tracks
        // depending on image format. For example, 86f format exports 86 tracks
        let track_ct = match image.track_map[0].len() {
            39..=50 => 40,
            79..=90 => 80,
            _ => {
                log::error!(
                    "Raw::save_image(): Unsupported track count: {}",
                    image.track_map[0].len()
                );
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        let track_sector_size = if let Some(css) = image.consistency.consistent_sector_size {
            css as usize
        } else {
            log::warn!("Raw::save_image(): Image has inconsistent sector counts per track. Data will be lost.");

            let track_idx = image.track_map[0][0];
            let track = &image.track_pool[track_idx];
            track.get_sector_ct()
        };

        log::trace!("Raw::save_image(): Using {} sectors per track.", track_sector_size);

        for c in 0..track_ct {
            for h in 0..image.heads() as usize {
                let ti = image.track_map[h][c];
                let track = &mut image.track_pool[ti];

                for s in 1..(track_sector_size + 1) {
                    match track.read_sector(
                        DiskChs::new(c as u16, h as u8, s as u8),
                        None,
                        RwSectorScope::DataOnly,
                        false,
                    ) {
                        Ok(read_sector) => {
                            let mut new_buf = read_sector.read_buf.clone();

                            match new_buf.len().cmp(&DEFAULT_SECTOR_SIZE) {
                                Ordering::Greater => {
                                    log::warn!(
                                        "Raw::save_image(): c:{} h:{} Sector {} is too large: {}. Truncating to {}",
                                        c,
                                        h,
                                        s,
                                        new_buf.len(),
                                        DEFAULT_SECTOR_SIZE
                                    );
                                    new_buf.truncate(DEFAULT_SECTOR_SIZE);
                                }
                                Ordering::Less => {
                                    log::warn!(
                                        "Raw::save_image(): c:{} h:{} Sector {} is too small: {}. Padding with 0",
                                        c,
                                        h,
                                        s,
                                        new_buf.len()
                                    );
                                    new_buf.extend(vec![0u8; DEFAULT_SECTOR_SIZE - new_buf.len()]);
                                }
                                Ordering::Equal => {}
                            }

                            output
                                .write_all(new_buf.as_ref())
                                .map_err(|_e| DiskImageError::IoError)?;
                        }
                        Err(e) => {
                            log::error!("Raw::save_image(): Error reading c:{} h:{} s:{} err: {}", c, h, s, e);
                            return Err(DiskImageError::DataError);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
