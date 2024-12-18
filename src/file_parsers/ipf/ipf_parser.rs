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

use crate::{
    bitstream::{
        mfm::{MfmCodec, MFM_BYTE_LEN},
        TrackDataStream,
    },
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
    io::{ReadSeek, ReadWriteSeek, Write},
    track::{bitstream::BitStreamTrack, Track},
    track_schema::{TrackElementInstance, TrackSchema},
    types::{
        BitStreamTrackParams,
        DiskCh,
        DiskDataResolution,
        DiskDescriptor,
        Platform,
        TrackDataEncoding,
        TrackDataRate,
        TrackDensity,
    },
    DiskImage,
    DiskImageError,
    DiskImageFileFormat,
    FoxHashMap,
    LoadingCallback,
    ParserWriteCompatibility,
};
use binrw::BinRead;
use bit_vec::BitVec;

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

        let desc = DiskDescriptor {
            geometry: DiskCh::new((info_record.max_track + 1) as u16, (info_record.max_side + 1) as u8),
            data_encoding: TrackDataEncoding::Mfm,
            density: TrackDensity::Double,
            data_rate: TrackDataRate::from(TrackDensity::Double),
            rpm: None,
            write_protect: None,
        };

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
        if let Some(encoder) = info_record.encoder_type_enum {
            match encoder {
                EncoderType::Caps => {
                    Self::decode_caps_amiga_track(reader, image, info_record, image_record, data)?;
                }
                EncoderType::Sps => {
                    Self::decode_sps_track(reader, image, info_record, image_record, data)?;
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

    pub fn decode_caps_amiga_track<RWS>(
        reader: &mut RWS,
        image: &mut DiskImage,
        info_record: &InfoRecord,
        image_record: &ImageRecord,
        data: &DataRecordInfo,
    ) -> Result<(), DiskImageError>
    where
        RWS: ReadSeek,
    {
        image.set_resolution(DiskDataResolution::BitStream);

        log::debug!("-------------------------- Decoding CAPS Track ----------------------------------");
        // log::debug!(
        //     "Track {} bitct: {:6} block_ct: {:02} data_bits: {}",
        //     image_record.ch(),
        //     image_record.track_bits,
        //     image_record.block_count,
        //     image_record.data_bits,
        // );
        log::debug!("Image Record: {:#?}", image_record);

        // Density is *probably* double. Guess from bitcell count or assume double.
        let data_rate =
            TrackDataRate::from(TrackDensity::from_bitcells(image_record.track_bits).unwrap_or(TrackDensity::Double));

        // // Create empty BitVec for track data.
        // let track_bits = BitVec::from_elem(image_record.track_bits as usize, false);
        // // Amiga is *probably* MFM encoded.
        // let codec = Box::new(MfmCodec::new(track_bits, Some(image_record.track_bits as usize), None));

        let start_clock = image_record.start_bit_pos % 2 != 0;

        // There's a variety of approaches here - we could craft a BitStreamTrack in isolation
        // and then attach it to the Disk, or we can add an empty track and then write to it.
        // I'm going to try the latter approach first.
        let new_track_idx = image.add_empty_track(
            image_record.ch(),
            TrackDataEncoding::Mfm,
            data_rate,
            image_record.track_bits as usize,
            Some(true),
        )?;
        let track = image.track_by_idx_mut(new_track_idx).ok_or_else(|| {
            log::error!("Failed to get mutable track for image.");
            DiskImageError::FormatParseError
        })?;

        // let mut bitstream_track = track.as_bitstream_track_mut().ok_or_else(|| {
        //     log::error!("Failed to get mutable bitstream track for image.");
        //     DiskImageError::FormatParseError
        // })?;

        // let params = BitStreamTrackParams {
        //     schema: Some(TrackSchema::Amiga),
        //     ch: image_record.ch(),
        //     encoding: TrackDataEncoding::Mfm,
        //     data_rate,
        //     rpm: None,
        //     bitcell_ct: Some(image_record.track_bits as usize),
        //     data: &[],
        //     weak: None,
        //     hole: None,
        //     detect_weak: false,
        // };
        //
        // let mut track = BitStreamTrack::new_optional_ctx(&params, None)?;
        {
            // Seek to the start position for the first block.
            let bitstream = match track.stream_mut() {
                Some(stream) => stream,
                None => {
                    log::error!("Failed to get mutable stream for track.");
                    return Err(DiskImageError::FormatParseError);
                }
            };

            log::debug!("Seeking to {} for first block.", image_record.start_bit_pos & !0xF);
            let mut cursor = image_record.start_bit_pos as usize & !0xF;
            //bitstream.seek(std::io::SeekFrom::Start(image_record.start_bit_pos as u64))?;

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
                let encoded_bytes = Self::decode_caps_block(reader, info_record, block, bitstream, &mut cursor)?;

                if encoded_bytes != data_bytes as usize {
                    log::warn!(
                        "Block {} decoded {} bytes, but expected {} bytes.",
                        bi,
                        encoded_bytes,
                        data_bytes
                    );
                }

                // As far as I can tell there's no field that gives the un-decoded length of the data elements.

                // let pos = reader.stream_position()?;
                // if pos - data_offset != block.data_bytes.unwrap() as u64 {
                //     log::error!(
                //         "Reached End element with {} bytes remaining in data block.",
                //         data_bytes - (pos - data_offset)
                //     );
                //     return Err(DiskImageError::ImageCorruptError(
                //         "Data element length mismatch.".to_string(),
                //     ));
                // }
            }
        }

        let track = image.track_by_idx_mut(new_track_idx).ok_or_else(|| {
            log::error!("Failed to get mutable track for image.");
            DiskImageError::FormatParseError
        })?;

        let mut bitstream_track = track.as_bitstream_track_mut().ok_or_else(|| {
            log::error!("Failed to get mutable bitstream track for image.");
            DiskImageError::FormatParseError
        })?;

        bitstream_track.set_schema(TrackSchema::Amiga);

        bitstream_track.rescan()?;

        Ok(())
    }

    pub fn decode_sps_track<RWS>(
        reader: &mut RWS,
        image: &mut DiskImage,
        info_record: &InfoRecord,
        image_record: &ImageRecord,
        data: &DataRecordInfo,
    ) -> Result<(), DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("-------------------------- Decoding SPS Track ----------------------------------");
        log::debug!(
            "Track {} bitct: {:6} block_ct: {:02} data_bits: {}",
            image_record.ch(),
            image_record.track_bits,
            image_record.block_count,
            image_record.data_bits,
        );
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
        Ok(())
    }

    pub fn decode_caps_block<RWS>(
        reader: &mut RWS,
        info_record: &InfoRecord,
        block: &BlockDescriptor,
        bitstream: &mut TrackDataStream,
        cursor: &mut usize,
    ) -> Result<usize, DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("-------------------------- Decoding CAPS Block ----------------------------------");
        //log::trace!("Block: {:#?}", block);
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
        let mut decoded_bytes = 0;

        while !data_element.data_head.is_null() {
            let data_type = data_element.data_head.data_type();

            let data = if let Some(samples) = &data_element.data_sample {
                match samples {
                    DataSample::Bytes(data) => {
                        log::debug!(
                            "Data element contains: {} bytes: {:02X?}",
                            data.len(),
                            &data[0..std::cmp::min(16, data.len())]
                        );
                        data
                    }
                    DataSample::Bits(bits) => {
                        // This shouldn't really happen in a CAPS block...
                        log::warn!("Unhandled: Bit samples in CAPS block!");
                        log::debug!("Data element contains: {} bits", bits.len());

                        &bits.to_bytes()
                    }
                }
            }
            else {
                log::error!("Data element has no samples!");
                return Err(DiskImageError::ImageCorruptError(
                    "Data element has no samples.".to_string(),
                ));
            };

            let wrote = match data_type {
                DataType::Sync => {
                    // Write SYNC bytes RAW (they are already MFM-encoded!)
                    log::trace!(
                        "Writing raw Sync bytes: {:02X?}",
                        &data[0..std::cmp::min(16, data.len())]
                    );
                    // Write the raw bytes
                    bitstream.write_raw_buf(data, *cursor);
                    data.len() / 2
                }
                DataType::Data => {
                    // Encode data bytes as MFM
                    log::trace!(
                        "Encoding data element: {:02X?}",
                        &data[0..std::cmp::min(16, data.len())]
                    );
                    bitstream.write_encoded_buf(data, *cursor);
                    data.len()
                }
                DataType::Gap => {
                    // Encode gap bytes as MFM
                    log::trace!("Encoding GAP element: {:02X?}", &data[0..std::cmp::min(16, data.len())]);
                    bitstream.write_encoded_buf(data, *cursor);
                    data.len()
                }
                DataType::End => {
                    // End of data block
                    log::debug!("End of data block.");
                    break;
                }
                _ => {
                    log::warn!("Unknown data element type: {:?}", data_type);
                    data.len()
                }
            };

            decoded_bytes += wrote;
            *cursor += wrote * MFM_BYTE_LEN;

            // Read the next data element
            element_ct += 1;
            data_element = DataStreamElement::read_args(reader, (false, data_bytes))?;
        }

        log::debug!(
            "Read {} data elements from CAPS block, wrote {} MFM bytes to track",
            element_ct,
            decoded_bytes * 2
        );
        Ok(decoded_bytes * 2)
    }

    pub fn save_image<RWS: ReadWriteSeek>(
        _image: &DiskImage,
        _opts: &ParserWriteOptions,
        _output: &mut RWS,
    ) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }
}
