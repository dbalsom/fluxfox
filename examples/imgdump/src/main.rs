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

    examples/imgdump/src/main.rs

    This is a simple example of how to use FluxFox to read a disk image and
    dump information from it, such a specified sector or track, in hex or binary
    format.
*/
use bpaf::*;
use fluxfox::{diskimage::RwSectorScope, DiskCh, DiskChs, DiskChsn, DiskChsnQuery, DiskImage};
use std::{
    io::{BufWriter, Cursor, Write},
    path::PathBuf,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Out {
    debug: bool,
    filename: PathBuf,
    cylinder: Option<u16>,
    phys_cylinder: Option<u16>,
    head: Option<u8>,
    phys_head: Option<u8>,
    sector: Option<u8>,
    n: Option<u8>,
    row_size: usize,
    structure: bool,
    find: Option<String>,
    dump_dupe_mark: bool,
    silent: bool,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let silent = long("silent")
        .help("Suppress all output except errors and requested data")
        .switch();

    let filename = short('i')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    let dump_dupe_mark = long("dump_dupe_mark").help("Dump Duplication mark if present").switch();

    let phys_cylinder = long("phys_c")
        .help("Physical cylinder")
        .argument::<u16>("PHYS_CYLINDER")
        .optional();

    let cylinder = short('c')
        .long("cylinder")
        .help("Target cylinder")
        .argument::<u16>("CYLINDER")
        .optional();

    let head = short('h')
        .long("head")
        .help("Target head")
        .argument::<u8>("HEAD")
        .optional();

    let phys_head = long("phys_h")
        .help("Physical cylinder")
        .argument::<u8>("PHYS_HEAD")
        .optional();

    let sector = short('s')
        .long("sector")
        .help("Target sector")
        .argument::<u8>("SECTOR")
        .optional();

    let n = short('n')
        .long("sector_size")
        .help("Sector size (override)")
        .argument::<u8>("SIZE")
        .optional();

    let row_size = short('r')
        .long("row_size")
        .help("Number of bytes per row")
        .argument::<usize>("ROWSIZE")
        .fallback(16);

    let structure = long("structure")
        .help("Dump IDAM header and data CRC in addition to data.")
        .switch();

    let find = long("find")
        .help("String to find")
        .argument::<String>("FIND_STRING")
        .optional();

    construct!(Out {
        debug,
        filename,
        cylinder,
        phys_cylinder,
        head,
        phys_head,
        sector,
        n,
        row_size,
        structure,
        find,
        dump_dupe_mark,
        silent
    })
    .to_options()
    .descr("imginfo: display info about disk image")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();
    let mut file_vec = match std::fs::read(&opts.filename.clone()) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };
    let mut cursor = Cursor::new(&mut file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut cursor) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            std::process::exit(1);
        }
    };

    if !opts.silent {
        println!("Detected disk image type: {}", disk_image_type);
    }

    let mut disk = match DiskImage::load(&mut cursor, Some(opts.filename.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    let handle = std::io::stdout();
    let mut buf = BufWriter::new(handle);

    if opts.dump_dupe_mark {
        if let Some((dupe_ch, dupe_chsn)) = disk.find_duplication_mark() {
            // let rsr = match disk.read_sector(dupe_ch, DiskChs::from(dupe_chsn), None, RwSectorScope::DataOnly, true) {
            //     Ok(rsr) => rsr,
            //     Err(e) => {
            //         eprintln!("Error reading sector: {}", e);
            //         std::process::exit(1);
            //     }
            // };

            let dump_string = match disk.dump_sector_string(dupe_ch, DiskChsnQuery::from(dupe_chsn), None, None) {
                Ok(dump_string) => dump_string,
                Err(e) => {
                    eprintln!("Error dumping sector: {}", e);
                    std::process::exit(1);
                }
            };

            //println!("Duplication mark found at {} with ID {}", dupe_ch, dupe_chsn);
            println!("{}", dump_string);
            std::process::exit(0);
        } else {
            println!("No duplication mark found.");
        }
    }

    if opts.cylinder.is_none() || opts.head.is_none() {
        eprintln!("Cylinder and head must be specified.");
        std::process::exit(1);
    }

    // Specify the physical cylinder and head. If these are not explicitly provided, we assume
    // that the physical cylinder and head are the same as the target cylinder and head.
    let mut phys_ch = DiskCh::new(opts.cylinder.unwrap(), opts.head.unwrap());
    if let Some(phys_cylinder) = opts.phys_cylinder {
        phys_ch.set_c(phys_cylinder);
    }

    if let Some(phys_head) = opts.phys_head {
        phys_ch.set_h(phys_head);
    }

    // If sector was provided, dump the sector.
    if let Some(sector) = opts.sector {
        // Dump the specified sector in hex format to stdout.

        let id_chs = DiskChs::new(opts.cylinder.unwrap(), opts.head.unwrap(), sector);

        let (scope, calc_crc) = match opts.structure {
            true => (RwSectorScope::DataElement, true),
            false => (RwSectorScope::DataOnly, false),
        };

        let rsr = match disk.read_sector(
            phys_ch,
            DiskChsnQuery::new(id_chs.c(), id_chs.h(), id_chs.s(), opts.n),
            opts.n,
            None,
            scope,
            true,
        ) {
            Ok(rsr) => rsr,
            Err(e) => {
                eprintln!("Error reading sector: {}", e);
                std::process::exit(1);
            }
        };

        _ = writeln!(&mut buf, "Data length: {}", rsr.data_len);

        let data_slice = match scope {
            RwSectorScope::DataOnly => &rsr.read_buf[rsr.data_idx..rsr.data_idx + rsr.data_len],
            RwSectorScope::DataElement => &rsr.read_buf,
        };

        if let Some(find_string) = &opts.find {
            let find_bytes = find_string.as_bytes();
            let mut found = false;
            for i in 0..data_slice.len() {
                if data_slice[i..].starts_with(find_bytes) {
                    _ = writeln!(&mut buf, "Found {} at offset {}", find_string, i);
                    found = true;
                    break;
                }
            }
            if !found {
                _ = writeln!(&mut buf, "Did not find search string.");
            }
        } else {
            println!(
                "Dumping sector from {} with id {} in hex format, with scope {:?}:",
                phys_ch,
                DiskChsn::from((id_chs, opts.n.unwrap_or(2))),
                scope
            );
            _ = fluxfox::util::dump_slice(data_slice, 0, opts.row_size, &mut buf);

            // If we requested DataBlock scope, we can independently calculate the CRC, so do that now.
            if calc_crc {
                let calculated_crc = fluxfox::util::crc_ibm_3740(&data_slice[0..0x104], None);
                _ = writeln!(&mut buf, "Calculated CRC: {:04X}", calculated_crc);
            }
        }
    } else {
        // No sector was provided, dump the whole track.

        let ch = DiskCh::new(opts.cylinder.unwrap(), opts.head.unwrap());

        println!("Dumping track {} in hex format:", ch);

        let rtr = match disk.read_track(ch, None) {
            Ok(rtr) => rtr,
            Err(e) => {
                eprintln!("Error reading track: {}", e);
                std::process::exit(1);
            }
        };

        _ = fluxfox::util::dump_slice(&rtr.read_buf, 0, opts.row_size, &mut buf);
    }
}
