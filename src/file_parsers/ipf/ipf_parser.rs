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

//! A format parser for the Interchangeable Preservation Format (IPF).
//! IPF is a format devised by the Software Preservation Society for the
//! preservation of magnetic media. It is a complex format that includes
//! a variety of metadata and data structures to support the preservation
//! of a wide variety of disk formats.
//!
//! This parser is a work in progress and is not yet complete.
//!
//! References used, MAME's IPF parser (BSD licensed)
//! https://github.com/mamedev/mame/blob/master/src/lib/formats/ipf_dsk.cpp
//!
//! IPF documentation by Jean Louis-Guerin
//! https://www.kryoflux.com/download/ipf_documentation_v1.6.pdf

use binrw::BinRead;

use crate::{
    file_parsers::{
        bitstream_flags,
        ipf::{
            chunk::{IpfChunk, IpfChunkType},
            data_record::{BlockDescriptor, BlockFlags, DataRecord},
            image_record::ImageRecord,
            info_record::{EncoderType, InfoRecord},
            stream_element::{DataSample, DataStreamElement, DataType},
        },
        reader_len,
        FormatCaps,
        ParserReadOptions,
        ParserWriteOptions,
    },
    io::{ReadSeek, ReadWriteSeek},
    track_schema::TrackElementInstance,
    types::Platform,
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashMap,
    LoadingCallback,
    ParserWriteCompatibility,
};

struct DataRecordInfo {
    pub data_record: DataRecord,
    pub edb_offset: u64,
    pub blocks: Vec<BlockDescriptor>,
}

pub struct IpfParser {
    /// Detected and compatible fluxfox Platforms this IPF contains.
    /// An IPF may contain multiple platforms - consider dual and triple format disks were made
    /// (Amiga/Atari ST/PC)
    platforms: Vec<Platform>,
}

impl IpfParser {
    #[allow(dead_code)]
    fn format() -> DiskImageFileFormat {
        DiskImageFileFormat::PceBitstreamImage
    }

    pub(crate) fn capabilities() -> FormatCaps {
        bitstream_flags() | FormatCaps::CAP_COMMENT | FormatCaps::CAP_WEAK_BITS
    }

    pub fn platforms() -> Vec<Platform> {
        // IPF images should really support anything (i think), but I'm only aware of
        // IPF collections for Amiga and AtariST, only one of which we support.
        vec![Platform::Amiga]
    }

    pub(crate) fn extensions() -> Vec<&'static str> {
        vec!["ipf"]
    }

    pub(crate) fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        let mut detected = false;
        _ = image.seek(std::io::SeekFrom::Start(0));

        // The first chunk in an IPF file must be the CAPS chunk
        if let Ok(file_header) = IpfChunk::read(&mut image) {
            if file_header.chunk_type == Some(IpfChunkType::Caps) {
                detected = true;
            }
        }

        detected
    }

    /// Return the compatibility of the image with the parser.
    /// Currently, writing to IPF is not supported. It is unlikely it ever will be implemented,
    /// to avoid controversy.
    pub(crate) fn can_write(_image: Option<&DiskImage>) -> ParserWriteCompatibility {
        ParserWriteCompatibility::Incompatible
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut reader: RWS,
        disk_image: &mut DiskImage,
        _opts: &ParserReadOptions,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_source_format(DiskImageFileFormat::PceBitstreamImage);

        // Get length of reader
        let image_len = reader_len(&mut reader)?;

        // Seek to start of read_buf.
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Read the first chunk (CAPS chunk)
        let header = Self::read_chunk(&mut reader)?;

        // First chunk must be CAPS header.
        if header.chunk_type != Some(IpfChunkType::Caps) {
            return Err(DiskImageError::UnknownFormat);
        }

        log::debug!("Parsed CAPS chunk: {:#?}", header);

        let mut encoder_type = 0u32;

        // IPF ImageRecords all define a key that is referenced by DataRecords -
        // when we encounter a DataRecord, we must resolve the corresponding ImageRecord
        // to know how many BlockDescriptors to expect.

        // It appears that the ImageRecord key is just the index, but the scheme allows for it
        // not to be, so we'll take the cautious approach and store a hash map of indexes into
        // the pool of collected ImageRecords.
        let mut image_pool: Vec<ImageRecord> = Vec::with_capacity(200);
        let mut image_map: FoxHashMap<u32, usize> = FoxHashMap::with_capacity(200);
        let mut data_pool: Vec<DataRecordInfo> = Vec::new();

        let mut info_record_opt: Option<InfoRecord> = None;

        while let Ok(chunk) = Self::read_chunk(&mut reader) {
            match chunk.chunk_type {
                Some(IpfChunkType::Info) => {
                    let info_record: InfoRecord = chunk.into_inner::<InfoRecord>()?;
                    log::debug!("InfoRecord: {:#?}", info_record);
                    log::debug!(
                        "Setting encoder_type to {} ({:?})",
                        info_record.encoder_type,
                        info_record.encoder_type_enum
                    );
                    encoder_type = info_record.encoder_type;
                    info_record_opt = Some(info_record);
                }
                Some(IpfChunkType::Image) => {
                    let image_record: ImageRecord = chunk.into_inner()?;
                    //log::debug!("ImageRecord: {:?}", image_record);
                    log::debug!("Hashing ImageRecord with key {}", image_record.key());
                    image_map.insert(image_record.key(), image_pool.len());
                    image_pool.push(image_record);
                }
                Some(IpfChunkType::Data) => {
                    let data_record: DataRecord = chunk.into_inner()?;
                    log::trace!("Parsed DataRecord: {:#?}", data_record);

                    log::debug!("DataRecord has ImageRecord key of {}", data_record.key());

                    // Resolve the ImageRecord via map -> pool index -> image_pool chain
                    let image_record = image_map
                        .get(&data_record.key())
                        .and_then(|&index| image_pool.get(index))
                        .ok_or_else(|| {
                            log::error!("No ImageRecord found for DataRecord with key {}.", data_record.key());
                            DiskImageError::ImageCorruptError(format!(
                                "No ImageRecord found for DataRecord with key {}.",
                                data_record.key()
                            ))
                        })?;

                    // Extra Data Block begins here, with an array of BlockDescriptors.
                    // Save the stream position at the start of the EDB so we can calculate where
                    // the next Data Record begins.
                    let edb_offset = reader.stream_position()?;

                    let mut blocks = Vec::with_capacity(20);
                    for _ in 0..image_record.block_count {
                        let block_descriptor = BlockDescriptor::read_args(&mut reader, (encoder_type,))?;
                        log::trace!("Parsed BlockDescriptor: {:#?}", block_descriptor);
                        blocks.push(block_descriptor);
                    }

                    let bytes_left = image_len - reader.stream_position()?;
                    let edb_len = data_record.length;

                    log::debug!(
                        "DataRecord reports EDB length of {} bytes and a CRC of {:08X}, {} bytes left in stream.",
                        edb_len,
                        data_record.crc,
                        bytes_left
                    );
                    data_pool.push(DataRecordInfo {
                        data_record,
                        edb_offset,
                        blocks,
                    });

                    // Calculate address of next DataRecord
                    let next_data_record = edb_offset + edb_len as u64;

                    // Address cannot be greater than the length of the image.
                    if next_data_record > image_len {
                        log::error!("Next DataRecord address exceeds image length.");
                        return Err(DiskImageError::ImageCorruptError(
                            "A DataRecord offset exceeded image length.".to_string(),
                        ));
                    }

                    // Seek to the next DataRecord. Hope we find one!
                    reader.seek(std::io::SeekFrom::Start(next_data_record))?;
                }
                _ => {
                    println!("Unknown chunk type: {:?}", chunk.chunk_type);
                }
            }
        }

        let bytes_left = image_len - reader.stream_position()?;
        println!("Last chunk read with {} bytes left in stream.", bytes_left);

        let mut sorted_pool: Vec<usize> = (0..image_pool.len()).collect();
        // Sort ImageRecord indices by physical track.
        sorted_pool.sort_by(|&a, &b| image_pool[a].cmp(&image_pool[b]));

        let info_record = info_record_opt.ok_or_else(|| {
            log::error!("No InfoRecord found in IPF image.");
            DiskImageError::ImageCorruptError("No InfoRecord found in IPF image.".to_string())
        })?;

        for pi in sorted_pool.iter() {
            Self::process_track(&mut reader, disk_image, &info_record, &image_pool[*pi], &data_pool[*pi])?;
        }

        Ok(())
    }

    fn process_track<RWS>(
        reader: &mut RWS,
        image: &mut DiskImage,
        info_record: &InfoRecord,
        image_record: &ImageRecord,
        data: &DataRecordInfo,
    ) -> Result<(), DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!(
            "process_track(): Track {} bitct: {:6} block_ct: {:02} data_bits: {}",
            image_record.ch(),
            image_record.track_bits,
            image_record.block_count,
            image_record.data_bits,
        );

        let encoder = if let Some(encoder) = info_record.encoder_type_enum {
            match encoder {
                EncoderType::Caps => {
                    log::debug!("Processing CAPS-encoded track");
                }
                EncoderType::Sps => {
                    log::debug!("Processing SPS-encoded track.");
                }
                EncoderType::Unknown => {
                    log::error!("Invalid encoder type: {:02X}", info_record.encoder_type);
                    return Err(DiskImageError::ImageCorruptError(format!(
                        "Invalid encoder type: {:02X}",
                        info_record.encoder_type
                    )));
                }
            }
            encoder
        }
        else {
            log::error!("Invalid encoder type: {:02X}", info_record.encoder_type);
            return Err(DiskImageError::ImageCorruptError(format!(
                "Invalid encoder type: {:02X}",
                info_record.encoder_type
            )));
        };

        for (bi, block) in data.blocks.iter().enumerate() {
            log::debug!(
                "Block {}: data offset: {} data: [bytes: {:?} bits: {}], gap: [bytes: {:?} bits: {}]",
                bi,
                data.edb_offset + block.data_offset as u64,
                block.data_bytes,
                block.data_bits,
                block.gap_bytes,
                block.gap_bits
            );

            // reader.seek(std::io::SeekFrom::Start(data.edb_offset + block.data_offset as u64))?;
            //
            // let mut debug_buf = [0; 16];
            // reader.read_exact(&mut debug_buf)?;
            //log::warn!("Data element: {:02X?}", debug_buf);

            // Seek to the first data element
            let data_offset = data.edb_offset + block.data_offset as u64;
            reader.seek(std::io::SeekFrom::Start(data_offset))?;

            match encoder {
                EncoderType::Caps => {
                    let data_bytes = if let Some(bytes) = block.data_bytes {
                        bytes as u64
                    }
                    else {
                        log::error!("CAPS block descriptor missing data_bytes.");
                        return Err(DiskImageError::ImageCorruptError(
                            "CAPS block descriptor missing data_bytes.".to_string(),
                        ));
                    };

                    // CAPS descriptor have valid data_bytes. Ignore block flags!
                    let elements = Self::decode_caps_block(reader, info_record, block)?;
                    let pos = reader.stream_position()?;

                    if pos - data_offset != block.data_bytes.unwrap() as u64 {
                        log::error!(
                            "Reached End element with {} bytes remaining in data block.",
                            data_bytes - (pos - data_offset)
                        );
                        return Err(DiskImageError::ImageCorruptError(
                            "Data element length mismatch.".to_string(),
                        ));
                    }
                }
                EncoderType::Sps => {
                    // SPS block descriptor should have valid block flags!
                    let data_in_bits = if let Some(flags) = &block.block_flags {
                        log::debug!("Block flags: {:?}", flags);
                        flags.contains(BlockFlags::DATA_IN_BITS)
                    }
                    else {
                        log::error!("SPS block descriptor missing block flags.");
                        return Err(DiskImageError::ImageCorruptError(
                            "SPS block descriptor missing block flags.".to_string(),
                        ));
                    };
                    return Err(DiskImageError::IncompatibleImage(format!(
                        "Unimplemented encoder type: {:02X}",
                        info_record.encoder_type
                    )));
                }
                _ => {
                    log::error!("Invalid encoder type: {:02X}", info_record.encoder_type);
                    return Err(DiskImageError::ImageCorruptError(format!(
                        "Invalid encoder type: {:02X}",
                        info_record.encoder_type
                    )));
                }
            }
        }
        // let mut track = Track::new(image_record.ch());
        // let mut track_data = Vec::new();
        //
        // for block in block_descriptors {
        //     let block_data = reader.read_exact(block.length as usize)?;
        //     track_data.push((block.start, block_data));
        // }
        //
        // track.set_data(track_data);
        // image.add_track(track);
        Ok(())
    }

    pub fn decode_caps_block<RWS>(
        reader: &mut RWS,
        info_record: &InfoRecord,
        block: &BlockDescriptor,
    ) -> Result<Vec<TrackElementInstance>, DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("-------------------------- Decoding CAPS Block ----------------------------------");

        let data_bytes = if let Some(bytes) = &block.data_bytes {
            *bytes as usize
        }
        else {
            log::error!("CAPS block descriptor missing data_bytes.");
            return Err(DiskImageError::ImageCorruptError(
                "CAPS block descriptor missing data_bytes.".to_string(),
            ));
        };

        let mut data_element = DataStreamElement::read_args(reader, (false, data_bytes))?;

        let mut element_ct = 0;
        while !data_element.data_head.is_null() {
            if let Some(samples) = &data_element.data_sample {
                match samples {
                    DataSample::Bytes(data) => {
                        log::debug!(
                            "Data element contains: {} bytes: {:02X?}",
                            data.len(),
                            &data[0..std::cmp::min(16, data.len())]
                        );
                    }
                    DataSample::Bits(bits) => {
                        log::debug!("Data element contains: {} bits", bits.len());
                    }
                }
            }
            // Read the next data element
            element_ct += 1;
            data_element = DataStreamElement::read_args(reader, (false, data_bytes))?;
        }

        log::debug!("Read {} data elements from CAPS block.", element_ct);
        Ok(Vec::new())
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        _image: &DiskImage,
        _opts: &ParserWriteOptions,
        _output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
