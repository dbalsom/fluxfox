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
use crate::types::{DiskDataEncoding, DiskDensity};
use std::{
    fmt,
    fmt::{Display, Formatter},
};

pub mod flux_revolution;
#[macro_use]
pub mod pll;
mod histogram;

pub use flux_revolution::FluxRevolutionType;

//pub const AVERAGE_FLUX_DENSITY: f64 = 2.636; // Average number of bits encoded per flux transition

#[doc(hidden)]
#[macro_export]
macro_rules! format_us {
    ($value:expr) => {
        format!("{:.4}μs", $value * 1_000_000.0)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! format_ms {
    ($value:expr) => {
        format!("{:.4}ms", $value * 1_000.0)
    };
}

#[derive(PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FluxTransition {
    Short,
    Medium,
    Long,
    Other,
}

impl Display for FluxTransition {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            FluxTransition::Short => write!(f, "S"),
            FluxTransition::Medium => write!(f, "M"),
            FluxTransition::Long => write!(f, "L"),
            FluxTransition::Other => write!(f, "X"),
        }
    }
}

impl FluxTransition {
    #[allow(dead_code)]
    pub fn to_bits(&self) -> &[bool] {
        match self {
            FluxTransition::Short => &[true, false],
            FluxTransition::Medium => &[true, false, false],
            FluxTransition::Long => &[true, false, false, false],
            FluxTransition::Other => &[],
        }
    }
}

#[derive(Default)]
pub struct FluxStats {
    pub total: u32,
    pub short: u32,
    pub short_time: f64,
    pub medium: u32,
    pub long: u32,
    pub too_short: u32,
    pub too_long: u32,
    pub too_slow_bits: u32,

    pub shortest_flux: f64,
    pub longest_flux:  f64,
}

impl Display for FluxStats {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Total: {} S: {} M: {} L: {} Shortest: {} Longest: {} Too Short: {} Too Long: {}",
            self.total,
            self.short,
            self.medium,
            self.long,
            format_us!(self.shortest_flux),
            format_us!(self.longest_flux),
            self.too_short,
            self.too_long
        )
    }
}

impl FluxStats {
    pub fn detect_density(&self, mfi: bool) -> Option<DiskDensity> {
        let mut avg = self.short_avg();
        log::debug!(
            "FluxStats::detect_density(): Transition average: {:.4}",
            format_us!(avg)
        );

        if mfi {
            avg *= 2.0;
        }

        match avg {
            1.0e-6..=3e-6 => Some(DiskDensity::High),
            3e-6..=5e-6 => Some(DiskDensity::Double),
            _ => None,
        }
    }

    fn short_avg(&self) -> f64 {
        if self.short == 0 {
            0.0
        }
        else {
            self.short_time / self.short as f64
        }
    }
}

impl FluxStats {
    pub fn detect_encoding(&self) -> Option<DiskDataEncoding> {
        let medium_freq = self.medium as f64 / self.total as f64;

        // If we have fewer than 5% medium transitions, it is likely an FM track
        if medium_freq > 0.05 {
            Some(DiskDataEncoding::Mfm)
        }
        else {
            Some(DiskDataEncoding::Fm)
        }
    }
}
