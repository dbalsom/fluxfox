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
use crate::chs::{DiskCh, DiskChs};
use crate::diskimage::{DiskConsistency, DiskDescriptor};
use crate::file_parsers::ParserWriteCompatibility;
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::util::{get_length, read_ascii};
use crate::{
    DiskDataEncoding, DiskDataRate, DiskImage, DiskImageError, DiskImageFormat, FoxHashMap, FoxHashSet,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead, BinReaderExt};
use regex::Regex;

pub const IMD_HEADER_REX: &'static str = r"(?s)IMD (?<v_major>\d)\.(?<v_minor>\d{2}): (?<day>\d{2})/(?<month>\d{2})/(?<year>\d{4}) (?<hh>\d{2}):(?<mm>\d{2}):(?<ss>\d{2})(?<comment>.*)?";

pub struct ImdFormat;

#[derive(Debug)]
#[binrw]
pub struct ImdTrack {
    pub mode: u8,
    c: u8,
    h: u8,
    sector_ct: u8,
    sector_size: u8,
}

impl ImdTrack {
    pub fn c(&self) -> u8 {
        self.c
    }
    pub fn h(&self) -> u8 {
        self.h & 0x0F
    }
    pub fn is_valid(&self) -> bool {
        self.mode < 6 && (self.h & !0xC0) < 2 && self.sector_size < 7
    }
    pub fn has_head_map(&self) -> bool {
        self.h & 0x40 != 0
    }
    pub fn has_cylinder_map(&self) -> bool {
        self.h & 0x80 != 0
    }
    pub fn sector_size(&self) -> usize {
        imd_sector_size_to_usize(self.sector_size).unwrap()
    }
}

fn imd_mode_to_rate(data_rate: u8) -> Option<(DiskDataRate, DiskDataEncoding)> {
    match data_rate {
        0 => Some((DiskDataRate::Rate500Kbps, DiskDataEncoding::Fm)),
        1 => Some((DiskDataRate::Rate300Kbps, DiskDataEncoding::Fm)),
        2 => Some((DiskDataRate::Rate250Kbps, DiskDataEncoding::Fm)),
        3 => Some((DiskDataRate::Rate500Kbps, DiskDataEncoding::Mfm)),
        4 => Some((DiskDataRate::Rate300Kbps, DiskDataEncoding::Mfm)),
        5 => Some((DiskDataRate::Rate250Kbps, DiskDataEncoding::Mfm)),
        _ => None,
    }
}

fn imd_sector_size_to_usize(sector_size: u8) -> Option<usize> {
    match sector_size {
        0 => Some(128),
        1 => Some(256),
        2 => Some(512),
        3 => Some(1024),
        4 => Some(2048),
        5 => Some(4096),
        6 => Some(8192),
        _ => None,
    }
}

pub struct ImdSectorData {
    data: Vec<u8>,
    deleted: bool,
    error: bool,
}

impl ImdFormat {
    fn format() -> DiskImageFormat {
        DiskImageFormat::ImageDisk
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let _raw_len = get_length(&mut image).map_or(0, |l| l as usize);
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let (Some(header_str), _) = read_ascii(&mut image, None) {
            if let Some(_caps) = Regex::new(IMD_HEADER_REX).unwrap().captures(&header_str) {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        // TODO: Determine what data representations would lead to data loss for IMD.
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        // Assign the disk geometry or return error.
        let _raw_len = get_length(&mut image).map_err(|_e| DiskImageError::UnknownFormat)? as usize;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let (Some(header_str), terminator) = read_ascii(&mut image, None) {
            if let Some(caps) = Regex::new(IMD_HEADER_REX).unwrap().captures(&header_str) {
                let v_major = &caps["v_major"];
                let v_minor = &caps["v_minor"];
                let comment_match = caps.name("comment");
                let comment = comment_match.map(|c| c.as_str().to_string());

                log::trace!(
                    "from_image: Detected IMD header version: {}.{} terminator: {:02X}, comment: {}",
                    v_major,
                    v_minor,
                    terminator,
                    &comment.clone().unwrap_or("None".to_string())
                );

                disk_image.comment = comment;
            }
        }

        let mut header_offset = image.seek(std::io::SeekFrom::Current(0)).unwrap();
        let mut sector_counts: FoxHashMap<u8, u32> = FoxHashMap::new();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();

        let mut rate_opt = None;
        let mut encoding_opt = None;

        let mut consistent_track_length = None;
        let mut last_track_length = None;

        let mut consistent_sector_size = None;
        let mut last_sector_size = None;

        let mut track_ct = 0;

        while let Ok(track_header) = ImdTrack::read_le(&mut image) {
            log::trace!("from_image: Track header: {:?} @ {:X}", &track_header, header_offset);
            log::trace!("from_image: Track header valid: {}", &track_header.is_valid());
            if !track_header.is_valid() {
                log::error!("from_image: Invalid track header at offset {:X}", header_offset);
                return Err(DiskImageError::FormatParseError);
            }

            log::trace!(
                "from_image: Track has cylinder map: {} head map: {}",
                &track_header.has_cylinder_map(),
                &track_header.has_head_map()
            );

            if last_track_length.is_none() {
                // First track. Set the first track length seen as the consistent track length.
                last_track_length = Some(track_header.sector_ct as u8);
                consistent_track_length = Some(track_header.sector_ct as u8);
            } else {
                // Not the first track. See if track length has changed.
                if last_track_length.unwrap() != track_header.sector_ct as u8 {
                    consistent_track_length = None;
                }
            }

            if last_sector_size.is_none() {
                // First track. Set the first sector size seen as the consistent sector size.
                last_sector_size = Some(track_header.sector_size as u32);
                consistent_sector_size = Some(track_header.sector_size as u32);
            } else {
                // Not the first track. See if sector size has changed.
                if last_sector_size.unwrap() != track_header.sector_size as u32 {
                    consistent_sector_size = None;
                }
            }

            //let sector_size = imd_sector_size_to_usize(track_header.sector_size).unwrap();
            let mut sector_numbers = vec![0; track_header.sector_ct as usize];
            let mut cylinder_map = vec![track_header.c(); track_header.sector_ct as usize];
            let mut head_map = vec![track_header.h(); track_header.sector_ct as usize];

            // Keep a histogram of sector counts.
            if track_header.sector_ct > 0 {
                sector_counts
                    .entry(track_header.sector_ct)
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
            }

            // Keep a set of heads seen.
            heads_seen.insert(track_header.h());

            image
                .read_exact(&mut sector_numbers)
                .map_err(|_e| DiskImageError::IoError)?;

            if track_header.has_cylinder_map() {
                image
                    .read_exact(&mut cylinder_map)
                    .map_err(|_e| DiskImageError::IoError)?;
            }

            if track_header.has_head_map() {
                image.read_exact(&mut head_map).map_err(|_e| DiskImageError::IoError)?;
            }

            log::trace!(
                "from_image: Track sector numbers: {:?} Cyl map: {:?} Head map: {:?}",
                &sector_numbers,
                &cylinder_map,
                &head_map
            );

            // Add track to image.
            let (data_rate, data_encoding) = match imd_mode_to_rate(track_header.mode) {
                Some((rate, encoding)) => (rate, encoding),
                None => return Err(DiskImageError::FormatParseError),
            };

            if rate_opt.is_none() {
                rate_opt = Some(data_rate);
            }
            if encoding_opt.is_none() {
                encoding_opt = Some(data_encoding);
            }

            log::trace!("Adding track: C: {} H: {}", track_header.c, track_header.h);
            disk_image.add_track_bytestream(
                data_encoding,
                data_rate,
                DiskCh::from((track_header.c(), track_header.h())),
            );

            // Read all sectors for this track.
            for s in 0..sector_numbers.len() {
                // Read data byte marker.
                let data_marker: u8 = image.read_le().map_err(|_e| DiskImageError::IoError)?;

                match data_marker {
                    0x00..=0x08 => {
                        let data = ImdFormat::read_data(data_marker, track_header.sector_size(), &mut image)?;

                        log::trace!(
                            "from_image: Sector {}: Data Marker: {:02X} Data ({}): {:02X?} Deleted: {} Error: {}",
                            s + 1,
                            data_marker,
                            &data.data.len(),
                            &data.data[0..16],
                            &data.deleted,
                            &data.error
                        );

                        // Add this sector to track.
                        disk_image.master_sector(
                            DiskChs::from((track_header.c(), track_header.h(), sector_numbers[s])),
                            sector_numbers[s],
                            Some(cylinder_map[s]),
                            Some(head_map[s]),
                            &data.data,
                            None,
                            data.error,
                            data.deleted,
                        )?;
                    }
                    _ => {
                        return Err(DiskImageError::FormatParseError);
                    }
                }
            }

            header_offset = image.seek(std::io::SeekFrom::Current(0)).unwrap();

            if track_header.sector_ct == 0 {
                continue;
            }
            track_ct += 1;
        }

        disk_image.set_data_rate(rate_opt.unwrap());
        disk_image.set_data_encoding(encoding_opt.unwrap());

        disk_image.consistency = DiskConsistency {
            weak: false,
            deleted: false,
            consistent_sector_size,
            consistent_track_length,
        };

        let most_common_sector_count = sector_counts
            .iter()
            .max_by_key(|&(_, count)| count)
            .map(|(&value, _)| value)
            .unwrap_or(0);

        let head_ct = heads_seen.len() as u8;

        disk_image.image_format = DiskDescriptor {
            geometry: DiskChs::from((track_ct / head_ct, head_ct, most_common_sector_count)),
            data_rate: rate_opt.unwrap(),
            data_encoding: encoding_opt.unwrap(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
        };

        Ok(disk_image)
    }

    fn read_data<RWS: ReadSeek>(
        data_marker: u8,
        sector_size: usize,
        image: &mut RWS,
    ) -> Result<ImdSectorData, DiskImageError> {
        match data_marker {
            0x00 => {
                // Sector data unavailable.
                Ok(ImdSectorData {
                    data: Vec::new(),
                    deleted: false,
                    error: false,
                })
            }
            0x01 => {
                // Normal data - sector_size bytes follow.
                let mut data = vec![0; sector_size];
                image.read_exact(&mut data).map_err(|_e| DiskImageError::IoError)?;
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: false,
                })
            }
            0x02 => {
                // Compressed data: A single byte follows, repeated sector_size times.
                let data_byte = image.read_le().map_err(|_e| DiskImageError::IoError)?;
                let data = vec![data_byte; sector_size];
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: false,
                })
            }
            0x03 => {
                // Normal data with 'deleted' address-mark.
                let mut data = vec![0; sector_size];
                image.read_exact(&mut data).map_err(|_e| DiskImageError::IoError)?;
                Ok(ImdSectorData {
                    data,
                    deleted: true,
                    error: false,
                })
            }
            0x04 => {
                // Compressed data with 'deleted' address-mark.
                // A single byte follows, repeated sector_size times.
                let data_byte = image.read_le().map_err(|_e| DiskImageError::IoError)?;
                let data = vec![data_byte; sector_size];
                Ok(ImdSectorData {
                    data,
                    deleted: true,
                    error: false,
                })
            }
            0x05 => {
                // Normal data with 'error' indicator.
                let mut data = vec![0; sector_size];
                image.read_exact(&mut data).map_err(|_e| DiskImageError::IoError)?;
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: true,
                })
            }
            0x06 => {
                // Compressed data with 'error' indicator.
                let data_byte = image.read_le().map_err(|_e| DiskImageError::IoError)?;
                let data = vec![data_byte; sector_size];
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: true,
                })
            }
            _ => {
                return Err(DiskImageError::FormatParseError);
            }
        }
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
