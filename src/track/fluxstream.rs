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

    src/track/fluxstream.rs

    Implements the Fluxstream track type and the Track trait for same.

*/

use std::{
    any::Any,
    sync::{Arc, Mutex},
};

use super::{Track, TrackConsistency, TrackInfo};
use crate::{
    bitstream::TrackDataStream,
    flux::{
        flux_revolution::FluxRevolution,
        histogram::FluxHistogram,
        pll::{Pll, PllPreset},
    },
    format_us,
    track::{bitstream::BitStreamTrack, metasector::MetaSectorTrack},
    track_schema::{system34::System34Standard, TrackMetadata, TrackSchema},
    types::{
        chs::DiskChsnQuery,
        BitStreamTrackParams,
        DiskCh,
        DiskChs,
        DiskChsn,
        DiskDataEncoding,
        DiskDataRate,
        DiskDataResolution,
        DiskDensity,
        DiskRpm,
        ReadSectorResult,
        ReadTrackResult,
        RwSectorScope,
        ScanSectorResult,
        SectorDescriptor,
        SharedDiskContext,
        WriteSectorResult,
    },
    DiskImageError,
    SectorMapEntry,
};

use sha1_smol::Digest;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluxStreamTrack {
    encoding: DiskDataEncoding,
    schema: Option<TrackSchema>,
    data_rate: DiskDataRate,
    ch: DiskCh,

    revolutions: Vec<FluxRevolution>,
    decoded_revolutions: Vec<Option<BitStreamTrack>>,
    best_revolution: usize,
    density: DiskDensity,
    rpm: DiskRpm,

    dirty:    bool,
    resolved: Option<BitStreamTrack>,

    #[cfg_attr(feature = "serde", serde(skip))]
    shared: Option<Arc<Mutex<SharedDiskContext>>>,
}

#[cfg_attr(feature = "serde", typetag::serde)]
impl Track for FluxStreamTrack {
    fn resolution(&self) -> DiskDataResolution {
        DiskDataResolution::FluxStream
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_metasector_track(&self) -> Option<&MetaSectorTrack> {
        None
    }

    fn as_bitstream_track(&self) -> Option<&BitStreamTrack> {
        None
    }

    fn as_fluxstream_track(&self) -> Option<&FluxStreamTrack> {
        self.as_any().downcast_ref::<FluxStreamTrack>()
    }

    fn as_fluxstream_track_mut(&mut self) -> Option<&mut FluxStreamTrack> {
        self.as_any_mut().downcast_mut::<FluxStreamTrack>()
    }

    fn ch(&self) -> DiskCh {
        self.ch
    }

    fn set_ch(&mut self, new_ch: DiskCh) {
        self.ch = new_ch;
    }

    fn encoding(&self) -> DiskDataEncoding {
        self.encoding
    }

    fn info(&self) -> TrackInfo {
        if let Some(resolved) = self.get_bitstream() {
            let ti = resolved.info();
            log::debug!("FluxStreamTrack::info(): Bitstream info: {:?}", ti);
            return ti;
        }

        TrackInfo {
            encoding: self.encoding,
            schema: self.schema,
            data_rate: self.data_rate,
            density: Some(DiskDensity::from(self.data_rate)),
            rpm: Some(self.rpm),
            bit_length: 0,
            sector_ct: 0,
        }
    }

    fn metadata(&self) -> Option<&TrackMetadata> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.metadata();
        }
        None
    }

    fn sector_ct(&self) -> usize {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.sector_ct();
        }
        0
    }

    fn has_sector_id(&self, id: u8, _id_chsn: Option<DiskChsn>) -> bool {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.has_sector_id(id, _id_chsn);
        }
        false
    }

    fn sector_list(&self) -> Vec<SectorMapEntry> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.sector_list();
        }
        Vec::new()
    }

    fn add_sector(&mut self, _sd: &SectorDescriptor, _alternate: bool) -> Result<(), DiskImageError> {
        Err(DiskImageError::UnsupportedFormat)
    }

    /// Read the sector data from the sector identified by 'chs'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags where are needed
    /// when handling MetaSector images.
    /// When reading a BitStream image, the sector data includes the address mark and crc.
    /// Offsets are provided within ReadSectorResult so these can be skipped when processing the
    /// read operation.
    fn read_sector(
        &self,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
        scope: RwSectorScope,
        debug: bool,
    ) -> Result<ReadSectorResult, DiskImageError> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.read_sector(id, n, offset, scope, debug);
        }
        Err(DiskImageError::ResolveError)
    }

    fn scan_sector(
        &self,
        id: DiskChsnQuery,
        n: Option<u8>,
        offset: Option<usize>,
    ) -> Result<ScanSectorResult, DiskImageError> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.scan_sector(id, n, offset);
        }
        Err(DiskImageError::ResolveError)
    }

    fn write_sector(
        &mut self,
        id: DiskChsnQuery,
        offset: Option<usize>,
        write_data: &[u8],
        scope: RwSectorScope,
        write_deleted: bool,
        debug: bool,
    ) -> Result<WriteSectorResult, DiskImageError> {
        let old_dirty = self.dirty;
        self.dirty = true;
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.write_sector(id, offset, write_data, scope, write_deleted, debug);
        }
        self.dirty = old_dirty;
        Err(DiskImageError::ResolveError)
    }

    fn recalculate_sector_crc(&mut self, id: DiskChsnQuery, offset: Option<usize>) -> Result<(), DiskImageError> {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.recalculate_sector_crc(id, offset);
        }
        Err(DiskImageError::ResolveError)
    }

    fn hash(&mut self) -> Digest {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.hash();
        }

        Digest::default()
    }

    /// Read all sectors from the track identified by 'ch'. The data is returned within a
    /// ReadSectorResult struct which also sets some convenience metadata flags which are needed
    /// when handling MetaSector images.
    /// Unlike read_sectors, the data returned is only the actual sector data. The address marks and
    /// CRCs are not included in the data.
    /// This function is intended for use in implementing the Read Track FDC command.
    fn read_all_sectors(&mut self, _ch: DiskCh, n: u8, eot: u8) -> Result<ReadTrackResult, DiskImageError> {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.read_all_sectors(_ch, n, eot);
        }

        Err(DiskImageError::ResolveError)
    }

    fn get_next_id(&self, chs: DiskChs) -> Option<DiskChsn> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.get_next_id(chs);
        }
        None
    }

    fn read_track(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.read_track(overdump);
        }
        Err(DiskImageError::ResolveError)
    }

    fn read_track_raw(&mut self, overdump: Option<usize>) -> Result<ReadTrackResult, DiskImageError> {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.read_track_raw(overdump);
        }
        Err(DiskImageError::ResolveError)
    }

    fn has_weak_bits(&self) -> bool {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.has_weak_bits();
        }
        false
    }

    fn format(
        &mut self,
        standard: System34Standard,
        format_buffer: Vec<DiskChsn>,
        fill_pattern: &[u8],
        gap3: usize,
    ) -> Result<(), DiskImageError> {
        let old_dirty = self.dirty;
        self.dirty = true;
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.format(standard, format_buffer, fill_pattern, gap3);
        }
        self.dirty = old_dirty;
        Err(DiskImageError::ResolveError)
    }

    fn track_consistency(&self) -> Result<TrackConsistency, DiskImageError> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.track_consistency();
        }
        Err(DiskImageError::ResolveError)
    }

    fn track_stream(&self) -> Option<&TrackDataStream> {
        if let Some(resolved) = self.get_bitstream() {
            return resolved.track_stream();
        }
        None
    }

    fn track_stream_mut(&mut self) -> Option<&mut TrackDataStream> {
        if let Some(resolved) = self.get_bitstream_mut() {
            return resolved.track_stream_mut();
        }
        None
    }
}

impl Default for FluxStreamTrack {
    fn default() -> Self {
        FluxStreamTrack::new()
    }
}

impl FluxStreamTrack {
    pub fn new() -> Self {
        FluxStreamTrack {
            encoding: Default::default(),
            schema: None,
            data_rate: Default::default(),
            ch: Default::default(),
            revolutions: Vec::new(),
            decoded_revolutions: Vec::new(),
            best_revolution: 0,
            density: DiskDensity::Double,
            rpm: DiskRpm::Rpm300,
            dirty: false,
            resolved: None,
            shared: None,
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

    pub(crate) fn normalize(&mut self) {
        // Drop revolutions that didn't decode at least 100 bits
        // TODO: Can we do this while keeping the best revolution index valid?
        self.revolutions.retain(|r| r.bitstream.len() > 100);
        self.best_revolution = 0;
    }

    pub(crate) fn add_revolution(&mut self, ch: DiskCh, data: &[f64], index_time: f64) -> &mut FluxRevolution {
        let new_stream = FluxRevolution::from_f64(ch, data, index_time);
        self.revolutions.push(new_stream);
        self.revolutions.last_mut().unwrap()
    }

    pub(crate) fn add_revolution_from_u16(&mut self, ch: DiskCh, data: &[u16], index_time: f64, timebase: f64) {
        let new_stream = FluxRevolution::from_u16(ch, data, index_time, timebase);
        self.revolutions.push(new_stream);
    }

    pub fn set_revolution(&mut self, index: usize) {
        if index < self.revolutions.len() {
            self.best_revolution = index;
        }
    }

    pub fn revolution_ct(&self) -> usize {
        self.revolutions.len()
    }

    pub fn revolution(&self, index: usize) -> Option<&FluxRevolution> {
        self.revolutions.get(index)
    }

    pub fn revolution_mut(&mut self, index: usize) -> Option<&mut FluxRevolution> {
        self.revolutions.get_mut(index)
    }

    pub fn revolution_iter(&self) -> impl Iterator<Item = &FluxRevolution> {
        self.revolutions.iter()
    }

    pub fn revolution_iter_mut(&mut self) -> impl Iterator<Item = &mut FluxRevolution> {
        self.revolutions.iter_mut()
    }

    /// Decode all revolutions in the track. Use 'base_clock' to set the base clock for the PLL,
    /// if provided. If not provided, the base clock is estimated based on the flux transition
    /// count, but this can be ambiguous. If no base clock is provided, and we cannot guess, we
    /// will assume a double density track.
    pub(crate) fn decode_revolutions(
        &mut self,
        clock_hint: Option<f64>,
        rpm_hint: Option<DiskRpm>,
    ) -> Result<(), DiskImageError> {
        self.decoded_revolutions = Vec::new();

        for (i, revolution) in self.revolutions.iter_mut().enumerate() {
            self.decoded_revolutions.push(None);
            let ft_ct = revolution.ft_ct();

            // Use the rpm hint if provided, otherwise try to derive from the revolution's index time,
            // falling back to 300 RPM if neither works.
            let mut base_rpm =
                rpm_hint.unwrap_or(DiskRpm::try_from_index_time(revolution.index_time).unwrap_or(DiskRpm::Rpm300));

            log::debug!("decode_revolutions:() using base rpm: {}", base_rpm);

            let mut base_clock;
            let base_clock_opt = match clock_hint {
                Some(hint) => {
                    log::debug!("decode_revolutions(): Revolution {}: Using clock hint: {}", i, hint);
                    Some(hint)
                }
                None => {
                    // Try to estimate base clock and rpm based on flux transition count.
                    // This is not perfect - we may need to adjust the clock later.
                    let base_clock_opt = match ft_ct {
                        20_000..41_666 => Some(2e-6),
                        50_000.. => Some(1e-6),
                        _ => {
                            log::warn!(
                                "decode_revolutions(): Revolution {} has ambiguous FT count: {}. Falling back to histogram clock detection.",
                                i,
                                ft_ct
                            );
                            None
                        }
                    };

                    log::debug!(
                        "decode_revolutions(): Revolution {}: Estimating clock by FT count: {} Base clock: {:?}",
                        i,
                        ft_ct,
                        base_clock_opt
                    );

                    base_clock_opt
                }
            };

            log::debug!("Base clock after flux count check is {:?}", base_clock_opt);

            let index_time = revolution.index_time;
            let rev_rpm = 60.0 / index_time;
            let f_rpm = f64::from(base_rpm);

            // If RPM calculated from the index time seems accurate, trust it over the rpm hint.
            base_rpm = match rev_rpm {
                255.0..345.0 => DiskRpm::Rpm300,
                345.0..414.0 => DiskRpm::Rpm360,
                _ => {
                    log::error!(
                        "Revolution {} RPM is out of range ({:.2}). Assuming {}",
                        i,
                        rev_rpm,
                        base_rpm
                    );
                    // TODO: Fall back to calculating rpm from sum of flux times?
                    base_rpm
                }
            };

            log::debug!("Base RPM after index time check is {:?}", base_rpm);

            base_clock = if let Some(base_clock) = base_clock_opt {
                // Handling the case of a double-density disk imaged in a 360 RPM drive is a pain.
                // For now, let's assume that anything higher than a 1.5us base clock is double density,
                // in which case we will adjust the clock by the relative RPM.
                base_rpm.adjust_clock(base_clock)
            }
            else {
                // Try to determine the base clock and RPM based on the revolution histogram.
                let mut full_hist = FluxHistogram::new(&revolution.flux_deltas, 1.0);
                let base_transition_time_opt = full_hist.base_transition_time();

                if let Some(base_transition_time) = base_transition_time_opt {
                    let hist_period = base_transition_time / 2.0;
                    log::debug!(
                        "decode_revolutions(): Revolution {}: Histogram base period {:.4}",
                        i,
                        format_us!(hist_period)
                    );
                    hist_period
                }
                else {
                    log::warn!(
                        "decode_revolutions(): Revolution {}: No base clock hint, and full histogram base period not found. Assuming 2us bitcell.",
                        i
                    );
                    2e-6
                }
            };

            // Create PLL and decode revolution.
            let mut pll = Pll::from_preset(PllPreset::Aggressive);

            // Create histogram for start of revolution (first 2% of track)
            let mut hist = FluxHistogram::new(&revolution.flux_deltas, 0.02);
            let base_transition_time_opt = hist.base_transition_time();
            if base_transition_time_opt.is_none() {
                log::warn!(
                    "decode_revolutions(): Revolution {}: Unable to detect track start transition time.",
                    i
                );
            }

            if let Some(base_transition_time) = base_transition_time_opt {
                let hist_period = base_transition_time / 2.0;
                let difference_ratio = (hist_period - base_clock) / base_clock;
                if difference_ratio.abs() < 0.25 {
                    log::debug!(
                        "decode_revolutions(): Revolution {}: Histogram refined clock to {}",
                        i,
                        format_us!(hist_period),
                    );
                    base_clock = hist_period;
                }
                else {
                    log::warn!(
                        "decode_revolutions(): Revolution {}: Start of track histogram clock {} is too far from base {}, not adjusting clock.",
                        i,
                        format_us!(hist_period),
                        format_us!(base_clock)
                    );
                }
            }

            pll.set_clock(1.0 / base_clock, None);
            log::debug!(
                "decode_revolutions(): Decoding revolution {}: Bitrate: {:.2}, Base period {}, {:.2}rpm",
                i,
                1.0 / base_clock,
                format_us!(base_clock),
                f_rpm
            );

            let flux_stats = revolution.decode_direct(&mut pll);

            let (bitstream_data, bitcell_ct) = revolution.bitstream_data();
            let params = BitStreamTrackParams {
                encoding: revolution.encoding,
                data_rate: DiskDataRate::from(revolution.data_rate.unwrap() as u32), // Data rate should be Some after decoding
                rpm: Some(base_rpm),
                ch: revolution.ch,
                bitcell_ct: Some(bitcell_ct),
                data: &bitstream_data,
                weak: None,
                hole: None,
                detect_weak: false,
            };
            let bitstream_track = BitStreamTrack::new(
                params,
                self.shared
                    .clone()
                    .expect("Attempted to decode track before adding it."),
            )?;

            self.decoded_revolutions[i] = Some(bitstream_track);

            log::debug!("decode_revolutions(): Decoded revolution {}: {}", i, flux_stats);
        }

        Ok(())
    }

    pub fn synthesize_revolutions(&mut self) {
        let synthetic_revs: Vec<FluxRevolution> = self
            .revolutions
            .windows(2) // Create pairs of successive elements
            .flat_map(|pair| FluxRevolution::from_adjacent_pair(&pair[0], &pair[1])) // Call make_foo on each pair
            .collect();

        self.revolutions.extend(synthetic_revs);
    }

    pub fn analyze_revolutions(&mut self) {
        let mut best_revolution = 0;
        let mut best_score = 0;

        if self.revolutions.is_empty() {
            log::warn!("FluxStreamTrack::analyze_revolutions(): No revolutions to analyze.");
            return;
        }

        for (i, bitstream) in self.decoded_revolutions.iter().enumerate() {
            if let Some(track) = bitstream {
                let score = track.calc_quality_score();
                let bad_sectors = track
                    .sector_list()
                    .iter()
                    .filter(|s| !s.attributes.data_crc_valid)
                    .count();

                log::debug!(
                    "FluxStreamTrack::analyze_revolutions(): Revolution {}, ft_ct: {} bitcells: {} bad sectors: {} score: {}",
                    i,
                    self.revolutions[i].ft_ct(),
                    track.info().bit_length,
                    bad_sectors,
                    score
                );

                // Higher bitstream quality score = better revolution.
                if score > best_score {
                    best_score = score;
                    best_revolution = i;
                }
            }
        }
        log::debug!(
            "FluxStreamTrack::analyze_revolutions(): Best revolution is {}/{} with score {}",
            best_revolution,
            self.revolutions.len(),
            best_score
        );

        self.best_revolution = best_revolution;

        let rev_ref = self
            .revolutions
            .get_mut(best_revolution)
            .expect("Best revolution not found.");

        self.encoding = rev_ref.encoding;
    }

    fn get_bitstream(&self) -> Option<&BitStreamTrack> {
        if let Some(resolved) = &self.resolved {
            return Some(resolved);
        }
        else if self.best_revolution < self.revolutions.len() {
            if let Some(track) = &self.decoded_revolutions[self.best_revolution] {
                return Some(track);
            }
        }
        log::warn!(
            "get_bitstream(): No track resolved for {} Best: {} Revolutions: {}",
            self.ch,
            self.best_revolution,
            self.revolutions.len()
        );
        None
    }

    fn get_bitstream_mut(&mut self) -> Option<&mut BitStreamTrack> {
        if let Some(resolved) = &mut self.resolved {
            return Some(resolved);
        }
        else if self.best_revolution < self.revolutions.len() {
            if let Some(track) = &mut self.decoded_revolutions[self.best_revolution] {
                return Some(track);
            }
        }
        log::warn!(
            "get_bitstream_mut(): No track resolved for {} Best: {} Revolutions: {}",
            self.ch,
            self.best_revolution,
            self.revolutions.len()
        );
        None
    }

    pub(crate) fn set_shared(&mut self, shared: Arc<Mutex<SharedDiskContext>>) {
        self.shared = Some(shared);
    }
}
