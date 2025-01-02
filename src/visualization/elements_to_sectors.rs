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
        types::{VizArc, VizElement, VizElementFlags, VizElementInfo, VizPoint2d, VizSector},
        RenderTrackMetadataParams,
        TurningDirection,
        VizDisplayList,
    },
    DiskImage,
    DiskVisualizationError,
};
use std::{cmp::min, f32::consts::TAU};

/// Calculate a [VizArc] from a center point, radius, and start and end angles in radians.
pub fn calc_arc(center: &VizPoint2d, radius: f32, start_angle: f32, end_angle: f32) -> VizArc {
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
    center: &VizPoint2d,
    start_angle: f32,
    end_angle: f32,
    inner_radius: f32,
    outer_radius: f32,
) -> VizSector {
    let inner = calc_arc(center, inner_radius, start_angle, end_angle);
    let outer = calc_arc(center, outer_radius, end_angle, start_angle);

    VizSector::from((outer, inner))
}

pub struct CalcElementParams {
    pub center: VizPoint2d,
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

/// Create a [VizDisplayList] collection for a single side of a [DiskImage].
/// # Arguments:
/// - `disk_image`: The [DiskImage] to render.
/// - `radius`: The radius of the disk surface. This can be set to the smallest dimension of the
///      bitmap you intend to render the resulting display list to, or leave it as 1.0 to create
///      a list of elements in the range [(-1,-1), (1,1)].
/// - `p`: A reference to a [RenderTrackMetadataParams] object containing the parameters for
///      rendering the disk.
pub fn visualize_disk_elements(
    disk_image: &DiskImage,
    radius: f32,
    p: &RenderTrackMetadataParams,
) -> Result<VizDisplayList, DiskVisualizationError> {
    let rtracks = collect_streams(p.head, disk_image);
    let rmetadata = collect_metadata(p.head, disk_image);
    let num_tracks = min(rtracks.len(), p.track_limit);

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    let mut display_list = VizDisplayList::new(p.direction);

    // Maximum size of a metadata item that can overlap the index without being excluded
    // from rendering. Large sectors (8192 bytes) will fill the entire disk surface, so are not
    // particularly useful to render.
    let overlap_max = (1024 + 6) * 16;
    let total_radius = radius;
    let mut min_radius = p.min_radius_fraction * total_radius; // Scale min_radius to pixel value

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
        p.min_radius_fraction * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    // Calculate the rendered width of each track, excluding the track gap.
    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let center = VizPoint2d::from((radius, radius));

    let (clip_start, clip_end) = match p.direction {
        TurningDirection::Clockwise => (0.0, TAU),
        TurningDirection::CounterClockwise => (0.0, TAU),
    };

    for draw_markers in [false, true].iter() {
        for (ti, track_meta) in rmetadata.iter().enumerate() {
            let mut has_elements = false;
            let outer_radius = total_radius - (ti as f32 * track_width);
            let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));

            // Look for metadata items crossing the index, and emit them first.
            // These elements will be clipped at the index boundary, so we will have two
            // VizElementMetadata objects for each metadata item that crosses the index.
            if !p.draw_sector_lookup && !*draw_markers {
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
                                DiskCh::new(ti as u16, p.head),
                                None,
                                None,
                                None,
                            ),
                        };

                        display_list.push(overlap_metadata);
                    }
                }
            }

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
                    info:   VizElementInfo::new(generic_element, DiskCh::new(ti as u16, p.head), None, None, None),
                };

                display_list.push(element_metadata);
            }

            // If a track contained no elements and 'draw_empty_tracks' is set, emit a `NullElement`
            // that fills the entire track.
            if !has_elements && p.draw_empty_tracks {
                let element_sector = calc_sector(
                    &VizPoint2d::new(center.x, center.y),
                    clip_start,
                    clip_end,
                    inner_radius,
                    outer_radius,
                );
                let mut element_flags = VizElementFlags::default();
                element_flags.set(VizElementFlags::EMPTY_TRACK, true);
                let element_metadata = VizElement {
                    sector: element_sector,
                    flags:  element_flags,
                    info:   VizElementInfo::new(
                        GenericTrackElement::NullElement,
                        DiskCh::new(ti as u16, p.head),
                        None,
                        None,
                        None,
                    ),
                };

                display_list.push(element_metadata);
            }
        }
    }

    Ok(display_list)
}
