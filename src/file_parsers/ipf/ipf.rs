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

use crate::{
    file_parsers::{
        bitstream_flags,
        ipf::{
            chunk::{IpfChunk, IpfChunkType},
            data_block::BlockDescriptor,
            data_record::DataRecord,
            image_record::ImageRecord,
            info_record::{EncoderType, InfoRecord},
        },
        reader_len,
        FormatCaps,
        ParserReadOptions,
        ParserWriteOptions,
    },
    io::{ReadSeek, ReadWriteSeek},
    source_map::MapDump,
    types::{DiskCh, DiskDescriptor, Platform, TrackDataEncoding, TrackDataRate, TrackDensity},
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashMap,
    LoadingCallback,
    ParserWriteCompatibility,
};
use binrw::BinRead;

pub(crate) struct DataRecordInfo {
    pub(crate) data_record: DataRecord,
    pub(crate) edb_offset: u64,
    pub(crate) blocks: Vec<BlockDescriptor>,
}

pub struct IpfParser {}

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
        // Pass a data limit of 0 so we don't end up reading a huge chunk from an invalid file.
        if let Ok(file_header) = IpfChunk::read_args(&mut image, (0,)) {
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
        disk_image.set_source_format(DiskImageFileFormat::IpfImage);

        // Request a source map, if options specified.
        //let null = !opts.flags.contains(ReadFlags::CREATE_SOURCE_MAP);
        disk_image.assign_source_map(true);

        // Create a new parser instance with the source map.
        // let mut parser = IpfParser {
        //     platforms: Self::platforms(),
        // };

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
                    info_record.write_to_map(disk_image.source_map_mut(), 0);
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

        let platforms = info_record.platforms();

        if platforms.is_empty() {
            log::warn!("IPF image is not for any compatible platform.");
            //return Err(DiskImageError::IncompatibleImage("IPF image is not for any compatible platform.".to_string()));
        }

        for pi in sorted_pool.iter() {
            Self::process_track(&mut reader, disk_image, &info_record, &image_pool[*pi], &data_pool[*pi])?;
        }

        let desc = DiskDescriptor {
            platforms: (!platforms.is_empty()).then_some(platforms),
            geometry: DiskCh::new((info_record.max_track + 1) as u16, (info_record.max_side + 1) as u8),
            data_encoding: TrackDataEncoding::Mfm,
            density: TrackDensity::Double,
            data_rate: TrackDataRate::from(TrackDensity::Double),
            rpm: None,
            write_protect: None,
        };

        log::debug!("Source Map:");
        log::debug!("\n{:?}", disk_image.source_map());

        disk_image.descriptor = desc;
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
        let image_node = image_record.write_to_map(image.source_map_mut(), 0);
        let data_node = data.data_record.write_to_map(image.source_map_mut(), image_node);

        if let Some(encoder) = info_record.encoder_type_enum {
            match encoder {
                EncoderType::V1 => {
                    Self::decode_v1_track(reader, image, info_record, image_record, data_node, data)?;
                }
                EncoderType::V2 => {
                    Self::decode_v2_track(reader, image, info_record, image_record, data_node, data)?;
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

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        _image: &DiskImage,
        _opts: &ParserWriteOptions,
        _output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
