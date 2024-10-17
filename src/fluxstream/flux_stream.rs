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
use crate::fluxstream::pll::Pll;
use crate::fluxstream::FluxTransition;
use crate::DiskDensity;
use bit_vec::BitVec;

pub const AVERAGE_FLUX_DENSITY: f64 = 2.636; // Average number of bits encoded per flux transition

pub struct RawFluxTrack {
    pub sample_freq: f64,
    pub revolutions: Vec<RawFluxStream>,
    pub density: DiskDensity,
}

impl RawFluxTrack {
    pub fn new(sample_freq: f64) -> Self {
        log::trace!(
            "RawFluxTrack::new(): Creating track with {}Mhz sample frequency.",
            sample_freq / 1e6
        );

        RawFluxTrack {
            sample_freq,
            revolutions: Vec::new(),
            density: DiskDensity::Double,
        }
    }

    pub fn density(&self) -> DiskDensity {
        self.density
    }

    pub fn set_density(&mut self, density: DiskDensity) {
        self.density = density;
    }

    pub fn is_empty(&self) -> bool {
        self.revolutions.is_empty()
    }

    pub fn normalize(&mut self) {
        // Drop revolutions that didn't decode at least 100 bits
        self.revolutions.retain(|r| r.bitstream.len() > 100);
    }

    pub fn add_revolution(&mut self, data: &[f64], data_rate: f64) {
        let new_stream = RawFluxStream::from_f64(data, data_rate);
        self.revolutions.push(new_stream);
    }

    pub fn add_revolution_from_u16(&mut self, data: &[u16], data_rate: f64, timebase: f64) {
        let new_stream = RawFluxStream::from_u16(data, data_rate, timebase);
        self.revolutions.push(new_stream);
    }

    pub fn revolution(&self, index: usize) -> Option<&RawFluxStream> {
        self.revolutions.get(index)
    }

    pub fn revolution_mut(&mut self, index: usize) -> Option<&mut RawFluxStream> {
        self.revolutions.get_mut(index)
    }
}

pub struct RawFluxStream {
    pub data_rate: f64,
    pub flux_deltas: Vec<f64>,
    pub transitions: Vec<FluxTransition>,
    pub bitstream: BitVec,
}

impl RawFluxStream {
    pub fn from_f64(deltas: &[f64], data_rate: f64) -> Self {
        RawFluxStream {
            data_rate,
            flux_deltas: deltas.to_vec(),
            transitions: Vec::with_capacity(deltas.len()),
            bitstream: BitVec::with_capacity((data_rate as usize) * 2),
        }
    }

    pub fn from_u16(data: &[u16], data_rate: f64, timebase: f64) -> Self {
        log::debug!("Using timebase of {:.3}ns", timebase * 1e9);
        let mut new = RawFluxStream {
            data_rate,
            flux_deltas: Vec::with_capacity(data.len()),
            transitions: Vec::with_capacity(data.len()),
            bitstream: BitVec::with_capacity((data_rate as usize) * 2),
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

        log::warn!("from_u16(): {} NFA cells found", nfa_count);
        new
    }

    pub fn transition_ct(&self) -> usize {
        self.transitions.len()
    }

    pub fn guess_density(&self, mfi: bool) -> Option<DiskDensity> {
        let mut avg = self.transition_avg();
        log::debug!("guess_density(): Transition average: {}", avg);

        if mfi {
            avg *= 2.0;
        }

        match avg {
            1.0e-6..=3.5e-6 => Some(DiskDensity::High),
            3.5e-6..=6e-6 => Some(DiskDensity::Double),
            _ => None,
        }
    }

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
        self.decode_bitstream();
        log::trace!(
            "RawFluxStream::decode(): Decoded {} transitions into {} bits, ratio: {}",
            self.transitions.len(),
            self.bitstream.len(),
            self.bitstream.len() as f64 / self.transitions.len() as f64
        );
    }

    pub fn decode2(&mut self, pll: &mut Pll, set_rate: bool) {
        if set_rate {
            log::trace!("RawFluxStream::decode(): Setting PLL rate to {}", self.data_rate);
            pll.set_clock(self.data_rate, None);
        }

        self.bitstream = pll.decode(self);

        log::trace!(
            "RawFluxStream::decode(): Decoded {} transitions into {} bits, ratio: {}",
            self.flux_deltas.len(),
            self.bitstream.len(),
            self.bitstream.len() as f64 / self.flux_deltas.len() as f64
        );
    }

    fn decode_bitstream(&mut self) {
        self.bitstream.clear();

        let mut other_ct = 0;
        for transition in self.transitions.iter() {
            match transition {
                FluxTransition::Short => {
                    self.bitstream.push(true);
                    self.bitstream.push(false);
                }
                FluxTransition::Medium => {
                    self.bitstream.push(true);
                    self.bitstream.push(false);
                    self.bitstream.push(false);
                }
                FluxTransition::Long => {
                    self.bitstream.push(true);
                    self.bitstream.push(false);
                    self.bitstream.push(false);
                    self.bitstream.push(false);
                }
                FluxTransition::Other => {
                    // ??
                    other_ct += 1;
                }
            }
        }

        log::warn!("decode_bitstream(): {} unknown flux transition deltas", other_ct);
    }

    pub fn delta_iter(&self) -> std::slice::Iter<f64> {
        self.flux_deltas.iter()
    }
}
