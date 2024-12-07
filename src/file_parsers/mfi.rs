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

    src/parsers/pfi.rs

    A parser for the PFI disk image format.

    PFI format images are PCE flux stream images, an internal format used by the PCE emulator and
    devised by Hampa Hug.

    It is a chunk-based format similar to RIFF.

*/

use crate::{
    file_parsers::{bitstream_flags, FormatCaps, ParserWriteCompatibility},
    format_ms,
    format_us,
    io::{ReadSeek, ReadWriteSeek},
    track::fluxstream::FluxStreamTrack,
    types::DiskDescriptor,
    LoadingStatus,
    StandardFormat,
};

use crate::{
    types::{chs::DiskCh, DiskDataEncoding, DiskDataRate, DiskDensity, DiskPhysicalDimensions},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    LoadingCallback,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

pub const OLD_SIGNATURE: &[u8; 15] = b"MESSFLOPPYIMAGE";
pub const NEW_SIGNATURE: &[u8; 15] = b"MAMEFLOPPYIMAGE";

pub const THREE_POINT_FIVE_INCH: &[u8; 4] = b"3   ";
pub const FIVE_POINT_TWO_FIVE_INCH: &[u8; 4] = b"525 ";
pub const EIGHT_INCH: &[u8; 4] = b"8   ";

pub const CYLINDER_MASK: u32 = 0x3FFFFFFF;
//pub const MFI_TIME_UNIT: f64 = 1.0 / 200_000_000.0;

// Disk form factors - defined in MAME src/lib/formats/flopimg.h
impl TryFrom<&[u8; 4]> for DiskPhysicalDimensions {
    type Error = DiskImageError;

    fn try_from(value: &[u8; 4]) -> Result<Self, Self::Error> {
        match value {
            THREE_POINT_FIVE_INCH => Ok(DiskPhysicalDimensions::Dimension3_5),
            FIVE_POINT_TWO_FIVE_INCH => Ok(DiskPhysicalDimensions::Dimension5_25),
            EIGHT_INCH => Ok(DiskPhysicalDimensions::Dimension8),
            _ => Err(DiskImageError::UnsupportedFormat),
        }
    }
}

impl TryFrom<(&[u8; 4], DiskPhysicalDimensions)> for StandardFormat {
    type Error = DiskImageError;

    fn try_from(value: (&[u8; 4], DiskPhysicalDimensions)) -> Result<Self, Self::Error> {
        match value {
            (b"SSSD", _) => {
                // Single sided single density (8" format)
                Err(DiskImageError::UnsupportedFormat)
            }
            (b"SSDD", DiskPhysicalDimensions::Dimension5_25) => {
                // This could be 160K or 180K, we don't really know
                Ok(StandardFormat::PcFloppy180)
            }
            (b"DSSD", _) => {
                // Double sided single density (8" format)
                Err(DiskImageError::UnsupportedFormat)
            }
            (b"DSDD", DiskPhysicalDimensions::Dimension5_25) => {
                // This could be 320K or 360K, we don't really know
                Ok(StandardFormat::PcFloppy360)
            }
            (b"DSDD", DiskPhysicalDimensions::Dimension3_5) => {
                // 720K 3.5" disk
                Ok(StandardFormat::PcFloppy720)
            }
            (b"DSHD", DiskPhysicalDimensions::Dimension5_25) => {
                // MAME src doesn't seem to mention this one...
                Ok(StandardFormat::PcFloppy1200)
            }
            (b"DSHD", DiskPhysicalDimensions::Dimension3_5) => {
                // 1.44M 3.5" disk
                Ok(StandardFormat::PcFloppy1440)
            }
            (b"DSED", DiskPhysicalDimensions::Dimension3_5) => {
                // 2.88M 3.5" disk
                Ok(StandardFormat::PcFloppy2880)
            }
            _ => Err(DiskImageError::UnsupportedFormat),
        }
    }
}

pub struct MfiFormat;

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct MfiFileHeader {
    pub id: [u8; 16],
    pub cylinders: u32,
    pub heads: u32,
    pub form_factor: u32,
    pub variant: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct MfiTrackHeader {
    pub offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub write_splice: u32,
}

pub struct MfiTrackData {
    pub ch:   DiskCh,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub enum FluxEntryType {
    Flux = 0,
    Nfa = 1,
    Hole = 2,
    End = 3,
}

#[allow(dead_code)]
pub struct MfiTrackZone {
    start: u32,
    end:   u32,
}

impl MfiFormat {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["mfi"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = MfiFileHeader::read_be(&mut image) {
            if file_header.id[0..15] == *OLD_SIGNATURE || file_header.id[0..15] == *NEW_SIGNATURE {
                detected = true;
            }
        }

        detected
    }

    /// Return the compatibility of the image with the parser.
    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut read_buf: RWS,
        disk_image: &mut DiskImage,
        callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        if let Some(ref callback_fn) = callback {
            // Let caller know to show a progress bar
            callback_fn(LoadingStatus::ProgressSupport);
        }

        disk_image.set_source_format(DiskImageFileFormat::MameFloppyImage);
        let disk_len = read_buf.seek(std::io::SeekFrom::End(0))?;

        // Seek to start of image.
        read_buf.seek(std::io::SeekFrom::Start(0))?;

        let file_header = MfiFileHeader::read(&mut read_buf)?;

        if file_header.id[0..15] != *NEW_SIGNATURE {
            log::error!(
                "Old MFI format {:?} not implemented.",
                std::str::from_utf8(&file_header.id[0..15]).unwrap()
            );
            return Err(DiskImageError::UnsupportedFormat);
        }

        let file_form_factor = DiskPhysicalDimensions::try_from(&file_header.form_factor.to_le_bytes()).ok();
        if let Some(form_factor) = file_form_factor {
            log::debug!("Got MFI file form factor: {:?}", form_factor);
        }
        else {
            log::error!(
                "Unknown or unsupported disk form factor: {:08X}",
                file_header.form_factor
            );
            return Err(DiskImageError::UnsupportedFormat);
        }

        if let Ok(standard_format) =
            StandardFormat::try_from((&file_header.variant.to_le_bytes(), file_form_factor.unwrap()))
        {
            log::debug!("Got MFI file standard format: {:?}", standard_format);
        }
        else {
            log::error!(
                "Unknown or unsupported disk standard format: {:08X}",
                file_header.variant
            );
            return Err(DiskImageError::UnsupportedFormat);
        }

        let file_ch = DiskCh::from(((file_header.cylinders & CYLINDER_MASK) as u16, file_header.heads as u8));
        let file_resolution = file_header.cylinders >> 30;
        log::trace!("Got MFI file: ch: {} resolution: {}", file_ch, file_resolution);

        let mut track_list: Vec<MfiTrackHeader> = Vec::with_capacity(84 * 2);

        if file_ch.c() == 0 || file_ch.h() == 0 {
            log::error!("Invalid MFI file: cylinders or heads was 0");
            return Err(DiskImageError::ImageCorruptError(
                "Cylinders or heads was 0".to_string(),
            ));
        }

        let mut last_offset: u32 = 0;
        for ch in file_ch.iter() {
            let DiskCh { c, h } = ch;
            let track_header = MfiTrackHeader::read(&mut read_buf)?;

            log::trace!(
                "Track {} at offset: {} compressed: {} uncompressed: {}",
                ch,
                track_header.offset,
                track_header.compressed_size,
                track_header.uncompressed_size
            );

            if (track_header.compressed_size > 0) && (track_header.offset < last_offset) {
                log::error!(
                    "Invalid MFI file: non-zero length track {} offset {} is less than last offset ({}).",
                    ch,
                    track_header.offset,
                    last_offset
                );
                return Err(DiskImageError::ImageCorruptError(
                    "Non-empty track offset less than last offset".to_string(),
                ));
            }

            if track_header.offset as u64 > disk_len {
                log::error!(
                    "Invalid MFI file: track {} offset {} is greater than file length.",
                    ch,
                    track_header.offset
                );
                return Err(DiskImageError::ImageCorruptError(
                    "Track offset greater than file length".to_string(),
                ));
            }

            // Ignore offsets of 0 - they indicate empty tracks.
            if track_header.offset != 0 {
                last_offset = track_header.offset;
            }

            track_list.push(track_header);
        }

        log::debug!("Got {} track entries.", track_list.len());

        let mut tracks: Vec<MfiTrackData> = Vec::with_capacity(track_list.len());

        let mut ch_cursor = DiskCh::new(0, 0);
        for (ti, entry) in track_list.iter().enumerate() {
            log::debug!(
                "Track {} at offset: {} compressed: {} uncompressed: {}",
                ti,
                entry.offset,
                entry.compressed_size,
                entry.uncompressed_size
            );

            if entry.offset == 0 || entry.compressed_size == 0 || entry.uncompressed_size == 0 {
                // All of the above are indicative of an empty/unformatted track.

                // Push empty trackdata
                tracks.push(MfiTrackData {
                    ch:   ch_cursor,
                    data: Vec::new(),
                });
            }
            else {
                // Read in compressed track data.
                let mut track_data = vec![0u8; entry.compressed_size as usize];
                read_buf.seek(std::io::SeekFrom::Start(entry.offset as u64))?;
                read_buf.read_exact(&mut track_data)?;

                // Decompress track data.
                let mut decompressed_data = vec![0u8; entry.uncompressed_size as usize];

                let mut decompress = flate2::Decompress::new(true);
                match decompress.decompress(&track_data, &mut decompressed_data, flate2::FlushDecompress::Finish) {
                    Ok(flate2::Status::Ok) | Ok(flate2::Status::StreamEnd) => {
                        log::debug!("Successfully decompressed track data for track {}", ch_cursor);
                    }
                    Ok(flate2::Status::BufError) => {
                        log::error!("Decompression buffer error reading track {} data.", ch_cursor);
                        return Err(DiskImageError::ImageCorruptError(format!(
                            "Decompression buffer error reading track {} data",
                            ch_cursor
                        )));
                    }
                    Err(e) => {
                        log::error!("Decompression error reading track data: {:?}", e);
                        return Err(DiskImageError::ImageCorruptError(format!(
                            "Decompression error reading track {} data: {:?}",
                            ch_cursor, e
                        )));
                    }
                }

                // Push uncompressed trackdata
                tracks.push(MfiTrackData {
                    ch:   ch_cursor,
                    data: decompressed_data.to_vec(),
                });
            }

            // Advance ch
            ch_cursor.seek_next_track(file_ch);
        }

        let mut disk_density = None;

        let total_tracks = tracks.len();

        let mut last_data_rate = None;
        let mut last_bitcell_ct = None;

        for (ti, track) in tracks.iter().enumerate() {
            let flux_track = Self::process_track_data_new(track)?;

            if flux_track.is_empty() {
                if last_data_rate.is_none() || last_bitcell_ct.is_none() {
                    log::error!("Track 0 cannot be unformatted.");
                    return Err(DiskImageError::ImageCorruptError(
                        "Track 0 cannot be unformatted.".to_string(),
                    ));
                }

                log::warn!(
                    "Flux track appears unformatted. Adding empty track of {:?} density",
                    disk_density
                );

                disk_image.add_empty_track(
                    track.ch,
                    DiskDataEncoding::Mfm,
                    last_data_rate.unwrap(),
                    last_bitcell_ct.unwrap(),
                )?;
            }
            else {
                let new_track = disk_image.add_track_fluxstream(track.ch, flux_track, None, None)?;
                let info = new_track.info();

                log::debug!(
                    "Added {} track {} containing {} bits to image...",
                    track.ch,
                    info.encoding,
                    info.bit_length,
                );

                last_data_rate = Some(info.data_rate);
                last_bitcell_ct = Some(info.bit_length);

                // let data_rate = DiskDataRate::from(flux_track.density());
                // let stream = flux_track.revolution(0).unwrap();
                // let (stream_bytes, stream_bit_ct) = stream.bitstream_data();
                // log::trace!(
                //     "Adding track {} containing {} bits to image...",
                //     track.ch,
                //     stream_bit_ct
                // );
                //
                // let params = BitStreamTrackParams {
                //     encoding: DiskDataEncoding::Mfm,
                //     data_rate,
                //     rpm: None,
                //     ch: track.ch,
                //     bitcell_ct: Some(stream_bit_ct),
                //     data: &stream_bytes,
                //     weak: None,
                //     hole: None,
                //     detect_weak: false,
                // };
                // disk_image.add_track_bitstream(params)?;
                //
                // last_data_rate = Some(data_rate);
                // last_bitcell_ct = Some(stream_bit_ct);

                if disk_density.is_none() {
                    // Set disk density to the first track's density.
                    log::debug!("Setting disk density to {:?}", info.density);
                    disk_density = info.density;
                }

                if let Some(ref callback_fn) = callback {
                    let progress = ti as f64 / total_tracks as f64;
                    callback_fn(LoadingStatus::Progress(progress));
                }
            }
        }

        disk_image.descriptor = DiskDescriptor {
            geometry: file_ch,
            data_rate: disk_density.unwrap_or(DiskDensity::Double).into(),
            density: disk_density.unwrap_or(DiskDensity::Double),
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
            write_protect: Some(true),
        };

        Ok(())
    }

    pub fn process_track_data_new(track: &MfiTrackData) -> Result<FluxStreamTrack, DiskImageError> {
        let mut fluxes = Vec::with_capacity(track.data.len() / 4);
        let mut total_flux_time = 0.0;
        let mut nfa_zones = Vec::new();
        let mut hole_zones = Vec::new();
        let mut flux_ct = 0;
        let mut current_nfa_zone = None;
        let mut current_hole_zone = None;

        for u32_bytes in track.data.chunks_exact(4) {
            let flux_entry = u32::from_le_bytes(u32_bytes.try_into().unwrap());

            let (flux_type, flux_delta) = MfiFormat::mfi_read_flux(flux_entry);

            match flux_type {
                FluxEntryType::Flux => {
                    // Process flux entry
                    let flux_delta_f64 = flux_delta as f64 * 1e-9;
                    //log::trace!("Flux entry: {} {}", flux_ct, format_us!(flux_delta_f64));
                    total_flux_time += flux_delta_f64;
                    fluxes.push(flux_delta_f64);
                    flux_ct += 1;
                }
                FluxEntryType::Nfa => {
                    // Process NFA entry
                    if current_nfa_zone.is_some() {
                        log::warn!("NFA entry found while already in NFA zone.");
                        if current_hole_zone.is_some() {
                            log::error!("HOLE entry found while already in NFA zone.");
                        }
                    }
                    else {
                        // Start NFA zone
                        current_nfa_zone = Some(MfiTrackZone {
                            start: flux_delta,
                            end:   0,
                        });
                    }
                }
                FluxEntryType::Hole => {
                    // Process hole entry
                    if current_hole_zone.is_some() {
                        log::warn!("HOLE entry found while already in HOLE zone.");
                        if current_nfa_zone.is_some() {
                            log::error!("NFA entry found while already in HOLE zone.");
                        }
                    }
                    else {
                        // Start HOLE zone
                        current_hole_zone = Some(MfiTrackZone {
                            start: flux_delta,
                            end:   0,
                        });
                    }
                }
                FluxEntryType::End => {
                    // End of zone
                    if current_nfa_zone.is_some() {
                        // End NFA zone
                        current_nfa_zone.as_mut().unwrap().end = flux_delta;
                        nfa_zones.push(current_nfa_zone.take().unwrap());
                    }
                    else if current_hole_zone.is_some() {
                        // End HOLE zone
                        current_hole_zone.as_mut().unwrap().end = flux_delta;
                        hole_zones.push(current_hole_zone.take().unwrap());
                    }
                    else {
                        log::warn!("END ZONE entry found without an active zone.");
                    }
                }
            }
        }

        log::trace!(
            "Track {} has {} flux entries over {}, {} NFA zones, and {} HOLE zones.",
            track.ch,
            flux_ct,
            format_ms!(total_flux_time),
            nfa_zones.len(),
            hole_zones.len()
        );

        // let mut pll = Pll::from_preset(PllPreset::Aggressive);
        // pll.set_clock(1_000_000.0, None);
        //let mut flux_track = FluxStreamTrack::new(1.0 / 2e-6);
        let mut flux_track = FluxStreamTrack::new();

        flux_track.add_revolution(track.ch, &fluxes, 0.2); // 200ms
                                                           //let flux_stream = flux_track.revolution_mut(0).unwrap();
                                                           //let rev_stats = flux_stream.decode_direct(&mut pll);
                                                           //let rev_encoding = flux_stream.encoding();

        // let new_track = disk_image.add_track_fluxstream(track.ch, flux_track, None, None);
        //
        // let rev_density = match rev_stats.detect_density(true) {
        //     Some(d) => {
        //         log::debug!("Revolution {} density: {:?}", 0, d);
        //         d
        //     }
        //     None => {
        //         log::error!("Unable to detect track density!");
        //         //return Err(DiskImageError::IncompatibleImage);
        //         DiskDensity::Double
        //     }
        // };
        //
        // flux_track.set_density(rev_density);
        // flux_track.normalize();

        Ok(flux_track)
    }

    pub fn mfi_read_flux(flux_entry: u32) -> (FluxEntryType, u32) {
        let flux_type = match flux_entry >> 28 {
            0 => FluxEntryType::Flux,
            1 => FluxEntryType::Nfa,
            2 => FluxEntryType::Hole,
            3 => FluxEntryType::End,
            _ => unreachable!(),
        };

        (flux_type, flux_entry & 0x3FFFFFFF)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
