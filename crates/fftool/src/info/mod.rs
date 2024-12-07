/*
    fftool
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
use crate::{args::GlobalOptions, read_file};
use anyhow::{bail, Error};
use fluxfox::{flux::FluxRevolutionType, prelude::*};

pub mod args;

pub(crate) fn run(_global: &GlobalOptions, params: &args::InfoParams) -> Result<(), Error> {
    let mut reader = read_file(&params.in_file)?;

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            bail!("Error detecting disk image type: {}", e);
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader, Some(params.in_file.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            bail!("Error loading disk image: {}", e);
        }
    };

    println!("Disk image info:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_info(&mut std::io::stdout());
    println!();

    println!("Disk consistency report:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_consistency(&mut std::io::stdout());

    println!("Image can be represented by the following formats with write support:");
    let formats = disk.compatible_formats(true);
    for format in formats {
        println!("  {} [{}]", format.0, format.1.join(", "));
    }

    if let Some(bootsector) = disk.boot_sector() {
        println!("Disk has a boot sector");
        if bootsector.has_valid_bpb() {
            println!("Boot sector with valid BPB detected:");
            println!("{}", "-".repeat(79));
            let _ = bootsector.dump_bpb(&mut std::io::stdout());
        }
        else {
            println!("Boot sector has an invalid BPB");
        }
    }
    println!();

    if params.track_list || params.sector_list || params.rev_list {
        let _ = dump_track_map(&mut std::io::stdout(), &disk, params.sector_list, params.rev_list);
    }

    Ok(())
}

pub fn dump_track_map<W: std::io::Write>(
    mut out: W,
    disk: &DiskImage,
    sectors: bool,
    revolutions: bool,
) -> Result<(), Error> {
    let head_map = disk.get_sector_map();

    for (head_idx, head) in head_map.iter().enumerate() {
        out.write_fmt(format_args!("Head {} [{} tracks]\n", head_idx, head.len()))?;
        for (track_idx, track) in head.iter().enumerate() {
            let ch = DiskCh::new(track_idx as u16, head_idx as u8);

            if let Some(track_ref) = disk.track(ch) {
                match track_ref.resolution() {
                    DiskDataResolution::MetaSector => {
                        out.write_fmt(format_args!("\tTrack {}\n", track_idx))?;
                    }
                    DiskDataResolution::FluxStream | DiskDataResolution::BitStream => {
                        let stream = track_ref.track_stream().expect("Couldn't retrieve track stream!");
                        out.write_fmt(format_args!(
                            "\tTrack {}: [{} encoding, {} bits]\n",
                            track_idx,
                            track_ref.encoding(),
                            stream.len()
                        ))?;
                    }
                }

                if revolutions {
                    if let Some(flux_track) = track_ref.as_fluxstream_track() {
                        let source_ct = flux_track
                            .revolution_iter()
                            .filter(|r| matches!(r.stats().rev_type, FluxRevolutionType::Source))
                            .count();

                        out.write_fmt(format_args!("\t\tSource Revolutions ({}):\n", source_ct))?;
                        for revolution in flux_track
                            .revolution_iter()
                            .filter(|r| matches!(r.stats().rev_type, FluxRevolutionType::Source))
                        {
                            let rev_stats = revolution.stats();
                            out.write_fmt(format_args!(
                                "\t\t\tFlux ct: {} Bitcells: {} First ft: {:.4} Last ft: {:.4}\n",
                                rev_stats.ft_ct,
                                rev_stats.bitcell_ct,
                                rev_stats.first_ft * 1e6,
                                rev_stats.last_ft * 1e6
                            ))?;
                        }

                        let synthetic_count = flux_track.revolution_ct() - source_ct;

                        if synthetic_count > 0 {
                            out.write_fmt(format_args!(
                                "\t\tSynthetic Revolutions ({}):\n",
                                flux_track.revolution_ct() - source_ct
                            ))?;
                            for revolution in flux_track
                                .revolution_iter()
                                .filter(|r| matches!(r.stats().rev_type, FluxRevolutionType::Synthetic))
                            {
                                let rev_stats = revolution.stats();
                                out.write_fmt(format_args!(
                                    "\t\t\tFlux ct: {} Bitcells: {} First ft: {:.4} Last ft: {:.4}\n",
                                    rev_stats.ft_ct,
                                    rev_stats.bitcell_ct,
                                    rev_stats.first_ft * 1e6,
                                    rev_stats.last_ft * 1e6
                                ))?;
                            }
                        }
                    }
                }

                if sectors {
                    out.write_fmt(format_args!("\t\tSectors ({}):\n", track.len()))?;
                    for sector in track {
                        out.write_fmt(format_args!(
                            "\t\t\t{} address_crc_valid: {} data_crc_valid: {} deleted: {}\n",
                            sector.chsn,
                            sector.attributes.address_crc_valid,
                            sector.attributes.data_crc_valid,
                            sector.attributes.deleted_mark
                        ))?;
                    }
                }
            }
        }
    }

    Ok(())
}
