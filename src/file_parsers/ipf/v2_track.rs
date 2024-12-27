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

//! Decoding functions for IPF encoder v2 tracks (SXX).

use crate::{
    bitstream::{mfm::MFM_BYTE_LEN, TrackDataStream},
    file_parsers::ipf::{
        data_block::{BlockDescriptor, BlockFlags},
        image_record::ImageRecord,
        info_record::InfoRecord,
        ipf::IpfParser,
        stream_element::{DataSample, DataStreamElement, DataType, GapSample, GapStreamElement},
    },
    io::ReadSeek,
    prelude::{DiskDataResolution, TrackDataEncoding, TrackDataRate, TrackDensity},
    source_map::{MapDump, OptionalSourceMap},
    track_schema::TrackSchema,
    DiskImage,
    DiskImageError,
};
use binrw::BinRead;
use bit_vec::BitVec;

pub enum GapFillDirection {
    Forwards,
    Backwards,
}

impl IpfParser {
    pub(crate) fn decode_v2_track<RWS>(
        reader: &mut RWS,
        image: &mut DiskImage,
        info_record: &InfoRecord,
        image_record: &ImageRecord,
        record_node: usize,
        data: &crate::file_parsers::ipf::ipf::DataRecordInfo,
    ) -> Result<(), DiskImageError>
    where
        RWS: ReadSeek,
    {
        image.set_resolution(DiskDataResolution::BitStream);

        log::debug!("-------------------------- Decoding V2 (SXX) Track ----------------------------------");
        log::debug!(
            "Track {} bitct: {:6} block_ct: {:02} data_bits: {}",
            image_record.ch(),
            image_record.track_bits,
            image_record.block_count,
            image_record.data_bits,
        );
        //log::trace!("Image Record: {:#?}", image_record);

        // Density is *probably* double. Guess from bitcell count or assume double.
        let data_rate =
            TrackDataRate::from(TrackDensity::from_bitcells(image_record.track_bits).unwrap_or(TrackDensity::Double));

        // // Create empty BitVec for track data.
        // let track_bits = BitVec::from_elem(image_record.track_bits as usize, false);
        // // Amiga is *probably* MFM encoded.
        // let codec = Box::new(MfmCodec::new(track_bits, Some(image_record.track_bits as usize), None));

        //let start_clock = image_record.start_bit_pos % 2 != 0;

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

        // After we get a reference to the track, the disk image will be mutably borrowed until
        // the end of track processing, and we won't be able to get a mutable reference to the
        // source map.
        //
        // We fall back to the trusty ol take hack to get around this. But now we have to put it
        // back on error if we want to preserve it.
        //
        // A better design would probably be to construct a detached track object and then attach
        // it to the image after it is built. Or, if we store tracks as options, I'd rather take
        // the track than the source map as it would simplify error handling.
        //
        // TODO: Revisit this design

        let mut source_map = image.take_source_map().unwrap();
        let track = match image.track_by_idx_mut(new_track_idx) {
            Some(track) => track,
            None => {
                image.put_source_map(source_map);
                log::error!("Failed to get mutable track for image.");
                return Err(DiskImageError::FormatParseError);
            }
        };

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
                    image.put_source_map(source_map);
                    log::error!("Failed to get mutable stream for track.");
                    return Err(DiskImageError::FormatParseError);
                }
            };

            log::trace!("Seeking to {} for first block.", image_record.start_bit_pos & !0xF);
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

                let encoded_bytes = match Self::decode_v2_data_block(
                    reader,
                    &mut source_map,
                    data.edb_offset,
                    block,
                    record_node,
                    bitstream,
                    &mut cursor,
                ) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        image.put_source_map(source_map);
                        log::error!("Failed to decode V2 block: {}", e);
                        return Err(e);
                    }
                };

                // if encoded_bytes != data_bytes as usize {
                //     log::warn!(
                //         "Block {} decoded {} bytes, but expected {} bytes.",
                //         bi,
                //         encoded_bytes,
                //         data_bytes
                //     );
                // }

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

        let track = match image.track_by_idx_mut(new_track_idx) {
            Some(track) => track,
            None => {
                image.put_source_map(source_map);
                log::error!("Failed to get mutable track for image.");
                return Err(DiskImageError::FormatParseError);
            }
        };

        let bitstream_track = match track.as_bitstream_track_mut() {
            Some(track) => track,
            None => {
                image.put_source_map(source_map);
                log::error!("Failed to get mutable bitstream track for image.");
                return Err(DiskImageError::FormatParseError);
            }
        };

        bitstream_track.rescan(Some(TrackSchema::Amiga))?;
        // Finally, put the source map back on the image.
        image.put_source_map(source_map);

        Ok(())
    }

    pub fn decode_v2_data_block<RWS>(
        reader: &mut RWS,
        source_map: &mut Box<dyn OptionalSourceMap>,
        edb_offset: u64,
        block: &BlockDescriptor,
        record_node: usize,
        bitstream: &mut TrackDataStream,
        cursor: &mut usize,
    ) -> Result<usize, DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("-------------------------- Decoding V2 Data Block --------------------------------");
        // Write BlockDescriptor to source map
        let block_node = block.write_to_map(source_map, record_node);
        if block_node == 0 {
            log::error!("Invalid block descriptor!");
            return Err(DiskImageError::ImageCorruptError(
                "V2 block descriptor missing gap_offset.".to_string(),
            ));
        }

        // V2 Block Descriptor should have gap_offset and cell_type
        let gap_offset = if let Some(gap) = &block.gap_offset {
            *gap as usize
        }
        else {
            log::error!("V2 block descriptor missing gap_offset.");
            return Err(DiskImageError::ImageCorruptError(
                "V2 block descriptor missing gap_offset.".to_string(),
            ));
        };

        let cell_type = if let Some(cell_type) = &block.cell_type {
            *cell_type as usize
        }
        else {
            log::error!("V2 block descriptor missing cell_type.");
            return Err(DiskImageError::ImageCorruptError(
                "V2 block descriptor missing gap_offset.".to_string(),
            ));
        };

        // V2 Block Descriptor should have flags
        let flags = if let Some(flags) = &block.block_flags {
            log::debug!("Block flags: {:?}", flags);
            flags
        }
        else {
            log::error!("V2 block descriptor missing block flags.");
            return Err(DiskImageError::ImageCorruptError(
                "V2 block descriptor missing block flags.".to_string(),
            ));
        };

        // Read GapStreamElements
        // -----------------------------------------------------------------------------------------
        // These only exist on v2 tracks
        // They seem to come before the data elements, but I'm not sure if that's always the case.
        // In any case, the order doesn't really matter because the offsets will determine where
        // they are found.

        let mut decoded_bytes = 0;

        // Gap elements are only present if gap_bits > 0
        if block.gap_bits > 0 {
            // Safe to unwrap: we've already failed if gap_offset is None
            let gap_offset = edb_offset + block.gap_offset.unwrap() as u64;
            log::trace!("Seeking to gap offset: {}", gap_offset);
            reader.seek(std::io::SeekFrom::Start(gap_offset))?;

            // Read forward gap list, if present
            if block.block_flags.as_ref().unwrap().contains(BlockFlags::FORWARD_GAP) {
                let gap_node = source_map
                    .add_child(block_node, "Forward Gap List", Default::default())
                    .index();
                Self::read_v2_gap_elements(
                    reader,
                    source_map,
                    block,
                    gap_node,
                    bitstream,
                    GapFillDirection::Backwards,
                )?;
            }
            // Read reverse gap list, if present
            if block.block_flags.as_ref().unwrap().contains(BlockFlags::BACKWARD_GAP) {
                let gap_node = source_map
                    .add_child(block_node, "Backward Gap List", Default::default())
                    .index();
                Self::read_v2_gap_elements(
                    reader,
                    source_map,
                    block,
                    gap_node,
                    bitstream,
                    GapFillDirection::Forwards,
                )?;
            }
        }

        // Seek to the first data element
        let data_offset = edb_offset + block.data_offset as u64;
        log::trace!("Seeking to data offset: {}", data_offset);
        match reader.seek(std::io::SeekFrom::Start(data_offset)) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to seek to data element: {}", e);
                return Err(DiskImageError::from(e));
            }
        }
        decoded_bytes += Self::decode_v2_data_elements(reader, source_map, block, block_node, bitstream, cursor)?;

        // Render gap
        Self::write_gap_elements(block, bitstream, cursor, None, None)?;

        Ok(decoded_bytes * 2)
    }

    pub fn read_v2_gap_elements<RWS>(
        reader: &mut RWS,
        source_map: &mut Box<dyn OptionalSourceMap>,
        block: &BlockDescriptor,
        block_node: usize,
        bitstream: &mut TrackDataStream,
        direction: GapFillDirection,
    ) -> Result<BitVec, DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("------------------------ Decoding V2 GapStreamElements ---------------------------");
        let mut gap_element = GapStreamElement::read(reader)?;
        // Write gap element to source map
        let _gap_node = gap_element.write_to_map(source_map, block_node);

        let mut element_ct = 0;

        log::debug!("Total gap bits: {}", block.gap_bits);

        let mut repeat_ct = None;
        let mut bit_vec = BitVec::new();

        while !gap_element.gap_head.is_null() {
            let gap_type = gap_element.gap_head.gap_type();
            let wrote = if let Some(samples) = &gap_element.gap_sample {
                match samples {
                    GapSample::RepeatCt(ct) => {
                        repeat_ct = Some(*ct);
                        0
                    }
                    GapSample::Sample(bits) => {
                        let repeat = if let Some(repeat_ct) = repeat_ct {
                            repeat_ct
                        }
                        else {
                            log::warn!("Gap element has no repeat count!");
                            1
                        };
                        repeat_ct = None;
                        bit_vec = BitVec::from_fn(bits.len() * repeat, |i| bits[i % bits.len()]);
                        0
                    }
                }
            }
            else {
                break;
            };

            //decoded_bytes += wrote;
            //*cursor += wrote * MFM_BYTE_LEN;

            // Read the next data element
            element_ct += 1;
            gap_element = GapStreamElement::read(reader)?;
            // Write data element to source map
            let _gap_node = gap_element.write_to_map(source_map, block_node);
        }

        log::debug!("Read {} gap elements from V12 block", element_ct,);
        Ok(bit_vec)
    }

    pub fn decode_v2_data_elements<RWS>(
        reader: &mut RWS,
        source_map: &mut Box<dyn OptionalSourceMap>,
        block: &BlockDescriptor,
        block_node: usize,
        bitstream: &mut TrackDataStream,
        cursor: &mut usize,
    ) -> Result<usize, DiskImageError>
    where
        RWS: ReadSeek,
    {
        log::debug!("------------------------ Decoding V2 DataStreamElements ---------------------------");

        // Read DataStreamElements
        // -----------------------------------------------------------------------------------------
        // Pass DATA_IN_BITS flag to data element reader
        let mut data_element = DataStreamElement::read(reader)?;
        // Write data element to source map
        let _data_node = data_element.write_to_map(source_map, block_node);

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
                        // This shouldn't really happen in a V1 block...
                        log::warn!("Unhandled: Bit samples in V1 block!");
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
            data_element = DataStreamElement::read_args(
                reader,
                (
                    block.block_flags.as_ref().unwrap().contains(BlockFlags::DATA_IN_BITS),
                    0,
                ),
            )?;
            // Write data element to source map
            let _data_node = data_element.write_to_map(source_map, block_node);
        }

        log::debug!(
            "Read {} data elements from V1 block, wrote {} MFM bytes to track",
            element_ct,
            decoded_bytes * 2
        );
        Ok(decoded_bytes * 2)
    }

    pub fn write_gap_elements(
        block: &BlockDescriptor,
        bitstream: &mut TrackDataStream,
        cursor: &mut usize,
        forwards_bits: Option<BitVec>,
        backwards_bits: Option<BitVec>,
    ) -> Result<(), DiskImageError> {
        // Advance track cursor to the end of the gap to write the next set of data elements.
        *cursor += block.gap_bits as usize;
        Ok(())
    }
}
