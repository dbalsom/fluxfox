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
use crate::{
    flux::{
        pll::{Pll, PllDecodeStatEntry},
        FluxStats,
        FluxTransition,
    },
    DiskCh,
    DiskDataEncoding,
};
use bit_vec::BitVec;
use histogram::{Bucket, Histogram};
use std::cmp::Ordering;

/// Type of revolution.
/// `Source` is a direct read from the disk image.
/// `Synthetic` is a generated revolution, usually shifting a flux from one source revolution
///             to another.
#[derive(Copy, Clone, Debug)]
pub enum FluxRevolutionType {
    Source,
    Synthetic,
}

/// A struct containing statistics about a flux revolution.
pub struct FluxRevolutionStats {
    /// The type of revolution.
    pub rev_type: FluxRevolutionType,
    /// The data encoding detected for the revolution.
    pub encoding: DiskDataEncoding,
    /// The data rate of the revolution in bits per second.
    pub data_rate: f64,
    /// The time taken to read the revolution in seconds.
    pub index_time: f64,
    /// The number of flux transitions in the revolution.
    pub ft_ct: usize,
    /// The number of bits decoded from the revolution.
    pub bitcell_ct: usize,
    /// The duration of the first flux transition in the revolution.
    pub first_ft: f64,
    /// The duration of the last flux transition in the revolution.
    pub last_ft: f64,
}

/// A struct representing one revolution of a fluxstream track.
pub struct FluxRevolution {
    /// The type of revolution.
    pub rev_type: FluxRevolutionType,
    /// The physical cylinder and head of the revolution.
    pub ch: DiskCh,
    /// The data rate of the revolution in bits per second, or None if not determined.
    pub data_rate: Option<f64>,
    /// The time taken to read the revolution in seconds.
    pub index_time: f64,
    /// The list of times between flux transitions, in seconds.
    pub flux_deltas: Vec<f64>,
    /// The list of transitions decoded from the flux deltas as `FluxTransition` enums.
    pub transitions: Vec<FluxTransition>,
    /// The bitstream decoded from the flux deltas.
    pub bitstream: BitVec,
    /// The bit errors found in the bitstream.
    pub biterrors: BitVec,
    /// The data encoding detected for the revolution.
    pub encoding: DiskDataEncoding,
    /// Statistics from the PLL decoding process.
    pub pll_stats: Vec<PllDecodeStatEntry>,
}

impl FluxRevolution {
    /// Retrieve the data encoding detected for the revolution.
    pub fn encoding(&self) -> DiskDataEncoding {
        self.encoding
    }

    /// Retrieve statistics about a decoded revolution.
    pub fn stats(&self) -> FluxRevolutionStats {
        let computed_data_rate = self.bitstream.len() as f64 * (1.0 / self.index_time);
        FluxRevolutionStats {
            rev_type: self.rev_type,
            encoding: self.encoding,
            data_rate: self.data_rate.unwrap_or(computed_data_rate),
            index_time: self.index_time,
            ft_ct: self.flux_deltas.len(),
            bitcell_ct: self.bitstream.len(),
            first_ft: *self.flux_deltas.first().unwrap_or(&0.0),
            last_ft: *self.flux_deltas.last().unwrap_or(&0.0),
        }
    }

    /// Create a new `FluxRevolution` from a list of durations between flux transitions in seconds.
    pub fn from_f64(ch: DiskCh, deltas: &[f64], index_time: f64) -> Self {
        FluxRevolution {
            rev_type: FluxRevolutionType::Source,
            ch,
            data_rate: None,
            index_time,
            flux_deltas: deltas.to_vec(),
            transitions: Vec::with_capacity(deltas.len()),
            bitstream: BitVec::with_capacity(deltas.len() * 3),
            biterrors: BitVec::with_capacity(deltas.len() * 3),
            encoding: DiskDataEncoding::Mfm,
            pll_stats: Vec::new(),
        }
    }

    /// Create a new `FluxRevolution` from a list of durations between flux transitions, given
    /// in integer ticks of the provided clock period `timebase`.
    pub fn from_u16(ch: DiskCh, data: &[u16], index_time: f64, timebase: f64) -> Self {
        log::debug!("FluxRevolution::from_u16(): Using timebase of {:.3}ns", timebase * 1e9);
        let mut new = FluxRevolution {
            rev_type: FluxRevolutionType::Source,
            ch,
            data_rate: None,
            index_time,
            flux_deltas: Vec::with_capacity(data.len()),
            transitions: Vec::with_capacity(data.len()),
            bitstream: BitVec::with_capacity(data.len() * 3),
            biterrors: BitVec::with_capacity(data.len() * 3),
            encoding: DiskDataEncoding::Mfm,
            pll_stats: Vec::new(),
        };
        let mut nfa_count = 0;
        for cell in data {
            if *cell == 0 {
                nfa_count += 1;
                continue;
            }

            // Convert to float seconds
            let seconds = *cell as f64 * timebase;
            new.flux_deltas.push(seconds);
        }

        log::warn!("FluxRevolution::from_u16(): {} NFA cells found", nfa_count);
        new
    }

    /// Create new synthetic `FluxRevolution`s from a pair of adjacent revolutions.
    /// Fluxes are shifted from one revolution to another to correct for index jitter.
    pub(crate) fn from_adjacent_pair(first: &FluxRevolution, second: &FluxRevolution) -> Vec<FluxRevolution> {
        let mut new_revolutions = Vec::new();

        let flux_ct_diff = (first.flux_deltas.len() as i64 - second.flux_deltas.len() as i64).abs();

        match first.flux_deltas.len().cmp(&second.flux_deltas.len()) {
            Ordering::Greater if flux_ct_diff == 2 => {
                log::debug!(
                    "FluxRevolution::from_adjacent_pair(): First revolution is candidate for flux shift to second."
                );

                let mut first_deltas = first.flux_deltas.clone();
                let shift_delta = first_deltas.pop();

                let mut second_deltas = second.flux_deltas.clone();
                second_deltas.insert(0, shift_delta.unwrap());

                let new_first = FluxRevolution {
                    rev_type: FluxRevolutionType::Synthetic,
                    ch: first.ch,
                    data_rate: first.data_rate,
                    index_time: first.index_time,
                    transitions: Vec::with_capacity(first_deltas.len()),
                    flux_deltas: first_deltas,
                    bitstream: BitVec::with_capacity(first.bitstream.capacity()),
                    biterrors: BitVec::with_capacity(first.bitstream.capacity()),
                    encoding: DiskDataEncoding::Mfm,
                    pll_stats: Vec::new(),
                };

                let new_second = FluxRevolution {
                    rev_type: FluxRevolutionType::Synthetic,
                    ch: second.ch,
                    data_rate: second.data_rate,
                    index_time: second.index_time,
                    transitions: Vec::with_capacity(second_deltas.len()),
                    flux_deltas: second_deltas,
                    bitstream: BitVec::with_capacity(second.bitstream.capacity()),
                    biterrors: BitVec::with_capacity(second.bitstream.capacity()),
                    encoding: DiskDataEncoding::Mfm,
                    pll_stats: Vec::new(),
                };

                new_revolutions.push(new_first);
                new_revolutions.push(new_second);
            }
            Ordering::Less if flux_ct_diff == 2 => {
                log::debug!(
                    "FluxRevolution::from_adjacent_pair(): Second revolution is candidate for flux shift to first."
                );

                let mut first_deltas = first.flux_deltas.clone();
                let mut second_deltas = second.flux_deltas.clone();

                let shift_delta = second_deltas.remove(0);
                first_deltas.push(shift_delta);

                let new_first = FluxRevolution {
                    rev_type: FluxRevolutionType::Synthetic,
                    ch: first.ch,
                    data_rate: first.data_rate,
                    index_time: first.index_time,
                    transitions: Vec::with_capacity(first_deltas.len()),
                    flux_deltas: first_deltas,
                    bitstream: BitVec::with_capacity(first.bitstream.capacity()),
                    biterrors: BitVec::with_capacity(first.bitstream.capacity()),
                    encoding: DiskDataEncoding::Mfm,
                    pll_stats: Vec::new(),
                };

                let new_second = FluxRevolution {
                    rev_type: FluxRevolutionType::Synthetic,
                    ch: second.ch,
                    data_rate: second.data_rate,
                    index_time: second.index_time,
                    transitions: Vec::with_capacity(second_deltas.len()),
                    flux_deltas: second_deltas,
                    bitstream: BitVec::with_capacity(second.bitstream.capacity()),
                    biterrors: BitVec::with_capacity(second.bitstream.capacity()),
                    encoding: DiskDataEncoding::Mfm,
                    pll_stats: Vec::new(),
                };

                new_revolutions.push(new_first);
                new_revolutions.push(new_second);
            }
            _ => {}
        }

        new_revolutions
    }

    /// Retrieve the number of flux transitions in this revolution.
    pub(crate) fn ft_ct(&self) -> usize {
        self.flux_deltas.len()
    }

    /// Retrieve the vector of `PllDecodeStatEntry` structs from the PLL decoding process.
    #[allow(dead_code)]
    pub(crate) fn pll_stats(&self) -> &Vec<PllDecodeStatEntry> {
        &self.pll_stats
    }

    /// Locate local maxima in a histogram by bucket.
    fn find_local_maxima(hist: &Histogram) -> Vec<(u64, std::ops::RangeInclusive<u64>)> {
        let mut peaks = vec![];
        let mut previous_bucket: Option<Bucket> = None;
        let mut current_bucket: Option<Bucket> = None;

        // Calculate total count for threshold
        let total_count: u64 = hist.into_iter().map(|bucket| bucket.count()).sum();
        let threshold = (total_count as f64 * 0.005).round() as u64;

        for bucket in hist.into_iter() {
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

        peaks
    }

    /// Attempt to calculate the base (short) transition time from a histogram.
    pub fn base_transition_time(&self, hist: &Histogram) -> Option<f64> {
        let peaks = Self::find_local_maxima(hist);

        if peaks.len() < 2 {
            log::warn!("FluxRevolution::base_transition_time(): Not enough peaks found");
            return None;
        }

        let first_peak = &peaks[0].1;

        let range_median = (first_peak.start() + first_peak.end()) / 2;
        // Convert back to seconds
        Some(range_median as f64 / 1_000_000_000.0)
    }

    /// Produce a Histogram over a percentage of the flux deltas in the revolution.
    pub fn histogram(&self, percent: f32) -> Histogram {
        // from docs:
        // grouping_power should be set such that 2^(-1 * grouping_power) is an acceptable relative error.
        // Rephrased, we can plug in the acceptable relative error into grouping_power = ceil(log2(1/e)).
        // For example, if we want to limit the error to 0.1% (0.001) we should set grouping_power = 7.

        // Max value power of 2^14 = 16384 (16us)
        // Grouping power of 3 produces sharp spikes without false maxima
        let mut hist = Histogram::new(3, 14).unwrap();

        let take_count = (self.flux_deltas.len() as f32 * percent).round() as usize;
        log::debug!("FluxRevolution::histogram(): Taking {} flux deltas", take_count);
        for delta_ns in self
            .flux_deltas
            .iter()
            .take(take_count)
            .map(|d| (*d * 1_000_000_000.0) as u64)
        {
            _ = hist.increment(delta_ns);
        }

        let peaks = Self::find_local_maxima(&hist);

        for peak in peaks {
            log::debug!(
                "FluxRevolution::histogram(): Peak at range: {:?} ct: {}",
                peak.1,
                peak.0
            );
        }

        //Self::print_horizontal_histogram_with_labels(&hist, 16);
        hist
    }

    /// Debugging function to print a histogram in ASCII.
    #[allow(dead_code)]
    fn print_horizontal_histogram_with_labels(hist: &Histogram, height: usize) {
        let mut max_count = 0;
        let mut buckets = vec![];

        // Step 1: Collect buckets and find max count for scaling
        for bucket in hist.into_iter() {
            max_count = max_count.max(bucket.count());
            buckets.push(bucket);
        }

        // Step 2: Initialize 2D array for histogram, filled with spaces
        let width = buckets.len();
        let mut graph = vec![vec![' '; width]; height];

        // Step 3: Plot each bucket count as a column of asterisks
        for (i, bucket) in buckets.iter().enumerate() {
            let bar_height = if max_count > 0 {
                (bucket.count() as f64 / max_count as f64 * height as f64).round() as usize
            }
            else {
                0
            };
            for row in (height - bar_height)..height {
                graph[row][i] = '*';
            }
        }

        // Step 4: Print the graph row by row
        for row in &graph {
            println!("{}", row.iter().collect::<String>());
        }

        // Step 5: Print bucket start values vertically
        let max_label_len = buckets.iter().map(|b| b.start().to_string().len()).max().unwrap_or(0);
        for i in 0..max_label_len {
            let row: String = buckets
                .iter()
                .map(|b| {
                    let label = b.start().to_string();
                    label.chars().nth(i).unwrap_or(' ')
                })
                .collect();
            println!("{}", row);
        }
    }

    pub fn transition_ct(&self) -> usize {
        self.transitions.len()
    }

    /// Retrieve the average time between flux transitions in seconds for the entire revolution.
    /// Note: this value is probably not reliable for determining any specific heuristics.
    pub fn transition_avg(&self) -> f64 {
        let mut t_sum = 0.0;
        let mut t_ct = 0;
        for t in self.flux_deltas.iter() {
            if *t > 0.0 {
                t_ct += 1;
                t_sum += *t;
            }
        }
        t_sum / t_ct as f64
    }

    pub fn bitstream_data(&self) -> (Vec<u8>, usize) {
        (self.bitstream.to_bytes(), self.bitstream.len())
    }

    pub fn decode(&mut self, pll: &mut Pll) {
        self.transitions = pll.decode_transitions(self);
        //self.decode_bitstream();
        log::trace!(
            "FluxRevolution::decode(): Decoded {} transitions into {} bits, ratio: {}",
            self.transitions.len(),
            self.bitstream.len(),
            self.bitstream.len() as f64 / self.transitions.len() as f64
        );
    }

    pub fn decode_direct(&mut self, pll: &mut Pll) -> FluxStats {
        let mut decode_result = pll.decode(self, DiskDataEncoding::Mfm);
        let encoding = decode_result
            .flux_stats
            .detect_encoding()
            .unwrap_or(DiskDataEncoding::Mfm);

        if decode_result.markers.is_empty() && matches!(encoding, DiskDataEncoding::Fm) {
            // If we detected FM encoding, decode again as FM
            log::warn!("FluxRevolution::decode(): No markers found. Track might be FM encoded? Re-decoding...");

            let fm_result = pll.decode(self, DiskDataEncoding::Fm);
            if fm_result.markers.is_empty() {
                log::warn!("FluxRevolution::decode(): No markers found in FM decode. Keeping MFM.");
                self.encoding = DiskDataEncoding::Mfm;
            }
            else {
                log::debug!("FluxRevolution::decode(): Found FM marker! Setting track to FM encoding.");
                self.encoding = DiskDataEncoding::Fm;
                decode_result = fm_result;
            }
        }

        self.bitstream = decode_result.bits;

        log::trace!(
            "FluxRevolution::decode(): Decoded {} transitions into {} bits with {} encoding, ratio: {}",
            self.flux_deltas.len(),
            self.bitstream.len(),
            self.encoding,
            self.bitstream.len() as f64 / self.flux_deltas.len() as f64
        );

        self.data_rate = Some(self.bitstream.len() as f64 * (1.0 / self.index_time) / 2.0);
        self.pll_stats = decode_result.pll_stats;
        decode_result.flux_stats
    }

    /// Create an iterator over the flux delta times in a revolution.
    pub fn delta_iter(&self) -> std::slice::Iter<f64> {
        self.flux_deltas.iter()
    }
}
