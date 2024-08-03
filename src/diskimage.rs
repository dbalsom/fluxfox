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
use crate::bitstream::mfm::MfmDecoder;
use crate::bitstream::raw::RawDecoder;
use crate::chs::{DiskCh, DiskChs};
use crate::detect::detect_image_format;
use crate::file_parsers::ImageParser;
use crate::io::{Read, ReadSeek, Seek, SeekFrom};
use crate::structure_parsers::system34::{System34Element, System34Marker, System34Parser};
use crate::structure_parsers::{
    DiskStructureElement, DiskStructureMetadata, DiskStructureMetadataItem, DiskStructureParser,
};
use crate::{DiskDataEncoding, DiskDataRate, DiskImageError, DiskRpm, EncodingPhase, DEFAULT_SECTOR_SIZE};
use bit_vec::BitVec;
use std::arch::x86_64::_mm_stream_ss;
use std::fmt::Display;

/// An enumeration describing the type of disk image.
#[derive(Copy, Clone, Debug)]
pub enum DiskImageFormat {
    RawSectorImage,
    ImageDisk,
    PceSectorImage,
    PceBitstreamImage,
    MfmBitstreamImage,
    TeleDisk,
    KryofluxStream,
    HfeImage,
}

impl Display for DiskImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFormat::RawSectorImage => "Raw Sector Image".to_string(),
            DiskImageFormat::PceSectorImage => "PCE Sector Image".to_string(),
            DiskImageFormat::PceBitstreamImage => "PCE Bitstream Image".to_string(),
            DiskImageFormat::ImageDisk => "ImageDisk".to_string(),
            DiskImageFormat::TeleDisk => "TeleDisk".to_string(),
            DiskImageFormat::KryofluxStream => "Kryoflux Stream".to_string(),
            DiskImageFormat::MfmBitstreamImage => "HxC MFM Bitstream Image".to_string(),
            DiskImageFormat::HfeImage => "HFEv1 Bitstream Image".to_string(),
        };
        write!(f, "{}", str)
    }
}

/// An enumeration describing the type of disk image.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum FloppyFormat {
    Unknown,
    FloppyCustom(DiskChs),
    PcFloppy160,
    PcFloppy180,
    PcFloppy320,
    PcFloppy360,
    PcFloppy720,
    PcFloppy1200,
    PcFloppy1440,
    PcFloppy2880,
}

impl FloppyFormat {
    /// Returns the CHS geometry corresponding to the DiskImageType.
    pub fn get_chs(&self) -> DiskChs {
        match self {
            FloppyFormat::Unknown => DiskChs::default(),
            FloppyFormat::FloppyCustom(chs) => *chs,
            FloppyFormat::PcFloppy160 => DiskChs::new(40, 1, 8),
            FloppyFormat::PcFloppy180 => DiskChs::new(40, 1, 9),
            FloppyFormat::PcFloppy320 => DiskChs::new(40, 2, 8),
            FloppyFormat::PcFloppy360 => DiskChs::new(40, 2, 9),
            FloppyFormat::PcFloppy720 => DiskChs::new(80, 2, 9),
            FloppyFormat::PcFloppy1200 => DiskChs::new(80, 2, 15),
            FloppyFormat::PcFloppy1440 => DiskChs::new(80, 2, 18),
            FloppyFormat::PcFloppy2880 => DiskChs::new(80, 2, 36),
        }
    }

    pub fn get_encoding(&self) -> DiskDataEncoding {
        DiskDataEncoding::Mfm
    }

    pub fn get_data_rate(&self) -> DiskDataRate {
        match self {
            FloppyFormat::Unknown => DiskDataRate::Rate500Kbps,
            FloppyFormat::FloppyCustom(_) => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy160 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy180 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy320 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy360 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy720 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy1200 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy1440 => DiskDataRate::Rate500Kbps,
            FloppyFormat::PcFloppy2880 => DiskDataRate::Rate500Kbps,
        }
    }

    pub fn get_rpm(&self) -> DiskRpm {
        match self {
            FloppyFormat::Unknown => DiskRpm::Rpm360,
            FloppyFormat::FloppyCustom(_) => DiskRpm::Rpm360,
            FloppyFormat::PcFloppy160 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy180 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy320 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy360 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy720 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy1200 => DiskRpm::Rpm360,
            FloppyFormat::PcFloppy1440 => DiskRpm::Rpm300,
            FloppyFormat::PcFloppy2880 => DiskRpm::Rpm300,
        }
    }

    pub fn get_image_format(&self) -> DiskDescriptor {
        DiskDescriptor {
            geometry: self.get_chs(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            data_encoding: DiskDataEncoding::Mfm,
            data_rate: DiskDataRate::Rate500Kbps,
            rpm: Some(DiskRpm::Rpm300),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            FloppyFormat::Unknown => 0,
            FloppyFormat::FloppyCustom(chs) => chs.c() as usize * chs.h() as usize * chs.s() as usize * 512,
            FloppyFormat::PcFloppy160 => 163_840,
            FloppyFormat::PcFloppy180 => 184_320,
            FloppyFormat::PcFloppy320 => 327_680,
            FloppyFormat::PcFloppy360 => 368_640,
            FloppyFormat::PcFloppy720 => 737_280,
            FloppyFormat::PcFloppy1200 => 1_228_800,
            FloppyFormat::PcFloppy1440 => 1_474_560,
            FloppyFormat::PcFloppy2880 => 2_949_120,
        }
    }
}

impl From<FloppyFormat> for usize {
    fn from(format: FloppyFormat) -> Self {
        format.size()
    }
}

impl From<usize> for FloppyFormat {
    fn from(size: usize) -> Self {
        match size {
            163_840 => FloppyFormat::PcFloppy160,
            184_320 => FloppyFormat::PcFloppy180,
            327_680 => FloppyFormat::PcFloppy320,
            368_640 => FloppyFormat::PcFloppy360,
            737_280 => FloppyFormat::PcFloppy720,
            1_228_800 => FloppyFormat::PcFloppy1200,
            1_474_560 => FloppyFormat::PcFloppy1440,
            2_949_120 => FloppyFormat::PcFloppy2880,
            _ => FloppyFormat::Unknown,
        }
    }
}

/// A DiskConsistency structure maintains information about the consistency of a disk image.
pub struct DiskConsistency {
    /// Whether the disk image contains weak bits.
    pub weak: bool,
    /// Whether the disk image contains deleted sectors.
    pub deleted: bool,
    /// The sector size if the disk image has consistent sector sizes, otherwise None.
    pub consistent_sector_size: Option<u32>,
    /// The track length in sectors if the disk image has consistent track lengths, otherwise None.
    pub consistent_track_length: Option<u8>,
}

impl Default for DiskConsistency {
    fn default() -> Self {
        Self {
            weak: false,
            deleted: false,
            consistent_sector_size: None,
            consistent_track_length: None,
        }
    }
}

/// Per-track format settings. In most cases, this will not change per-track. Some formats encode
/// this per-track, so we store it here.
pub struct TrackFormat {
    pub data_encoding: DiskDataEncoding,
    pub data_sync: Option<EncodingPhase>,
    pub data_rate: DiskDataRate,
}

pub enum TrackDataStream {
    Raw(RawDecoder),
    Mfm(MfmDecoder),
    Fm(BitVec),
    Gcr(BitVec),
}

/// TrackData stores the raw data for a track.
/// A weak bit mask is stored along with the data.
///
pub enum TrackData {
    BitStream {
        cylinder: u16,
        head: u8,
        data_clock: u32,
        data: TrackDataStream,
        metadata: Vec<TrackRegion>,
    },
    ByteStream {
        cylinder: u16,
        head: u8,
        sectors: Vec<TrackSectorIndex>,
        data: Vec<u8>,
        weak_mask: Vec<u8>,
    },
}

pub enum TrackRegionType {
    TrackIndex,
    SectorId,
    SectorDam,
    SectorData,
    Gap1,
    Gap2,
    Gap3,
    Gap4a,
    Gap4b,
    Sync,
    Other,
}

pub struct TrackRegion {
    pub m_type: TrackRegionType,
    pub bit_start: usize,
    pub bit_length: usize,
    pub crc: Option<(u16, u16, bool)>,
}

pub struct TrackSectorIndex {
    pub sector_id: u8,
    pub cylinder_id: u16,
    pub head_id: u8,
    pub t_idx: usize,
    pub len: usize,
    pub crc_error: bool,
    pub deleted_mark: bool,
}

/// A Disk Track is a circular region of the disk surface in which a number of sectors are stored.
/// Certain disk operations can be performed on an entire track, such as reading and formatting.
pub struct DiskTrack {
    /// A track comprises a vector of indices into the DiskImage sector pool.
    pub format: TrackFormat,
    pub data: TrackData,
    pub metadata: DiskStructureMetadata,
}

#[derive(Copy, Clone, Default)]
pub struct DiskDescriptor {
    /// The basic geometry of the disk. Not all tracks present need to conform to the specified sector count (s).
    pub geometry: DiskChs,
    /// The "default" sector size of the disk. Larger or smaller sectors may still be present in the disk image.
    pub default_sector_size: usize,
    /// The default data encoding used. The disk may still contain tracks in different encodings.
    pub data_encoding: DiskDataEncoding,
    /// The data rate of the disk
    pub data_rate: DiskDataRate,
    /// The rotation rate of the disk. If not provided, this can be determined from other parameters.
    pub rpm: Option<DiskRpm>,
}

pub struct ReadSectorResult {
    pub deleted_mark: bool,
    pub crc_error: bool,
    pub wrong_cylinder: bool,
    pub data: Vec<u8>,
}

/// A DiskImage represents an image of a floppy disk in memory. It comprises a pool of sectors, and an ordered
/// list of tracks that reference sectors in the pool.
/// Sectors may be variable length due to various copy protection schemes.
pub struct DiskImage {
    pub disk_format: FloppyFormat,
    pub image_format: DiskDescriptor,
    pub consistency: DiskConsistency,
    pub sector_size: usize,
    // The volume name of the disk image, if any.
    pub volume_name: Option<String>,
    // An ASCII comment embedded in the disk image, if any.
    pub comment: Option<String>,
    /// A pool of track data structures, potentially in any order.
    pub track_pool: Vec<DiskTrack>,
    /// An array of vectors containing indices into the track pool. The first index is the head
    /// number, the second is the cylinder number.
    pub track_map: [Vec<usize>; 2],
    pub sector_map: [Vec<Vec<usize>>; 2],
}

impl Default for DiskImage {
    fn default() -> Self {
        Self {
            disk_format: FloppyFormat::PcFloppy360,
            image_format: DiskDescriptor::default(),
            consistency: Default::default(),
            sector_size: DEFAULT_SECTOR_SIZE,
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            sector_map: [Vec::new(), Vec::new()],
        }
    }
}

impl DiskImage {
    pub fn detect_format<RS: ReadSeek>(mut image: &mut RS) -> Result<DiskImageFormat, DiskImageError> {
        detect_image_format(&mut image)
    }

    pub fn new(disk_format: FloppyFormat) -> Self {
        Self {
            disk_format,
            image_format: disk_format.get_image_format(),
            sector_size: DEFAULT_SECTOR_SIZE,
            consistency: DiskConsistency {
                weak: false,
                deleted: false,
                consistent_sector_size: Some(DEFAULT_SECTOR_SIZE as u32),
                consistent_track_length: Some(disk_format.get_chs().s()),
            },
            volume_name: None,
            comment: None,
            track_pool: Vec::new(),
            track_map: [Vec::new(), Vec::new()],
            sector_map: [Vec::new(), Vec::new()],
        }
    }

    pub fn load<RS: ReadSeek>(image_io: &mut RS) -> Result<Self, DiskImageError> {
        let format = DiskImage::detect_format(image_io)?;
        let image = format.load_image(image_io)?;
        Ok(image)
    }

    pub fn set_volume_name(&mut self, name: String) {
        self.volume_name = Some(name);
    }

    pub fn volume_name(&self) -> Option<&str> {
        self.volume_name.as_deref()
    }

    pub fn set_data_rate(&mut self, rate: DiskDataRate) {
        self.image_format.data_rate = rate;
    }

    pub fn data_rate(&self) -> DiskDataRate {
        self.image_format.data_rate
    }

    pub fn set_data_encoding(&mut self, encoding: DiskDataEncoding) {
        self.image_format.data_encoding = encoding;
    }

    pub fn data_encoding(&self) -> DiskDataEncoding {
        self.image_format.data_encoding
    }

    pub fn set_image_format(&mut self, format: DiskDescriptor) {
        self.image_format = format;
    }

    pub fn image_format(&self) -> DiskDescriptor {
        self.image_format
    }

    pub fn heads(&self) -> u8 {
        self.image_format.geometry.h()
    }

    pub fn add_track_bytestream(&mut self, data_encoding: DiskDataEncoding, data_rate: DiskDataRate, ch: DiskCh) {
        assert!(ch.h() < 2);

        let format = TrackFormat {
            data_encoding,
            data_sync: None,
            data_rate,
        };
        //self.tracks[ch.h() as usize].push(DiskTrack {
        self.track_pool.push(DiskTrack {
            format,
            metadata: DiskStructureMetadata::default(),
            data: TrackData::ByteStream {
                cylinder: ch.c(),
                head: ch.h(),
                sectors: Vec::new(),
                data: Vec::new(),
                weak_mask: Vec::new(),
            },
        });
        self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);
    }

    pub fn add_track_bitstream(
        &mut self,
        data_encoding: DiskDataEncoding,
        data_rate: DiskDataRate,
        ch: DiskCh,
        data_clock: u32,
        data: &[u8],
        weak: Option<&[u8]>,
    ) -> Result<(), DiskImageError> {
        if ch.h() >= 2 {
            return Err(DiskImageError::SeekError);
        }

        if weak.is_some() && (data.len() != weak.unwrap().len()) {
            return Err(DiskImageError::ParameterError);
        }

        let data = BitVec::from_bytes(data);
        let weak_mask = BitVec::from_elem(data.len(), false);

        log::trace!("add_track_bitstream(): Encoding is {:?}", data_encoding);
        let (mut data_stream, markers) = match data_encoding {
            DiskDataEncoding::Mfm => {
                let mut data_stream = TrackDataStream::Mfm(MfmDecoder::new(data, Some(weak_mask)));
                let markers = System34Parser::scan_track_markers(&mut data_stream);

                System34Parser::create_clock_map(&markers, data_stream.clock_map_mut().unwrap());
                (data_stream, markers)
            }
            DiskDataEncoding::Fm => {
                // TODO: Handle FM encoding sync
                (TrackDataStream::Raw(RawDecoder::new(data, Some(weak_mask))), Vec::new())
            }
            _ => (TrackDataStream::Raw(RawDecoder::new(data, Some(weak_mask))), Vec::new()),
        };

        let format = TrackFormat {
            data_encoding,
            data_sync: data_stream.get_sync(),
            data_rate,
        };

        let mut metadata = DiskStructureMetadata::new(System34Parser::scan_track_metadata(&mut data_stream, markers));

        let sector_offsets = metadata
            .items
            .iter()
            .filter_map(|i| {
                if let DiskStructureElement::System34(System34Element::Data(crc)) = i.elem_type {
                    //log::trace!("Got Data element, returning start address: {}", i.start);
                    Some(i.start)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        log::warn!(
            "Retrieved {} sector bitstream offsets from metadata.",
            sector_offsets.len()
        );

        self.track_pool.push(DiskTrack {
            format,
            metadata,
            data: TrackData::BitStream {
                cylinder: ch.c(),
                head: ch.h(),
                data_clock,
                data: data_stream,
                metadata: Vec::new(),
            },
        });
        self.track_map[ch.h() as usize].push(self.track_pool.len() - 1);

        Ok(())
    }

    /// Master a new sector to a track.
    /// This function is only valid for ByteStream track data.
    pub fn master_sector(
        &mut self,
        chs: DiskChs,
        id: u8,
        cylinder_id: Option<u16>,
        head_id: Option<u8>,
        sector_data: &[u8],
        weak: Option<&[u8]>,
        crc_error: bool,
        deleted_mark: bool,
    ) -> Result<(), DiskImageError> {
        if chs.h() >= 2 || self.track_map[chs.h() as usize].len() < chs.c() as usize {
            return Err(DiskImageError::SeekError);
        }

        // Create an empty weak bit mask if none is provided.
        let weak_buf_vec = match weak {
            Some(weak_buf) => weak_buf.to_vec(),
            None => vec![0; sector_data.len()],
        };

        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &mut self.track_pool[ti];

        match track.data {
            TrackData::ByteStream {
                ref mut sectors,
                ref mut data,
                ref mut weak_mask,
                ..
            } => {
                sectors.push(TrackSectorIndex {
                    sector_id: id,
                    cylinder_id: cylinder_id.unwrap_or(chs.c()),
                    head_id: head_id.unwrap_or(chs.h()),
                    t_idx: data.len(),
                    len: data.len(),
                    crc_error,
                    deleted_mark,
                });
                data.extend(sector_data);
                weak_mask.extend(weak_buf_vec);
            }
            TrackData::BitStream { .. } => {
                return Err(DiskImageError::UnsupportedFormat);
            }
        }

        Ok(())
    }

    pub fn get_sector_bit_index(&self, seek_chs: DiskChs) -> Option<usize> {
        let ti = self.track_map[seek_chs.h() as usize][seek_chs.c() as usize];
        let track = &self.track_pool[ti];

        log::trace!("metadata items: {:?}", track.metadata.items);

        match &track.data {
            TrackData::BitStream { .. } => {
                let mut last_marker_was_idam = false;
                let mut idam_chs: Option<DiskChs> = None;
                for mdi in &track.metadata.items {
                    match mdi {
                        DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::Marker(System34Marker::Idam, _)),
                            chs,
                            ..
                        } => {
                            if let Some(idam_chs) = chs {
                                if *idam_chs == seek_chs {
                                    log::warn!("Found matching IDAM at CHS: {:?}", idam_chs);
                                    last_marker_was_idam = true;
                                }
                            }
                            idam_chs = *chs;
                        }
                        DiskStructureMetadataItem {
                            elem_type: DiskStructureElement::System34(System34Element::Data(_)),
                            ..
                        } => {
                            log::debug!(
                                "Found DAM at CHS: {:?}, index: {} last idam matched? {}",
                                idam_chs,
                                mdi.start,
                                last_marker_was_idam
                            );
                            if last_marker_was_idam {
                                return Some(mdi.start);
                            }
                        }
                        _ => {}
                    }
                }
            }
            TrackData::ByteStream { sectors, data, .. } => {}
        }

        None
    }

    /// Read the sectors of 'len' bytes from the disk image starting at the sector mark given by 'chs'.
    pub fn read_sector(&mut self, chs: DiskChs, ct: usize, len: usize) -> Result<ReadSectorResult, DiskImageError> {
        if chs.h() >= 2 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }

        let sector_offset = match self.get_sector_bit_index(chs) {
            Some(offset) => offset,
            None => {
                log::error!("Sector marker not found reading sector!");
                return Err(DiskImageError::SeekError);
            }
        };

        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &mut self.track_pool[ti];

        let mut read_vec = Vec::new();

        let mut sid = chs.s();
        let mut deleted_mark = false;
        let mut crc_error = false;
        let mut wrong_cylinder = false;

        for _s in 0..ct {
            match &mut track.data {
                TrackData::BitStream {
                    data: TrackDataStream::Mfm(mfm_decoder),
                    ..
                } => {
                    read_vec = vec![0; 512];

                    mfm_decoder.seek(SeekFrom::Start((sector_offset >> 1) as u64 + 32));
                    mfm_decoder.read_exact(&mut read_vec);

                    log::trace!("read_sector(): Found sector_id: {} at offset: {}", sid, sector_offset);
                }
                TrackData::ByteStream { sectors, data, .. } => {
                    for si in sectors {
                        if si.sector_id == sid {
                            log::trace!(
                                "read_sector(): Found sector_id: {} at t_idx: {}",
                                si.sector_id,
                                si.t_idx
                            );
                            read_vec.extend(data[si.t_idx..std::cmp::min(si.t_idx + len, data.len())].to_vec());

                            if si.crc_error {
                                crc_error = true;
                            }

                            if si.cylinder_id != chs.c() {
                                wrong_cylinder = true;
                            }

                            if si.deleted_mark {
                                deleted_mark = true;
                            }
                        }
                    }
                }
                _ => {
                    return Err(DiskImageError::UnsupportedFormat);
                }
            }
            sid += 1;
        }

        Ok(ReadSectorResult {
            deleted_mark,
            crc_error,
            wrong_cylinder,
            data: read_vec,
        })
    }

    pub fn is_id_valid(&self, chs: DiskChs) -> bool {
        if chs.h() >= 2 || chs.c() as usize >= self.track_map[chs.h() as usize].len() {
            return false;
        }
        let ti = self.track_map[chs.h() as usize][chs.c() as usize];
        let track = &self.track_pool[ti];

        match &track.data {
            TrackData::BitStream { .. } => {
                return false;
            }
            TrackData::ByteStream { sectors, .. } => {
                for si in sectors {
                    if si.sector_id == chs.s() {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn dump_info<W: crate::io::Write>(&mut self, mut out: W) {
        out.write_fmt(format_args!("Disk Format: {:?}\n", self.disk_format))
            .unwrap();
        out.write_fmt(format_args!("Geometry: {}\n", self.image_format.geometry))
            .unwrap();
        out.write_fmt(format_args!("Volume Name: {:?}\n", self.volume_name))
            .unwrap();

        if let Some(comment) = &self.comment {
            out.write_fmt(format_args!("Comment: {:?}\n", comment)).unwrap();
        }

        out.write_fmt(format_args!("Data Rate: {}\n", self.image_format.data_rate))
            .unwrap();
        out.write_fmt(format_args!("Data Encoding: {}\n", self.image_format.data_encoding))
            .unwrap();
    }

    pub fn dump_sector_hex<W: crate::io::Write>(
        &mut self,
        chs: DiskChs,
        bytes_per_row: usize,
        mut out: W,
    ) -> Result<(), DiskImageError> {
        let si = self.read_sector(chs, 1, 512)?;

        let rows = si.data.len() / bytes_per_row;
        let last_row_size = si.data.len() % bytes_per_row;

        for r in 0..rows {
            out.write_fmt(format_args!("{:04X}| ", r * bytes_per_row)).unwrap();
            for b in 0..bytes_per_row {
                out.write_fmt(format_args!("{:02X} ", si.data[r * bytes_per_row + b]))
                    .unwrap();
            }
            out.write_fmt(format_args!("\n")).unwrap();
        }

        Ok(())
    }
}
