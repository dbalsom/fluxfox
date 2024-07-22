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

use crate::chs::DiskChs;
use crate::detect::chs_from_raw_size;
use crate::diskimage::{DiskConsistency, DiskImage, FloppyFormat, TrackFormat};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::parsers::ParserWriteCompatibility;
use crate::util::get_length;
use crate::{DiskImageError, DiskImageFormat, DEFAULT_SECTOR_SIZE};

pub struct RawFormat;

impl RawFormat {
    fn format() -> DiskImageFormat {
        DiskImageFormat::RawSectorImage
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let raw_len = get_length(&mut image).map_or(0, |l| l as usize);
        if chs_from_raw_size(raw_len).is_some() {
            true
        } else {
            false
        }
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut raw: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        // Assign the disk geometry or return error.
        let raw_len = get_length(&mut raw).map_err(|_e| DiskImageError::UnknownFormat)? as usize;

        let floppy_format = FloppyFormat::from(raw_len);
        if floppy_format == FloppyFormat::Unknown {
            return Err(DiskImageError::UnknownFormat);
        }

        let disk_chs = floppy_format.get_chs();
        println!("Disk CHS: {}", disk_chs);
        let data_rate = floppy_format.get_data_rate();
        let data_encoding = floppy_format.get_encoding();

        let mut cursor_chs = DiskChs::default();

        raw.seek(std::io::SeekFrom::Start(0))
            .map_err(|_e| DiskImageError::IoError)?;

        let track_size = disk_chs.s() as usize * DEFAULT_SECTOR_SIZE;
        let track_ct = raw_len / track_size;
        let track_ct_overflow = raw_len % track_size;

        if track_ct_overflow != 0 {
            return Err(DiskImageError::UnknownFormat);
        }

        let mut sector_buffer = vec![0u8; DEFAULT_SECTOR_SIZE];

        // Insert sectors in order encountered.
        for _t in 0..track_ct {
            disk_image.add_track(
                TrackFormat {
                    data_rate,
                    data_encoding,
                },
                cursor_chs.into(),
            );

            for sector_id in 0..disk_chs.s() {
                raw.read_exact(&mut sector_buffer)
                    .map_err(|_e| DiskImageError::IoError)?;

                //log::trace!("Importing sector {} of length {}", cursor_chs, DEFAULT_SECTOR_SIZE);
                disk_image.write_sector(cursor_chs, sector_id, None, None, &sector_buffer, None)?;
                cursor_chs.seek_forward(1, &disk_chs);
            }
        }

        disk_image.consistency = DiskConsistency {
            weak: false,
            deleted: false,
            consistent_sector_size: Some(DEFAULT_SECTOR_SIZE as u32),
            consistent_track_length: Some(disk_chs.s()),
        };
        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(image: &DiskImage, mut output: &mut RWS) -> Result<(), DiskImageError> {
        let mut total_len = 0;
        for track_n in 0..image.tracks[0].len() {
            for head in 0..2 {
                let track = &image.tracks[head][track_n];

                for sector in &track.data.sectors {
                    let chs = DiskChs::from((track_n as u8, head as u8, sector.sector_id as u8));

                    let sector_len = std::cmp::min(sector.len, DEFAULT_SECTOR_SIZE);
                    /*
                    log::trace!(
                        "Exporting sector {} of length {}, total_len: {}",
                        chs, sector_len, total_len
                    );
                    */
                    output
                        .write_all(
                            track.data.data
                                [sector.t_idx..std::cmp::min(sector.t_idx + sector_len, track.data.data.len())]
                                .as_ref(),
                        )
                        .map_err(|_e| DiskImageError::IoError)?;

                    total_len += sector_len;
                }
            }
        }

        Ok(())
    }
}
