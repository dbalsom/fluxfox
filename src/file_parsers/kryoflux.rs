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

    src/parsers/kryoflux.rs

    A parser for the KryoFlux stream image format.

    Kryoflux files (.raw) represent the raw stream of flux transitions for
    a single track on a disk. A set of files is used to represent a complete
    disk image.


*/
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{bitstream_flags, FormatCaps};
use crate::fluxstream::flux_stream::RawFluxTrack;
use crate::fluxstream::pll::{Pll, PllPreset};
use crate::fluxstream::FluxTransition;
use crate::io::{ReadBytesExt, ReadSeek, ReadWriteSeek};
use crate::{io, DiskDataResolution};
use crate::{
    DiskCh, DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat, DiskRpm,
    ParserWriteCompatibility, StandardFormat, DEFAULT_SECTOR_SIZE,
};
use binrw::binrw;
use binrw::{BinRead, BinReaderExt};
use std::path::{Path, PathBuf};

use crate::util::{get_length, read_ascii};

pub const KFX_DEFAULT_MCK: f64 = ((18432000.0 * 73.0) / 14.0) / 2.0;
pub const KFX_DEFAULT_SCK: f64 = KFX_DEFAULT_MCK / 2.0;
pub const KFX_DEFAULT_ICK: f64 = KFX_DEFAULT_MCK / 16.0;

pub enum OobBlock {
    Invalid(u8),
    StreamInfo,
    Index,
    StreamEnd,
    KfInfo,
    Eof,
}

fn read_oob_block<R: ReadBytesExt>(reader: &mut R) -> OobBlock {
    let byte = reader.read_u8().unwrap_or(0);
    //log::trace!("Read OOB block type: {:02X}", byte);

    match byte {
        0x01 => OobBlock::StreamInfo,
        0x02 => OobBlock::Index,
        0x03 => OobBlock::StreamEnd,
        0x04 => OobBlock::KfInfo,
        0x0D => OobBlock::Eof,
        _ => OobBlock::Invalid(byte),
    }
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct StreamInfoBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub transfer_time_ms: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct IndexBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub sample_counter: u32,
    pub index_counter: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct StreamEndBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub hw_status_code: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct KfInfoBlock {
    pub size: u16,
    // null terminated ascii string follows
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct EofBlock {
    pub size: u16,
}

pub struct KfxFormat {
    mck: f64,
    sck: f64,
    ick: f64,
    last_index_counter: Option<u32>,
    first_flux_offset: Option<u64>,
    idx_ct: u32,
    flux_ovl: u32,
}

impl Default for KfxFormat {
    fn default() -> Self {
        KfxFormat {
            mck: KFX_DEFAULT_MCK,
            sck: KFX_DEFAULT_SCK,
            ick: KFX_DEFAULT_ICK,
            last_index_counter: None,
            first_flux_offset: None,
            idx_ct: 0,
            flux_ovl: 0,
        }
    }
}

impl KfxFormat {
    pub fn extensions() -> Vec<&'static str> {
        vec!["raw"]
    }

    pub fn capabilities() -> FormatCaps {
        bitstream_flags()
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }

        let byte = image.read_u8().unwrap_or(0);

        // Assume stream starts with an OOB header byte(?)
        byte == 0x0D
    }

    pub fn can_write(image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut image: RWS,
        append_image: Option<DiskImage>,
    ) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = append_image.unwrap_or_default();
        disk_image.set_resolution(DiskDataResolution::BitStream);

        disk_image.set_source_format(DiskImageFormat::KryofluxStream);

        let mut kfx_context = KfxFormat::default();

        image.seek(std::io::SeekFrom::Start(0))?;

        // Create vector of streams.
        let mut streams: Vec<Vec<f64>> = Vec::with_capacity(5);
        // Create first stream.
        streams.push(Vec::with_capacity(100_000));

        // Create vector of index times.
        let mut index_times: Vec<f64> = Vec::with_capacity(5);

        let mut eof = false;
        while !eof {
            eof = kfx_context.read_block(&mut image, &mut streams, &mut index_times)?;
        }

        let mut pll = Pll::from_preset(PllPreset::Aggressive);
        //pll.set_clock(2e-6, None);
        let mut flux_track = RawFluxTrack::new(1.0 / 2e-6);

        // log::debug!("Found {} partial revolutions in stream", streams.len());
        // for (si, stream) in streams.iter().enumerate() {
        //     log::debug!("  Rev {}: {} samples", si, stream.len());
        // }
        log::debug!(
            "Found {} complete revolutions in stream, with {} index times",
            kfx_context.idx_ct - 1,
            index_times.len()
        );

        for ((ri, rev), index_time) in streams.iter().enumerate().skip(1).zip(index_times.iter()) {
            log::debug!("  Rev {}: {} samples index_time: {}", ri, rev.len(), index_time);

            let rev_rpm = 60.0 / index_time;

            let clock_adjust;
            if (280.0..=380.0).contains(&rev_rpm) {
                clock_adjust = rev_rpm / 300.0;

                log::warn!(
                    "Revolution {} RPM is {:.2}, adjusting clock to {:.2}%",
                    ri,
                    rev_rpm,
                    clock_adjust * 100.0
                );
            } else {
                log::error!("Revolution {} RPM is {:.2}, out of range.", ri, rev_rpm);
                return Err(DiskImageError::IncompatibleImage);
            }

            log::debug!(
                "Adding revolution {} containing {} bitcells to RawFluxTrack",
                ri,
                rev.len()
            );
            pll.adjust_clock(clock_adjust);
            flux_track.add_revolution(rev, pll.get_clock());
            pll.reset_clock();
        }

        let rev = 1;
        let flux_stream = flux_track.revolution_mut(rev).unwrap();
        flux_stream.decode2(&mut pll, true);

        let rev_density = match flux_stream.guess_density(false) {
            Some(d) => {
                log::debug!("Revolution {} density: {:?}", rev, d);
                d
            }
            None => {
                log::error!("Unable to detect track density!");
                //return Err(DiskImageError::IncompatibleImage);
                DiskDensity::Double
            }
        };

        let (track_data, track_bits) = flux_track.revolution_mut(rev).unwrap().bitstream_data();

        let data_rate = DiskDataRate::from(rev_density);

        // Get last ch in image.
        let next_ch = if disk_image.track_ch_iter().count() == 0 {
            log::debug!("No tracks in image, starting at c:0 h:0");
            DiskCh::new(0, 0)
        } else {
            let mut last_ch = disk_image.track_ch_iter().last().unwrap_or(DiskCh::new(0, 0));
            log::debug!("Previous track in image: {}", last_ch);

            last_ch.seek_next_track(2);
            last_ch
        };

        if track_bits < 100 {
            log::warn!("Track contains less than 100 bits. Adding empty track.");
            disk_image.add_empty_track(next_ch, DiskDataEncoding::Mfm, data_rate, 100_000)?;
        } else {
            log::debug!("Adding track {} containing {} bits to image...", next_ch, track_bits);

            disk_image.add_track_bitstream(
                DiskDataEncoding::Mfm,
                data_rate,
                next_ch,
                DiskDataRate::from(rev_density).into(),
                Some(track_bits),
                &track_data,
                None,
            )?;
        }

        log::debug!("Track added.");

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((40, 2)),
            data_rate,
            density: rev_density,
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: Some(true),
        };

        Ok(disk_image)
    }

    pub fn save_image<RWS: ReadWriteSeek>(image: &DiskImage, output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn read_block<RWS: ReadSeek>(
        &mut self,
        image: &mut RWS,
        streams: &mut Vec<Vec<f64>>,
        index_times: &mut Vec<f64>,
    ) -> Result<bool, DiskImageError> {
        let current_pos = image.stream_position()?;
        let byte = image.read_u8()?;

        // If we've reached the stream position indicated by the last index block,
        // we're starting a new revolution.

        if let Some(offset) = self.first_flux_offset {
            if current_pos >= offset {
                log::debug!("Starting new revolution at pos: {}", current_pos);
                streams.push(Vec::new());
                self.first_flux_offset = None;
                self.idx_ct += 1;
            }
        }

        //log::trace!("Read block type: {:02X}", byte);
        match byte {
            0x00..=0x07 => {
                // Flux2 block
                let byte2 = image.read_u8()?;
                let flux_u32 = u16::from_be_bytes([byte, byte2]) as u32;
                let flux = (self.flux_ovl + flux_u32) as f64 / self.sck;

                streams.last_mut().unwrap().push(flux);

                self.flux_ovl = 0;
            }
            0x08 => {
                // Nop1 block
                // Do nothing
            }
            0x09 => {
                // Nop2 block
                // Skip one byte
                image.seek(std::io::SeekFrom::Current(1))?;
            }
            0x0A => {
                // Nop3 block
                // Skip two bytes
                image.seek(std::io::SeekFrom::Current(2))?;
            }
            0x0B => {
                // Ovl16 block
                self.flux_ovl = self.flux_ovl.saturating_add(0x10000);
            }
            0x0C => {
                // Flux3 block
                let byte2 = image.read_u8()?;
                let byte3 = image.read_u8()?;
                let flux_u32 = u16::from_be_bytes([byte2, byte3]) as u32;
                let flux = (self.flux_ovl + flux_u32) as f64 / self.sck;

                streams.last_mut().unwrap().push(flux);

                self.flux_ovl = 0;
            }
            0x0D => {
                // OOB block
                let oob_block = read_oob_block(image);

                match oob_block {
                    OobBlock::Invalid(oob_byte) => {
                        log::error!("Invalid OOB block type: {:02X}", oob_byte);
                    }
                    OobBlock::StreamInfo => {
                        let sib = StreamInfoBlock::read(image)?;
                        log::trace!(
                            "StreamInfo block: pos: {} time: {}",
                            sib.stream_pos,
                            sib.transfer_time_ms
                        );
                    }
                    OobBlock::Index => {
                        let ib = IndexBlock::read(image)?;

                        let index_time = ib.index_counter as f64 / self.ick;

                        if let Some(last_index_counter) = self.last_index_counter {
                            let index_delta = ib.index_counter.wrapping_sub(last_index_counter);
                            let index_time_delta = index_delta as f64 / self.ick;
                            log::debug!(
                                "Index block: current_pos: {} next_pos: {} sample_ct: {} index_ct: {} delta: {:.6} rpm: {:.3}",
                                current_pos,
                                ib.stream_pos,
                                ib.sample_counter,
                                ib.index_counter,
                                index_time_delta,
                                60.0 / index_time_delta
                            );
                            index_times.push(index_time_delta);
                        } else {
                            log::debug!(
                                "Index block: current_pos: {} pos: {} sample_ct: {} index_ct: {}",
                                current_pos,
                                ib.stream_pos,
                                ib.sample_counter,
                                ib.index_counter
                            );
                        }

                        self.last_index_counter = Some(ib.index_counter);

                        // Record the offset of the flux immediately following the index.
                        // When we reach this offset, we will start a new revolution.
                        self.first_flux_offset = Some(ib.stream_pos as u64);
                    }
                    OobBlock::StreamEnd => {
                        let seb = StreamEndBlock::read(image)?;
                        log::debug!(
                            "StreamEnd block: pos: {} hw_status: {:02X}",
                            seb.stream_pos,
                            seb.hw_status_code
                        );
                        match seb.hw_status_code {
                            0 => {
                                log::debug!("Hardware status reported OK");
                            }
                            1 => {
                                log::error!("A buffering issue was recorded in the stream. Stream may be corrupt");
                                return Err(DiskImageError::ImageCorruptError);
                            }
                            2 => {
                                log::error!("No index signal was detected.");
                                return Err(DiskImageError::ImageCorruptError);
                            }
                            _ => {
                                log::error!("Unknown hardware status. Hope it wasn't important!");
                            }
                        }
                    }
                    OobBlock::KfInfo => {
                        log::debug!("KfInfo block");
                        let _kib = KfInfoBlock::read(image)?;
                        // Ascii string follows
                        let mut string_end = false;
                        let mut string = String::new();
                        while !string_end {
                            let (str_opt, terminator) = read_ascii(image, None);
                            if let Some(s) = &str_opt {
                                log::debug!("KfInfo str: {}", s);
                                let (sck_opt, ick_opt) = kfx_parse_str(s);
                                if let Some(sck) = sck_opt {
                                    log::debug!("Set SCK to {}", sck);
                                    self.sck = sck;
                                }
                                if let Some(ick) = ick_opt {
                                    log::debug!("Set ICK to {}", ick);
                                    self.ick = ick;
                                }
                                string.push_str(s);
                            }
                            log::warn!("terminator: {:02X}", terminator);
                            string_end = str_opt.is_none() || terminator == 0;
                        }
                    }
                    OobBlock::Eof => {
                        log::debug!("EOF block");
                        return Ok(true);
                    }
                }
            }
            _ => {
                // Flux1 block
                let flux = (self.flux_ovl + byte as u32) as f64 / self.sck;
                streams.last_mut().unwrap().push(flux);
                self.flux_ovl = 0;
            }
        }

        // Return whether we reached end of file
        Ok(false)
    }

    pub fn expand_kryoflux_set(
        filepath: PathBuf,
        directory: Option<Vec<PathBuf>>,
    ) -> Result<(Vec<PathBuf>, DiskCh), DiskImageError> {
        let mut set_vec = Vec::new();

        // Isolate the base path and filename
        let base_path = filepath.parent().unwrap_or(Path::new(""));
        let base_name = filepath.file_name().ok_or(DiskImageError::FsError)?;

        // Create a regex for any string that ends in \d{2}\.\d\.raw
        let re = regex::Regex::new(r"(.*)\d{2}\.\d\.raw").unwrap();
        // Check if the base name matches the pattern
        let caps = re.captures(base_name.to_str().ok_or(DiskImageError::FsError)?);

        let file_listing = match directory {
            Some(d) => d,
            None => std::fs::read_dir(base_path)?
                .map(|res| res.map(|entry| entry.path()))
                .collect::<Result<Vec<PathBuf>, crate::io::Error>>()?,
        };

        //log::debug!("File listing: {:?}", file_listing);

        let mut set_ch = DiskCh::new(0, 0);
        if let Some(c) = caps {
            // If it does, get the base name and extension
            let base_name = c.get(1).ok_or(DiskImageError::FsError)?;
            let ext = ".raw";

            let mut cylinders = 0;
            let mut heads = 0;

            let mut c: u16 = 0;
            let mut h: u8 = 0;
            let mut found_file = true;
            while found_file {
                // Construct a test filename from the base name, cylinder and head number, and extension
                let test_name = format!("{}{:02}.{}{}", base_name.as_str(), c, h, ext);

                // Check if the test file exists
                if file_listing.iter().any(|f| *f.file_name().unwrap() == *test_name) {
                    log::debug!("Found filename in set: {}", test_name);

                    if h > 0 {
                        h = h.wrapping_add(1)
                    }

                    // If it does, add it to the set
                    set_vec.push(base_path.join(test_name));
                    h += 1;
                    if h > 1 {
                        h = 0;
                        c += 1;
                    }
                } else if h == 0 {
                    found_file = false;
                }
            }

            set_ch = DiskCh::new(c + 1, h);
        }

        Ok((set_vec, set_ch))
    }
}

fn kfx_parse_str(s: &str) -> (Option<f64>, Option<f64>) {
    // use a regex to parse the clock info string
    // ex: 'sck=24027428.5714285, ick=3003428.5714285625'
    let re = regex::Regex::new(r"sck=(\d+\.\d+), ick=(\d+\.\d+)").unwrap();

    let caps = re.captures(s);
    if let Some(c) = caps {
        let sck = c.get(1).and_then(|m| m.as_str().parse::<f64>().ok());
        let ick = c.get(2).and_then(|m| m.as_str().parse::<f64>().ok());
        (sck, ick)
    } else {
        (None, None)
    }
}

fn kfx_transition_ct_to_bitrate(count: usize) -> Option<DiskDataRate> {
    match count {
        35000..=60000 => Some(DiskDataRate::Rate250Kbps),
        70000..=120000 => Some(DiskDataRate::Rate500Kbps),
        140000..=240000 => Some(DiskDataRate::Rate1000Kbps),
        _ => None,
    }
}

fn print_transitions(transitions: Vec<FluxTransition>) {
    for t in transitions {
        print!("{}", t);
    }
}
