/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2026 Daniel Balsom

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

mod kfx;
mod pbm;
mod prng;
mod sampler;

use std::{fs, io::Write, path::Path};

use crate::{
    prng::Rng,
    sampler::{synthesize_flux_from_pbm, YMode},
};

use bpaf::Bpaf;
use pbm::Pbm;

use kfx::{DEFAULT_ICK_HZ, DEFAULT_SCK_HZ};
use regex::Regex;

#[derive(Clone, Debug, Bpaf)]
#[bpaf(options, version)]
/// Convert a PBM (P1/P4) into a KryoFlux raw-stream file (.kfs)
struct Cli {
    /// Total samples across one full track (e.g. 100000)
    #[bpaf(long("samples"), argument("N"))]
    samples: usize,

    /// Minimum flux transition duration, microseconds
    #[bpaf(long("min-ft"), argument("US"))]
    min_ft_us: f64,

    /// Maximum flux transition duration, microseconds
    #[bpaf(long("max-ft"), argument("US"))]
    max_ft_us: f64,

    /// Uniform random jitter added to EACH flux (±J µs)
    #[bpaf(long("jitter-us"), argument("US"))]
    jitter_us: Option<f64>,

    /// RNG seed (u64)
    #[bpaf(long("seed"), argument("S"), fallback(0x00C0_FFEEu64))]
    seed: u64,

    /// Vertical sampling: alternate|centroid|bottom|top
    #[bpaf(long("y-mode"), argument("MODE"), fallback(YMode::Alternate))]
    y_mode: YMode,

    /// Output KryoFlux raw-stream file
    #[bpaf(long("out"), argument("OUT"))]
    out: String,

    /// Sample clock (Hz)
    #[bpaf(long("sck-hz"), argument("HZ"), fallback(DEFAULT_SCK_HZ))]
    sck_hz: f64,

    /// Index clock (Hz)
    #[bpaf(long("ick-hz"), argument("HZ"), fallback(DEFAULT_ICK_HZ))]
    ick_hz: f64,

    /// Starting Index Counter (u32)
    #[bpaf(long("index-seed"), argument("SEED"), fallback(123_456_789u32))]
    index_seed: u32,

    /// Number of revolutions to export (repeat flux)
    #[bpaf(long("revs"), argument("N"), fallback(1usize))]
    revs: usize,

    /// KFInfo 'name'
    #[bpaf(long("kf-name"), argument("NAME"), fallback("FluxPainter".to_string()))]
    kf_name: String,

    /// KFInfo 'version'
    #[bpaf(long("kf-version"), argument("VER"), fallback("1.0".to_string()))]
    kf_version: String,

    /// PBM path (P1 ASCII or P4 binary)
    #[bpaf(positional("PBM"))]
    pbm_path: String,
}

fn main() {
    let cli = cli().run();

    let pbm = match Pbm::load(&cli.pbm_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load PBM: {e:?}");
            std::process::exit(3);
        }
    };

    let min_ft_seconds = cli.min_ft_us * 1e-6;
    let max_ft_seconds = cli.max_ft_us * 1e-6;
    let max_offset_seconds = max_ft_seconds - min_ft_seconds;

    if max_offset_seconds < 0.0 {
        eprintln!("Error: --max-ft must be >= --min-ft");
        std::process::exit(1);
    }

    let jitter_seconds = match cli.jitter_us {
        Some(us) => us * 1e-6,
        None => {
            if pbm.height > 0 {
                max_offset_seconds / (pbm.height as f64) / 2.0
            }
            else {
                0.0
            }
        }
    };

    let mut rng = Rng::new(cli.seed);

    let one_rev_flux = match synthesize_flux_from_pbm(
        &pbm,
        cli.samples,
        min_ft_seconds,
        max_offset_seconds,
        jitter_seconds,
        &mut rng,
        cli.y_mode,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Flux synthesis error: {e:?}");
            std::process::exit(4);
        }
    };

    // Create encoder
    let encoder = kfx::KfxEncoder::new(cli.sck_hz, cli.ick_hz);

    // Encode multi-revolution stream
    // I tried using one stream just book-ended with index markers, but some tools complained
    // Giving them three revolutions seems to appease them
    let bytes = match encoder.encode_multi_revs(&one_rev_flux, cli.revs) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Encode error: {e:?}");
            std::process::exit(5);
        }
    };

    // Validate output filename contains track/head like "00.0" before the final ".raw"
    // Ensure callers provide an output filename that includes the two-digit track and single-digit head
    // e.g. out00.0.raw — importers need the NN.H.raw format.
    if let Some(file_name) = Path::new(&cli.out).file_name().and_then(|s| s.to_str()) {
        let re = Regex::new(r"\d{2}\.\d\.raw$").expect("invalid regex");
        if !re.is_match(file_name) {
            eprintln!("Output filename '{}' does not contain a track/head spec like '00.0' before .raw. Please use a name like: nameNN.H.raw (e.g. out00.0.raw)", file_name);
            std::process::exit(2);
        }
    }
    else {
        eprintln!("Invalid output path: '{}'", cli.out);
        std::process::exit(2);
    }

    if let Err(e) = fs::File::create(Path::new(&cli.out)).and_then(|mut f| f.write_all(&bytes)) {
        eprintln!("Write error: {e}");
        std::process::exit(6);
    }

    println!("OK: wrote {} bytes, {} rev(s) to {}", bytes.len(), cli.revs, cli.out);

    let total_time_s: f64 = one_rev_flux.iter().sum();
    println!("Track length: {:.4} ms", total_time_s * 1000.0);
    println!("Total flux transitions: {}", one_rev_flux.len());
    println!("Jitter: {:.4} us", jitter_seconds * 1e6);
}
