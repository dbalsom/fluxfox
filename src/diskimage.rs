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
use crate::detect::detect_image_format;
use crate::io::ReadSeek;
use crate::parsers::ImageParser;
use crate::{DiskDataEncoding, DiskDataRate, DiskImageError, DiskRpm, DEFAULT_SECTOR_SIZE};
use std::fmt::Display;

/// An enumeration describing the type of disk image.
#[derive(Copy, Clone, Debug)]
pub enum DiskImageFormat {
    RawSectorImage,
    ImageDisk,
    TeleDisk,
    KryofluxStream,
}

impl Display for DiskImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DiskImageFormat::RawSectorImage => "Raw Sector Image".to_string(),
            DiskImageFormat::ImageDisk => "ImageDisk".to_string(),
            DiskImageFormat::TeleDisk => "TeleDisk".to_string(),
            DiskImageFormat::KryofluxStream => "Kryoflux Stream".to_string(),
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

    pub fn get_image_format(&self) -> ImageFormat {
        ImageFormat {
            geometry: self.get_chs(),
            default_sector_size: DEFAULT_SECTOR_SIZE,
            data_encoding: DiskDataEncoding::Mfm,
            data_rate: DiskDataRate::Rate500Kbps,
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

/// A sector definition maintains an index into continuous track data.
/// This permits overlapping sectors, and reading beyond sector boundaries.
pub struct DiskSector {
    // The sector id written to the sector. Sector IDs may not be sequential (e.g. sector interleaving)
    pub id: u8,
    // The cylinder id written to the sector. This may not match the actual physical cylinder
    // the sector resides in.
    pub cylinder_id: u8,
    // The head id written to the sector. This may not match the actual physical head.
    pub head_id: u8,
    /// The physical location of the sector on the disk as CHS.
    pub chs: DiskChs,
    /// The physical length of the sector.
    pub len: usize,
    /// The sector's index into the track data vector.
    pub t_idx: usize,
}

/// Per-track format settings. In most cases, this will not change per-track. Some formats encode
/// this per-track, so we store it here.
pub struct TrackFormat {
    pub data_encoding: DiskDataEncoding,
    pub data_rate: DiskDataRate,
}

/// TrackData stores the raw data for a track.
/// A weak bit mask is stored along with the data.
pub struct TrackData {
    pub cylinder: u8,
    pub head: u8,
    pub sectors: Vec<TrackSectorIndex>,
    pub data: Vec<u8>,
    pub weak_mask: Vec<u8>,
}

pub struct TrackSectorIndex {
    pub sector_id: u8,
    pub cylinder_id: u8,
    pub head_id: u8,
    pub t_idx: usize,
    pub len: usize,
}

impl DiskSector {}

/// A Disk Track is a circular region of the disk surface in which a number of sectors are stored.
/// Certain disk operations can be performed on an entire track, such as reading and formatting.
pub struct DiskTrack {
    /// A track comprises a vector of indices into the DiskImage sector pool.
    pub format: TrackFormat,
    pub data: TrackData,
}

#[derive(Copy, Clone, Default)]
pub struct ImageFormat {
    /// The basic geometry of the disk.
    pub geometry: DiskChs,
    /// The "default" sector size of the disk. Larger or smaller sectors may be present in the disk image.
    pub default_sector_size: usize,
    /// The data encoding used
    pub data_encoding: DiskDataEncoding,
    /// The data rate of the disk
    pub data_rate: DiskDataRate,
}

/// A DiskImage represents an image of a floppy disk in memory. It comprises a pool of sectors, and an ordered
/// list of tracks that reference sectors in the pool.
/// Sectors may be variable length due to various copy protection schemes.
pub struct DiskImage {
    pub disk_format: FloppyFormat,
    pub image_format: ImageFormat,
    pub consistency: DiskConsistency,
    pub sector_size: usize,
    // The volume name of the disk image, if any.
    pub volume_name: Option<String>,
    // An ASCII comment embedded in the disk image, if any.
    pub comment: Option<String>,
    /// An array of track vectors. The containing array represents the number of heads (maximum of 2)
    /// and the vectors represent the tracks on the disk.
    pub tracks: [Vec<DiskTrack>; 2],
}

impl Default for DiskImage {
    fn default() -> Self {
        Self {
            disk_format: FloppyFormat::PcFloppy360,
            image_format: ImageFormat::default(),
            consistency: Default::default(),
            sector_size: DEFAULT_SECTOR_SIZE,
            volume_name: None,
            comment: None,
            tracks: [Vec::new(), Vec::new()],
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
            tracks: [Vec::new(), Vec::new()],
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

    pub fn set_image_format(&mut self, format: ImageFormat) {
        self.image_format = format;
    }

    pub fn image_format(&self) -> ImageFormat {
        self.image_format
    }

    pub fn add_track(&mut self, format: TrackFormat, ch: DiskCh) {
        assert!(ch.h < 2);
        self.tracks[ch.h as usize].push(DiskTrack {
            format,
            data: TrackData {
                cylinder: ch.c,
                head: ch.h,
                sectors: Vec::new(),
                data: Vec::new(),
                weak_mask: Vec::new(),
            },
        });
    }

    pub fn write_sector(
        &mut self,
        chs: DiskChs,
        id: u8,
        cylinder_id: Option<u8>,
        head_id: Option<u8>,
        data: &[u8],
        weak: Option<&[u8]>,
    ) -> Result<(), DiskImageError> {
        if chs.h() >= 2 || self.tracks[chs.h() as usize].len() < chs.c() as usize {
            return Err(DiskImageError::SeekError);
        }

        // Create an empty weak bit mask if none is provided.
        let weak_buf_vec = match weak {
            Some(weak_buf) => weak_buf.to_vec(),
            None => vec![0; data.len()],
        };

        let track = &mut self.tracks[chs.h() as usize][chs.c() as usize];

        track.data.sectors.push(TrackSectorIndex {
            sector_id: id,
            cylinder_id: cylinder_id.unwrap_or(chs.c()),
            head_id: head_id.unwrap_or(chs.h()),
            t_idx: track.data.data.len(),
            len: data.len(),
        });
        track.data.data.extend(data);
        track.data.weak_mask.extend(weak_buf_vec);

        Ok(())
    }

    /// Read the specified 'len' bytes from the disk image starting at the sector mark given by 'chs'.
    pub fn read_sector(&self, chs: DiskChs, len: usize) -> Result<Vec<u8>, DiskImageError> {
        if chs.h() >= 2 || chs.c() as usize >= self.tracks[chs.h() as usize].len() {
            return Err(DiskImageError::SeekError);
        }
        let track = &self.tracks[chs.h() as usize][chs.c() as usize];
        for s in &track.data.sectors {
            if s.sector_id == chs.s() {
                return Ok(track.data.data[s.t_idx..std::cmp::min(s.t_idx + len, track.data.data.len())].to_vec());
            }
        }
        Err(DiskImageError::SeekError)
    }

    pub fn dump_info<W: crate::io::Write>(&self, mut out: W) {
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
}
