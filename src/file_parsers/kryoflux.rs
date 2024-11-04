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

    src/parsers/kryoflux.rs

    A parser for the KryoFlux stream image format.

    Kryoflux files (.raw) represent the raw stream of flux transitions for
    a single track on a disk. A set of files is used to represent a complete
    disk image.


*/
use crate::diskimage::DiskDescriptor;
use crate::file_parsers::{bitstream_flags, FormatCaps};
use crate::flux::pll::{Pll, PllPreset};
use crate::flux::FluxTransition;
use crate::io::{ReadBytesExt, ReadSeek, ReadWriteSeek};
use crate::track::fluxstream::FluxStreamTrack;
use crate::util::read_ascii;
use crate::{format_us, io, DiskDataResolution, FoxHashSet, LoadingCallback};
use crate::{
    DiskCh, DiskDataEncoding, DiskDataRate, DiskImage, DiskImageError, DiskImageFormat, ParserWriteCompatibility,
    DEFAULT_SECTOR_SIZE,
};
use binrw::binrw;
use binrw::BinRead;
use std::path::{Path, PathBuf};

pub const KFX_DEFAULT_MCK: f64 = ((18432000.0 * 73.0) / 14.0) / 2.0;
pub const KFX_DEFAULT_SCK: f64 = KFX_DEFAULT_MCK / 2.0;
pub const KFX_DEFAULT_ICK: f64 = KFX_DEFAULT_MCK / 16.0;

pub enum OsbBlock {
    Invalid(u8),
    StreamInfo,
    Index,
    StreamEnd,
    KfInfo,
    Eof,
}

fn read_osb_block<R: ReadBytesExt>(reader: &mut R) -> OsbBlock {
    let byte = reader.read_u8().unwrap_or(0);
    //log::trace!("Read OOB block type: {:02X}", byte);

    match byte {
        0x01 => OsbBlock::StreamInfo,
        0x02 => OsbBlock::Index,
        0x03 => OsbBlock::StreamEnd,
        0x04 => OsbBlock::KfInfo,
        0x0D => OsbBlock::Eof,
        _ => OsbBlock::Invalid(byte),
    }
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct StreamInfoBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub transfer_time_ms: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct IndexBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub sample_counter: u32,
    pub index_counter: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct StreamEndBlock {
    pub size: u16,
    pub stream_pos: u32,
    pub hw_status_code: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct KfInfoBlock {
    pub size: u16,
    // null terminated ascii string follows
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct EofBlock {
    pub size: u16,
}

pub struct KfxFormat {
    sck: f64,
    ick: f64,
    last_index_counter: Option<u32>,
    current_offset_idx: usize,
    idx_ct: u32,
    flux_ovl: u32,
}

impl Default for KfxFormat {
    fn default() -> Self {
        KfxFormat {
            sck: KFX_DEFAULT_SCK,
            ick: KFX_DEFAULT_ICK,
            last_index_counter: None,
            current_offset_idx: 0,
            idx_ct: 0,
            flux_ovl: 0,
        }
    }
}

impl KfxFormat {
    pub fn extensions() -> Vec<&'static str> {
        vec!["raw"]
    }

    pub fn capabilities() -> FormatCaps {
        bitstream_flags()
    }

    pub fn detect<RWS: ReadSeek>(mut image: RWS) -> bool {
        if image.seek(std::io::SeekFrom::Start(0)).is_err() {
            return false;
        }

        let byte = image.read_u8().unwrap_or(0);

        // Assume stream starts with an OOB header byte(?)
        byte == 0x0D
    }

    pub fn can_write(_image: &DiskImage) -> ParserWriteCompatibility {
        ParserWriteCompatibility::UnsupportedFormat
    }

    pub(crate) fn load_image<RWS: ReadSeek>(
        mut image: RWS,
        disk_image: &mut DiskImage,
        _callback: Option<LoadingCallback>,
    ) -> Result<(), DiskImageError> {
        disk_image.set_resolution(DiskDataResolution::FluxStream);
        disk_image.set_source_format(DiskImageFormat::KryofluxStream);

        let mut kfx_context = KfxFormat::default();

        image.seek(io::SeekFrom::Start(0))?;

        // Create vector of streams.
        let mut streams: Vec<Vec<f64>> = Vec::with_capacity(5);
        // Create first stream.
        streams.push(Vec::with_capacity(100_000));

        // Create vector of index times.
        let mut index_times: Vec<f64> = Vec::with_capacity(5);
        // Create vector if index offsets
        let mut index_offsets: Vec<u64> = Vec::with_capacity(5);

        // Read the steam once to gather the index offsets.
        log::debug!("Scanning stream for index blocks...");
        let mut stream_position = 0;
        let mut eof = false;
        while !eof {
            eof =
                kfx_context.read_index_block(&mut image, &mut index_offsets, &mut stream_position, &mut index_times)?;
        }

        kfx_context.current_offset_idx = 0;

        // Read the stream again now that we know where the indexes are
        log::debug!("Reading stream... [Found {} index offsets]", index_offsets.len());
        image.seek(io::SeekFrom::Start(0))?;
        stream_position = 0;
        eof = false;
        while !eof {
            eof = kfx_context.read_block(&mut image, &index_offsets, &mut stream_position, &mut streams)?;
        }

        let mut pll = Pll::from_preset(PllPreset::Aggressive);
        //pll.set_clock(2e-6, None);
        let mut flux_track = FluxStreamTrack::new();

        // log::debug!("Found {} partial revolutions in stream", streams.len());
        // for (si, stream) in streams.iter().enumerate() {
        //     log::debug!("  Rev {}: {} samples", si, stream.len());
        // }
        let complete_revs = (kfx_context.idx_ct - 1) as usize;

        // We need to have at least two index markers to have a complete revolution.
        if complete_revs < 1 || index_offsets.len() < 2 {
            log::error!("Stream did not contain a complete revolution.");
            return Err(DiskImageError::IncompatibleImage);
        }

        log::debug!(
            "Found {} complete revolutions in stream, with {} index times",
            complete_revs,
            index_times.len()
        );

        // Get last ch in image.
        let next_ch = if disk_image.track_ch_iter().count() == 0 {
            log::debug!("No tracks in image, starting at c:0 h:0");
            DiskCh::new(0, 0)
        }
        else {
            let mut last_ch = disk_image.track_ch_iter().last().unwrap_or(DiskCh::new(0, 0));
            log::debug!("Previous track in image: {} heads: {}", last_ch, disk_image.heads());

            last_ch.seek_next_track(disk_image.heads());
            last_ch
        };

        for ((_ri, rev), index_time) in streams
            .iter()
            .enumerate()
            .skip(1)
            .take(complete_revs)
            .zip(index_times.iter())
        {
            flux_track.add_revolution(next_ch, rev, pll.get_clock(), *index_time);
        }

        #[cfg(feature = "plot")]
        {
            let plot_rev: usize = std::cmp::min(0, complete_revs - 1);
            let flux_rev = flux_track.revolution_mut(plot_rev).unwrap();

            let plot_stats = flux_rev.pll_stats();

            let x: Vec<f64> = plot_stats.iter().map(|point| point.time).collect();
            let len: Vec<f64> = plot_stats.iter().map(|point| point.len).collect();
            let predicted: Vec<f64> = plot_stats.iter().map(|point| point.predicted).collect();
            let clk_samples: Vec<f64> = plot_stats.iter().map(|point| point.clk).collect();
            let win_min: Vec<f64> = plot_stats.iter().map(|point| point.window_min).collect();
            let win_max: Vec<f64> = plot_stats.iter().map(|point| point.window_max).collect();
            let phase_err: Vec<f64> = plot_stats.iter().map(|point| point.phase_err).collect();
            let phase_err_i: Vec<f64> = plot_stats.iter().map(|point| point.phase_err_i).collect();

            use plotly::common::{Line, Marker, Mode};
            use plotly::layout::Axis;
            use plotly::*;
            let mut plot = Plot::new();
            let flux_times = Scatter::new(x.clone(), len.clone())
                .mode(Mode::Markers)
                .name("FT length")
                .marker(Marker::new().size(2).color(Rgba::new(0, 128, 0, 1.0)));
            let predicted_times = Scatter::new(x.clone(), predicted)
                .mode(Mode::Markers)
                .name("FT length")
                .marker(Marker::new().size(2).color(Rgba::new(0, 255, 0, 0.5)));
            let clock_trace = Scatter::new(x.clone(), clk_samples)
                .mode(Mode::Lines)
                .name("PLL Clock")
                .line(Line::new().color(Rgba::new(128, 0, 0, 1.0)));

            let window_trace = Scatter::new(
                x.iter().flat_map(|&x| vec![x, x]).collect::<Vec<_>>(), // Duplicate each x for the start and end points
                win_min
                    .iter()
                    .zip(&win_max)
                    .flat_map(|(&start, &end)| vec![start, end])
                    .collect::<Vec<_>>(), // Flatten each pair of y1, y2
            )
            .mode(Mode::Lines) // Use lines to draw each segment
            .name("PLL Window");

            let win_min_trace = Scatter::new(x.clone(), win_min.clone())
                .mode(Mode::Markers)
                .name("Window min")
                .marker(Marker::new().size(3).color(Rgba::new(128, 128, 0, 0.6)));
            let win_max_trace = Scatter::new(x.clone(), win_max.clone())
                .mode(Mode::Markers)
                .name("Window max")
                .marker(Marker::new().size(3).color(Rgba::new(0, 128, 128, 0.6)));

            let error_trace = Scatter::new(x.clone(), phase_err.clone())
                .mode(Mode::Lines)
                .name("Phase Error")
                .line(Line::new().color(Rgba::new(0, 0, 128, 1.0)));

            let error_i_trace = Scatter::new(x.clone(), phase_err_i.clone())
                .mode(Mode::Lines)
                .name("Integrated error")
                .line(Line::new().color(Rgba::new(255, 255, 0, 1.0)));

            //let candle_trace = Candlestick::new(x.clone(), win_min.clone(), win_max.clone(), win_min.clone(), win_max.clone());

            let mut path = PathBuf::from("plots");
            if !path.exists() {
                std::fs::create_dir(path.clone())?;
            }
            let filename = format!("pll_{}_{}.html", next_ch.c(), next_ch.h());

            // let flux_filename = format!("flux_{}_{}.csv", next_ch.c(), next_ch.h());
            // let flux_path = path.join(flux_filename);
            //
            // use std::io::Write;
            // let mut flux_file = std::fs::File::create(flux_path)?;
            // for (x, y) in x.iter().zip(len.clone().iter()) {
            //     writeln!(flux_file, "{},{}", x, y)?;
            // }

            path.push(filename);
            //plot.add_trace(candle_trace);

            plot.add_trace(error_trace);
            plot.add_trace(error_i_trace);
            plot.add_trace(win_min_trace);
            plot.add_trace(win_max_trace);
            //plot.add_trace(window_trace);
            plot.add_trace(predicted_times);
            plot.add_trace(flux_times);

            use plotly::color::Rgba;
            use plotly::layout::{Shape, ShapeLayer, ShapeLine, ShapeType};

            // // Create a list of shapes representing each PLL window
            // let shapes: Vec<Shape> = x
            //     .iter()
            //     .enumerate()
            //     .map(|(i, &start)| {
            //         let x0 = start;
            //         let x1 = if i + 1 < x.len() { x[i + 1] } else { x0 };
            //         let min = win_min[i];
            //         let max = win_max[i];
            //
            //         Shape::new()
            //             .shape_type(ShapeType::Rect)
            //             .x0(x0)
            //             .x1(x1)
            //             .y0(min)
            //             .y1(max)
            //             .layer(ShapeLayer::Below)
            //             .line(ShapeLine::new().width(0.0))
            //             .fill_color(Rgba::new(128, 128, 128, 0.3))
            //     })
            //     .collect();
            //
            // log::warn!("Plotting {} shapes", shapes.len());
            let mut layout = Layout::new().y_axis(Axis::new().range(vec![-1.0e-6, 10.0e-6]));
            // layout = layout.shapes(shapes);

            plot.add_trace(clock_trace);
            plot.set_layout(layout);
            plot.write_html(path);
        }

        // let rev_encoding = flux_rev.encoding();
        // let rev_density = match rev_stats.detect_density(false) {
        //     Some(d) => {
        //         log::debug!("Revolution {} density: {:?}", rev, d);
        //         d
        //     }
        //     None => {
        //         log::error!(
        //             "Unable to detect rev {} track {} density: {}",
        //             rev,
        //             next_ch,
        //             flux_rev.transition_avg()
        //         );
        //         //return Err(DiskImageError::IncompatibleImage);
        //         DiskDensity::Double
        //     }
        // };

        // let (track_data, track_bits) = flux_track.revolution_mut(rev).unwrap().bitstream_data();
        //
        // let data_rate = DiskDataRate::from(rev_density);
        //
        // if track_bits < 1000 {
        //     log::warn!("Track contains less than 1000 bits. Adding empty track.");
        //     disk_image.add_empty_track(next_ch, DiskDataEncoding::Mfm, data_rate, 100_000)?;
        // }
        // else {
        //     log::debug!(
        //         "Adding {:?} track {} containing {} bits to image...",
        //         rev_encoding,
        //         next_ch,
        //         track_bits
        //     );
        //
        //     let params = BitStreamTrackParams {
        //         encoding: rev_encoding,
        //         data_rate,
        //         ch: next_ch,
        //         bitcell_ct: Some(track_bits),
        //         data: &track_data,
        //         weak: None,
        //         hole: None,
        //         detect_weak: false,
        //     };
        //     disk_image.add_track_bitstream(params)?;
        // }

        let data_rate = disk_image.data_rate();

        // Get hints from disk image if we aren't the first track.
        let (clock_hint, rpm_hint) = if !disk_image.track_pool.is_empty() {
            (
                Some(disk_image.descriptor.density.base_clock()),
                disk_image.descriptor.rpm,
            )
        }
        else {
            (None, None)
        };

        let new_track = disk_image.add_track_fluxstream(next_ch, flux_track, clock_hint, rpm_hint)?;

        let (new_density, new_rpm) = if new_track.get_sector_ct() == 0 {
            log::warn!("Track did not decode any sectors. Not updating disk image descriptor.");
            (disk_image.descriptor.density, disk_image.descriptor.rpm)
        }
        else {
            let info = new_track.info();
            log::debug!(
                "Updating disk descriptor with density: {:?} and RPM: {:?}",
                info.density,
                info.rpm
            );
            (info.density.unwrap_or(disk_image.descriptor.density), info.rpm)
        };

        log::debug!("Track added.");

        disk_image.descriptor = DiskDescriptor {
            geometry: disk_image.geometry(),
            data_rate,
            density: new_density,
            data_encoding: DiskDataEncoding::Mfm,
            default_sector_size: DEFAULT_SECTOR_SIZE,
            rpm: new_rpm,
            write_protect: Some(true),
        };

        Ok(())
    }

    pub fn save_image<RWS: ReadWriteSeek>(_image: &DiskImage, _output: &mut RWS) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    fn read_index_block<RWS: ReadSeek>(
        &mut self,
        image: &mut RWS,
        index_offsets: &mut Vec<u64>,
        stream_position: &mut u64,
        index_times: &mut Vec<f64>,
    ) -> Result<bool, DiskImageError> {
        let file_offset = image.stream_position()?;
        let byte = image.read_u8()?;

        match byte {
            0x00..=0x07 => {
                // Flux2 block
                image.seek(std::io::SeekFrom::Current(1))?;
                *stream_position += 2;
            }
            0x09 => {
                // Nop2 block
                // Skip one byte
                image.seek(std::io::SeekFrom::Current(1))?;
                *stream_position += 2;
            }
            0x0A => {
                // Nop3 block
                // Skip two bytes
                image.seek(std::io::SeekFrom::Current(2))?;
                *stream_position += 3;
            }
            0x0B => {
                // Ovl16 block
                *stream_position += 1;
            }
            0x0C => {
                // Flux3 block
                image.seek(std::io::SeekFrom::Current(2))?;
                *stream_position += 3;
            }
            0x0D => {
                // OOB block
                let oob_block = read_osb_block(image);

                match oob_block {
                    OsbBlock::Invalid(oob_byte) => {
                        log::error!("Invalid OOB block type: {:02X}", oob_byte);
                    }
                    OsbBlock::StreamInfo => {
                        let _sib = StreamInfoBlock::read(image)?;
                    }
                    OsbBlock::Index => {
                        let ib = IndexBlock::read(image)?;

                        //let index_time = ib.index_counter as f64 / self.ick;

                        if let Some(last_index_counter) = self.last_index_counter {
                            let index_delta = ib.index_counter.wrapping_sub(last_index_counter);
                            let index_time_delta = index_delta as f64 / self.ick;
                            index_times.push(index_time_delta);

                            let sample_time = ib.sample_counter as f64 / self.sck;

                            log::debug!(
                                "Index block: file_offset: {} next_pos: {} sample_ct: {} ({}) index_ct: {} delta: {:.6} rpm: {:.3}",
                                file_offset,
                                ib.stream_pos,
                                ib.sample_counter,
                                format_us!(sample_time),
                                ib.index_counter,
                                index_time_delta,
                                60.0 / index_time_delta
                            );
                        }
                        else {
                            let sample_time = ib.sample_counter as f64 / self.sck;

                            log::debug!(
                                "Index block: file_offset: {} next_pos: {} sample_ct: {} ({}) index_ct: {}",
                                file_offset,
                                ib.stream_pos,
                                ib.sample_counter,
                                format_us!(sample_time),
                                ib.index_counter
                            );
                        }

                        index_offsets.push(ib.stream_pos as u64);

                        // If stream_pos is behind us, we need to go back and create a revolution
                        // at stream_pos
                        if (ib.stream_pos as u64) < *stream_position {
                            log::warn!(
                                "Stream pos is behind current stream position: {} < {}",
                                ib.stream_pos,
                                stream_position
                            );
                        }

                        self.last_index_counter = Some(ib.index_counter);
                    }
                    OsbBlock::StreamEnd => {
                        let _seb = StreamEndBlock::read(image)?;
                    }
                    OsbBlock::KfInfo => {
                        log::debug!("KfInfo block");
                        let _kib = KfInfoBlock::read(image)?;
                        // Ascii string follows
                        let mut string_end = false;
                        while !string_end {
                            let (str_opt, terminator) = read_ascii(image, None, None);
                            string_end = str_opt.is_none() || terminator == 0;
                        }
                    }
                    OsbBlock::Eof => {
                        log::debug!("EOF block");
                        return Ok(true);
                    }
                }
            }
            _ => {
                // Flux1 block
                *stream_position += 1;
            }
        }

        // Return whether we reached end of file
        Ok(false)
    }

    fn read_block<RWS: ReadSeek>(
        &mut self,
        image: &mut RWS,
        index_offsets: &[u64],
        stream_position: &mut u64,
        streams: &mut Vec<Vec<f64>>,
    ) -> Result<bool, DiskImageError> {
        let file_offset = image.stream_position()?;
        let byte = image.read_u8()?;

        // If we've reached the stream position indicated by the last index block,
        // we're starting a new revolution.
        if (self.current_offset_idx < index_offsets.len())
            && (*stream_position >= index_offsets[self.current_offset_idx])
        {
            log::debug!(
                "Starting new revolution at stream_pos: {}, file_offset: {}",
                *stream_position,
                file_offset
            );
            streams.push(Vec::new());
            self.current_offset_idx += 1;
            self.idx_ct += 1;
        }

        //log::trace!("Read block type: {:02X}", byte);
        match byte {
            0x00..=0x07 => {
                // Flux2 block
                let byte2 = image.read_u8()?;
                let flux_u32 = u16::from_be_bytes([byte, byte2]) as u32;
                let flux = (self.flux_ovl + flux_u32) as f64 / self.sck;

                *stream_position += 2;
                streams.last_mut().unwrap().push(flux);

                self.flux_ovl = 0;
            }
            0x08 => {
                // Nop1 block
                // Do nothing
                *stream_position += 1;
            }
            0x09 => {
                // Nop2 block
                // Skip one byte
                image.seek(std::io::SeekFrom::Current(1))?;
                *stream_position += 2;
            }
            0x0A => {
                // Nop3 block
                // Skip two bytes
                image.seek(std::io::SeekFrom::Current(2))?;
                *stream_position += 3;
            }
            0x0B => {
                // Ovl16 block
                self.flux_ovl = self.flux_ovl.saturating_add(0x10000);
            }
            0x0C => {
                // Flux3 block
                let byte2 = image.read_u8()?;
                let byte3 = image.read_u8()?;
                let flux_u32 = u16::from_be_bytes([byte2, byte3]) as u32;
                let flux = (self.flux_ovl + flux_u32) as f64 / self.sck;

                streams.last_mut().unwrap().push(flux);

                self.flux_ovl = 0;
            }
            0x0D => {
                // OSB block. OSB blocks do not advance the stream position.
                let osb_block = read_osb_block(image);

                match osb_block {
                    OsbBlock::Invalid(oob_byte) => {
                        log::error!("Invalid OOB block type: {:02X}", oob_byte);
                    }
                    OsbBlock::StreamInfo => {
                        let sib = StreamInfoBlock::read(image)?;
                        log::trace!(
                            "StreamInfo block: pos: {} time: {}",
                            sib.stream_pos,
                            sib.transfer_time_ms
                        );
                    }
                    OsbBlock::Index => {
                        let _ib = IndexBlock::read(image)?;
                    }
                    OsbBlock::StreamEnd => {
                        let seb = StreamEndBlock::read(image)?;
                        log::debug!(
                            "StreamEnd block: end_pos: {} stream_pos: {} offset: {} hw_status: {:02X}",
                            seb.stream_pos,
                            *stream_position,
                            file_offset,
                            seb.hw_status_code
                        );

                        if seb.stream_pos as u64 != *stream_position {
                            log::warn!(
                                "StreamEnd position does not match stream position: {} != {}",
                                seb.stream_pos,
                                *stream_position
                            );
                        }

                        match seb.hw_status_code {
                            0 => {
                                log::debug!("Hardware status reported OK");
                            }
                            1 => {
                                log::error!("A buffering issue was recorded in the stream. Stream may be corrupt");
                                return Err(DiskImageError::ImageCorruptError);
                            }
                            2 => {
                                log::error!("No index signal was detected.");
                                return Err(DiskImageError::ImageCorruptError);
                            }
                            _ => {
                                log::error!("Unknown hardware status. Hope it wasn't important!");
                            }
                        }
                    }
                    OsbBlock::KfInfo => {
                        log::debug!("KfInfo block");
                        let _kib = KfInfoBlock::read(image)?;
                        // Ascii string follows
                        let mut string_end = false;
                        let mut string = String::new();
                        while !string_end {
                            let (str_opt, terminator) = read_ascii(image, None, None);
                            if let Some(s) = &str_opt {
                                log::debug!("KfInfo str: {}", s);
                                let (sck_opt, ick_opt) = kfx_parse_str(s);
                                if let Some(sck) = sck_opt {
                                    log::debug!("Set SCK to {}", sck);
                                    self.sck = sck;
                                }
                                if let Some(ick) = ick_opt {
                                    log::debug!("Set ICK to {}", ick);
                                    self.ick = ick;
                                }
                                string.push_str(s);
                            }
                            //log::warn!("terminator: {:02X}", terminator);
                            string_end = str_opt.is_none() || terminator == 0;
                        }
                    }
                    OsbBlock::Eof => {
                        log::debug!("EOF block");
                        return Ok(true);
                    }
                }
            }
            _ => {
                // Flux1 block
                let flux = (self.flux_ovl + byte as u32) as f64 / self.sck;
                streams.last_mut().unwrap().push(flux);
                *stream_position += 1;
                self.flux_ovl = 0;
            }
        }

        // Return whether we reached end of file
        Ok(false)
    }

    /// Resolves a supplied PathBuf into a vector of PathBufs representing a KryoFlux set.
    /// The set can be resolved from a provided list of PathBufs passed via 'directory', or from the
    /// base directory of the 'filepath' argument, if 'directory' is None.
    /// This allows building a set from either a directory listing or a list of files from a ZIP
    /// archive.
    pub fn expand_kryoflux_set(
        filepath: PathBuf,
        directory: Option<Vec<PathBuf>>,
    ) -> Result<(Vec<PathBuf>, DiskCh), DiskImageError> {
        let mut set_vec = Vec::new();

        // Isolate the base path and filename
        let base_path = filepath.parent().unwrap_or(Path::new(""));
        let base_name = filepath.file_name().ok_or(DiskImageError::FsError)?;

        let mut cylinders_seen: FoxHashSet<u32> = FoxHashSet::new();
        let mut heads_seen: FoxHashSet<u32> = FoxHashSet::new();

        // Create a regex for any filename that ends in two digits, a period, a single digit,
        // and then the extension '.raw'
        let re = regex::Regex::new(r"(.*)(\d{2})\.(\d)\.raw").unwrap();
        // Match the regex against the base name
        let caps = re.captures(base_name.to_str().ok_or(DiskImageError::FsError)?);

        // Use the provided directory listing if Some, otherwise get all directory entries from
        // the base path.
        let file_listing = match directory {
            Some(d) => d,
            None => std::fs::read_dir(base_path)?
                .map(|res| res.map(|entry| entry.path()))
                .collect::<Result<Vec<PathBuf>, crate::io::Error>>()?,
        };

        //log::debug!("File listing: {:?}", file_listing);

        let mut set_ch = DiskCh::new(0, 0);
        if let Some(c) = caps {
            // If it does, get the base name and extension
            let base_name = c.get(1).ok_or(DiskImageError::FsError)?;
            let ext = ".raw";

            let mut c: u16 = 0;
            let mut h: u8 = 0;

            let mut stop_searching = false;
            let last_expected_cyl = 79;
            while !stop_searching {
                // Construct a test filename from the base name, cylinder and head number, and extension
                let test_name = format!("{}{:02}.{}{}", base_name.as_str().to_ascii_lowercase(), c, h, ext);

                // Check if the test file exists.
                // The lowercase check here is necessary as some kryoflux sets I have seen have mixed
                // case filenames. (Track00.0.raw, Track00.1.raw, track01.0.raw)
                if file_listing
                    .iter()
                    .any(|f| *f.file_name().unwrap().to_ascii_lowercase() == *test_name)
                {
                    log::debug!("Found filename in set: {}", test_name);

                    if h > 0 {
                        h = h.wrapping_add(1)
                    }

                    // If file exists, add it to the set
                    set_vec.push(base_path.join(test_name));
                    // Add cylinder and head to the set of seen values
                    cylinders_seen.insert(c as u32);
                    heads_seen.insert(h as u32);
                }
                else if h == 0 {
                    // We didn't find a file representing side 0 of the next cylinder.

                    // We could just have a set missing tracks. We should not necessarily consider
                    // this an error - some disk images have blank tracks in the middle. We should
                    // only stop searching if we're past the last expected cylinder.
                    if c > last_expected_cyl {
                        stop_searching = true;
                    }
                }

                h += 1;
                if h > 1 {
                    h = 0;
                    c += 1;
                }
            }

            set_ch = DiskCh::new(cylinders_seen.len() as u16, heads_seen.len() as u8);
        }

        Ok((set_vec, set_ch))
    }
}

fn kfx_parse_str(s: &str) -> (Option<f64>, Option<f64>) {
    // use a regex to parse the clock info string
    // ex: 'sck=24027428.5714285, ick=3003428.5714285625'
    let re = regex::Regex::new(r"sck=(\d+\.\d+), ick=(\d+\.\d+)").unwrap();

    let caps = re.captures(s);
    if let Some(c) = caps {
        let sck = c.get(1).and_then(|m| m.as_str().parse::<f64>().ok());
        let ick = c.get(2).and_then(|m| m.as_str().parse::<f64>().ok());
        (sck, ick)
    }
    else {
        (None, None)
    }
}

#[allow(dead_code)]
fn kfx_transition_ct_to_bitrate(count: usize) -> Option<DiskDataRate> {
    match count {
        35000..=60000 => Some(DiskDataRate::Rate250Kbps),
        70000..=120000 => Some(DiskDataRate::Rate500Kbps),
        140000..=240000 => Some(DiskDataRate::Rate1000Kbps),
        _ => None,
    }
}

#[allow(dead_code)]
fn print_transitions(transitions: Vec<FluxTransition>) {
    for t in transitions {
        print!("{}", t);
    }
}
