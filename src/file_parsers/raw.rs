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

use std::cmp::Ordering;

use crate::{
    detect::chs_from_raw_size,
    diskimage::DiskImage,
    file_parsers::{FormatCaps, ParserWriteCompatibility},
    io::{ReadSeek, ReadWriteSeek},
    track_schema::system34::System34Standard,
    types::{
        chs::{DiskChsn, DiskChsnQuery},
        DiskCh,
        DiskDataResolution,
        DiskDensity,
        DiskDescriptor,
    },
    util::get_length,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
    StandardFormat,
    DEFAULT_SECTOR_SIZE,
};

pub struct RawFormat;

impl RawFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::RawSectorImage
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

    pub(crate) fn can_write(image: Option<&DiskImage>) -> ParserWriteCompatibility {
        image
            .map(|image| {
                if !image.consistency.image_caps.is_empty() {
                    // RAW sector images support no capability flags.
                    log::warn!("RAW sector images do not support capability flags.");
                    ParserWriteCompatibility::DataLoss
                }
                else {
                    ParserWriteCompatibility::Ok
                }
            })
            .unwrap_or(ParserWriteCompatibility::Ok)
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut raw: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::RawSectorImage);
        disk_image.set_resolution(DiskDataResolution::BitStream);

        // Assign the disk geometry or return error.
        let raw_len = get_length(&mut raw).map_err(|_e| DiskImageError::UnknownFormat)? as usize;

        let floppy_format = match StandardFormat::try_from(raw_len) {
            Ok(floppy_format) => {
                log::trace!("Raw::load_image(): Detected format {}", floppy_format);
                floppy_format
            }
            Err(e) => {
                log::error!("Raw::load_image(): Error detecting format: {}", e);
                return Err(DiskImageError::UnknownFormat);
            }
        };

        let disk_chs = floppy_format.chs();
        log::trace!("Raw::load_image(): Disk CHS: {}", disk_chs);
        let data_rate = floppy_format.data_rate();
        let data_encoding = floppy_format.encoding();
        let bitcell_ct = floppy_format.bitcell_ct();
        let rpm = floppy_format.rpm();
        let gap3 = floppy_format.gap3();

        raw.seek(std::io::SeekFrom::Start(0))?;

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
        for DiskCh { c, h } in disk_chs.ch().iter() {
            log::trace!("Raw::load_image(): Adding new track: c:{} h:{}", c, h);
            let new_track_idx = disk_image.add_empty_track(DiskCh::new(c, h), data_encoding, data_rate, bitcell_ct)?;
            let mut format_buffer = Vec::with_capacity(disk_chs.s() as usize);
            let mut track_pattern = Vec::with_capacity(DEFAULT_SECTOR_SIZE * disk_chs.s() as usize);

            log::trace!("Raw::load_image(): Formatting track with {} sectors", disk_chs.s());
            for s in 1..disk_chs.s() + 1 {
                let sector_chsn = DiskChsn::new(c, h, s, 2);
                raw.read_exact(&mut sector_buffer)?;
                //log::warn!("Raw::load_image(): Sector data: {:X?}", sector_buffer);
                track_pattern.extend(sector_buffer.clone());
                format_buffer.push(sector_chsn);
            }

            let td = disk_image
                .track_by_idx_mut(new_track_idx)
                .ok_or(DiskImageError::FormatParseError)?;

            //log::warn!("Raw::load_image(): Track pattern: {:X?}", track_pattern);
            td.format(System34Standard::Ibm, format_buffer, &track_pattern, gap3)?;
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

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(disk: &mut DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        let format = disk.closest_format(true).ok_or(DiskImageError::UnsupportedFormat)?;

        // An IMG file basically represents DOS's view of a disk. Non-standard sectors may as well not
        // exist. We'll just write out the sectors in the standard order using DiskChsn::iter().
        for chsn in format.chsn().iter() {
            log::debug!("Raw::save_image(): Writing sector: {}...", chsn);

            match disk.read_sector_basic(chsn.ch(), DiskChsnQuery::from(chsn), None) {
                Ok(read_buf) => {
                    log::trace!("Raw::save_image(): Read {} bytes from sector: {}", read_buf.len(), chsn);
                    let mut new_buf = read_buf.clone();

                    match new_buf.len().cmp(&chsn.n_size()) {
                        Ordering::Greater => {
                            log::warn!(
                                "Raw::save_image(): Sector {} is too large ({}). Truncating to {} bytes",
                                chsn,
                                new_buf.len(),
                                chsn.n_size()
                            );
                            new_buf.truncate(chsn.n_size());
                        }
                        Ordering::Less => {
                            log::warn!(
                                "Raw::save_image(): Sector {} is too small ({}). Padding with to {} bytes",
                                chsn,
                                new_buf.len(),
                                chsn.n_size()
                            );
                            new_buf.extend(vec![0u8; chsn.n_size() - new_buf.len()]);
                        }
                        Ordering::Equal => {}
                    }

                    //println!("Raw::save_image(): Writing chs: {}...", chs);
                    output.write_all(new_buf.as_ref())?;
                }
                Err(e) => {
                    log::error!("Raw::save_image(): Error reading sector {}: {}", chsn, e);
                    return Err(DiskImageError::DataError);
                }
            }
        }

        output.flush()?;
        Ok(())
    }
}
