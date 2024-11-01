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

    src/parsers/scp.rs

    A parser for the SuperCardPro format.

    SCP is a flux stream format originally invented for use by the SuperCardPro
    hardware.

    SCP images can be produced by a variety of different tools, and usually
    contain bad metadata fields because these tools do not require them to be
    set properly on export.

    Fields like disk type and RPM are almost universally unreliable. We attempt
    to calculate the disk parameters ourselves as a result.

*/
use crate::diskimage::{BitStreamTrackParams, DiskDescriptor};
use crate::file_parsers::{bitstream_flags, FormatCaps};
use crate::flux::pll::{Pll, PllPreset};
use crate::flux::FluxTransition;
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::track::fluxstream::FluxStreamTrack;
use crate::{
    DiskCh, DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat, DiskRpm,
    LoadingCallback, ParserWriteCompatibility, StandardFormat, DEFAULT_SECTOR_SIZE,
};
use binrw::binrw;
use binrw::{BinRead, BinReaderExt};

pub const BASE_CAPTURE_RES: u32 = 25;
pub const SCP_FLUX_TIME_BASE: u32 = 25;

pub const SCP_TRACK_COUNT: usize = 168;
//pub const MAX_TRACK_NUMBER: usize = SCP_TRACK_COUNT - 1;

pub const SCP_FB_INDEX: u8 = 0b0000_0001;
//pub const SCP_FB_TPI: u8 = 0b0000_0010;
pub const SCP_FB_RPM: u8 = 0b0000_0100;
pub const SCP_FB_TYPE: u8 = 0b0000_1000;
pub const SCP_FB_READONLY: u8 = 0b0001_0000;
pub const SCP_FB_FOOTER: u8 = 0b0010_0000;
pub const SCP_FB_EXTENDED_MODE: u8 = 0b0100_0000;
pub const SCP_NON_SCP_CAPTURE: u8 = 0b1000_0000;

#[derive(Debug)]
pub enum ScpDiskManufacturer {
    Cbm = 0x00,
    Atari = 0x10,
    Apple = 0x20,
    Pc = 0x30,
    Tandy = 0x40,
    TI = 0x50,
    Roland = 0x60,
    Amstrad = 0x70,
    Other = 0x80,
    TapeDrive = 0xE0,
    HardDrive = 0xF0,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpFileHeader {
    pub id: [u8; 3],
    pub version: u8,
    pub disk_type: u8,
    pub revolutions: u8,
    pub start_track: u8,
    pub end_track: u8,
    pub flags: u8,
    pub bit_cell_width: u8,
    pub heads: u8,
    pub resolution: u8,
    pub checksum: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackOffsetTable {
    pub track_offsets: [u32; 168],
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackHeader {
    pub id: [u8; 3],
    pub track_number: u8,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct ScpTrackRevolution {
    pub index_time: u32,
    pub length: u32,
    pub data_offset: u32,
}

fn scp_parse_version(version_byte: u8) -> (u8, u8) {
    let major = version_byte >> 4;
    let minor = version_byte & 0x0F;
    (major, minor)
}

fn scp_disk_type(type_byte: u8) -> Option<(ScpDiskManufacturer, Option<StandardFormat>)> {
    let manufacturer = match type_byte & 0xF0 {
        0x00 => ScpDiskManufacturer::Cbm,
        0x10 => ScpDiskManufacturer::Atari,
        0x20 => ScpDiskManufacturer::Apple,
        0x30 => ScpDiskManufacturer::Pc,
        0x40 => ScpDiskManufacturer::Tandy,
        0x50 => ScpDiskManufacturer::TI,
        0x60 => ScpDiskManufacturer::Roland,
        0x70 => ScpDiskManufacturer::Amstrad,
        0x80 => ScpDiskManufacturer::Other,
        0xE0 => ScpDiskManufacturer::TapeDrive,
        0xF0 => ScpDiskManufacturer::HardDrive,
        _ => return None,
    };

    let subtype = type_byte & 0x0F;
    let disk_format = match manufacturer {
        ScpDiskManufacturer::Pc => match subtype {
            0x00 => Some(StandardFormat::PcFloppy360),
            0x01 => Some(StandardFormat::PcFloppy720),
            0x02 => Some(StandardFormat::PcFloppy1200),
            0x03 => Some(StandardFormat::PcFloppy1440),
            _ => None,
        },
        ScpDiskManufacturer::Tandy => match subtype {
            0x00 => None,
            0x01 => Some(StandardFormat::PcFloppy180),
            0x02 => None,
            0x03 => Some(StandardFormat::PcFloppy360),
            _ => None,
        },
        _ => None,
    };

    Some((manufacturer, disk_format))
}

pub struct ScpFormat {}

impl ScpFormat {
    pub fn extensions() -> Vec<&'static str> {
        vec!["scp"]
    }

    pub fn capabilities() -> FormatCaps {
        bitstream_flags()
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }
        let header = if let Ok(header) = ScpFileHeader::read(&mut image) {
            header
        }
        else {
            return false;
        };

        header.id == "SCP".as_bytes()
    }

    pub fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFormat::SuperCardPro);

        let disk_image_size = read_buf.seek(std::io::SeekFrom::End(0))?;

        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let header = ScpFileHeader::read(&mut read_buf)?;
        if header.id != "SCP".as_bytes() {
            return Err(DiskImageError::UnsupportedFormat);
        }
        log::debug!("Detected SCP file.");

        let (disk_manufacturer, disk_type) = match scp_disk_type(header.disk_type) {
            Some(dt) => {
                log::debug!("Disk type: Manufacturer {:?} Type: {:?} (*unreliable)", dt.0, dt.1);
                dt
            }
            None => {
                log::error!("Unknown SCP disk type: {:02X} (*unreliable)", header.disk_type);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };

        match disk_type.is_some() {
            true => {
                log::debug!(
                    "Have supported disk type. Manufacturer: {:?} Type: {:?}",
                    disk_manufacturer,
                    disk_type
                );
            }
            _ => {
                log::warn!(
                    "Unsupported SCP disk type. Manufacturer: {:?} Type: {:1X}",
                    disk_manufacturer,
                    header.disk_type & 0x0F
                );
                //return Err(DiskImageError::UnsupportedFormat);
            }
        }

        let disk_major_ver;
        let disk_minor_ver;

        // Handle various flags now.
        if header.flags & SCP_FB_FOOTER != 0 {
            log::debug!("Extension footer is present.");
        }
        else {
            log::debug!("Extension footer is NOT present.");
            (disk_major_ver, disk_minor_ver) = scp_parse_version(header.version);
            log::debug!(
                "SCP version {}.{} ({:02X})",
                disk_major_ver,
                disk_minor_ver,
                header.version
            );
        }

        let disk_rpm = if header.flags & SCP_FB_RPM != 0 {
            DiskRpm::Rpm300
        }
        else {
            DiskRpm::Rpm360
        };
        log::debug!("Reported Disk RPM: {:?} (*unreliable)", disk_rpm);

        let disk_readonly = header.flags & SCP_FB_READONLY == 0;
        log::debug!("Disk read-only flag: {}", disk_readonly);

        if header.flags & SCP_FB_INDEX != 0 {
            log::debug!("Tracks aligned at index mark.");
        }
        else {
            log::debug!("Tracks not aligned at index mark.");
        }

        if header.flags & SCP_FB_EXTENDED_MODE != 0 {
            log::error!("Extended mode SCP images not supported.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let flux_normalized = header.flags & SCP_FB_TYPE != 0;
        log::debug!("Flux data normalization flag: {}", flux_normalized);

        if header.flags & SCP_NON_SCP_CAPTURE == 0 {
            log::debug!("SCP image was created by SuperCardPro device.");
        }
        else {
            log::debug!("SCP image was not created by SuperCardPro device.");
        }

        log::debug!("Disk contains {} revolutions per track.", header.revolutions);
        log::debug!(
            "Starting track: {} Ending track: {}",
            header.start_track,
            header.end_track
        );
        log::debug!(
            "Bit cell width: {}",
            if header.bit_cell_width == 0 {
                16
            }
            else {
                header.bit_cell_width
            }
        );
        if header.bit_cell_width != 0 {
            log::error!("Non-standard bit cell width not supported.");
            return Err(DiskImageError::UnsupportedFormat);
        }

        let disk_heads = match header.heads {
            0 => 2,
            1 => 1,
            2 => {
                log::error!("SCP images with just side 1 are not supported.");
                return Err(DiskImageError::UnsupportedFormat);
            }
            _ => {
                log::error!("Unsupported number of disk heads: {}", header.heads);
                return Err(DiskImageError::UnsupportedFormat);
            }
        };
        log::debug!("Image has {} heads.", disk_heads);

        let capture_resolution = BASE_CAPTURE_RES + (header.resolution as u32 * BASE_CAPTURE_RES);
        let capture_resolution_seconds = capture_resolution as f64 * 1e-9;
        log::debug!(
            "Capture resolution: {}ns ({:.9} seconds)",
            capture_resolution,
            capture_resolution_seconds
        );

        if header.checksum == 0 {
            log::debug!("Image has CRC==0. Skipping CRC verification.");
        }
        else {
            log::debug!("Image CRC: {:08X}", header.checksum);
            log::debug!("Image CRC not verified.");
        }

        let mut track_table_len = SCP_TRACK_COUNT;
        let mut track_offsets: Vec<u32> = Vec::new();

        // Read int the first track offset. Its value establishes a lower bound for the size of the
        // track offset table. SCP files SHOULD contain 'SCP_TRACK_COUNT' track offsets, but some
        // are observed to contain fewer.
        let track_offset: u32 = read_buf.read_le()?;
        log::trace!("Track offset table entry {} : {:08X}", 0, track_offset);
        if track_offset < 0x10 {
            log::error!("Invalid track offset table.");
            return Err(DiskImageError::FormatParseError);
        }
        let max_table_size = (track_offset as usize - 0x10) / 4;
        if max_table_size < track_table_len {
            track_table_len = max_table_size;
            log::warn!(
                "Track offset table is too short. Truncating to {} entries.",
                track_table_len
            );
        }
        track_offsets.push(track_offset);

        let mut last_offset = track_offset;
        // Loop through the rest of the offset table entries.
        for to in 0..max_table_size - 1 {
            let track_offset: u32 = read_buf.read_le()?;

            if track_offset > 0 {
                if (track_offset <= last_offset) || (track_offset as u64 >= disk_image_size) {
                    log::error!("Bad track offset: {:08X} at entry {}", track_offset, to);
                    return Err(DiskImageError::FormatParseError);
                }
                else if track_offset > 0 {
                    log::trace!("Track offset table entry {} : {:08X}", to, track_offset);
                    track_offsets.push(track_offset);
                }
            }
            else {
                break;
            }
            last_offset = track_offset;
        }
        log::debug!("Got {} track offsets.", track_offsets.len());

        let mut c = 0;
        let mut h = 0;

        let mut disk_datarate = None;

        for (_ti, offset) in track_offsets.iter().enumerate() {
            // Seek to the track header.
            read_buf.seek(std::io::SeekFrom::Start(*offset as u64))?;

            // Read the track header.
            let track_header = ScpTrackHeader::read(&mut read_buf)?;
            log::debug!(
                "Track number: {} c:{} h:{} offset: {:08X}",
                track_header.track_number,
                c,
                h,
                offset,
            );

            // Verify header.
            if track_header.id != "TRK".as_bytes() {
                log::error!("Expected track header signature, got: {:?}", track_header.id);
                return Err(DiskImageError::UnsupportedFormat);
            }

            // Read in revolutions.
            let mut revolutions = Vec::new();
            for _ in 0..header.revolutions {
                let revolution = ScpTrackRevolution::read(&mut read_buf)?;
                revolutions.push(revolution);
            }

            let mut pll = Pll::from_preset(PllPreset::Aggressive);
            //pll.set_clock(2e-6, None);
            //let mut flux_track = RawFluxTrack::new(1.0 / capture_resolution_seconds);
            let mut flux_track = FluxStreamTrack::new();

            #[allow(clippy::never_loop)]
            for (ri, rev) in revolutions.iter().enumerate() {
                // Calculate RPM of revolution.
                let rev_nanos = (rev.index_time * SCP_FLUX_TIME_BASE) as f64;
                let rev_seconds = rev_nanos * 1e-9;
                let rev_rpm = 60.0 / rev_seconds;

                let clock_adjust;
                if (280.0..=380.0).contains(&rev_rpm) {
                    clock_adjust = rev_rpm / 300.0;

                    log::warn!(
                        "Revolution {} RPM is {:.2}, adjusting clock to {:.2}%",
                        ri,
                        rev_rpm,
                        clock_adjust * 100.0
                    );
                }
                else {
                    log::error!("Revolution {} RPM is {:.2}, out of range.", ri, rev_rpm);
                    return Err(DiskImageError::IncompatibleImage);
                }

                pll.adjust_clock(clock_adjust);

                log::debug!(
                    "Revolution {}: rpm: {} index time: {:08} length: {:08} flux offset: {:08}",
                    ri,
                    rev_rpm,
                    rev.index_time,
                    rev.length,
                    rev.data_offset
                );

                // Read flux data for this revolution.
                let mut data = vec![0u16; rev.length as usize];
                read_buf.seek(std::io::SeekFrom::Start(*offset as u64 + rev.data_offset as u64))?;

                for d in &mut data {
                    *d = read_buf.read_be()?;
                }

                log::trace!(
                    "Adding revolution {} containing {} bitcells to RawFluxTrack",
                    ri,
                    data.len()
                );
                flux_track.add_revolution_from_u16(
                    DiskCh::new(c, h),
                    &data,
                    pll.get_clock(),
                    rev_seconds,
                    capture_resolution_seconds,
                );
                pll.reset_clock();
            }

            let rev = 0;
            let flux_stream = flux_track.revolution_mut(rev).unwrap();
            flux_stream.decode_direct(&mut pll, true);

            let rev = 1;
            let flux_stream = flux_track.revolution_mut(rev).unwrap();
            let rev_stats = flux_stream.decode_direct(&mut pll, false);

            //let flux_ct = flux_stream.transition_ct();
            //let rev_encoding = flux_stream.encoding();
            let rev_density = match rev_stats.detect_density(false) {
                Some(d) => d,
                None => {
                    log::error!("Unable to detect track density, skipping track");
                    continue;
                }
            };

            // let rev_bitrate = if let Some(bitrate) = scp_transition_ct_to_bitrate(flux_ct) {
            //     bitrate
            // } else {
            //     log::error!("Unable to detect track bitrate, skipping track");
            //     continue;
            // };

            if disk_datarate.is_none() {
                disk_datarate = Some(DiskDataRate::from(rev_density));
            }
            // }
            // log::trace!("Track density: {:?} bitrate: {:?}", rev_density, rev_bitrate);

            let (track_data, track_bits) = flux_track.revolution_mut(rev).unwrap().bitstream_data();

            let params = BitStreamTrackParams {
                encoding: DiskDataEncoding::Mfm,
                data_rate: DiskDataRate::from(rev_density),
                ch: DiskCh::new(c, h),
                bitcell_ct: Some(track_bits),
                data: &track_data,
                weak: None,
                hole: None,
                detect_weak: false,
            };

            disk_image.add_track_bitstream(params)?;

            // Increment cylinder/head for next track
            h += 1;
            if h == disk_heads {
                h = 0;
                c += 1;
            }
        }

        log::trace!("Read {} valid track offsets.", track_offsets.len());

        disk_image.descriptor = DiskDescriptor {
            geometry: DiskCh::from((c as u16, disk_heads)),
            data_rate: disk_datarate.unwrap(),
            density: DiskDensity::from(disk_datarate.unwrap()),
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: Some(disk_readonly),
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}

#[allow(dead_code)]
fn scp_transition_ct_to_bitrate(count: usize) -> Option<DiskDataRate> {
    match count {
        35000..=60000 => Some(DiskDataRate::Rate250Kbps),
        70000..=120000 => Some(DiskDataRate::Rate500Kbps),
        140000..=240000 => Some(DiskDataRate::Rate1000Kbps),
        _ => None,
    }
}

#[allow(dead_code)]
fn print_transitions(transitions: Vec<FluxTransition>) {
    for t in transitions {
        print!("{}", t);
    }
}
