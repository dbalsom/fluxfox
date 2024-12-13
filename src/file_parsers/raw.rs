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
    file_parsers::{FormatCaps, ParserReadOptions, ParserWriteCompatibility, ParserWriteOptions},
    io::{ReadSeek, ReadWriteSeek},
    prelude::DiskChs,
    track_schema::system34::System34Standard,
    types::{
        chs::{DiskChsn, DiskChsnQuery},
        AddSectorParams,
        DiskCh,
        DiskDataResolution,
        DiskDescriptor,
        MetaSectorTrackParams,
        Platform,
        TrackDensity,
    },
    util::get_length,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
    StandardFormat,
};

pub struct RawFormat;

impl RawFormat {
    #[allow(dead_code)]
    pub(crate) fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::RawSectorImage
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        const BASE_EXTENSIONS: &[&str] = &["img", "ima", "dsk", "bin"];

        #[cfg(feature = "amiga")]
        const EXTRA_EXTENSIONS: &[&str] = &["adf"];
        #[cfg(not(feature = "amiga"))]
        const EXTRA_EXTENSIONS: &[&str] = &[];

        [BASE_EXTENSIONS, EXTRA_EXTENSIONS].concat()
    }

    pub(crate) fn platforms() -> Vec<Platform> {
        vec![Platform::IbmPc, Platform::Amiga]
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
                if !image.analysis.image_caps.is_empty() {
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
        _opts: &ParserReadOptions,
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

        let track_size = disk_chs.s() as usize * floppy_format.sector_size();
        let track_ct = raw_len / track_size;

        if disk_chs.c() as usize * disk_chs.h() as usize != track_ct {
            log::error!("Raw::load_image(): Calculated track count does not match standard image.");
            return Err(DiskImageError::UnknownFormat);
        }

        let track_ct_overflow = raw_len % track_size;
        if track_ct_overflow != 0 {
            return Err(DiskImageError::UnknownFormat);
        }

        match Platform::from(floppy_format) {
            Platform::Amiga => {
                #[cfg(feature = "adf")]
                {
                    log::warn!(
                        "Raw::load_image(): ADF will be loaded as MetaSector until Amiga formatting is implemented."
                    );
                    RawFormat::load_as_metasector(raw, disk_image, floppy_format, _opts, _callback)
                }
                #[cfg(not(feature = "adf"))]
                {
                    log::error!("Raw::load_image(): Detected ADF raw image but `adf` feature not enabled.");
                    Err(DiskImageError::UnsupportedFormat)
                }
            }
            Platform::IbmPc => RawFormat::load_as_bitstream(raw, disk_image, floppy_format, _opts, _callback),
            _ => {
                log::error!(
                    "Raw::load_image(): Unsupported format/platform: {}/{}",
                    floppy_format,
                    Platform::from(floppy_format)
                );
                Err(DiskImageError::UnsupportedFormat)
            }
        }
    }

    fn load_as_bitstream<RWS: ReadSeek>(
        mut raw: RWS,
        disk_image: &mut DiskImage,
        floppy_format: StandardFormat,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        let disk_chs = floppy_format.chs();
        log::debug!("Raw::load_as_bitstream(): Disk CHS: {}", disk_chs);
        let data_rate = floppy_format.data_rate();
        let data_encoding = floppy_format.encoding();
        let bitcell_ct = floppy_format.bitcell_ct();
        let rpm = floppy_format.rpm();
        let gap3 = floppy_format.gap3();

        raw.seek(std::io::SeekFrom::Start(0))?;

        // Despite being a sector-based format, we convert to a bitstream based image by providing
        // the raw sector data to each track's format function.
        let mut sector_buffer = vec![0u8; floppy_format.sector_size()];

        // Iterate through all standard tracks
        for DiskCh { c, h } in disk_chs.ch().iter() {
            log::trace!("Raw::load_as_bitstream(): Adding new track: c:{} h:{}", c, h);
            let new_track_idx = disk_image.add_empty_track(DiskCh::new(c, h), data_encoding, data_rate, bitcell_ct)?;
            let mut format_buffer = Vec::with_capacity(disk_chs.s() as usize);
            let mut track_pattern = Vec::with_capacity(floppy_format.sector_size() * disk_chs.s() as usize);

            log::trace!(
                "Raw::load_as_bitstream(): Formatting track with {} sectors",
                disk_chs.s()
            );
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
            density: TrackDensity::from(data_rate),
            rpm: Some(rpm),
            write_protect: None,
        };

        Ok(())
    }

    fn load_as_metasector<RWS: ReadSeek>(
        mut raw: RWS,
        disk_image: &mut DiskImage,
        floppy_format: StandardFormat,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        let disk_chs = floppy_format.chs();
        log::trace!("Raw::load_as_metasector(): Disk CHS: {}", disk_chs);
        let data_rate = floppy_format.data_rate();
        let data_encoding = floppy_format.encoding();
        let rpm = floppy_format.rpm();

        let mut sector_buffer = vec![0u8; floppy_format.sector_size()];

        // Seek to the beginning of image reader
        raw.seek(std::io::SeekFrom::Start(0))?;

        // Iterate through all sectors in the standard format
        for ch in disk_chs.ch().iter() {
            log::trace!("Raw::load_as_metasector(): Adding new track: {}", ch);
            let params = MetaSectorTrackParams {
                ch,
                encoding: data_encoding,
                data_rate,
            };
            let new_track = disk_image.add_track_metasector(&params)?;

            for DiskChs { s, .. } in disk_chs.iter() {
                log::trace!("Raw::load_as_metasector(): Adding sector {} to track", s);
                raw.read_exact(&mut sector_buffer)?;

                let chs = DiskChs::from((ch, s));
                let sector_params = AddSectorParams {
                    id_chsn: DiskChsn::from((chs, floppy_format.chsn().n())),
                    data: &sector_buffer,
                    weak_mask: None,
                    hole_mask: None,
                    attributes: Default::default(),
                    alternate: false,
                    bit_index: None,
                };

                new_track.add_sector(&sector_params)?;
            }
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: disk_chs.into(),
            data_rate,
            data_encoding,
            density: TrackDensity::from(data_rate),
            rpm: Some(rpm),
            write_protect: None,
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        disk: &mut DiskImage,
        _opts: &ParserWriteOptions,
        output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        let format = disk.closest_format(true).ok_or(DiskImageError::UnsupportedFormat)?;
        log::debug!("Raw::save_image(): Using format: {}", format);
        // An IMG file basically represents DOS's view of a disk. Non-standard sectors may as well not
        // exist. The same basically applies for ADF files as well.

        // Write out the sectors in the standard order using DiskChsn::iter().
        for chsn in format.chsn().iter() {
            log::debug!("Raw::save_image(): Writing sector: {}...", chsn);

            match disk.read_sector_basic(chsn.ch(), DiskChsnQuery::from(chsn), None) {
                Ok(read_buf) => {
                    log::trace!("Raw::save_image(): Read {} bytes from sector: {}", read_buf.len(), chsn);
                    let mut new_buf = read_buf.to_vec();

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
