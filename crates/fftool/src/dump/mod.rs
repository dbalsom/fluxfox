/*
    fluxfox - fftool
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
pub(crate) mod args;

use anyhow::{bail, Error};
use std::io::{BufWriter, Write};

use crate::{args::GlobalOptions, read_file};
use fluxfox::{diskimage::RwSectorScope, DiskCh, DiskChs, DiskChsn, DiskImage};

pub(crate) fn run(global: &GlobalOptions, params: args::DumpParams) -> Result<(), Error> {
    let row_size = params.row_size.unwrap_or(16) as usize;
    let mut cursor = read_file(&params.in_file)?;

    let disk_image_type = match DiskImage::detect_format(&mut cursor) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    if !global.silent {
        println!("Detected disk image type: {}", disk_image_type);
    }

    let mut disk = match DiskImage::load(&mut cursor, Some(params.in_file.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    let mut buf = BufWriter::new(std::io::stdout());

    if params.dupe_mark {
        if let Some((dupe_ch, dupe_chsn)) = disk.find_duplication_mark() {
            let _rsr = match disk.read_sector(dupe_ch, DiskChs::from(dupe_chsn), None, RwSectorScope::DataOnly, true) {
                Ok(rsr) => rsr,
                Err(e) => {
                    bail!("Error reading sector: {}", e);
                }
            };

            let dump_string = match disk.dump_sector_string(dupe_ch, DiskChs::from(dupe_chsn), None) {
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

        let scope = RwSectorScope::DataOnly;
        let calc_crc = false;
        // let (scope, calc_crc) = match params.structure {
        //     true => (RwSectorScope::DataBlock, true),
        //     false => (RwSectorScope::DataOnly, false),
        // };

        _ = writeln!(&mut buf, "reading sector...");
        _ = buf.flush();

        let rsr = match disk.read_sector(phys_ch, id_chs, params.n, scope, true) {
            Ok(rsr) => rsr,
            Err(e) => {
                bail!("Error reading sector: {}", e);
            }
        };

        _ = writeln!(&mut buf, "Data idx: {} length: {}", rsr.data_idx, rsr.data_len);

        let data_slice = match scope {
            RwSectorScope::DataOnly => &rsr.read_buf[rsr.data_idx..rsr.data_idx + rsr.data_len],
            RwSectorScope::DataBlock => &rsr.read_buf,
        };

        if !global.silent {
            println!(
                "Dumping sector from {} with id {} in hex format, with scope {:?}:",
                phys_ch,
                DiskChsn::from((id_chs, params.n.unwrap_or(2))),
                scope
            );
        }

        _ = fluxfox::util::dump_slice(data_slice, 0, row_size, &mut buf);

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

        let rtr = if params.raw {
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

            match track.read_track_raw(None) {
                Ok(rtr) => {
                    //println!("* read track raw *");
                    rtr
                }
                Err(e) => {
                    bail!("Error reading track: {}", e);
                }
            }
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

            match disk.read_track(ch, None) {
                Ok(rtr) => {
                    //println!("* read track *");
                    rtr
                }
                Err(e) => {
                    bail!("Error reading track: {}", e);
                }
            }
        };

        _ = fluxfox::util::dump_slice(&rtr.read_buf, 0, row_size, &mut buf);

        Ok(())
    }
}
