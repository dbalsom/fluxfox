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

    src/parsers/mfm.rs

    A parser for the MFM disk image format.

    MFM format images are bitstream images produced by the HxC disk emulator software.
*/
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::ParserWriteCompatibility;
use crate::io::{ReadSeek, ReadWriteSeek};
use crate::{
    DiskCh, DiskChs, DiskDataEncoding, DiskDataRate, DiskDensity, DiskImage, DiskImageError, DiskImageFormat,
    DEFAULT_SECTOR_SIZE,
};
use binrw::{binrw, BinRead};

pub struct MfmFormat;

#[derive(Debug)]
#[binrw]
#[derive(Default)]
struct MfmFileHeader {
    id: [u8; 6],
    unused: u8,
    track_ct: u16,
    head_ct: u8,
    rpm: u16,
    bit_rate: u16,
    if_type: u8,
    track_list_offset: u32,
}

#[derive(Debug)]
#[binrw]
struct MfmTrackHeader {
    track_no: u16,
    side_no: u8,
    track_size: u32,
    track_offset: u32,
}

#[derive(Debug)]
#[binrw]
struct MfmAdvancedTrackHeader {
    track_no: u16,
    side_no: u8,
    rpm: u16,
    bit_rate: u16,
    track_size: u32,
    track_offset: u32,
}

pub enum TrackHeader {
    Standard(MfmTrackHeader),
    Advanced(MfmAdvancedTrackHeader),
}

impl MfmFormat {
    fn format() -> DiskImageFormat {
        DiskImageFormat::PceBitstreamImage
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        if let Ok(file_header) = MfmFileHeader::read_le(&mut image) {
            if &file_header.id == "HXCMFM".as_bytes() {
                detected = true;
            }
        }

        detected
    }

    pub(crate) fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        // TODO: Determine what data representations would lead to data loss for MFM images
        ParserWriteCompatibility::Ok
    }

    pub(crate) fn load_image<RWS: ReadSeek>(mut image: RWS) -> Result<DiskImage, DiskImageError> {
        let mut disk_image = DiskImage::default();

        image
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_e| DiskImageError::IoError)?;

        let mut file_header = Default::default();
        if let Ok(file_header_inner) = MfmFileHeader::read_le(&mut image) {
            file_header = file_header_inner;
            if file_header.id != "HXCMFM".as_bytes() {
                log::trace!("load_image(): File header ID not detected.");
                return Err(DiskImageError::UnsupportedFormat);
            }
        }

        let advanced_tracks = file_header.if_type & 0x80 != 0;
        log::trace!(
            "load_image(): TracksPerSide: {} Heads: {} RPM: {} BitRate: {} IfType: {:02X} Advanced tracks: {}",
            file_header.track_ct,
            file_header.head_ct,
            file_header.rpm,
            file_header.bit_rate,
            file_header.if_type,
            advanced_tracks
        );

        let disk_data_rate = DiskDataRate::from(file_header.bit_rate as u32 * 100);

        let total_tracks = file_header.track_ct as usize * file_header.head_ct as usize;

        let mut track_headers = Vec::new();

        // If the advanced header flag is set, a table of 'total_tracks' advanced track headers follows the file header.
        // Otherwise, a table of 'total_tracks' standard track headers follows the file header.

        image
            .seek(std::io::SeekFrom::Start(file_header.track_list_offset as u64))
            .map_err(|_e| DiskImageError::IoError)?;

        for _t in 0..total_tracks {
            match advanced_tracks {
                true => {
                    let track_header = MfmAdvancedTrackHeader::read_le(&mut image);
                    match track_header {
                        Ok(track_header) => {
                            let track_no = track_header.track_no as usize;
                            let side_no = track_header.side_no as usize;
                            let track_size = track_header.track_size as usize;
                            let track_offset = track_header.track_offset as usize;

                            log::trace!(
                                "load_image(): Track: {} Side: {} Size: {} Offset: {}",
                                track_no,
                                side_no,
                                track_size,
                                track_offset
                            );

                            track_headers.push(TrackHeader::Advanced(track_header));
                        }
                        Err(e) => {
                            log::error!("load_image(): Error reading track header: {:?}", e);
                            return Err(DiskImageError::FormatParseError);
                        }
                    }
                }
                false => {
                    let track_header = MfmTrackHeader::read_le(&mut image);
                    match track_header {
                        Ok(track_header) => {
                            let track_no = track_header.track_no as usize;
                            let side_no = track_header.side_no as usize;
                            let track_size = track_header.track_size as usize;
                            let track_offset = track_header.track_offset as usize;

                            log::trace!(
                                "load_image(): Track: {} Side: {} Size: {} Offset: {}",
                                track_no,
                                side_no,
                                track_size,
                                track_offset
                            );

                            track_headers.push(TrackHeader::Standard(track_header));
                        }
                        Err(e) => {
                            log::error!("load_image(): Error reading track header: {:?}", e);
                            return Err(DiskImageError::FormatParseError);
                        }
                    }
                }
            }
        }

        // We now have a table of tracks. Read the data for each track and add it to the DiskImage.

        for header in &track_headers {
            let mut cylinder = 0;
            let mut head = 0;
            let mut track_data = Vec::new();
            let mut data_rate = 0;
            match header {
                TrackHeader::Standard(s_header) => {
                    track_data = MfmFormat::read_track_data(
                        &mut image,
                        s_header.track_offset as u64,
                        s_header.track_size as usize,
                    )?;
                    head = s_header.side_no;
                    cylinder = s_header.track_no as u8;
                    data_rate = file_header.bit_rate as u32 * 100;
                }
                TrackHeader::Advanced(a_header) => {
                    track_data = MfmFormat::read_track_data(
                        &mut image,
                        a_header.track_offset as u64,
                        a_header.track_size as usize,
                    )?;
                    head = a_header.side_no;
                    cylinder = a_header.track_no as u8;
                    data_rate = a_header.bit_rate as u32 * 100;
                }
            }

            disk_image.add_track_bitstream(
                DiskDataEncoding::Mfm,
                disk_data_rate,
                DiskCh::from((cylinder as u16, head)),
                data_rate,
                &track_data,
                None,
            )?;
        }

        disk_image.image_format = DiskDescriptor {
            geometry: DiskChs::from((file_header.track_ct, file_header.head_ct, 0)),
            data_rate: disk_data_rate,
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: None,
        };

        Ok(disk_image)
    }

    fn read_track_data<RWS: ReadSeek>(image: &mut RWS, offset: u64, size: usize) -> Result<Vec<u8>, DiskImageError> {
        let mut track_data = vec![0u8; size];

        image
            .seek(std::io::SeekFrom::Start(offset))
            .map_err(|_e| DiskImageError::IoError)?;

        image
            .read_exact(&mut track_data)
            .map_err(|_e| DiskImageError::IoError)?;

        Ok(track_data)
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
