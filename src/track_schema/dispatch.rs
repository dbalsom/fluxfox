/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

#[cfg(feature = "amiga")]
use crate::track_schema::amiga::AmigaSchema;

use crate::{
    bitstream_codec::TrackDataStream,
    prelude::RwScope,
    source_map::SourceMap,
    track::{TrackAnalysis, TrackSectorScanResult},
    track_schema::{
        system34::System34Schema,
        TrackElementInstance,
        TrackMarker,
        TrackMarkerItem,
        TrackMetadata,
        TrackSchema,
        TrackSchemaParser,
    },
    types::IntegrityCheck,
    SectorIdQuery,
};
use bit_vec::BitVec;
use std::ops::Range;

const SCHEMA_ERR: &str = "You must enable at least one platform feature!";

impl TrackSchemaParser for TrackSchema {
    /*
    fn find_data_pattern(&self, track: &TrackDataStream, pattern: &[u8], offset: usize) -> Option<usize> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_data_pattern(track, pattern, offset),
            TrackSchema::Amiga => todo!(),
        }
    }
    */

    fn analyze_elements(&self, metadata: &TrackMetadata) -> TrackAnalysis {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::analyze_elements(metadata),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::analyze_elements(metadata),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn find_next_marker(&self, track: &TrackDataStream, offset: usize) -> Option<(TrackMarker, usize)> {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::find_next_marker(track, offset),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::find_next_marker(track, offset),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
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
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::find_marker(track, marker, offset, limit),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::find_marker(track, marker, offset, limit),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn match_sector_element<'a>(
        &self,
        id: impl Into<SectorIdQuery>,
        elements: &[TrackElementInstance],
        index: usize,
        limit: Option<usize>,
    ) -> TrackSectorScanResult {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::find_sector_element(id, elements, index, limit),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::find_sector_element(id, elements, index, limit),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn decode_element(
        &self,
        track: &TrackDataStream,
        element: &TrackElementInstance,
        scope: RwScope,
        buf: &mut [u8],
    ) -> (Range<usize>, Option<IntegrityCheck>) {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::decode_element(track, element, scope, buf),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::decode_element(track, element, scope, buf),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn encode_element(
        &self,
        track: &mut TrackDataStream,
        element: &mut TrackElementInstance,
        offset: usize,
        scope: RwScope,
        buf: &[u8],
    ) -> usize {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::encode_element(track, element, scope, buf),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::encode_element(track, element, scope, buf),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    /*
    fn find_element(&self, track: &TrackDataStream, element: TrackElement, offset: usize) -> Option<usize> {
        #[allow(clippy::match_single_binding)]
        match self {
            TrackSchema::System34 => System34Schema::find_element(track, element, offset),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => todo!(),
        }
    }
    */

    fn scan_for_markers(&self, track: &TrackDataStream) -> Vec<TrackMarkerItem> {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::scan_markers(track),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::scan_markers(track),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn scan_for_elements(
        &self,
        track: &mut TrackDataStream,
        markers: Vec<TrackMarkerItem>,
    ) -> Vec<TrackElementInstance> {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::scan_metadata(track, markers),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::scan_for_elements(track, markers),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn create_clock_map(&self, markers: &[TrackMarkerItem], clock_map: &mut BitVec) {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::create_clock_map(markers, clock_map),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::create_clock_map(markers, clock_map),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn crc_u16(&self, track: &mut TrackDataStream, bit_index: usize, end: usize) -> (u16, u16) {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::crc16(track, bit_index, end),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => todo!(),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn crc_u16_buf(&self, data: &[u8]) -> (u16, u16) {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::crc16_bytes(data),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => todo!(),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }

    fn build_element_map(&self, elements: &[TrackElementInstance]) -> SourceMap {
        #[allow(clippy::match_single_binding)]
        #[allow(unreachable_patterns)]
        match self {
            TrackSchema::System34 => System34Schema::build_element_map(elements),
            #[cfg(feature = "amiga")]
            TrackSchema::Amiga => AmigaSchema::build_element_map(elements),
            _ => {
                panic!("{}", SCHEMA_ERR)
            }
        }
    }
}
