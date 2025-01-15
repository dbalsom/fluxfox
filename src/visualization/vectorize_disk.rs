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

//! Methods for emitting lists of VizSector objects from the elements of a
//! track.

use crate::{
    track_schema::GenericTrackElement,
    types::DiskCh,
    visualization::{
        collect_metadata,
        collect_streams,
        data_segmenter::DataSegmenter,
        metadata,
        stream,
        types::{
            display_list::{VizDataSliceDisplayList, *},
            shapes::{
                VizArc,
                VizCircle,
                VizDataSlice,
                VizElement,
                VizElementFlags,
                VizElementInfo,
                VizPoint2d,
                VizQuadraticArc,
                VizSector,
                VizShape,
            },
        },
        CommonVizParams,
        DiskHitTestResult,
        RenderDiskHitTestParams,
        RenderDiskSelectionParams,
        RenderGeometry,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        RenderVectorizationParams,
        RenderWinding,
        TurningDirection,
    },
    DiskImage,
    DiskVisualizationError,
    MAX_CYLINDER,
};
use std::{
    cmp::min,
    f32::consts::{PI, TAU},
    ops::Range,
};

pub struct CalcElementParams {
    pub center: VizPoint2d<f32>,
    pub start_angle: f32,
    pub end_angle: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub ch: DiskCh,
    pub color: u32,
    pub flags: VizElementFlags,
    pub element: Option<VizElementInfo>,
}

/// Create a [VizElementDisplayList] collection for a single side of a [DiskImage].
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///      visualization functions.
/// - `r`: A reference to a [RenderTrackMetadataParams] object containing the parameters for
///      rendering the disk.
#[deprecated(
    since = "0.1.0",
    note = "Can generate un-renderable geometry. Please use `vectorize_disk_elements_by_quadrants` instead"
)]
pub fn vectorize_disk_elements(
    disk_image: &DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackMetadataParams,
) -> Result<VizElementDisplayList, DiskVisualizationError> {
    let r_tracks = collect_streams(r.side, disk_image);
    let r_metadata = collect_metadata(r.side, disk_image);
    let num_tracks = min(r_tracks.len(), p.track_limit.unwrap_or(MAX_CYLINDER));

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    log::debug!("visualize_disk_elements(): Rendering {} tracks", num_tracks);

    let mut display_list = VizElementDisplayList::new(p.direction, r.side, num_tracks as u16);

    // Maximum size of a metadata item that can overlap the index without being excluded
    // from rendering. Large sectors (8192 bytes) will fill the entire disk surface, so are not
    // particularly useful to render.
    let overlap_max = (1024 + 6) * 16;
    let outer_radius = p.radius.unwrap_or(0.5);
    let mut min_radius = p.min_radius_ratio * outer_radius; // Scale min_radius to pixel value

    // If pinning has been specified, adjust the minimum radius.
    // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
    // is consistent with the last standard track.
    min_radius = if p.pin_last_standard_track {
        let normalized_track_ct = match num_tracks {
            0..50 => 40,
            50..90 => 80,
            90.. => 160,
        };
        let track_width = (outer_radius - min_radius) / normalized_track_ct as f32;
        let overdump = num_tracks.saturating_sub(normalized_track_ct);
        p.min_radius_ratio * outer_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    // Calculate the rendered width of each track, excluding the track gap.
    let track_width = (outer_radius - min_radius) / num_tracks as f32;
    let center = VizPoint2d::from((outer_radius, outer_radius));

    let (clip_start, clip_end) = match p.direction {
        TurningDirection::Clockwise => (0.0, TAU),
        TurningDirection::CounterClockwise => (0.0, TAU),
    };

    // We loop twice, once drawing all non-element markers, then drawing marker elements.
    // The reason for this is that markers are small and may be overwritten by overlapping sector
    // data elements.  This guarantees that markers are emitted last, and thus rendered on top of
    // all other elements.
    for draw_markers in [false, true].iter() {
        for (ti, track_meta) in r_metadata.iter().enumerate() {
            let mut has_elements = false;
            let outer_radius = outer_radius - (ti as f32 * track_width);
            let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));

            // Look for non-marker elements crossing the index, and emit them first.
            // These elements will be clipped at the index boundary, so we will have at least two
            // display list entries for each element that crosses the index.
            if !r.draw_sector_lookup && !*draw_markers {
                for meta_item in track_meta.items.iter() {
                    if meta_item.end >= r_tracks[ti].len() {
                        let meta_length = meta_item.end - meta_item.start;
                        let overlap_long = meta_length > overlap_max;

                        log::trace!(
                            "vectorize_disk_elements(): Overlapping metadata item at {}-{} len: {} max: {} long: {}",
                            meta_item.start,
                            meta_item.end,
                            meta_length,
                            overlap_max,
                            overlap_long,
                        );

                        has_elements = true;

                        let mut start_angle;
                        let mut end_angle;
                        if overlap_long {
                            start_angle = p.index_angle;
                            end_angle = p.index_angle
                                + ((((meta_item.start + overlap_max) % r_tracks[ti].len()) as f32
                                    / r_tracks[ti].len() as f32)
                                    * TAU);
                        }
                        else {
                            start_angle = p.index_angle;
                            end_angle = p.index_angle + ((meta_item.end as f32 / r_tracks[ti].len() as f32) * TAU);
                        }

                        if start_angle > end_angle {
                            std::mem::swap(&mut start_angle, &mut end_angle);
                        }

                        (start_angle, end_angle) = match p.direction {
                            TurningDirection::Clockwise => (start_angle, end_angle),
                            TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
                        };

                        if start_angle > end_angle {
                            std::mem::swap(&mut start_angle, &mut end_angle);
                        }

                        // Skip sectors that are outside the current quadrant
                        if end_angle <= clip_start || start_angle >= clip_end {
                            continue;
                        }

                        // Clamp start and end angle to quadrant boundaries
                        if start_angle < clip_start {
                            start_angle = clip_start;
                        }

                        if end_angle > clip_end {
                            end_angle = clip_end;
                        }

                        let overlap_sector = VizSector::from_angles(
                            &VizPoint2d::new(center.x, center.y),
                            r.winding,
                            start_angle,
                            end_angle,
                            inner_radius,
                            outer_radius,
                        );

                        let mut flags = VizElementFlags::default();
                        flags.set(VizElementFlags::OVERLAP_LONG, overlap_long);

                        let generic_element = GenericTrackElement::from(meta_item.element);
                        let element_info = VizElementInfo::new(
                            generic_element,
                            DiskCh::new(ti as u16, r.side),
                            meta_item.chsn,
                            None,
                            None,
                            None,
                        );
                        let overlap_metadata = VizElement::new(overlap_sector, flags, element_info);

                        display_list.push(ti, overlap_metadata);
                    }
                }
            }

            let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

            log::debug!("vectorize_disk_elements(): Rendering elements on track {}", ti);

            // Draw non-overlapping metadata.
            for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                log::debug!("vectorize_disk_elements(): Rendering element at {}", _mi);
                let generic_element = GenericTrackElement::from(meta_item.element);

                match generic_element {
                    GenericTrackElement::Marker { .. } if !*draw_markers => {
                        continue;
                    }
                    GenericTrackElement::Marker { .. } => {}
                    _ if *draw_markers => {
                        continue;
                    }
                    _ => {}
                }

                // Advance physical sector number for each sector header encountered.
                if meta_item.element.is_sector_header() {
                    phys_s = phys_s.wrapping_add(1);
                }

                has_elements = true;

                let mut start_angle = ((meta_item.start as f32 / r_tracks[ti].len() as f32) * TAU) + p.index_angle;
                let mut end_angle = ((meta_item.end as f32 / r_tracks[ti].len() as f32) * TAU) + p.index_angle;

                if start_angle > end_angle {
                    std::mem::swap(&mut start_angle, &mut end_angle);
                }

                // Invert the angles for clockwise rotation
                (start_angle, end_angle) = match p.direction {
                    TurningDirection::Clockwise => (start_angle, end_angle),
                    TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
                };

                // Normalize the angle to the range 0..2π
                // start_angle = (start_angle % TAU).abs();
                // end_angle = (end_angle % TAU).abs();

                // Exchange start and end if reversed
                if start_angle > end_angle {
                    std::mem::swap(&mut start_angle, &mut end_angle);
                }

                // // Skip sectors that are outside the current quadrant
                // if end_angle <= clip_start || start_angle >= clip_end {
                //     continue;
                // }

                // Clip the elements to one revolution
                if start_angle < clip_start {
                    start_angle = clip_start;
                }

                if end_angle > clip_end {
                    end_angle = clip_end;
                }

                let element_sector = VizSector::from_angles(
                    &VizPoint2d::new(center.x, center.y),
                    r.winding,
                    start_angle,
                    end_angle,
                    inner_radius,
                    outer_radius,
                );

                let element_flags = VizElementFlags::default();
                let element_info =
                    VizElementInfo::new(generic_element, DiskCh::new(ti as u16, r.side), None, None, None, None);
                let element_metadata = VizElement::new(element_sector, element_flags, element_info);

                log::debug!("vectorize_disk_elements(): Pushing element to display list");
                display_list.push(ti, element_metadata);
            }

            // If a track contained no elements and 'draw_empty_tracks' is set, emit a `NullElement`
            // that fills the entire track.
            if !has_elements && r.draw_empty_tracks {
                let element_sector = VizSector::from_angles(
                    &VizPoint2d::new(center.x, center.y),
                    r.winding,
                    clip_start,
                    clip_end,
                    inner_radius,
                    outer_radius,
                );
                let mut element_flags = VizElementFlags::default();
                element_flags.set(VizElementFlags::TRACK, true);
                let element_info = VizElementInfo::new(
                    GenericTrackElement::NullElement,
                    DiskCh::new(ti as u16, r.side),
                    None,
                    None,
                    None,
                    None,
                );
                let element_metadata = VizElement::new(element_sector, element_flags, element_info);

                display_list.push(ti, element_metadata);
            }
        }
    }

    Ok(display_list)
}

/// Create a [VizElementDisplayList] collection for a single side of a [DiskImage], splitting visual
/// elements into quadrants. This is useful when rendering a display list with a graphics library
/// that does not handle rendering major arcs gracefully. This function will render the elements
/// for the specified quadrant, or all quadrants if none are specified.
///
/// Quadrants of the circle are defined by the unit circle as:
/// `/1 0\`
/// `\2 3/`
/// Note that the order is reversed in Clockwise turning direction.
///
/// Quadrants are rendered counter-clockwise, starting from the top right quadrant (0). The order
/// of quadrant rendering is independent of the data turning direction.
///
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///        visualization functions.
/// - `r`: A reference to a [RenderTrackMetadataParams] object containing the parameters specific
///        to rendering metadata elements. The `quadrant` parameter specifies which quadrant to
///        render, or all quadrants if `None`.
///
/// # Returns:
/// A [VizElementDisplayList] containing the elements to render, or a [DiskVisualizationError] if
/// an error occurred, such as no tracks being found.
pub fn vectorize_disk_elements_by_quadrants(
    disk: &DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackMetadataParams,
) -> Result<VizElementDisplayList, DiskVisualizationError> {
    // Render the specified quadrant if provided, otherwise render all quadrants.
    let quadrant_list = r.quadrant.map(|q| vec![q]).unwrap_or(vec![0, 1, 2, 3]);

    // Maximum size of a metadata item that can overlap the index without being excluded
    // from rendering. Large sectors (8192 bytes) will fill the entire disk surface, so are not
    // particularly useful to render.
    let overlap_max = (1024 + 6) * 16;

    // Collect streams.
    let r_tracks = collect_streams(r.side, disk);
    let r_metadata = collect_metadata(r.side, disk);

    if r_tracks.len() != r_metadata.len() {
        return Err(DiskVisualizationError::InvalidParameter(
            "Mismatched track and metadata lengths".to_string(),
        ));
    }

    let tp = p.track_params(disk.track_ct(r.side as usize))?;

    let num_tracks = min(r_tracks.len(), p.track_limit.unwrap_or(MAX_CYLINDER));
    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    let mut display_list = VizElementDisplayList::new(p.direction, r.side, num_tracks as u16);
    log::debug!(
        "vectorize_disk_elements_by_quadrants(): Rendering {} tracks over quadrants: {:?}",
        num_tracks,
        quadrant_list
    );

    // Loop through each track and the track element metadata for each track.
    for (ti, track_meta) in r_metadata.iter().enumerate() {
        let (outer, middle, inner) = tp.radii(ti, true);

        // Loop through each quadrant and render the elements for that quadrant.
        for quadrant in &quadrant_list {
            // Set the appropriate clipping angles for the current quadrant.
            let quadrant_angles = match quadrant & 0x03 {
                0 => (0.0, PI / 2.0),
                1 => (PI / 2.0, PI),
                2 => (PI, 3.0 * PI / 2.0),
                3 => (3.0 * PI / 2.0, TAU),
                _ => unreachable!(),
            };

            let (clip_start, clip_end) = (quadrant_angles.0, quadrant_angles.1);

            // Emit a NullElement arc for this quadrant to represent the track background.
            // Note: must be a cubic arc due to 90-degree angle.
            let track_quadrant_arc = VizArc::from_angles(&tp.center, middle, clip_start, clip_end);

            let mut track_quadrant_flags = VizElementFlags::default();
            track_quadrant_flags.set(VizElementFlags::TRACK, true);
            // If no elements on this track, also set the empty track flag
            if track_meta.items.is_empty() {
                track_quadrant_flags.set(VizElementFlags::EMPTY_TRACK, true);
            }
            let element_info = VizElementInfo::new(
                GenericTrackElement::NullElement,
                DiskCh::new(ti as u16, r.side),
                None,
                None,
                None,
                None,
            );
            let element_metadata = VizElement::new(
                (track_quadrant_arc, tp.render_track_width),
                track_quadrant_flags,
                element_info,
            );

            display_list.push(ti, element_metadata);

            // Look for non-marker elements crossing the index, and emit them first.
            // These elements will always be drawn in quadrant 0, clipped at the index
            // boundary, so we will have at least two display list entries for each element
            // that crosses the index.
            if *quadrant == 0 && !r.draw_sector_lookup {
                for meta_item in track_meta.items.iter() {
                    let generic_element = GenericTrackElement::from(meta_item.element);
                    if matches!(generic_element, GenericTrackElement::Marker) {
                        // Skip markers.
                        continue;
                    }

                    if meta_item.end >= r_tracks[ti].len() {
                        let meta_length = meta_item.end - meta_item.start;
                        let meta_overlap = meta_item.end % r_tracks[ti].len();

                        let overlap_long = meta_length > overlap_max;

                        log::trace!(
                            "vectorize_disk_elements_by_quadrants(): Overlapping metadata item at {}-{} len: {} max: {} long: {}",
                            meta_item.start,
                            meta_item.end,
                            meta_length,
                            overlap_max,
                            overlap_long,
                        );

                        let mut start_angle;
                        let mut end_angle;
                        if overlap_long {
                            start_angle = p.index_angle;
                            end_angle = p.index_angle
                                + ((((meta_item.start + overlap_max) % r_tracks[ti].len()) as f32
                                    / r_tracks[ti].len() as f32)
                                    * TAU);
                        }
                        else {
                            // The start angle is the index angle.
                            start_angle = p.index_angle;
                            end_angle = p.index_angle + ((meta_overlap as f32 / r_tracks[ti].len() as f32) * TAU);
                        }

                        if start_angle > end_angle {
                            std::mem::swap(&mut start_angle, &mut end_angle);
                        }

                        (start_angle, end_angle) = match p.direction {
                            TurningDirection::Clockwise => (start_angle, end_angle),
                            TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
                        };

                        if start_angle > end_angle {
                            std::mem::swap(&mut start_angle, &mut end_angle);
                        }

                        // Skip sectors that are outside the current quadrant
                        if end_angle <= clip_start || start_angle >= clip_end {
                            continue;
                        }

                        // Clamp start and end angle to quadrant boundaries
                        if start_angle < clip_start {
                            start_angle = clip_start;
                        }

                        if end_angle > clip_end {
                            end_angle = clip_end;
                        }

                        match r.geometry {
                            RenderGeometry::Sector => {
                                let overlap_sector =
                                    VizSector::from_angles(&tp.center, r.winding, start_angle, end_angle, inner, outer);

                                let mut flags = VizElementFlags::default();
                                flags.set(VizElementFlags::OVERLAP_LONG, overlap_long);

                                let generic_element = GenericTrackElement::from(meta_item.element);
                                let element_info = VizElementInfo::new(
                                    generic_element,
                                    DiskCh::new(ti as u16, r.side),
                                    meta_item.chsn,
                                    None,
                                    None,
                                    None,
                                );
                                let overlap_metadata = VizElement::new(overlap_sector, flags, element_info);

                                display_list.push(ti, overlap_metadata);
                            }
                            RenderGeometry::Arc => {
                                let overlap_arc = VizArc::from_angles(&tp.center, middle, start_angle, end_angle);

                                let mut flags = VizElementFlags::default();
                                flags.set(VizElementFlags::OVERLAP_LONG, overlap_long);

                                let generic_element = GenericTrackElement::from(meta_item.element);
                                let element_info = VizElementInfo::new(
                                    generic_element,
                                    DiskCh::new(ti as u16, r.side),
                                    meta_item.chsn,
                                    None,
                                    None,
                                    None,
                                );
                                let overlap_metadata =
                                    VizElement::new((overlap_arc, tp.render_track_width), flags, element_info);

                                display_list.push(ti, overlap_metadata);
                            }
                        }
                    }
                }
            }

            // We loop through each track quadrant twice, once drawing all non-element markers, then
            // drawing marker elements.
            // The reason for this is that markers are small and may be overwritten by overlapping
            // sector data elements.  This guarantees that markers are emitted last, and thus
            // rendered on top of all other elements.
            for draw_markers in [false, true].iter() {
                let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

                // Draw non-overlapping metadata.
                for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                    let generic_element = GenericTrackElement::from(meta_item.element);

                    match generic_element {
                        GenericTrackElement::Marker if !*draw_markers => {
                            continue;
                        }
                        GenericTrackElement::Marker => {}
                        _ if *draw_markers => {
                            continue;
                        }
                        _ => {}
                    }

                    // Advance physical sector number for each sector header encountered.
                    if meta_item.element.is_sector_header() {
                        phys_s = phys_s.wrapping_add(1);
                    }

                    let mut start_angle = ((meta_item.start as f32 / r_tracks[ti].len() as f32) * TAU) + p.index_angle;
                    let mut end_angle = ((meta_item.end as f32 / r_tracks[ti].len() as f32) * TAU) + p.index_angle;

                    if start_angle > end_angle {
                        std::mem::swap(&mut start_angle, &mut end_angle);
                    }

                    // Invert the angles for clockwise rotation
                    (start_angle, end_angle) = match p.direction {
                        TurningDirection::Clockwise => (start_angle, end_angle),
                        TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
                    };

                    // Normalize the angle to the range 0..2π
                    // start_angle = (start_angle % TAU).abs();
                    // end_angle = (end_angle % TAU).abs();

                    // Exchange start and end if reversed
                    if start_angle > end_angle {
                        std::mem::swap(&mut start_angle, &mut end_angle);
                    }

                    // Skip sectors that are outside the current quadrant
                    if end_angle <= clip_start || start_angle >= clip_end {
                        continue;
                    }

                    // Clamp start and end angle to quadrant boundaries
                    start_angle = start_angle.max(clip_start);
                    end_angle = end_angle.min(clip_end);

                    match r.geometry {
                        RenderGeometry::Sector => {
                            let element_sector =
                                VizSector::from_angles(&tp.center, r.winding, start_angle, end_angle, inner, outer);

                            let flags = VizElementFlags::default();
                            let generic_element = GenericTrackElement::from(meta_item.element);
                            let element_info = VizElementInfo::new(
                                generic_element,
                                DiskCh::new(ti as u16, r.side),
                                meta_item.chsn,
                                None,
                                None,
                                None,
                            );
                            let element_metadata = VizElement::new(element_sector, flags, element_info);

                            display_list.push(ti, element_metadata);
                        }
                        RenderGeometry::Arc => {
                            let overlap_arc = VizArc::from_angles(&tp.center, middle, start_angle, end_angle);

                            let flags = VizElementFlags::default();
                            let generic_element = GenericTrackElement::from(meta_item.element);
                            let element_info = VizElementInfo::new(
                                generic_element,
                                DiskCh::new(ti as u16, r.side),
                                meta_item.chsn,
                                None,
                                None,
                                None,
                            );
                            let overlap_metadata =
                                VizElement::new((overlap_arc, tp.render_track_width), flags, element_info);

                            display_list.push(ti, overlap_metadata);
                        }
                    }
                }
            }
        }
    }

    Ok(display_list)
}

/// Return a [VizElementDisplayList] representing a selection on a disk image.
/// The selection will be divided into multiple display elements if it exceeds 90 degrees.
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///     visualization functions.
/// - `r`: A reference to a [RenderDiskSelectionParams] object containing the parameters for
///     rendering the disk selection.
pub fn vectorize_disk_selection(
    disk_image: &DiskImage,
    p: &CommonVizParams,
    r: &RenderDiskSelectionParams,
) -> Result<VizElementDisplayList, DiskVisualizationError> {
    let track = stream(r.ch, disk_image);
    let track_len = track.len();
    let r_metadata = metadata(r.ch, disk_image);

    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(disk_image.tracks(r.ch.h()) as usize, track_limit);

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    if r.ch.c() >= num_tracks as u16 {
        return Err(DiskVisualizationError::InvalidParameter(
            "Invalid track number".to_string(),
        ));
    }

    let mut display_list = VizElementDisplayList::new(p.direction, r.ch.h(), num_tracks as u16);

    // If no radius was specified, default to 0.5 - this creates a display list that is in the
    // range [(0,0)-(1,1)], suitable for transformations as desired by the output rasterizer.
    let total_radius = p.radius.unwrap_or(0.5);
    let mut min_radius = p.min_radius_ratio * total_radius; // Scale min_radius to pixel value

    // If pinning has been specified, adjust the minimum radius.
    // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
    // is consistent with the last standard track.
    min_radius = if p.pin_last_standard_track {
        let normalized_track_ct = match num_tracks {
            0..50 => 40,
            50..90 => 80,
            90.. => 160,
        };
        let track_width = (total_radius - min_radius) / normalized_track_ct as f32;
        let overdump = num_tracks.saturating_sub(normalized_track_ct);
        p.min_radius_ratio * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let center = VizPoint2d::from((total_radius, total_radius));

    let (clip_start, clip_end) = match p.direction {
        TurningDirection::Clockwise => (0.0, TAU),
        TurningDirection::CounterClockwise => (0.0, TAU),
    };

    for draw_markers in [false, true].iter() {
        let ti = r.ch.c() as usize;
        let track_meta = r_metadata;

        let outer_radius = total_radius - (ti as f32 * track_width);
        let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));

        let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

        // Draw non-overlapping metadata.
        for (_mi, meta_item) in track_meta.items.iter().enumerate() {
            let generic_element = GenericTrackElement::from(meta_item.element);

            match generic_element {
                GenericTrackElement::Marker { .. } if !*draw_markers => {
                    continue;
                }
                GenericTrackElement::Marker { .. } => {}
                _ if *draw_markers => {
                    continue;
                }
                _ => {}
            }

            // Advance physical sector number for each sector header encountered.
            if meta_item.element.is_sector_header() {
                phys_s = phys_s.wrapping_add(1);
            }

            if !meta_item.element.is_sector_data() || ((phys_s as usize) < r.sector_idx) {
                continue;
            }

            let mut start_angle = ((meta_item.start as f32 / track_len as f32) * TAU) + p.index_angle;
            let mut end_angle = ((meta_item.end as f32 / track_len as f32) * TAU) + p.index_angle;

            if start_angle > end_angle {
                std::mem::swap(&mut start_angle, &mut end_angle);
            }

            (start_angle, end_angle) = match p.direction {
                TurningDirection::Clockwise => (start_angle, end_angle),
                TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
            };

            // Exchange start and end if reversed
            if start_angle > end_angle {
                std::mem::swap(&mut start_angle, &mut end_angle);
            }

            // Skip sectors that are outside the current quadrant
            if end_angle <= clip_start || start_angle >= clip_end {
                continue;
            }

            // Clip the elements to one revolution
            if start_angle < clip_start {
                start_angle = clip_start;
            }

            if end_angle > clip_end {
                end_angle = clip_end;
            }

            let element_sector = VizSector::from_angles(
                &VizPoint2d::new(center.x, center.y),
                RenderWinding::Clockwise,
                start_angle,
                end_angle,
                inner_radius,
                outer_radius,
            );

            let element_flags = VizElementFlags::default();
            let element_info = VizElementInfo::new(
                generic_element,
                DiskCh::new(ti as u16, r.ch.h()),
                meta_item.chsn,
                None,
                None,
                None,
            );
            let element_metadata = VizElement::new(element_sector, element_flags, element_info);

            display_list.push(ti, element_metadata);
            // Rendered one sector, stop.
            break;
        }
    }

    Ok(display_list)
}

/// Return a [VizElementDisplayList] representing a selection on a disk image.
/// The selection will be divided into multiple display elements if it exceeds 90 degrees.
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///     visualization functions.
/// - `r`: A reference to a [RenderDiskSelectionParams] object containing the parameters for
///     rendering the disk selection.
/// - `flags`: A [VizElementFlags] object containing flags to apply to the selection.
pub fn vectorize_disk_hit_test(
    disk: &DiskImage,
    p: &CommonVizParams,
    r: &RenderDiskHitTestParams,
    flags: VizElementFlags,
) -> Result<DiskHitTestResult, DiskVisualizationError> {
    let tp = p.track_params(disk.track_ct(r.side as usize))?;
    let (x, y) = r.point.to_tuple();
    let (dx, dy) = (x - tp.center.x, y - tp.center.y);
    let distance = (dx.powi(2) + dy.powi(2)).sqrt();
    let angle = (dy.atan2(dx) + TAU) % TAU;

    // Allow for a small increment to the maximum radius to make it easier to select elements
    // on the outer edge of the disk. I call this 'coyote radius' as a reference to 'coyote time'
    // in platformer games - the amount of time you can spend in midair after running off a cliff.
    let coyote_radius = tp.render_track_width * 0.5;
    if distance > (tp.max_radius + coyote_radius) || distance < tp.min_radius {
        // Hit test coordinate is outside of data area.
        //log::trace!("Hit test at ({}, {}) is outside of data area", dx, dy);
        return Ok(DiskHitTestResult::default());
    }

    // log::trace!(
    //     "Hit test at ({}, {}) center ({}, {}) distance: {} angle: {}",
    //     dx,
    //     dy,
    //     tp.center.x,
    //     tp.center.y,
    //     distance,
    //     angle
    // );

    // Use the full render with for hit-testing - ignore track gap
    let track_offset = (distance - tp.min_radius) / tp.render_track_width;
    let cylinder = if distance > tp.max_radius {
        // In coyote radius - return cylinder 0
        0
    }
    else {
        (tp.num_tracks - 1).saturating_sub(track_offset.floor() as usize)
    };
    // Get the track metadata for this track.
    let track_opt = disk.track(DiskCh::new(cylinder as u16, r.side));
    if track_opt.is_none() {
        return Ok(DiskHitTestResult::default());
    }
    let track = track_opt.unwrap();
    let track_len = track.stream().and_then(|s| Some(s.len())).unwrap();

    let metadata_opt = track.metadata();
    if metadata_opt.is_none() {
        return Ok(DiskHitTestResult::default());
    }
    let r_metadata = metadata_opt.unwrap();

    // Selection can only be on one cylinder.
    let mut display_list = VizElementDisplayList::new(p.direction, r.side, 1);

    let center = tp.center;

    let (clip_start, clip_end) = (0.0, TAU);

    let normalized_angle = p.direction.adjust_angle(angle);

    // Calculate the bit index from angle and track length.
    let bit_index = ((normalized_angle / TAU) * track_len as f32) as usize;

    if let Some((ei, idx)) = r_metadata.hit_test(bit_index) {
        let generic_element = GenericTrackElement::from(ei.element);

        let mut start_angle = ((ei.start as f32 / track_len as f32) * TAU) + p.index_angle;
        let mut end_angle = ((ei.end as f32 / track_len as f32) * TAU) + p.index_angle;

        // Set a flag if the element is larger than the track. This will switch to circle rendering.
        let wrapping_element = (end_angle - start_angle) > TAU;

        // Invert the angles for clockwise rotation
        (start_angle, end_angle) = match p.direction {
            TurningDirection::Clockwise => (start_angle, end_angle),
            TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
        };

        // Exchange start and end if reversed
        if start_angle > end_angle {
            std::mem::swap(&mut start_angle, &mut end_angle);
        }

        let start_angle = start_angle.max(clip_start);

        let (outer_radius, mid_radius, inner_radius) = tp.radii(cylinder, true);

        // Start and end angles are now in the range 0..2π, but we can't emit cubic arcs longer
        // than 90 degrees. We need to break up the arc into multiple sectors if it exceeds 90 degrees
        // here.

        let shape = match r.geometry {
            RenderGeometry::Sector => VizShape::Sector(VizSector::from_angles(
                &VizPoint2d::new(center.x, center.y),
                RenderWinding::Clockwise,
                start_angle,
                end_angle,
                inner_radius,
                outer_radius,
            )),
            RenderGeometry::Arc => {
                if wrapping_element {
                    // If the element wraps around the track, render a full circle.
                    // A circle is stroked on the outside, by default, so give the inner radius.
                    VizShape::Circle(
                        VizCircle::new(&VizPoint2d::new(center.x, center.y), inner_radius),
                        outer_radius - inner_radius,
                    )
                }
                else {
                    VizShape::CubicArc(
                        VizArc::from_angles(&VizPoint2d::new(center.x, center.y), mid_radius, start_angle, end_angle),
                        outer_radius - inner_radius,
                    )
                }
            }
        };

        let info = VizElementInfo {
            element_type: generic_element,
            ch: DiskCh::new(cylinder as u16, r.side),
            chsn: ei.chsn,
            bit_range: Some(Range::from(ei.start..ei.end)),
            element_idx: Some(idx),
            sector_idx: None,
        };

        let element = VizElement { shape, flags, info };

        display_list.push(0, element);

        return Ok(DiskHitTestResult {
            display_list: Some(display_list),
            angle: normalized_angle,
            bit_index,
            track: cylinder as u16,
        });
    }

    //log::warn!("vectorize_disk_hit_test(): No element found at bit index {}", bit_index);

    Ok(DiskHitTestResult {
        display_list: None,
        bit_index,
        angle: normalized_angle,
        track: cylinder as u16,
    })
}

/// Return a [VizDataDisplayList] representing a selection on a disk image.
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///     visualization functions.
/// - `r`: A reference to a [RenderTrackDataParams] object containing the parameters for
///     rendering the disk track data
/// - `rv`: A reference to a [RenderVectorizationParams] object containing the parameters for
///     vectorizing the disk data.
pub fn vectorize_disk_data(
    disk_image: &DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    _rv: &RenderVectorizationParams,
) -> Result<VizDataSliceDisplayList, DiskVisualizationError> {
    // TODO: Implement offset?
    // Get the offset from the RenderRasterizationParams, which defines them in pixels.
    //let (x_offset, y_offset) = rv.pos_offset.unwrap_or(VizPoint2d::<f32>::default()).to_tuple();

    let total_radius = p.radius.unwrap_or(0.5);
    let max_radius = p.max_radius_ratio * total_radius;
    let mut min_radius = p.min_radius_ratio * total_radius;
    if max_radius <= min_radius {
        return Err(DiskVisualizationError::InvalidParameter(
            "max_radius must be greater than min_radius".to_string(),
        ));
    }

    let center = VizPoint2d::from((total_radius, total_radius));

    // Collect streams.
    let r_tracks = collect_streams(r.side, disk_image);
    // TODO: Vector data decode?
    //let r_metadata = collect_metadata(r.side, disk_image);

    let num_tracks = min(r_tracks.len(), p.track_limit.unwrap_or(MAX_CYLINDER));
    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    // Get the arc angle in radians for each slice.
    let slice_arc = TAU / r.slices as f32;
    // Overlap each slice a little bit to avoid rendering gaps between slices due to floating point errors.
    let slice_overlap = slice_arc * r.overlap;

    // If pinning has been specified, adjust the minimum radius.
    // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
    // is consistent with the last standard track.
    min_radius = if p.pin_last_standard_track {
        let normalized_track_ct = match num_tracks {
            0..50 => 40,
            50..90 => 80,
            90.. => 160,
        };
        let track_width = (max_radius - min_radius) / normalized_track_ct as f32;
        let overdump = num_tracks.saturating_sub(normalized_track_ct);
        p.min_radius_ratio * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    log::trace!("render_track_data(): Collected {} track references.", num_tracks);
    // for (ti, track) in r_tracks.iter().enumerate() {
    //     log::trace!("render_track_data(): Track {} length: {}", ti, track.len());
    // }

    // Calculate the rendered width of each track, excluding the track gap.
    let track_width = (max_radius - min_radius) / num_tracks as f32;
    let stroke_width = if p.track_gap == 0.0 {
        // If 0 gap specified, slightly increase the track width to avoid rendering sparkles between tracks
        track_width * 1.01
    }
    else {
        track_width * (1.0 - p.track_gap)
    };
    log::trace!("render_track_data(): Track width: {} gap: {}", track_width, p.track_gap);
    let mut display_list = VizDataSliceDisplayList::new(p.direction, num_tracks, stroke_width);

    for (ti, track) in r_tracks.iter().enumerate() {
        // Calculate the inner and outer radii for the track, then calculate the midpoint radius.
        // We stroke the data segment slices along this midpoint.
        let outer_radius = max_radius - (ti as f32 * track_width);
        //let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));
        let mid_radius = outer_radius - (track_width / 2.0);

        // Divide the track into segments, and calculate the bit/flux density of each segment.

        // The DataSegmenter iterator will yield integer chunk sizes that will distribute the
        // fractional bits evently across successive chunks, with chunk lengths summing to exactly
        // the track length.
        let data_segments: Vec<usize> = DataSegmenter::new(track.len(), r.slices).collect();
        //let densities: Vec<f32> = Vec::with_capacity(r.slices);
        let mut segment_bits = track.data().clone();
        let mut track_idx = 0;
        for (_si, segment_size) in data_segments.iter().enumerate() {
            // I am not sure how performant 'split_off' is. We should probably switch to
            // iter_chunks when that API stabilizes.

            // if segment_bits.len() < *segment_size {
            //     log::warn!(
            //         "Track {} segment {} size {} exceeds remaining bitvec length {}",
            //         ti,
            //         si,
            //         *segment_size,
            //         segment_bits.len()
            //     );
            // }
            let track_bits_remain = segment_bits.split_off(*segment_size);
            // log::trace!(
            //     "Split off {} bits from track {}. remaining bits: {}",
            //     segment_bits.len(),
            //     ti,
            //     track_bits_remain.len()
            // );

            let popcnt = segment_bits.count_ones();
            // Convert the popcnt to a ratio of the segment size from 0..1.0
            let density = popcnt as f32 / *segment_size as f32;

            if density < display_list.min_density {
                display_list.min_density = density;
            }

            if density > display_list.max_density {
                display_list.max_density = density;
            }

            // Calculate the start and end angles for the segment
            let mut start_angle = ((track_idx as f32 / track.len() as f32) * TAU) + p.index_angle;
            let mut end_angle = (((track_idx + *segment_size) as f32 / track.len() as f32) * TAU) + p.index_angle;
            end_angle += slice_overlap;

            // Invert the angle turning based on direction
            (start_angle, end_angle) = match p.direction {
                TurningDirection::Clockwise => (start_angle, end_angle),
                TurningDirection::CounterClockwise => (TAU - start_angle, TAU - end_angle),
            };

            // Since data slices are short, we can render them as quadratic arcs.
            // This saves us some space when rendering to SVG as quadratics have one fewer
            // control points than cubic Béziers. The savings are about 15% in file size.
            let data_slice = VizDataSlice {
                density,
                mapped_density: track.map_density(density),
                arc: VizQuadraticArc::from_angles(&center, mid_radius, start_angle, end_angle),
            };

            display_list.push(ti, data_slice);

            segment_bits = track_bits_remain;
            track_idx += *segment_size;
        }
    }

    Ok(display_list)
}
