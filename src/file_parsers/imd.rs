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
use crate::chs::{DiskCh, DiskChsn};
use crate::diskimage::{DiskDescriptor, SectorDescriptor};
use crate::file_parsers::{FormatCaps, ParserWriteCompatibility};
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::util::{get_length, read_ascii};
use crate::{
    DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat, FoxHashSet,
    LoadingCallback, ASCII_EOF, DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead, BinReaderExt};
use regex::Regex;

pub const IMD_HEADER_REX: &str = r"(?s)IMD (?<v_major>\d)\.(?<v_minor>\d{2}): (?<day>\d{2})/(?<month>\d{2})/(?<year>\d{4}) (?<hh>\d{2}):(?<mm>\d{2}):(?<ss>\d{2})(?<comment>.*)?";

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
    pub fn has_sector_size_map(&self) -> bool {
        self.h == 0xFF
    }
    pub fn sector_size(&self) -> Option<usize> {
        imd_sector_size_to_usize(self.sector_size)
    }
}

fn imd_mode_to_rate(data_rate: u8) -> Option<(DiskDataRate, DiskDataEncoding)> {
    match data_rate {
        0 => Some((DiskDataRate::Rate500Kbps(1.0), DiskDataEncoding::Fm)),
        1 => Some((DiskDataRate::Rate300Kbps(1.0), DiskDataEncoding::Fm)),
        2 => Some((DiskDataRate::Rate250Kbps(1.0), DiskDataEncoding::Fm)),
        3 => Some((DiskDataRate::Rate500Kbps(1.0), DiskDataEncoding::Mfm)),
        4 => Some((DiskDataRate::Rate300Kbps(1.0), DiskDataEncoding::Mfm)),
        5 => Some((DiskDataRate::Rate250Kbps(1.0), DiskDataEncoding::Mfm)),
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
    #[allow(dead_code)]
    fn format() -> DiskImageFormat {
        DiskImageFormat::ImageDisk
    }

    pub(crate) fn capabilities() -> FormatCaps {
        FormatCaps::empty()
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["imd"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let _raw_len = get_length(&mut image).map_or(0, |l| l as usize);
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        //log::debug!("Detecting IMD header...");
        if let (Some(header_str), _) = read_ascii(&mut image, Some(ASCII_EOF), None) {
            //log::debug!("Detected header: {}", &header_str);
            if let Some(_caps) = Regex::new(IMD_HEADER_REX).unwrap().captures(&header_str) {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFormat::ImageDisk);

        // Assign the disk geometry or return error.
        let _raw_len = get_length(&mut read_buf).map_err(|_e| DiskImageError::UnknownFormat)? as usize;
        _ = read_buf.seek(std::io::SeekFrom::Start(0));

        if let (Some(header_str), terminator) = read_ascii(&mut read_buf, Some(ASCII_EOF), None) {
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

        let mut header_offset = read_buf.stream_position().unwrap();
        let mut heads_seen: FoxHashSet<u8> = FoxHashSet::new();

        let mut rate_opt = None;
        let mut encoding_opt = None;

        let mut track_ct = 0;

        while let Ok(track_header) = ImdTrack::read_le(&mut read_buf) {
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

            //let sector_size = imd_sector_size_to_usize(track_header.sector_size).unwrap();
            let mut sector_numbers = vec![0; track_header.sector_ct as usize];
            let mut cylinder_map = vec![track_header.c(); track_header.sector_ct as usize];
            let mut head_map = vec![track_header.h(); track_header.sector_ct as usize];

            //let default_n = track_header.sector_size;
            let default_sector_size = track_header.sector_size();
            if default_sector_size.is_none() {
                return Err(DiskImageError::FormatParseError);
            }
            // Sector size map is in words; so double the bytes.
            let mut sector_size_map_u8: Vec<u8> = vec![0, track_header.sector_ct * 2];
            let mut sector_size_map: Vec<u16> =
                vec![default_sector_size.unwrap() as u16; track_header.sector_ct as usize];

            // Keep a set of heads seen.
            heads_seen.insert(track_header.h());

            read_buf.read_exact(&mut sector_numbers)?;

            if track_header.has_cylinder_map() {
                read_buf.read_exact(&mut cylinder_map)?;
            }

            if track_header.has_head_map() {
                read_buf.read_exact(&mut head_map)?;
            }

            // Note: This is listed as a 'proposed extension' in the IMD docs but apparently there
            // are images like this in the wild. 86box supports this extension.
            if track_header.has_sector_size_map() {
                read_buf.read_exact(&mut sector_size_map_u8)?;

                // Convert raw u8 to u16 values, little-endian.
                for (i, s) in sector_size_map_u8.chunks_exact(2).enumerate() {
                    sector_size_map[i] = u16::from_le_bytes([s[0], s[1]]);
                }
            }

            log::trace!(
                "from_image: Track sector numbers: {:?} Cyl map: {:?} Head map: {:?}",
                &sector_numbers,
                &cylinder_map,
                &head_map
            );

            // Add track to read_buf.
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
            let new_track = disk_image.add_track_metasector(
                data_encoding,
                data_rate,
                DiskCh::from((track_header.c() as u16, track_header.h())),
            )?;

            // Read all sectors for this track.
            for s in 0..sector_numbers.len() {
                // Read data byte marker.
                let data_marker: u8 = read_buf.read_le()?;
                let sector_size = sector_size_map[s] as usize;
                let sector_n = DiskChsn::bytes_to_n(sector_size);

                match data_marker {
                    0x00..=0x08 => {
                        let data = ImdFormat::read_data(data_marker, sector_size, &mut read_buf)?;

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
                        let sd = SectorDescriptor {
                            id_chsn: DiskChsn::new(cylinder_map[s] as u16, head_map[s], sector_numbers[s], sector_n),
                            data: data.data,
                            weak_mask: None,
                            hole_mask: None,
                            address_crc_error: false,
                            data_crc_error: data.error,
                            deleted_mark: data.deleted,
                            missing_data: false,
                        };

                        new_track.add_sector(&sd, false)?;
                    }
                    _ => {
                        return Err(DiskImageError::FormatParseError);
                    }
                }
            }

            header_offset = read_buf.stream_position()?;

            if track_header.sector_ct == 0 {
                continue;
            }
            track_ct += 1;
        }

        let head_ct = heads_seen.len() as u8;

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((track_ct as u16 / head_ct as u16, head_ct)),
            data_rate: rate_opt.unwrap(),
            data_encoding: encoding_opt.unwrap(),
            density: DiskDensity::from(rate_opt.unwrap()),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: None,
        };

        Ok(())
    }

    fn read_data<RWS: ReadSeek>(
        data_marker: u8,
        sector_size: usize,
        read_buf: &mut RWS,
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
                read_buf.read_exact(&mut data)?;
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: false,
                })
            }
            0x02 => {
                // Compressed data: A single byte follows, repeated sector_size times.
                let data_byte = read_buf.read_le()?;
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
                read_buf.read_exact(&mut data)?;
                Ok(ImdSectorData {
                    data,
                    deleted: true,
                    error: false,
                })
            }
            0x04 => {
                // Compressed data with 'deleted' address-mark.
                // A single byte follows, repeated sector_size times.
                let data_byte = read_buf.read_le()?;
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
                read_buf.read_exact(&mut data)?;
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: true,
                })
            }
            0x06 => {
                // Compressed data with 'error' indicator.
                let data_byte = read_buf.read_le()?;
                let data = vec![data_byte; sector_size];
                Ok(ImdSectorData {
                    data,
                    deleted: false,
                    error: true,
                })
            }
            _ => Err(DiskImageError::FormatParseError),
        }
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
