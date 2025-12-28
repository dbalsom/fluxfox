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

use crate::{pbm::Pbm, prng::Rng};

#[derive(Clone, Copy, Debug)]
pub enum YMode {
    Alternate,
    Centroid,
    Bottom,
    Top,
    Random,
}

impl std::str::FromStr for YMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "alternate" => Ok(YMode::Alternate),
            "centroid" => Ok(YMode::Centroid),
            "bottom" => Ok(YMode::Bottom),
            "top" => Ok(YMode::Top),
            "random" => Ok(YMode::Random),
            other => Err(format!(
                "unknown y-mode {other:?} (expected: alternate|centroid|bottom|top|random)"
            )),
        }
    }
}

// --------------------------------------
// Flux synthesis from image columns
// --------------------------------------

#[derive(Debug)]
pub enum SynthError {
    EmptyImage,
}

pub fn synthesize_flux_from_pbm(
    pbm: &Pbm,
    samples: usize,
    min_ft_seconds: f64,
    max_offset_seconds: f64,
    jitter_seconds: f64,
    rng: &mut Rng,
    y_mode: YMode,
) -> Result<Vec<f64>, SynthError> {
    if pbm.width == 0 || pbm.height == 0 {
        return Err(SynthError::EmptyImage);
    }

    let w = pbm.width;
    let h = pbm.height;
    let mut acc: f64 = 0.0;
    let mut flux: Vec<f64> = Vec::with_capacity(samples / 2);
    let mut multi_alt: usize = 0;

    for x in 0..samples {
        let sx = ((x as u128) * (w as u128) / (samples as u128)) as usize;
        let sx = sx.min(w - 1);

        let mut rows: Vec<usize> = Vec::with_capacity(h);
        for r_from_bottom in 0..h {
            let y = (h - 1) - r_from_bottom;
            if pbm.at(sx, y) {
                rows.push(r_from_bottom);
            }
        }

        if rows.is_empty() {
            acc += min_ft_seconds;
            continue;
        }

        // Choose vertical position -> fractional [0,1] from bottom to top
        let row_frac: f64 = match y_mode {
            YMode::Alternate => {
                let chosen = if rows.len() == 1 {
                    rows[0]
                }
                else {
                    let idx = multi_alt % rows.len();
                    multi_alt = multi_alt.wrapping_add(1);
                    rows[idx]
                };
                if h > 1 {
                    (chosen as f64) / ((h - 1) as f64)
                }
                else {
                    0.0
                }
            }
            YMode::Random => {
                let idx = (rng.next_u64() as usize) % rows.len();
                let chosen = rows[idx];
                if h > 1 {
                    (chosen as f64) / ((h - 1) as f64)
                }
                else {
                    0.0
                }
            }
            YMode::Centroid => {
                let sum: usize = rows.iter().copied().sum();
                let mean = (sum as f64) / (rows.len() as f64);
                if h > 1 {
                    mean / ((h - 1) as f64)
                }
                else {
                    0.0
                }
            }
            YMode::Bottom => {
                let m = *rows.iter().min().unwrap();
                if h > 1 {
                    (m as f64) / ((h - 1) as f64)
                }
                else {
                    0.0
                }
            }
            YMode::Top => {
                let m = *rows.iter().max().unwrap();
                if h > 1 {
                    (m as f64) / ((h - 1) as f64)
                }
                else {
                    0.0
                }
            }
        };

        let extra = row_frac * max_offset_seconds;
        let mut delta = acc + min_ft_seconds + extra;
        acc = 0.0;

        if jitter_seconds > 0.0 {
            let j = rng.uniform_range(-jitter_seconds, jitter_seconds);
            delta += j;
        }

        // Ensure delta is positive.
        if delta <= 0.0 {
            delta = 1e-9;
        }
        flux.push(delta);
    }
    Ok(flux)
}
