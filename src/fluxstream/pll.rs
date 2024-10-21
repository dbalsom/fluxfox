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
use crate::fluxstream::flux_stream::RawFluxStream;
use crate::fluxstream::FluxTransition;
use crate::DiskDataRate;
use bit_vec::BitVec;

const BASE_CLOCK: f64 = 2e-6; // Represents the default clock for a 300RPM, 250Kbps disk.

const SHORT_TRANSITION: f64 = 4.0e-6; // 4 µs
const MEDIUM_TRANSITION: f64 = 6.0e-6; // 6 µs
const LONG_TRANSITION: f64 = 8.0e-6; // 8 µs
const TOLERANCE: f64 = 0.5e-6; // 0.5 µs Tolerance for time deviation

pub enum PllPreset {
    Aggressive,
    Conservative,
}

macro_rules! format_us {
    ($value:expr) => {
        format!("{:.2}μs", $value * 1_000_000.0)
    };
}

pub struct Pll {
    pub pll_default_rate: f64,
    pub pll_rate: f64,
    pub pll_period: f64,
    pub max_adjust: f64,
    pub density_factor: f64,
}

impl Pll {
    pub fn new() -> Self {
        Pll {
            pll_default_rate: u32::from(DiskDataRate::Rate250Kbps) as f64 * 2.0,
            pll_rate: u32::from(DiskDataRate::Rate250Kbps) as f64 * 2.0,
            pll_period: BASE_CLOCK, // 2 µs
            max_adjust: 0.15,       // 15%
            density_factor: 2.0,
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
        if let Some(adj) = max_adj {
            self.max_adjust = adj;
        }
        assert!(self.pll_rate > 1.0);
        log::trace!(
            "Pll::set_clock(): Setting clock rate to {}, max adjust: {:?} new period: {}",
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
        log::debug!(
            "Pll::adjust_clock(): Adjusting clock by {}%  factor: {} old: {} new: {} period: {}",
            factor * 100.0,
            factor,
            old_rate,
            self.pll_rate,
            format_us!(self.pll_period)
        );
    }

    #[allow(dead_code)]
    pub fn decode_transitions(&mut self, stream: &RawFluxStream) -> Vec<FluxTransition> {
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
        } else if (duration - MEDIUM_TRANSITION).abs() <= TOLERANCE {
            FluxTransition::Medium
        } else if (duration - LONG_TRANSITION).abs() <= TOLERANCE {
            FluxTransition::Long
        } else {
            //log::trace!("unclassified duration: {}", duration);
            FluxTransition::Other
        }
    }

    pub fn decode(&mut self, stream: &RawFluxStream) -> BitVec {
        let mut output_bits = BitVec::new();

        let kp = 0.05; // Proportional gain factor. This multiplied with the phase error,
                       // to dampen clock phase adjustments.

        let min_clock = self.pll_period - (self.pll_period * self.max_adjust);
        let max_clock = self.pll_period + (self.pll_period * self.max_adjust);

        let mut phase_accumulator: f64 = 0.0;

        let mut last_flux_time = 0.0;
        let mut next_flux_time;
        // The first entry of the flux stream represents a transition time, so we start off the track
        // at the first actual flux transition. We will assume that this transition is perfectly
        // aligned within the center of the clock period by subtracting half the period from the
        // start time.
        let mut time = -self.pll_period / 2.0;
        let mut clock_ticks: u64 = 0;
        let mut clock_ticks_since_flux: u64 = 0;

        let mut flux_ct = 0;
        let mut too_fast_flux = 0;
        let mut too_slow_flux = 0;

        // Each delta time represents the time in seconds between two flux transitions.
        for &delta_time in stream.delta_iter() {
            if flux_ct == 0 {
                log::warn!("first flux transition: {} @({:.9})", format_us!(delta_time), time);
            }

            flux_ct += 1;
            // Set the time of the next flux transition.
            next_flux_time = last_flux_time + delta_time;

            log::trace!(
                "next flux in {} @ {} ({} clocks)",
                format_us!(delta_time),
                next_flux_time,
                delta_time / self.pll_period
            );

            // Tick the clock until we *pass* the time of the next flux transition.
            while (time + phase_accumulator) < next_flux_time {
                time += self.pll_period;
                clock_ticks_since_flux += 1;
                clock_ticks += 1;
                log::trace!(
                    "tick! time: {} pll_clock: {} next: {} clocks: {}",
                    time,
                    self.pll_period,
                    next_flux_time,
                    clock_ticks_since_flux
                );
            }

            time += phase_accumulator;
            phase_accumulator = 0.0;

            let flux_length = clock_ticks_since_flux;
            log::trace!("flux length: {}", flux_length);

            // Emit 0's and 1's based on the number of clock ticks since last flux transition.
            if flux_length < 2 {
                //log::warn!("too fast flux: {} @({})", clock_ticks_since_flux, time);
                too_fast_flux += 1;
            } else if flux_length < 5 {
                let zeros = flux_length - 1;
                for _ in 0..zeros {
                    output_bits.push(false);
                }
                output_bits.push(true);
            } else {
                //log::warn!("too slow flux: {} @({})", clock_ticks_since_flux, time);
                too_slow_flux += 1;
            }

            // Transition should be somewhere within our last clock period, ideally in the center of it.
            // Let's calculate the error.
            // First, we calculate the predicted flux time. This is the time the transition should
            // have arrived assuming no clock deviation since last flux.

            //let predicted_flux_time = last_flux_time + (clock_ticks_since_flux as f64 * self.pll_clock);
            let predicted_flux_time = last_flux_time + (clock_ticks_since_flux as f64 * self.pll_period);

            // The error is the difference between the actual flux time and the predicted flux time.
            let phase_error = next_flux_time - predicted_flux_time;

            // Calculate the proportional frequency adjustment.
            let p_term = kp * phase_error;

            log::trace!(
                "predicted time: {} phase error: {} p_accum: {} p_term: {}",
                predicted_flux_time,
                format_us!(phase_error),
                format_us!(phase_accumulator),
                p_term
            );

            // Adjust the clock frequency, and clamp it to the min/max values.
            self.pll_period += p_term;
            self.pll_period = self.pll_period.clamp(min_clock, max_clock);

            // Adjust the phase of the clock by shifting the time variable.
            phase_accumulator += phase_error;
            if phase_accumulator.abs() > self.pll_period {
                phase_accumulator %= self.pll_period;
            }
            // Save the last flux time for the next iteration.
            clock_ticks_since_flux = 0;
            last_flux_time = next_flux_time;
        }

        log::debug!(
            "Completed decoding of flux stream. Total FTs: {}, {} too fast, {} too slow",
            flux_ct,
            too_fast_flux,
            too_slow_flux
        );

        output_bits
    }
}
