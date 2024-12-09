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
    bitstream::TrackDataStream,
    track_schema::{
        system34::System34Schema,
        TrackElement,
        TrackMarker,
        TrackMarkerItem,
        TrackMetadataItem,
        TrackSchema,
        TrackSchemaTrait,
    },
};
use bit_vec::BitVec;

impl TrackSchemaTrait for TrackSchema {
    fn find_data_pattern(&self, track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_data_pattern(track, pattern, offset),
        }
    }
    fn find_next_marker(&self, track: &TrackDataStream, offset: usize) -> Option<(TrackMarker, usize)> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_next_marker(track, offset),
        }
    }
    fn find_marker(
        &self,
        track: &TrackDataStream,
        marker: TrackMarker,
        offset: usize,
        limit: Option<usize>,
    ) -> Option<(usize, u16)> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_marker(track, marker, offset, limit),
        }
    }

    fn find_element(&self, track: &TrackDataStream, element: TrackElement, offset: usize) -> Option<usize> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_element(track, element, offset),
        }
    }

    fn scan_track_markers(&self, track: &TrackDataStream) -> Vec<TrackMarkerItem> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::scan_track_markers(track),
        }
    }

    fn scan_track_metadata(
        &self,
        track: &mut TrackDataStream,
        markers: Vec<TrackMarkerItem>,
    ) -> Vec<TrackMetadataItem> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::scan_track_metadata(track, markers),
        }
    }

    fn create_clock_map(&self, markers: &[TrackMarkerItem], clock_map: &mut BitVec) {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::create_clock_map(markers, clock_map),
        }
    }

    fn crc16(&self, track: &mut TrackDataStream, bit_index: usize, end: usize) -> (u16, u16) {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::crc16(track, bit_index, end),
        }
    }

    fn crc16_bytes(&self, data: &[u8]) -> (u16, u16) {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::crc16_bytes(data),
        }
    }
}
