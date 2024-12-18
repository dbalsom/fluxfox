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
use crate::types::Platform;
use binrw::binrw;
use core::fmt::{self, Debug, Formatter};

/// An IPF Media Type.  Currently only floppy disks are defined (?) at least
/// as of the time of the writing of Jean Louis-Guerin's IPF documentation.
#[binrw]
#[brw(repr = u32)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MediaType {
    Unknown = 0,
    FloppyDisk = 1,
}

impl TryFrom<u32> for MediaType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MediaType::Unknown),
            1 => Ok(MediaType::FloppyDisk),
            _ => Err(()),
        }
    }
}

#[binrw]
#[brw(repr = u32)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EncoderType {
    Unknown = 0,
    Caps = 1,
    Sps = 2,
}

impl TryFrom<u32> for EncoderType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EncoderType::Unknown),
            1 => Ok(EncoderType::Caps),
            2 => Ok(EncoderType::Sps),
            _ => Err(()),
        }
    }
}

/// Supported IPF platforms. Is this list complete as of 2025? Who knows!
#[derive(Copy, Clone, PartialEq)]
pub enum IpfPlatform {
    None,
    Amiga,
    AtariSt,
    Pc,
    AmstradCpc,
    Spectrum,
    SamCoupe,
    Archimedes,
    C64,
    Atari8Bit,
}

/// Convert an [IpfPlatform] to a fluxfox [Platform]
/// Due to a lack of a `Platform::None` variant, this function returns an `Option<Platform>`
/// if successful, with `None` indicating `IpfPlatform::None`.
/// The IPF platform list typically pads the platform table to 4 entries, using
/// IpfPlatform::None.
impl TryFrom<IpfPlatform> for Option<Platform> {
    type Error = ();

    fn try_from(value: IpfPlatform) -> Result<Option<Platform>, Self::Error> {
        match value {
            IpfPlatform::None => Ok(None),
            IpfPlatform::Amiga => Ok(Some(Platform::Amiga)),
            IpfPlatform::AtariSt => Ok(Some(Platform::AtariSt)),
            IpfPlatform::Pc => Ok(Some(Platform::IbmPc)),
            IpfPlatform::AmstradCpc => Err(()),
            IpfPlatform::Spectrum => Err(()),
            IpfPlatform::SamCoupe => Err(()),
            IpfPlatform::Archimedes => Err(()),
            IpfPlatform::C64 => Err(()),
            IpfPlatform::Atari8Bit => Err(()),
        }
    }
}

impl Debug for IpfPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = match self {
            IpfPlatform::None => "None",
            IpfPlatform::Amiga => "Amiga",
            IpfPlatform::AtariSt => "AtariSt",
            IpfPlatform::Pc => "Pc",
            IpfPlatform::AmstradCpc => "AmstradCpc",
            IpfPlatform::Spectrum => "Spectrum",
            IpfPlatform::SamCoupe => "SamCoupe",
            IpfPlatform::Archimedes => "Archimedes",
            IpfPlatform::C64 => "C64",
            IpfPlatform::Atari8Bit => "Atari8Bit",
        };
        write!(f, "{}", name)
    }
}

impl TryFrom<u32> for IpfPlatform {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(IpfPlatform::None),
            1 => Ok(IpfPlatform::Amiga),
            2 => Ok(IpfPlatform::AtariSt),
            3 => Ok(IpfPlatform::Pc),
            4 => Ok(IpfPlatform::AmstradCpc),
            5 => Ok(IpfPlatform::Spectrum),
            6 => Ok(IpfPlatform::SamCoupe),
            7 => Ok(IpfPlatform::Archimedes),
            8 => Ok(IpfPlatform::C64),
            9 => Ok(IpfPlatform::Atari8Bit),
            _ => Err(()),
        }
    }
}

#[binrw]
#[brw(big)]
pub struct InfoRecord {
    pub(crate) media_type: u32, // Media type of the imaged media
    #[bw(ignore)]
    #[br(calc = MediaType::try_from(media_type).ok())]
    pub(crate) media_type_enum: Option<MediaType>, // Media type of the imaged media, parsed to MediaType
    pub(crate) encoder_type: u32, // Image encoder type (raw)
    #[bw(ignore)]
    #[br(calc = EncoderType::try_from(encoder_type).ok())]
    pub(crate) encoder_type_enum: Option<EncoderType>, // Image encoder type, parsed to EncoderType
    pub(crate) encoder_rev: u32, // Image encoder revision
    pub(crate) file_key: u32,   // Unique file key ID (for database purposes)
    pub(crate) file_rev: u32,   // Revision of the file. If there are no known revisions, revision should be 1.
    pub(crate) origin: u32,     // CRC32 value of the original .ctr file (no idea what that is)
    pub(crate) min_track: u32,  // Lowest track number (usually 0)
    pub(crate) max_track: u32,  // Highest track number (usually 83)
    pub(crate) min_side: u32,   // Lowest side (head) number - should be 0
    pub(crate) max_side: u32,   // Highest side (head) number - should be 1
    pub(crate) creation_date: u32, // Creation date (year, month, day) encoded
    pub(crate) creation_time: u32, // Creation time (hour, minute, second, tick) encoded
    pub(crate) platforms: [u32; 4], // Intended platforms. Up to four platforms per disk (to support multi-format disks)
    #[bw(ignore)]
    #[br(calc = platforms.iter().filter_map(|p| IpfPlatform::try_from(*p).ok()).collect())]
    pub(crate) platform_enums: Vec<IpfPlatform>, // Intended platforms. May contain fewer than 4 Platforms if conversion fails.
    pub(crate) disk_number: u32,   // Disk number in a multi-disc release
    pub(crate) creator_id: u32,    // Unique ID of the disk image creator
    pub(crate) reserved: [u8; 12], // Reserve for future use
}

impl Debug for InfoRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("InfoRecord")
            .field(
                "media_type_enum",
                &self
                    .media_type_enum
                    .as_ref()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|| "Unknown".to_string()),
            )
            .field(
                "encoder_type_enum",
                &self
                    .encoder_type_enum
                    .as_ref()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|| "Unknown".to_string()),
            )
            .field("encoder_rev", &self.encoder_rev)
            .field("file_key", &format!("{:08X}", self.file_key))
            .field("file_rev", &self.file_rev)
            .field("origin", &format!("{:08X}", self.origin))
            .field("min_track", &self.min_track)
            .field("max_track", &self.max_track)
            .field("min_side", &self.min_side)
            .field("max_side", &self.max_side)
            .field("creation_date", &format!("{:08X}", self.creation_date))
            .field("creation_time", &format!("{:08X}", self.creation_time))
            .field(
                "platform_enums",
                &self
                    .platform_enums
                    .iter()
                    .map(|platform| format!("{:?}", *platform)) // Convert each platform to a string
                    .collect::<Vec<_>>(),
            )
            .field("disk_number", &self.disk_number)
            .field("creator_id", &format!("{:08X}", self.creator_id))
            .finish()
    }
}
