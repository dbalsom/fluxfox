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

    flux/histogram.rs
*/

//!     This module defines a FluxHistogram structure which is used to determine
//!     the density, data rate and encoding of a flux track so that the PLL may
//!     be properly initialized for decoding.
//!
//!     Normally this is used internally by FluxRevolution, but can be used
//!     independently by format parsers. For example the MFI format parser uses
//!     a FluxHistogram to normalize track timings when 360RPM images are
//!     detected, as to not clutter normal flux processing logic with special
//!     cases.

use histogram::{Bucket, Histogram};

pub struct FluxHistogram {
    histogram: Histogram,
    maxima:    Vec<(u64, std::ops::RangeInclusive<u64>)>,
}

impl FluxHistogram {
    /// Produce a [FluxHistogram] over a fraction of the flux deltas in the revolution.
    /// # Arguments
    /// * `deltas` - A slice of F values representing flux deltas times
    /// * `fraction` - The fraction of the deltas to use in the histogram
    pub fn new(&self, deltas: &[f64], fraction: f64) -> Self {
        // from docs:
        // grouping_power should be set such that 2^(-1 * grouping_power) is an acceptable relative error.
        // Rephrased, we can plug in the acceptable relative error into grouping_power = ceil(log2(1/e)).
        // For example, if we want to limit the error to 0.1% (0.001) we should set grouping_power = 7.

        // Max value power of 2^14 = 16384 (16us)
        // Grouping power of 3 produces sharp spikes without false maxima
        let mut histogram = Histogram::new(3, 14).unwrap();

        let take_count = (deltas.len() as f64 * fraction).round() as usize;
        log::debug!("FluxRevolution::histogram(): Taking {} flux deltas", take_count);
        for delta_ns in deltas.iter().take(take_count).map(|d| Self::delta_to_u64(*d)) {
            _ = histogram.increment(delta_ns);
        }

        FluxHistogram {
            histogram,
            maxima: Vec::new(),
        }
    }

    fn delta_to_u64(value: f64) -> u64 {
        (value * 1_000_000_000.0) as u64
    }

    fn u64_to_delta(value: u64) -> f64 {
        value as f64 / 1_000_000_000.0
    }

    /// Locate local maxima in a histogram by bucket.
    fn find_local_maxima(&mut self, threshold: Option<f64>) -> &Vec<(u64, std::ops::RangeInclusive<u64>)> {
        let mut peaks = vec![];
        let mut previous_bucket: Option<Bucket> = None;
        let mut current_bucket: Option<Bucket> = None;

        // Calculate total count for threshold
        let total_count: u64 = self.histogram.into_iter().map(|bucket| bucket.count()).sum();
        let threshold = (total_count as f64 * threshold.unwrap_or(0.005)).round() as u64;

        for bucket in self.histogram.into_iter() {
            if let (Some(prev), Some(curr)) = (previous_bucket.as_ref(), current_bucket.as_ref()) {
                // Identify local maximum and apply threshold check
                if curr.count() >= prev.count() && curr.count() > bucket.count() && curr.count() >= threshold {
                    peaks.push((curr.count(), curr.start()..=curr.end()));
                }
            }
            // Update previous and current buckets
            previous_bucket = current_bucket.take();
            current_bucket = Some(bucket.clone());
        }

        self.maxima = peaks;
        &self.maxima
    }

    /// Attempt to calculate the base (short) transition time.
    /// You must have called `find_local_maxima()` first.
    pub fn base_transition_time(&self) -> Option<f64> {
        if self.maxima.is_empty() {
            log::warn!("FluxHistogram::base_transition_time(): No peaks found. Did you call find_local_maxima?");
            return None;
        }

        if self.maxima.len() < 2 {
            log::warn!("FluxHistogram::base_transition_time(): Not enough peaks found");
            return None;
        }

        let first_peak = &self.maxima[0].1;
        let range_median = (first_peak.start() + first_peak.end()) / 2;

        // Convert back to seconds
        Some(Self::u64_to_delta(range_median))
    }

    pub(crate) fn print_debug(&self) {
        for peak in self.maxima.iter() {
            log::debug!(
                "FluxRevolution::histogram(): Peak at range: {:?} ct: {}",
                peak.1,
                peak.0
            );
        }
    }
}
