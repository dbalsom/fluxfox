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
#[derive(Debug)]
#[allow(dead_code)]
pub enum EncodeError {
    NonFiniteTime { index: usize, value: f64 },
    NegativeTime { index: usize, value: f64 },
    Overflow { index: usize, seconds: f64, sck: f64 },
}

pub const DEFAULT_SCK_HZ: f64 = 24_027_428.57142857;
pub const DEFAULT_ICK_HZ: f64 = 3_003_428.571428571;

pub struct KfxEncoder {
    pub sck_hz: f64,
    pub ick_hz: f64,
    pub index_seed: u32,
    pub kfinfo_name: String,
    pub kfinfo_version: String,
}

impl KfxEncoder {
    pub fn new(sck_hz: f64, ick_hz: f64 ) -> Self {
        Self {
            sck_hz,
            ick_hz,
            index_seed: 123_456_789,
            kfinfo_name: "FluxPaint".to_string(),
            kfinfo_version: "1.0".to_string(),
        }
    }
    
    pub fn with_index_seed(mut self, index_seed: u32) -> Self {
        self.index_seed = index_seed;
        self
    }

    pub fn with_kfinfo(mut self, name: &str, version: &str) -> Self {
        self.kfinfo_name = name.to_string();
        self.kfinfo_version = version.to_string();
        self
    }

    pub fn encode_multi_revs(
        &self,
        one_rev_times_sec: &[f64],
        revs: usize,
    ) -> Result<Vec<u8>, EncodeError> {
        let (isb_one, total_sck_ticks_u64) = self.encode_one_revolution_isb(one_rev_times_sec)?;
        let isb_one_len = isb_one.len() as u32;
        let delta_index_u32 = ((total_sck_ticks_u64 as f64) * (self.ick_hz / self.sck_hz)).round() as u32;

        let mut out: Vec<u8> = Vec::with_capacity(isb_one.len() * revs + 128);
        let info = format!(
            "name={}, version={}, sck={}, ick={}, revs={}",
            self.kfinfo_name, self.kfinfo_version, self.sck_hz, self.ick_hz, revs
        );
        Self::push_kfinfo(&mut out, &info);

        let mut stream_pos: u32 = 0;
        let mut index_counter: u32 = self.index_seed;
        Self::push_index(&mut out, stream_pos, 0, index_counter);

        for _ in 0..revs {
            out.extend_from_slice(&isb_one);
            stream_pos = stream_pos.wrapping_add(isb_one_len);
            index_counter = index_counter.wrapping_add(delta_index_u32);
            Self::push_index(&mut out, stream_pos, 0, index_counter);
        }

        Self::push_stream_end_and_eof(&mut out, stream_pos);
        Ok(out)
    }

    fn encode_one_revolution_isb(&self, times_sec: &[f64]) -> Result<(Vec<u8>, u64), EncodeError> {
        let mut isb: Vec<u8> = Vec::with_capacity(times_sec.len() * 2);
        let mut total_ticks: u64 = 0;
        for (i, &t) in times_sec.iter().enumerate() {
            if !t.is_finite() {
                return Err(EncodeError::NonFiniteTime { index: i, value: t });
            }
            if t < 0.0 {
                return Err(EncodeError::NegativeTime { index: i, value: t });
            }
            let ticks_f = t * self.sck_hz;
            if ticks_f > (u64::MAX as f64) {
                return Err(EncodeError::Overflow {
                    index: i,
                    seconds: t,
                    sck: self.sck_hz,
                });
            }
            let ticks = ticks_f.round() as u64;
            total_ticks = total_ticks.wrapping_add(ticks);
            Self::push_flux_value(ticks, &mut isb);
        }
        Ok((isb, total_ticks))
    }

    fn push_flux_value(mut ticks: u64, out: &mut Vec<u8>) {
        while ticks > 0xFFFF {
            out.push(0x0B);
            ticks -= 0x10000;
        }
        let v = ticks as u32;
        if (0x0E..=0xFF).contains(&v) {
            out.push(v as u8);
        } else if (v <= 0x000D) || ((0x0100..=0x07FF).contains(&v)) {
            let hi = ((v >> 8) & 0x07) as u8;
            let lo = (v & 0xFF) as u8;
            out.push(hi);
            out.push(lo);
        } else {
            out.push(0x0C);
            out.push(((v >> 8) & 0xFF) as u8);
            out.push((v & 0xFF) as u8);
        }
    }

    fn push_kfinfo(out: &mut Vec<u8>, info: &str) {
        out.push(0x0D);
        out.push(0x04);
        let size = (info.len() as u16) + 1;
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(info.as_bytes());
        out.push(0);
    }

    fn push_index(out: &mut Vec<u8>, stream_pos: u32, sample_counter: u32, index_counter: u32) {
        out.push(0x0D);
        out.push(0x02);
        out.extend_from_slice(&0x000Cu16.to_le_bytes());
        out.extend_from_slice(&stream_pos.to_le_bytes());
        out.extend_from_slice(&sample_counter.to_le_bytes());
        out.extend_from_slice(&index_counter.to_le_bytes());
    }

    fn push_stream_end_and_eof(out: &mut Vec<u8>, stream_pos: u32) {
        out.push(0x0D);
        out.push(0x03);
        out.extend_from_slice(&0x0008u16.to_le_bytes());
        out.extend_from_slice(&stream_pos.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(0x0D);
        out.push(0x0D);
        out.extend_from_slice(&0x0D0Du16.to_le_bytes());
    }
}

impl Default for KfxEncoder {
    fn default() -> Self {
        Self::new(DEFAULT_SCK_HZ, DEFAULT_ICK_HZ)
    }
}