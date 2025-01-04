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

//! Methods for emitting lists of VizSector objects from the elements of a
//! track.

use crate::{
    track_schema::GenericTrackElement,
    types::DiskCh,
    visualization::{
        collect_metadata,
        collect_streams,
        display_list::VizDataSliceDisplayList,
        metadata,
        stream,
        types::{VizArc, VizElement, VizElementFlags, VizElementInfo, VizPoint2d, VizSector},
        CommonVizParams,
        RenderDiskSelectionParams,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        RenderVectorizationParams,
        TurningDirection,
        VizElementDisplayList,
    },
    DiskImage,
    DiskImageError,
    DiskVisualizationError,
    MAX_CYLINDER,
};
use std::{
    cmp::min,
    f32::consts::{PI, TAU},
};

/// Calculate a [VizArc] from a center point, radius, and start and end angles in radians.
pub fn calc_arc(center: &VizPoint2d<f32>, radius: f32, start_angle: f32, end_angle: f32) -> VizArc {
    let (x1, y1) = (
        center.x + radius * start_angle.cos(),
        center.y + radius * start_angle.sin(),
    );
    let (x4, y4) = (center.x + radius * end_angle.cos(), center.y + radius * end_angle.sin());

    let ax = x1 - center.x;
    let ay = y1 - center.y;
    let bx = x4 - center.x;
    let by = y4 - center.y;

    let q1 = ax * ax + ay * ay;
    let q2 = q1 + ax * bx + ay * by;
    let k2 = (4.0 / 3.0) * ((2.0 * q1 * q2).sqrt() - q2) / (ax * by - ay * bx);

    let (x2, y2) = (center.x + ax - k2 * ay, center.y + ay + k2 * ax);
    let (x3, y3) = (center.x + bx + k2 * by, center.y + by - k2 * bx);

    VizArc {
        start: VizPoint2d { x: x1, y: y1 },
        end:   VizPoint2d { x: x4, y: y4 },
        cp1:   VizPoint2d { x: x2, y: y2 },
        cp2:   VizPoint2d { x: x3, y: y3 },
    }
}

/// Calculate a [VizSector] from a center point, start and end angles in radians, and an inner and
/// outer radius.
#[inline]
pub fn calc_sector(
    center: &VizPoint2d<f32>,
    start_angle: f32,
    end_angle: f32,
    inner_radius: f32,
    outer_radius: f32,
) -> VizSector {
    let inner = calc_arc(center, inner_radius, start_angle, end_angle);
    let outer = calc_arc(center, outer_radius, end_angle, start_angle);

    VizSector::from((start_angle, end_angle, outer, inner))
}

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

/// Calculate a [VizElement] from a center point, start and end angles in radians, an inner
/// and outer radius, a color, and an optional [VizElementInfo].
pub fn calc_element(p: &CalcElementParams) -> VizElement {
    let mut element = p.element.clone().unwrap_or_default();
    element.ch = p.ch;

    let sector = calc_sector(&p.center, p.start_angle, p.end_angle, p.inner_radius, p.outer_radius);

    VizElement {
        sector,
        flags: p.flags.clone(),
        info: element,
    }
}

/// Create a [VizElementDisplayList] collection for a single side of a [DiskImage].
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `p`: A reference to a [CommonVizParams] object containing the parameters common to all
///      visualization functions.
/// - `r`: A reference to a [RenderTrackMetadataParams] object containing the parameters for
///      rendering the disk.
pub fn vectorize_disk_elements(
    disk_image: &DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackMetadataParams,
) -> Result<VizElementDisplayList, DiskVisualizationError> {
    let rtracks = collect_streams(r.head, disk_image);
    let rmetadata = collect_metadata(r.head, disk_image);
    let num_tracks = min(rtracks.len(), p.track_limit.unwrap_or(MAX_CYLINDER));

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    log::debug!("visualize_disk_elements(): Rendering {} tracks", num_tracks);

    let mut display_list = VizElementDisplayList::new(p.direction, num_tracks);

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
            50.. => 80,
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
        for (ti, track_meta) in rmetadata.iter().enumerate() {
            let mut has_elements = false;
            let outer_radius = outer_radius - (ti as f32 * track_width);
            let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));

            // Look for non-marker elements crossing the index, and emit them first.
            // These elements will be clipped at the index boundary, so we will have at least two
            // display list entries for each element that crosses the index.
            if !r.draw_sector_lookup && !*draw_markers {
                for meta_item in track_meta.items.iter() {
                    if meta_item.end >= rtracks[ti].len() {
                        let meta_length = meta_item.end - meta_item.start;
                        let overlap_long = meta_length > overlap_max;

                        log::trace!(
                            "visualize_disk_elements(): Overlapping metadata item at {}-{} len: {} max: {} long: {}",
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
                                + ((((meta_item.start + overlap_max) % rtracks[ti].len()) as f32
                                    / rtracks[ti].len() as f32)
                                    * TAU);
                        }
                        else {
                            start_angle = p.index_angle;
                            end_angle = p.index_angle + ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU);
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

                        let overlap_sector = calc_sector(
                            &VizPoint2d::new(center.x, center.y),
                            start_angle,
                            end_angle,
                            inner_radius,
                            outer_radius,
                        );

                        let mut flags = VizElementFlags::default();
                        flags.set(VizElementFlags::OVERLAP_LONG, overlap_long);

                        let generic_element = GenericTrackElement::from(meta_item.element);
                        let overlap_metadata = VizElement {
                            sector: overlap_sector,
                            flags,
                            info: VizElementInfo::new(
                                generic_element,
                                DiskCh::new(ti as u16, r.head),
                                None,
                                None,
                                None,
                            ),
                        };

                        display_list.push(ti, overlap_metadata);
                    }
                }
            }

            let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

            log::debug!("visualize_disk_elements(): Rendering elements on track {}", ti);

            // Draw non-overlapping metadata.
            for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                log::debug!("visualize_disk_elements(): Rendering element at {}", _mi);
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

                let mut start_angle = ((meta_item.start as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;
                let mut end_angle = ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;

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

                let element_sector = calc_sector(
                    &VizPoint2d::new(center.x, center.y),
                    start_angle,
                    end_angle,
                    inner_radius,
                    outer_radius,
                );

                let element_flags = VizElementFlags::default();
                let element_metadata = VizElement {
                    sector: element_sector,
                    flags:  element_flags,
                    info:   VizElementInfo::new(generic_element, DiskCh::new(ti as u16, r.head), None, None, None),
                };

                log::debug!("visualize_disk_elements(): Pushing element to display list");
                display_list.push(ti, element_metadata);
            }

            // If a track contained no elements and 'draw_empty_tracks' is set, emit a `NullElement`
            // that fills the entire track.
            if !has_elements && r.draw_empty_tracks {
                let element_sector = calc_sector(
                    &VizPoint2d::new(center.x, center.y),
                    clip_start,
                    clip_end,
                    inner_radius,
                    outer_radius,
                );
                let mut element_flags = VizElementFlags::default();
                element_flags.set(VizElementFlags::TRACK, true);
                let element_metadata = VizElement {
                    sector: element_sector,
                    flags:  element_flags,
                    info:   VizElementInfo::new(
                        GenericTrackElement::NullElement,
                        DiskCh::new(ti as u16, r.head),
                        None,
                        None,
                        None,
                    ),
                };

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
    disk_image: &DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackMetadataParams,
) -> Result<VizElementDisplayList, DiskVisualizationError> {
    let rtracks = collect_streams(r.head, disk_image);
    let rmetadata = collect_metadata(r.head, disk_image);
    let num_tracks = min(rtracks.len(), p.track_limit.unwrap_or(MAX_CYLINDER));

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    // Render the specified quadrant if provided, otherwise render all quadrants.
    let quadrant_list = r.quadrant.map(|q| vec![q]).unwrap_or(vec![0, 1, 2, 3]);

    log::debug!(
        "visualize_disk_elements_by_quadrants(): Rendering {} tracks over quadrants: {:?}",
        num_tracks,
        quadrant_list
    );

    let mut display_list = VizElementDisplayList::new(p.direction, num_tracks);

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
            50.. => 80,
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

    // let (clip_start, clip_end) = match p.direction {
    //     TurningDirection::Clockwise => (0.0, TAU),
    //     TurningDirection::CounterClockwise => (0.0, TAU),
    // };

    // Loop through each track and the track element metadata for each track.
    for (ti, track_meta) in rmetadata.iter().enumerate() {
        let outer_radius = outer_radius - (ti as f32 * track_width);
        let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));

        // Loop through each quadrant and render the elements for that quadrant.
        for quadrant in &quadrant_list {
            // Set the appropriate clipping angles for the current quadrant.
            let quadrant_angles_cc = match quadrant {
                0 => (0.0, PI / 2.0),
                1 => (PI / 2.0, PI),
                2 => (PI, 3.0 * PI / 2.0),
                3 => (3.0 * PI / 2.0, TAU),
                _ => return Err(DiskVisualizationError::InvalidParameter),
            };

            let (clip_start, clip_end) = (quadrant_angles_cc.0, quadrant_angles_cc.1);

            // Emit a NullElement for this quadrant to represent the track background.
            let track_quadrant_sector = calc_sector(
                &VizPoint2d::new(center.x, center.y),
                clip_start,
                clip_end,
                inner_radius,
                outer_radius,
            );
            let mut track_quadrant_flags = VizElementFlags::default();
            track_quadrant_flags.set(VizElementFlags::TRACK, true);
            // If no elements on this track, also set the empty track flag
            if track_meta.items.is_empty() {
                track_quadrant_flags.set(VizElementFlags::EMPTY_TRACK, true);
            }
            let element_metadata = VizElement {
                sector: track_quadrant_sector,
                flags:  track_quadrant_flags,
                info:   VizElementInfo::new(
                    GenericTrackElement::NullElement,
                    DiskCh::new(ti as u16, r.head),
                    None,
                    None,
                    None,
                ),
            };

            display_list.push(ti, element_metadata);

            // Look for non-marker elements crossing the index, and emit them first.
            // These elements will always be drawn in quadrant three, clipped at the index
            // boundary, so we will have at least two display list entries for each element
            // that crosses the index.
            if !r.draw_sector_lookup {
                for meta_item in track_meta.items.iter() {
                    let generic_element = GenericTrackElement::from(meta_item.element);
                    if matches!(generic_element, GenericTrackElement::Marker) {
                        // Skip markers. They are too small to overlap the index.
                    }

                    if meta_item.end >= rtracks[ti].len() {
                        let meta_length = meta_item.end - meta_item.start;
                        let overlap_long = meta_length > overlap_max;

                        log::trace!(
                            "visualize_disk_elements(): Overlapping metadata item at {}-{} len: {} max: {} long: {}",
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
                                + ((((meta_item.start + overlap_max) % rtracks[ti].len()) as f32
                                    / rtracks[ti].len() as f32)
                                    * TAU);
                        }
                        else {
                            start_angle = p.index_angle;
                            end_angle = p.index_angle + ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU);
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

                        let overlap_sector = calc_sector(
                            &VizPoint2d::new(center.x, center.y),
                            start_angle,
                            end_angle,
                            inner_radius,
                            outer_radius,
                        );

                        let mut flags = VizElementFlags::default();
                        flags.set(VizElementFlags::OVERLAP_LONG, overlap_long);

                        let generic_element = GenericTrackElement::from(meta_item.element);
                        let overlap_metadata = VizElement {
                            sector: overlap_sector,
                            flags,
                            info: VizElementInfo::new(
                                generic_element,
                                DiskCh::new(ti as u16, r.head),
                                None,
                                None,
                                None,
                            ),
                        };

                        display_list.push(ti, overlap_metadata);
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

                log::debug!("visualize_disk_elements(): Rendering elements on track {}", ti);

                // Draw non-overlapping metadata.
                for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                    log::debug!("visualize_disk_elements(): Rendering element at {}", _mi);
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

                    let mut start_angle = ((meta_item.start as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;
                    let mut end_angle = ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;

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

                    let element_sector = calc_sector(
                        &VizPoint2d::new(center.x, center.y),
                        start_angle,
                        end_angle,
                        inner_radius,
                        outer_radius,
                    );

                    let element_flags = VizElementFlags::default();
                    let element_metadata = VizElement {
                        sector: element_sector,
                        flags:  element_flags,
                        info:   VizElementInfo::new(generic_element, DiskCh::new(ti as u16, r.head), None, None, None),
                    };

                    log::debug!("visualize_disk_elements(): Pushing element to display list");
                    display_list.push(ti, element_metadata);
                }
            }
        }
    }

    Ok(display_list)
}

/// Return a [VizElementDisplayList] representing a selection on a disk image.
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
        return Err(DiskVisualizationError::InvalidParameter);
    }

    let mut display_list = VizElementDisplayList::new(p.direction, num_tracks);

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
            50.. => 80,
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

            let element_sector = calc_sector(
                &VizPoint2d::new(center.x, center.y),
                start_angle,
                end_angle,
                inner_radius,
                outer_radius,
            );

            let element_flags = VizElementFlags::default();
            let element_metadata = VizElement {
                sector: element_sector,
                flags:  element_flags,
                info:   VizElementInfo::new(generic_element, DiskCh::new(ti as u16, r.ch.h()), None, None, None),
            };

            display_list.push(ti, element_metadata);
            // Rendered one sector, stop.
            break;
        }
    }

    Ok(display_list)
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
    rv: &RenderVectorizationParams,
) -> Result<VizDataSliceDisplayList, DiskVisualizationError> {
    let display_list = VizDataSliceDisplayList::new(p.direction);

    // Get the offset from the RenderRasterizationParams, which defines them in pixels.
    let (x_offset, y_offset) = rv.pos_offset.unwrap_or(VizPoint2d::<f32>::default()).to_tuple();

    let total_radius = p.radius.unwrap_or(0.5);
    let mut min_radius = p.min_radius_ratio * total_radius;
    let center = VizPoint2d::from((total_radius, total_radius));

    let r_tracks = collect_streams(r.side, disk_image);
    let r_metadata = collect_metadata(r.side, disk_image);

    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(r_tracks.len(), track_limit);

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    // If pinning has been specified, adjust the minimum radius.
    // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
    // is consistent with the last standard track.
    min_radius = if p.pin_last_standard_track {
        let normalized_track_ct = match num_tracks {
            0..50 => 40,
            50.. => 80,
        };
        let track_width = (total_radius - min_radius) / normalized_track_ct as f32;
        log::debug!(
            "render_track_data(): track ct: {} normalized track ct: {}",
            num_tracks,
            normalized_track_ct
        );
        let overdump = num_tracks.saturating_sub(normalized_track_ct);
        p.min_radius_ratio * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    log::trace!("Collected {} track references.", num_tracks);
    for (ti, track) in r_tracks.iter().enumerate() {
        log::trace!("Track {} length: {}", ti, track.len());
    }

    let track_width = (total_radius - min_radius) / num_tracks as f32;

    for (ti, track) in r_tracks.iter().enumerate() {}

    Ok(display_list)
}
