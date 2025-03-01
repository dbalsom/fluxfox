/*
    fluxfox - fftool
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
pub(crate) mod args;

use crate::{args::GlobalOptions, dump::args::DumpParams, read_file};
use anyhow::{bail, Error, Result};
use fluxfox::prelude::*;
use std::io::{BufWriter, Write};

pub(crate) fn run(global: &GlobalOptions, params: &args::DumpParams) -> Result<(), Error> {
    let row_size = params.row_size.unwrap_or(16) as usize;
    let mut cursor = read_file(&params.in_file)?;

    let disk_image_type = match DiskImage::detect_format(&mut cursor, Some(&params.in_file)) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    if !global.silent {
        println!("Detected disk image type: {}", disk_image_type);
    }

    let mut disk = match DiskImage::load(&mut cursor, Some(&params.in_file), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    let mut buf = BufWriter::new(std::io::stdout());

    if params.dupe_mark {
        if let Some((dupe_ch, dupe_chsn)) = disk.find_duplication_mark() {
            let _rsr = match disk.read_sector(
                dupe_ch,
                DiskChsnQuery::from(dupe_chsn),
                None,
                None,
                RwScope::DataOnly,
                true,
            ) {
                Ok(rsr) => rsr,
                Err(e) => {
                    bail!("Error reading sector: {}", e);
                }
            };

            let dump_string = match disk.dump_sector_string(dupe_ch, DiskChsnQuery::from(dupe_chsn), None, None) {
                Ok(dump_string) => dump_string,
                Err(e) => {
                    bail!("Error dumping sector: {}", e);
                }
            };

            //println!("Duplication mark found at {} with ID {}", dupe_ch, dupe_chsn);
            println!("{}", dump_string);
            std::process::exit(0);
        }
        else {
            println!("No duplication mark found.");
        }
    }

    // Specify the physical cylinder and head. If these are not explicitly provided, we assume
    // that the physical cylinder and head are the same as the target cylinder and head.
    let mut phys_ch = DiskCh::new(params.cylinder, params.head);
    if let Some(phys_cylinder) = params.phys_cylinder {
        phys_ch.set_c(phys_cylinder);
    }

    if let Some(phys_head) = params.phys_head {
        phys_ch.set_h(phys_head);
    }

    let track_mut_opt = disk.track_mut(phys_ch);
    let track_mut = match track_mut_opt {
        Some(track_mut) => track_mut,
        None => {
            bail!("Specified track: {} not found.", phys_ch);
        }
    };

    if let Some(rev) = params.rev {
        if let Some(flux_track) = track_mut.as_fluxstream_track_mut() {
            flux_track.set_revolution(rev as usize);
        }
        else {
            bail!("Revolution number specified but track is not a flux track.");
        }
    }

    // If sector was provided, dump the sector.
    if let Some(sector) = params.sector {
        // Dump the specified sector in hex format to stdout.

        let id_chs = DiskChs::new(params.cylinder, params.head, sector);

        let scope = RwScope::DataOnly;
        let calc_crc = false;
        // let (scope, calc_crc) = match params.structure {
        //     true => (RwSectorScope::DataBlock, true),
        //     false => (RwSectorScope::DataOnly, false),
        // };

        _ = writeln!(&mut buf, "reading sector...");
        _ = buf.flush();

        let rsr = match disk.read_sector(phys_ch, DiskChsnQuery::from(id_chs), params.n, None, scope, true) {
            Ok(rsr) => rsr,
            Err(e) => {
                bail!("Error reading sector: {}", e);
            }
        };

        _ = writeln!(
            &mut buf,
            "Data idx: {} length: {}",
            rsr.data_range.start,
            rsr.data_range.len()
        );

        let data_slice = rsr.data();

        if !global.silent {
            println!(
                "Dumping sector from track {} with id {} in hex format, with scope {:?}:",
                phys_ch,
                DiskChsn::from((id_chs, params.n.unwrap_or(2))),
                scope
            );
        }

        if data_slice.len() >= 16 {
            log::debug!("read buf: {:02X?}", &rsr.read_buf[0..16]);
            log::debug!("data slice: {:02X?}", &data_slice[0..16]);
        }
        _ = fluxfox::util::dump_slice(data_slice, 0, row_size, 1, &mut buf);

        // If we requested DataBlock scope, we can independently calculate the CRC, so do that now.
        if calc_crc {
            let calculated_crc = fluxfox::util::crc_ibm_3740(&data_slice[0..0x104], None);
            _ = writeln!(&mut buf, "Calculated CRC: {:04X}", calculated_crc);
        }

        Ok(())
    }
    else {
        // No sector was provided, dump the whole track.
        let ch = DiskCh::new(params.cylinder, params.head);

        if params.clock_map {
            dump_clock_map(&mut disk, row_size, ch, &mut buf, global, params)?;
        }
        else {
            dump_track(&mut disk, row_size, ch, &mut buf, global, params)?;
        }

        Ok(())
    }
}

fn dump_track<W: Write>(
    disk: &mut DiskImage,
    row_size: usize,
    ch: DiskCh,
    buf: &mut W,
    global: &GlobalOptions,
    params: &DumpParams,
) -> Result<()> {
    if params.raw {
        let track = match disk.track_mut(ch) {
            Some(track) => track,
            None => {
                bail!("Specified track: {} not found.", ch);
            }
        };

        if !global.silent {
            println!(
                "Dumping track {} ({}, {} bits), raw, in hex format:",
                ch,
                track.info().encoding,
                track.info().bit_length
            );
        }

        let rtr = match track.read_raw(None) {
            Ok(rtr) => {
                //println!("* read track raw *");
                rtr
            }
            Err(e) => {
                bail!("Error reading track: {}", e);
            }
        };

        // In raw format, one byte = 8 bits, and it takes 2 bytes to represent 1 decoded byte.
        let element_size = match params.bit_address {
            true => 8,
            false => 2,
        };
        _ = fluxfox::util::dump_slice(&rtr.read_buf, 0, row_size, element_size, buf);
    }
    else {
        let track = match disk.track_mut(ch) {
            Some(track) => track,
            None => {
                bail!("Specified track: {} not found.", ch);
            }
        };

        if !global.silent {
            println!(
                "Dumping track {} ({}, {} bits), decoded, in hex format:",
                ch,
                track.info().encoding,
                track.info().bit_length
            );
        }

        let rtr = match disk.read_track(ch, None) {
            Ok(rtr) => {
                //println!("* read track *");
                rtr
            }
            Err(e) => {
                bail!("Error reading track: {}", e);
            }
        };

        // Each byte of the track is 16 MFM bits.
        let element_size = match params.bit_address {
            true => 16,
            false => 1,
        };
        _ = fluxfox::util::dump_slice(&rtr.read_buf, 0, row_size, element_size, buf);
    };

    Ok(())
}

fn dump_clock_map<W: Write>(
    disk: &mut DiskImage,
    row_size: usize,
    ch: DiskCh,
    buf: &mut W,
    global: &GlobalOptions,
    _params: &DumpParams,
) -> Result<()> {
    let track = match disk.track_mut(ch) {
        Some(track) => track,
        None => {
            bail!("Specified track: {} not found.", ch);
        }
    };

    if !global.silent {
        println!(
            "Dumping clock map for track {} ({}, {} bits), raw, in hex format:",
            ch,
            track.info().encoding,
            track.info().bit_length
        );
    }

    if let Some(codec) = track.stream_mut() {
        let map_vec = codec.clock_map().to_bytes();
        _ = fluxfox::util::dump_slice(&map_vec, 0, row_size, 8, buf);
    }
    else {
        bail!("Failed to resolve track codec");
    }

    Ok(())
}
