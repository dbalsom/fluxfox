/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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
use crate::{
    file_parsers::ipf::{crc::IpfCrcHasher, ipf::IpfParser},
    io::ReadSeek,
    DiskImageError,
};
use binrw::{binrw, BinRead};
use core::fmt;
use std::io::Cursor;

const CHUNK_DEFS: [&[u8; 4]; 8] = [b"CAPS", b"DUMP", b"DATA", b"TRCK", b"INFO", b"IMGE", b"CTEI", b"CTEX"];
pub const MAXIMUM_CHUNK_SIZE: usize = 0x100000; // Set some reasonable limit for chunk sizes. Here 1MB.

#[binrw]
#[brw(big)]
#[br(import(data_size_limit: Option<u32>))]
pub(crate) struct IpfChunk {
    pub id: [u8; 4],
    #[bw(ignore)]
    #[br(calc = <IpfChunkType>::try_from(&id).ok())]
    pub chunk_type: Option<IpfChunkType>,
    pub size: u32,
    pub crc: u32,
    #[br(count = {
        let chunk_size = size.saturating_sub(12);
        if let Some(limit) = data_size_limit {
            chunk_size.min(limit)
        }
        else {
            chunk_size
        }
    })]
    pub data: Vec<u8>,
    #[bw(ignore)]
    #[br(calc = IpfChunk::calculate_crc(&id, size, &data))] // Calculate the CRC based on fields
    pub calculated_crc: u32, // Computed CRC value
}

impl fmt::Debug for IpfChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IpfChunk")
            .field("id", &format!("{:02X?}", &self.id)) // Show the raw chunk ID bytes
            .field(
                "chunk_type",
                &self
                    .chunk_type
                    .as_ref()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|| "Unknown".to_string()),
            ) // Show the chunk type enum or "Unknown"
            .field("size", &self.size) // Show the size of the chunk
            .field("crc", &format!("{:08X}", self.crc)) // Show the CRC in hex format
            .field("data_length", &self.data.len()) // Show the length of the data
            .field("calculated_crc", &format!("{:08X}", self.calculated_crc)) // Show the calculated CRC in hex format
            .finish()
    }
}

impl IpfChunk {
    fn calculate_crc(id: &[u8; 4], size: u32, data: &[u8]) -> u32 {
        let mut hasher = IpfCrcHasher::new();
        hasher.update(id);
        hasher.update(&size.to_be_bytes());
        // When calculating chunk CRC, we treat the CRC field as if zeroed
        hasher.update(&[0; 4]);
        hasher.update(data);
        hasher.finalize()
    }

    fn is_crc_valid(&self) -> bool {
        if self.crc == self.calculated_crc {
            true
        }
        else {
            log::warn!(
                "IpfChunk::is_crc_valid(): CRC mismatch: {:08X} != {:08X}",
                self.crc,
                self.calculated_crc
            );
            false
        }
    }

    pub(crate) fn into_inner<T>(self) -> Result<T, DiskImageError>
    where
        for<'a> T: binrw::BinRead<Args<'a> = ()> + binrw::meta::ReadEndian,
    {
        let mut cursor = Cursor::new(self.data);
        let inner = T::read(&mut cursor).map_err(|e| DiskImageError::ImageCorruptError(e.to_string()))?;
        Ok(inner)
    }

    #[allow(dead_code)]
    pub(crate) fn into_inner_args<T, Args>(self, args: Args) -> Result<T, DiskImageError>
    where
        T: for<'a> BinRead<Args<'a> = Args> + binrw::meta::ReadEndian,
    {
        let mut cursor = Cursor::new(self.data);
        let inner = T::read_args(&mut cursor, args).map_err(|e| DiskImageError::ImageCorruptError(e.to_string()))?;
        Ok(inner)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum IpfChunkType {
    Caps = 0,
    Dump = 1,
    Data = 2,
    Track = 3,
    Info = 4,
    Image = 5,
    Ctei = 6,
    Ctex = 7,
}

// impl Debug for IpfChunkType {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         let chunk_bytes = match self {
//             IpfChunkType::Caps => CHUNK_DEFS[0],
//             IpfChunkType::Dump => CHUNK_DEFS[1],
//             IpfChunkType::Data => CHUNK_DEFS[2],
//             IpfChunkType::Track => CHUNK_DEFS[3],
//             IpfChunkType::Info => CHUNK_DEFS[4],
//             IpfChunkType::Image => CHUNK_DEFS[5],
//             IpfChunkType::Ctei => CHUNK_DEFS[6],
//             IpfChunkType::Ctex => CHUNK_DEFS[7],
//         };
//         write!(f, "{:04X?}", chunk_bytes)
//     }
// }

impl TryFrom<&[u8; 4]> for IpfChunkType {
    type Error = ();

    fn try_from(value: &[u8; 4]) -> Result<Self, Self::Error> {
        match value {
            b"CAPS" => Ok(IpfChunkType::Caps),
            b"DUMP" => Ok(IpfChunkType::Dump),
            b"DATA" => Ok(IpfChunkType::Data),
            b"TRCK" => Ok(IpfChunkType::Track),
            b"INFO" => Ok(IpfChunkType::Info),
            b"IMGE" => Ok(IpfChunkType::Image),
            b"CTEI" => Ok(IpfChunkType::Ctei),
            b"CTEX" => Ok(IpfChunkType::Ctex),
            _ => Err(()),
        }
    }
}

impl From<IpfChunkType> for &[u8; 4] {
    fn from(val: IpfChunkType) -> Self {
        &CHUNK_DEFS[val as usize]
    }
}

impl IpfParser {
    pub(crate) fn read_chunk<RWS: ReadSeek>(image: &mut RWS) -> Result<IpfChunk, DiskImageError> {
        //let chunk_pos = image.stream_position()?;

        // Read the chunk header with no data size limit (None parameter)
        let chunk = IpfChunk::read_args(image, (None,))?;
        //log::debug!("Read chunk: {:?}", chunk);

        if chunk.chunk_type.is_none() {
            log::error!("read_chunk(): Unknown chunk type: {:0X?}", chunk.id);
            log::warn!("Unknown chunk type: {:0X?}", chunk.id);
        }

        if chunk.size > MAXIMUM_CHUNK_SIZE as u32 {
            log::error!(
                "read_chunk(): Chunk length exceeds limit: {} > {}",
                chunk.size,
                MAXIMUM_CHUNK_SIZE
            );
            return Err(DiskImageError::IncompatibleImage(format!(
                "Chunk length exceeds limit: {} > {}",
                chunk.size, MAXIMUM_CHUNK_SIZE,
            )));
        }

        if !chunk.is_crc_valid() {
            log::error!("read_chunk(): CRC mismatch in {:?} chunk", chunk.chunk_type);
            return Err(DiskImageError::ImageCorruptError(format!(
                "CRC mismatch in {:?} chunk",
                chunk
                    .chunk_type
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|| "Unknown".to_string()),
            )));
        }

        Ok(chunk)
    }
}
