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
    flux::{flux_revolution::FluxRevolution, FluxStats, FluxTransition},
    format_ms,
    format_us,
    DiskDataEncoding,
    DiskDataRate,
};
use bit_vec::BitVec;
use std::io::Write;

const BASE_CLOCK: f64 = 2e-6; // Represents the default clock for a 300RPM, 250Kbps disk.

const SHORT_TRANSITION: f64 = 4.0e-6; // 4 µs
const MEDIUM_TRANSITION: f64 = 6.0e-6; // 6 µs
const LONG_TRANSITION: f64 = 8.0e-6; // 8 µs
const TOLERANCE: f64 = 0.5e-6; // 0.5 µs Tolerance for time deviation

pub struct PllDecodeStatEntry {
    pub time: f64,
    pub len: f64,
    pub predicted: f64,
    pub clk: f64,
    pub window_min: f64,
    pub window_max: f64,
    pub phase_err: f64,
    pub phase_err_i: f64,
}

#[allow(dead_code)]
pub enum PllPreset {
    Aggressive,
    Conservative,
}

pub struct PllDecodeResult {
    pub transitions: Vec<FluxTransition>,
    pub bits: BitVec,
    pub flux_stats: FluxStats,
    pub pll_stats: Vec<PllDecodeStatEntry>,
    pub markers: Vec<usize>,
}

pub struct Pll {
    pub pll_default_rate: f64,
    pub pll_rate: f64,
    pub pll_period: f64,
    pub working_period: f64,
    pub period_factor: f64,
    pub max_adjust: f64,
    pub density_factor: f64,

    pub clock_gain: f64,
    pub phase_gain: f64,
}

impl Pll {
    pub fn new() -> Self {
        Pll {
            pll_default_rate: u32::from(DiskDataRate::Rate250Kbps(1.0)) as f64 * 2.0,
            pll_rate: u32::from(DiskDataRate::Rate250Kbps(1.0)) as f64 * 2.0,
            pll_period: BASE_CLOCK, // 2 µs
            working_period: BASE_CLOCK,
            period_factor: 1.0,
            max_adjust: 0.15, // 15%
            density_factor: 2.0,
            clock_gain: 0.05,
            phase_gain: 0.65,
        }
    }

    pub fn from_preset(preset: PllPreset) -> Pll {
        match preset {
            PllPreset::Aggressive => Pll::new(),
            PllPreset::Conservative => Pll::new(),
        }
    }

    pub fn set_clock(&mut self, rate: f64, max_adj: Option<f64>) {
        self.pll_default_rate = rate;
        self.pll_rate = rate;
        self.pll_period = 1.0 / rate;
        self.working_period = self.pll_period;
        if let Some(adj) = max_adj {
            self.max_adjust = adj;
        }
        assert!(self.pll_rate > 1.0);
        log::debug!(
            "Pll::set_clock(): Setting clock rate to {:.2}, max adjust: {:.2} new period: {}",
            self.pll_rate,
            self.max_adjust,
            format_us!(self.pll_period)
        );
    }

    pub fn get_clock(&mut self) -> f64 {
        self.pll_rate
    }

    pub fn reset_clock(&mut self) {
        self.pll_rate = self.pll_default_rate;
        self.pll_period = 1.0 / self.pll_rate;
        self.working_period = self.pll_period;
        log::debug!(
            "Pll::reset_clock(): Resetting clock to default rate: {} period: {}",
            self.pll_rate,
            format_us!(self.pll_period)
        );
    }

    pub fn adjust_clock(&mut self, factor: f64) {
        let old_rate = self.pll_rate;
        self.pll_rate *= factor;
        self.pll_period = 1.0 / self.pll_rate;
        self.working_period = self.pll_period;
        log::debug!(
            "Pll::adjust_clock(): Adjusting clock by {:.4}%  factor: {:.4} old: {:.4} new: {:.4} period: {}",
            factor * 100.0,
            factor,
            old_rate,
            self.pll_rate,
            format_us!(self.pll_period)
        );
    }

    #[allow(dead_code)]
    pub fn decode_transitions(&mut self, stream: &FluxRevolution) -> Vec<FluxTransition> {
        let mut transitions = Vec::new();
        let mut valid_deltas = 0;
        let mut delta_avg = 0.0;

        for t in stream.delta_iter() {
            if *t > 0.0 {
                delta_avg += *t;
                valid_deltas += 1;
            }

            let transition = self.classify_transition(*t);
            transitions.push(transition);
        }

        let delta_avg = delta_avg / valid_deltas as f64;

        let other_ct = transitions.iter().filter(|t| **t == FluxTransition::Other).count();
        log::warn!(
            "Pll::decode_transitions(): {} avg transition time, {} unclassified transitions",
            delta_avg,
            other_ct
        );

        transitions
    }

    #[allow(dead_code)]
    pub fn classify_transition(&self, duration: f64) -> FluxTransition {
        // log::trace!(
        //     "Pll::classify_transition(): Duration: {} short delta: {} medium delta: {} long delta: {}",
        //     duration,
        //     (duration - SHORT_TRANSITION).abs(),
        //     (duration - MEDIUM_TRANSITION).abs(),
        //     (duration - LONG_TRANSITION).abs()
        // );

        if (duration - SHORT_TRANSITION).abs() <= TOLERANCE {
            FluxTransition::Short
        }
        else if (duration - MEDIUM_TRANSITION).abs() <= TOLERANCE {
            FluxTransition::Medium
        }
        else if (duration - LONG_TRANSITION).abs() <= TOLERANCE {
            FluxTransition::Long
        }
        else {
            //log::trace!("unclassified duration: {}", duration);
            FluxTransition::Other
        }
    }

    pub fn decode(&mut self, stream: &FluxRevolution, encoding: DiskDataEncoding) -> PllDecodeResult {
        match encoding {
            DiskDataEncoding::Mfm => self.decode_mfm(stream),
            DiskDataEncoding::Fm => self.decode_fm(stream),
            _ => {
                log::error!("Unsupported encoding: {:?}", encoding);
                self.decode_mfm(stream)
            }
        }
    }

    fn decode_mfm(&mut self, stream: &FluxRevolution) -> PllDecodeResult {
        let mut output_bits = BitVec::with_capacity(stream.flux_deltas.len() * 3);
        let mut error_bits = BitVec::with_capacity(stream.flux_deltas.len() * 3);
        let mut pll_stats = Vec::with_capacity(stream.flux_deltas.len());
        let mut phase_error: f64 = 0.0;
        let mut phase_adjust: f64 = 0.0;

        let mut transitions = Vec::new();
        let mut this_flux_time;
        // The first entry of the flux stream represents a transition time, so we start off the track
        // at the first actual flux transition. We will assume that this transition is perfectly
        // aligned within the center of the clock period by adding half the period from the
        // start time.
        let mut time = self.pll_period / 2.0;
        let mut last_flux_time = 0.0;
        let mut clock_ticks: u64 = 0;
        let mut clock_ticks_since_flux: u64 = 0;
        let mut shift_reg: u64 = 0;
        let mut markers = Vec::new();
        let mut zero_ct = 0;

        let min_clock = self.working_period - (self.working_period * self.max_adjust);
        let max_clock = self.working_period + (self.working_period * self.max_adjust);
        self.working_period = self.pll_period;

        let p_term = self.pll_period * self.phase_gain;

        let mut flux_stats = FluxStats {
            total: stream.flux_deltas.len() as u32,
            ..FluxStats::default()
        };

        let mut last_bit = false;
        let mut adjust_gate: i32 = 0;

        // Each delta time represents the time in seconds between two flux transitions.
        for (flux_ct, &delta_time) in stream.delta_iter().enumerate() {
            flux_stats.shortest_flux = delta_time.min(flux_stats.shortest_flux);
            flux_stats.longest_flux = delta_time.max(flux_stats.longest_flux);

            if flux_ct == 0 {
                flux_stats.shortest_flux = delta_time;
                log::debug!(
                    "decode_mfm(): first flux transition: {} @({})",
                    format_us!(delta_time),
                    format_ms!(time)
                );
            }

            // Set the time of the next flux transition.
            this_flux_time = last_flux_time + delta_time;

            // log::warn!(
            //     "next flux in {} @ {} ({:.4} clocks @ {})",
            //     format_us!(delta_time),
            //     format_ms!(next_flux_time),
            //     delta_time / self.working_period,
            //     format_us!(self.working_period)
            // );

            // Tick the clock until we *pass* the time of the next flux transition.
            time += phase_adjust;
            while time < this_flux_time {
                time += self.working_period;
                clock_ticks_since_flux += 1;
                clock_ticks += 1;
                // log::debug!(
                //     "tick! time: {} pll_clock: {} phase_adj: {} next_d: {} next_t: {} clocks: {}",
                //     time,
                //     self.working_period,
                //     phase_adjust,
                //     format_us!(delta_time),
                //     next_flux_time,
                //     clock_ticks_since_flux
                // );
            }

            let flux_length = clock_ticks_since_flux;
            //log::trace!("decode_mfm(): flux length: {}", flux_length);

            // Emit 0's and 1's based on the number of clock ticks since last flux transition.
            if flux_length < 2 {
                //log::warn!("too fast flux: {} @({})", clock_ticks_since_flux, time);
                flux_stats.too_short += 1;
            }
            else if flux_length > 4 {
                log::trace!(
                    "decode_mfm(): Too slow flux detected: #{} @({}), dt: {}, clocks: {}",
                    flux_ct,
                    format_ms!(time),
                    delta_time,
                    clock_ticks_since_flux,
                );
                flux_stats.too_long += 1;
                flux_stats.too_slow_bits += (flux_length - 4) as u32;
            }

            match flux_length {
                2 => {
                    flux_stats.short_time += delta_time;
                    flux_stats.short += 1;
                    transitions.push(FluxTransition::Short);
                }
                3 => {
                    flux_stats.medium += 1;
                    transitions.push(FluxTransition::Medium);
                }

                4 => {
                    flux_stats.long += 1;
                    transitions.push(FluxTransition::Long);
                }
                _ => {}
            }

            if flux_length > 0 {
                for _ in 0..flux_length - 1 {
                    zero_ct += 1;
                    output_bits.push(false);
                    last_bit = false;
                    shift_reg <<= 1;
                    // More than 3 0's in a row is an MFM error.
                    error_bits.push(zero_ct > 3);
                }

                // Emit a 1 since we had a transition...
                zero_ct = 0;
                // Two 1's in a row is an MFM error.
                error_bits.push(last_bit);
                output_bits.push(true);
                last_bit = true;
                shift_reg <<= 1;
                shift_reg |= 1;
            }

            // Look for marker.
            if shift_reg & 0xFFFF_FFFF_FFFF_0000 == 0x4489_4489_4489_0000 {
                log::trace!(
                    "decode_mfm(): Marker detected at {}, bitcell: {}",
                    format_ms!(time),
                    flux_ct - 64
                );
                markers.push(output_bits.len() - 64);
            }

            if zero_ct > 16 {
                //log::warn!("decode_mfm(): NFA zone @ {}??", format_ms!(time));
            }

            // Transition should be somewhere within our last clock period, ideally in the center of it.
            // Let's calculate the error.
            // First, we calculate the predicted flux time. This is the time the transition should
            // have arrived assuming no clock deviation since last flux.

            //let predicted_flux_time = last_flux_time + (clock_ticks_since_flux as f64 * self.pll_clock);
            let predicted_flux_time =
                last_flux_time + phase_adjust + (clock_ticks_since_flux as f64 * self.working_period);

            // The error is the difference between the actual flux time and the predicted flux time.
            //let phase_error = next_flux_time - predicted_flux_time;

            let window_max = (time - this_flux_time) + delta_time;
            let window_min = window_max - self.working_period;
            let window_center = window_max - self.working_period / 2.0;

            let last_phase_error = phase_error;
            phase_error = delta_time - window_center;
            //phase_error = this_flux_time - (time - self.working_period / 2.0);

            if phase_error < 0.0 {
                // If delta is negative...
                if adjust_gate < 0 {
                    adjust_gate -= 1;
                }
                else {
                    adjust_gate = -1;
                }
            }
            else if phase_error >= 0.0 {
                // If delta is positive...
                if adjust_gate > 0 {
                    adjust_gate += 1;
                }
                else {
                    adjust_gate = 1;
                }
            }

            // We calculate the change in phase error between pairs of fluxes as the primary
            // driver of clock adjustment.  Phase error alone is a bad indicator that the clock
            // is wrong vs the window needing to be shifted.
            //
            // Consider the simplest case where we have a single flux off-center in one window.
            // Its position in the window tells us nothing about the clock rate.
            // If the next flux is a perfect 2us delta, it will be off by just as much.  If we use
            // phase offset alone, then we'll end up adjusting the clock when it shouldn't have been
            // adjusted.
            //
            // If the phase_error_delta remains low, the clock is accurate, and it is the phase that
            // needs to be adjusted. If phase_error_delta is high, we need to adjust the clock more.
            //
            let phase_delta_error = phase_error - last_phase_error;

            // The idea of taking the smallest magnitude phase error from the last two fluxes is that
            // if one flux is well-centered, we have more of a clock problem than a phase
            // problem. So we use the minimum phase error to adjust phase instead of directly.
            let min_phase_error = if phase_error.abs() < last_phase_error.abs() {
                phase_error
            }
            else {
                last_phase_error
            };

            pll_stats.push(PllDecodeStatEntry {
                time,
                len: delta_time,
                predicted: window_min + phase_adjust,
                clk: self.working_period,
                window_min,
                window_max,
                phase_err: phase_error,
                phase_err_i: phase_adjust,
            });

            // Validate that flux is within expected window. if these fail our logic is bad.
            // log::warn!(
            //     "window start: {} flux_time: {} window end: {}",
            //     format_us!(window_min),
            //     format_us!(delta_time),
            //     format_us!(window_max),
            // );

            //assert!(delta_time <= window_max);
            //assert!(delta_time >= window_min);

            //phase_adjust = min_phase_error;
            //phase_adjust = (phase_adjust + (0.65 * min_phase_error)) % (self.working_period / 2.0);
            phase_adjust = 0.65 * min_phase_error;

            if flux_ct == 0 {
                log::debug!(
                    "decode_mfm(): first phase error: {} @({:.9})",
                    format_us!(phase_error),
                    time
                );
            }

            // Calculate the proportional frequency adjustment.
            //let clk_adjust = (p_term * phase_error) / self.working_period;
            //let clk_adjust = 0.05 * phase_delta_error;

            let mut clk_adjust = 0.0;
            if adjust_gate.abs() > 1 {
                clk_adjust = 0.05 * phase_error;
            }
            //let clk_adjust = 0.075 * phase_error;

            // log::debug!(
            //     "flux time: {} window center: {} phase error: {} clk_adjust: {} phase_adjust: {}",
            //     next_flux_time,
            //     window_center,
            //     format_us!(phase_error),
            //     format_us!(clk_adjust),
            //     format_us!(phase_adjust),
            // );

            // Adjust the clock frequency, and clamp it to the min/max values.
            self.working_period += clk_adjust;
            self.working_period = self.working_period.clamp(min_clock, max_clock);

            // Save the last flux time for the next iteration.
            clock_ticks_since_flux = 0;
            last_flux_time = this_flux_time;
        }

        _ = std::io::stdout().flush();

        log::debug!(
            "decode_mfm(): Completed decoding of MFM flux stream. Total clocks: {} markers: {} FT stats: {}",
            clock_ticks,
            markers.len(),
            flux_stats
        );

        PllDecodeResult {
            transitions,
            bits: output_bits,
            flux_stats,
            pll_stats,
            markers,
        }
    }

    fn decode_fm(&mut self, stream: &FluxRevolution) -> PllDecodeResult {
        let mut output_bits = BitVec::with_capacity(stream.flux_deltas.len() * 3);
        let pll_stats = Vec::with_capacity(stream.flux_deltas.len());

        let mut phase_accumulator: f64 = 0.0;

        let mut last_flux_time = 0.0;
        let mut next_flux_time;
        // The first entry of the flux stream represents a transition time, so we start off the track
        // at the first actual flux transition. We will assume that this transition is perfectly
        // aligned within the center of the clock period by subtracting half the period from the
        // start time.
        self.working_period = self.pll_period * 2.0;
        let min_clock = self.working_period - (self.working_period * self.max_adjust);
        let max_clock = self.working_period + (self.working_period * self.max_adjust);

        let mut time = -self.working_period / 2.0;
        let mut clock_ticks: u64 = 0;
        let mut clock_ticks_since_flux: u64 = 0;

        let mut shift_reg: u64 = 0;
        let mut markers = Vec::new();

        log::debug!(
            "decode_fm(): normal period: {} working period: {} min: {} max: {}",
            format_us!(self.pll_period),
            format_us!(self.working_period),
            format_us!(min_clock),
            format_us!(max_clock)
        );

        let mut flux_stats = FluxStats {
            total: stream.flux_deltas.len() as u32,
            ..FluxStats::default()
        };

        // Each delta time represents the time in seconds between two flux transitions.
        for (flux_ct, &delta_time) in stream.delta_iter().enumerate() {
            flux_stats.shortest_flux = delta_time.min(flux_stats.shortest_flux);
            flux_stats.longest_flux = delta_time.max(flux_stats.longest_flux);

            // pll_stats.push(PllDecodeStatEntry {
            //     time,
            //     len: delta_time,
            //     clk: self.working_period,
            //     phase_err: phase_accumulator,
            // });

            if flux_ct == 0 {
                flux_stats.shortest_flux = delta_time;
                log::debug!("first flux transition: {} @({:.9})", format_us!(delta_time), time);
            }

            // Set the time of the next flux transition.
            next_flux_time = last_flux_time + delta_time;

            // log::debug!(
            //     "next flux in {} @ {} ({} clocks)",
            //     format_us!(delta_time),
            //     next_flux_time,
            //     delta_time / self.working_period
            // );

            // Tick the clock until we *pass* the time of the next flux transition.
            while (time + phase_accumulator) < next_flux_time {
                time += self.working_period;
                clock_ticks_since_flux += 1;
                clock_ticks += 1;
                // log::debug!(
                //     "tick! time: {} pll_clock: {} next_d: {} next_t: {} clocks: {}",
                //     time,
                //     self.working_period,
                //     format_us!(delta_time),
                //     next_flux_time,
                //     clock_ticks_since_flux
                // );
            }

            time += phase_accumulator;
            phase_accumulator = 0.0;

            let flux_length = clock_ticks_since_flux;
            log::trace!("flux length: {}", flux_length);

            match flux_length {
                0 => {
                    flux_stats.too_short += 1;
                }
                1 => {
                    flux_stats.short_time += delta_time;
                    flux_stats.short += 1;
                    //print!("S");
                }
                2 => {
                    flux_stats.long += 1;
                    //print!("L");
                }
                _ => {
                    flux_stats.too_long += 1;
                    flux_stats.too_slow_bits += (flux_length - 4) as u32;
                    //print!("X");
                }
            }

            // Emit 0's and 1's based on the number of clock ticks since last flux transition.
            if flux_length == 0 {
                //log::error!("zero length flux detected at time: {}", time);
            }
            else {
                for _ in 0..flux_length.saturating_sub(1) {
                    output_bits.push(false);
                    shift_reg <<= 1;
                }
                // Emit a 1 since we had a transition...
                output_bits.push(true);
                shift_reg <<= 1;
                shift_reg |= 1;
            }

            // Look for FM marker.
            if shift_reg & 0xAAAA_AAAA_AAAA_AAAA == 0xAAAA_AAAA_AAAA_A02A {
                log::debug!(
                    "decode_fm(): Marker detected at {}, bitcell: {}",
                    format_ms!(time),
                    flux_ct - 16
                );
                markers.push(output_bits.len() - 16);
            }

            // Transition should be somewhere within our last clock period, ideally in the center of it.
            // Let's calculate the error.
            // First, we calculate the predicted flux time. This is the time the transition should
            // have arrived assuming no clock deviation since last flux.

            //let predicted_flux_time = last_flux_time + (clock_ticks_since_flux as f64 * self.pll_clock);
            let predicted_flux_time = last_flux_time + (clock_ticks_since_flux as f64 * self.working_period);

            // The error is the difference between the actual flux time and the predicted flux time.
            let phase_error = next_flux_time - predicted_flux_time;

            // Calculate the proportional frequency adjustment.
            let p_term = (self.phase_gain * phase_error) / self.working_period;

            // log::debug!(
            //     "predicted time: {} phase error: {} p_accum: {} kp: {} p_term: {}",
            //     predicted_flux_time,
            //     format_us!(phase_error),
            //     format_us!(phase_accumulator),
            //     self.kp,
            //     p_term
            // );

            // Adjust the clock frequency, and clamp it to the min/max values.
            self.working_period += p_term;
            self.working_period = self.working_period.clamp(min_clock, max_clock);

            // Adjust the phase of the clock by shifting the time variable.
            phase_accumulator += phase_error;
            if phase_accumulator.abs() > self.working_period {
                phase_accumulator %= self.working_period;
            }
            // Save the last flux time for the next iteration.
            clock_ticks_since_flux = 0;
            last_flux_time = next_flux_time;
        }

        log::debug!(
            "Completed decoding of FM flux stream. Total clocks: {} FT stats: {}",
            clock_ticks,
            flux_stats
        );

        PllDecodeResult {
            transitions: Vec::new(),
            bits: output_bits,
            flux_stats,
            pll_stats,
            markers,
        }
    }
}
