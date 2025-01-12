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

//! Module for rendering disk images to a pixmap, requiring tiny_skia

use crate::{
    track_schema::{GenericTrackElement, TrackElement},
    visualization::{
        collect_error_maps,
        collect_metadata,
        collect_streams,
        collect_weak_masks,
        metadata,
        stream,
        types::{color::VizColor, shapes::VizPoint2d},
        CommonVizParams,
        RenderDiskSelectionParams,
        RenderMaskType,
        RenderRasterizationParams,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        ResolutionType,
        TurningDirection,
        POPCOUNT_TABLE,
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

use tiny_skia::{
    BlendMode,
    Color,
    FillRule,
    GradientStop,
    LineCap,
    LineJoin,
    LinearGradient,
    Paint,
    PathBuilder,
    Pixmap,
    Point,
    PremultipliedColorU8,
    SpreadMode,
    Stroke,
    Transform,
};

/// Rasterize a representation of a disk's data to a Pixmap. This function samples the disk image
/// across U,V coordinates at the requested rendering resolution, and can be quite slow for large
/// resolutions (the number of pixels grows quadratically with the image size).
///
/// This technique can also result in aliasing artifacts, especially moiré. This is worst at 45
/// degree increments for reasons that are not entirely clear to me but probably due to the
/// trigonometric functions used.
///
/// Supersampling can help, but additional rendering cost.
///
/// Still, it is possible to render things that aren't practical with a vector-based approach,
/// such as rendering individual bits.
pub fn rasterize_track_data(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    rr: &RenderRasterizationParams,
) -> Result<(), DiskImageError> {
    // Render at the full supersampling resolution. The caller is responsible for down-sampling.
    let (width, height) = rr.render_size().to_tuple();
    let span = pixmap.width();

    // Get the offset from the RenderRasterizationParams, which defines them in pixels.
    let (x_offset, y_offset) = rr.pos_offset.unwrap_or(VizPoint2d::default()).to_tuple();

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let mut min_radius = p.min_radius_ratio * total_radius; // Scale min_radius to pixel value

    let r_tracks = collect_streams(r.side, disk_image);
    let r_metadata = collect_metadata(r.side, disk_image);

    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(r_tracks.len(), track_limit);

    if num_tracks == 0 {
        return Err(DiskImageError::IncompatibleImage("No tracks to visualize!".to_string()));
    }

    log::trace!("collected {} track references.", num_tracks);
    for (ti, track) in r_tracks.iter().enumerate() {
        log::trace!("track {} length: {}", ti, track.len());
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

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let pix_buf = pixmap.pixels_mut();

    let color_black = PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
    let color_white = PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();

    let skia_color = rr.image_bg_color.map(|color| Color::from(color));
    let color_bg: PremultipliedColorU8 = match skia_color {
        Some(color) => PremultipliedColorU8::from_rgba(
            (color.red() * 255.0) as u8,
            (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8,
            (color.alpha() * 255.0) as u8,
        )
        .unwrap(),
        None => PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap(),
    };

    // Draw the tracks
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let _distance_sq = dx * dx + dy * dy;
            let angle = (dy.atan2(dx) + PI) % TAU;
            //let angle = dy.atan2(dx) % TAU;

            if distance >= min_radius && distance <= total_radius {
                let track_offset = (distance - min_radius) / track_width;
                if track_offset.fract() < p.track_gap {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < num_tracks {
                    if r_tracks[track_index].is_empty() {
                        continue;
                    }
                    // Adjust angle for clockwise or counter-clockwise
                    let mut normalized_angle = match p.direction {
                        TurningDirection::Clockwise => angle - p.index_angle,
                        TurningDirection::CounterClockwise => TAU - (angle - p.index_angle),
                    };
                    // Normalize the angle to the range 0..2π
                    //normalized_angle = normalized_angle % TAU;
                    normalized_angle = (normalized_angle + PI) % TAU;
                    let bit_index = ((normalized_angle / TAU) * r_tracks[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    //let bit_index = min(bit_index, r_tracks[track_index].len() - 9);
                    let bit_index = bit_index % r_tracks[track_index].len();

                    let color = match r.resolution {
                        ResolutionType::Bit => {
                            if r_tracks[track_index][bit_index] {
                                color_white
                            }
                            else {
                                color_black
                            }
                        }
                        ResolutionType::Byte => {
                            // Calculate the byte value

                            // Don't decode empty tracks - there's no data to decode!
                            let decoded_bit_idx = (bit_index) & !0xF;
                            let decode_override = r.decode
                                && !r_metadata[track_index].items.is_empty()
                                && r_tracks[track_index].is_data(decoded_bit_idx, false);

                            let byte_value = match decode_override {
                                false => r_tracks[track_index].read_raw_u8(bit_index).unwrap_or_default(),
                                true => {
                                    // Only render bits in 16-bit steps.
                                    r_tracks[track_index]
                                        .read_decoded_u8(decoded_bit_idx)
                                        .unwrap_or_default()
                                }
                            };

                            let gray_value = POPCOUNT_TABLE[byte_value as usize];

                            PremultipliedColorU8::from_rgba(gray_value, gray_value, gray_value, 255).unwrap()
                        }
                    };

                    pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = color;
                }
            }
            else {
                pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = color_bg;
            }
        }
    }

    Ok(())
}

/// Render a representation of a track map to a `tiny_skia::Pixmap`.
/// The destination Pixmap is usually the result of a call to `render_track_data`.
/// The mask can be either a weak bit map or an error map
pub fn render_track_mask(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    map: RenderMaskType,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    rr: &RenderRasterizationParams,
) -> Result<(), DiskImageError> {
    let (width, height) = rr.image_size.to_tuple();
    let span = pixmap.width();

    // Get the offset from the RenderRasterizationParams, which defines them in pixels.
    let (x_offset, y_offset) = rr.pos_offset.unwrap_or(VizPoint2d::default()).to_tuple();

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let mut min_radius = p.min_radius_ratio * total_radius; // Scale min_radius to pixel value

    let track_refs = match map {
        RenderMaskType::WeakBits => collect_weak_masks(r.side, disk_image),
        RenderMaskType::Errors => collect_error_maps(r.side, disk_image),
    };

    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(track_refs.len(), track_limit);
    if num_tracks == 0 {
        return Err(DiskImageError::IncompatibleImage("No tracks to visualize!".to_string()));
    }
    // log::trace!("collected {} maps of type {:?}", num_tracks, map);
    // for (ti, track) in track_refs.iter().enumerate() {
    //     log::debug!("map {} has {} bits", ti, track.count_ones());
    //     log::trace!("track {} length: {}", ti, track.len());
    // }

    // If pinning has been specified, adjust the minimum radius.
    // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
    // is consistent with the last standard track.
    min_radius = if p.pin_last_standard_track {
        let normalized_track_ct = match num_tracks {
            0..50 => 40,
            50.. => 80,
        };
        let track_width = (total_radius - min_radius) / normalized_track_ct as f32;
        let overdump = num_tracks - normalized_track_ct;
        p.min_radius_ratio * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let _track_width_sq = track_width * track_width;
    let _render_track_width = track_width * (1.0 - p.track_gap);

    let pix_buf = pixmap.pixels_mut();

    let skia_color = rr.mask_color.map(|color| Color::from(color));
    let mask_color: PremultipliedColorU8 = match skia_color {
        Some(color) => PremultipliedColorU8::from_rgba(
            (color.red() * 255.0) as u8,
            (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8,
            (color.alpha() * 255.0) as u8,
        )
        .unwrap(),
        None => PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap(),
    };

    let color_trans: PremultipliedColorU8 = PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap();

    // Draw the tracks
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let _distance_sq = dx * dx + dy * dy;
            let angle = (dy.atan2(dx) + PI) % TAU;

            if distance >= min_radius && distance <= total_radius {
                let track_offset = (distance - min_radius) / track_width;
                if track_offset.fract() < p.track_gap {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < num_tracks {
                    // Adjust angle for clockwise or counter-clockwise
                    let normalized_angle = match p.direction {
                        TurningDirection::Clockwise => angle - p.index_angle,
                        TurningDirection::CounterClockwise => TAU - (angle - p.index_angle),
                    };

                    let normalized_angle = (normalized_angle + PI) % TAU;
                    let bit_index = ((normalized_angle / TAU) * track_refs[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    let bit_index = min(bit_index.saturating_sub(8), track_refs[track_index].len() - 17);

                    let word_index = bit_index / 16;
                    let word_value = if word_index < track_refs[track_index].len() / 16 - 1 {
                        let mut build_word: u16 = 0;
                        for bi in 0..16 {
                            build_word |= if track_refs[track_index][bit_index + bi] { 1 } else { 0 };
                            build_word <<= 1;
                        }
                        build_word
                    }
                    else {
                        0
                    };

                    if word_value != 0 {
                        pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = mask_color;
                    }
                }
            }
            else {
                pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = color_trans;
            }
        }
    }

    Ok(())
}

/// Render a representation of a disk's data to a `tiny_skia::Pixmap`, for a specific quadrant of
/// the unit circle.
/// Rendering is broken into quadrants to allow for multithreaded rendering of each quadrant, and
/// to avoid rendering arcs longer than 90 degrees.
pub fn rasterize_track_metadata_quadrant(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    p: &CommonVizParams,
    r: &RenderTrackMetadataParams,
    rr: &RenderRasterizationParams,
) -> Result<(), DiskVisualizationError> {
    let r_tracks = collect_streams(r.side, disk_image);
    let r_metadata = collect_metadata(r.side, disk_image);

    if r_tracks.len() != r_metadata.len() {
        return Err(DiskVisualizationError::InvalidImage);
    }

    let quadrant = r.quadrant.unwrap_or(0);
    let overlap_max = (1024 + 6) * 16;
    let t_params = p.track_params(r_tracks.len())?;

    let mut path_builder = PathBuilder::new();
    let center = Point::from(t_params.quadrant_center(quadrant));

    let draw_metadata_slice = |path_builder: &mut PathBuilder,
                               paint: &mut Paint,
                               start_angle: f32,
                               end_angle: f32,
                               inner_radius: f32,
                               outer_radius: f32,
                               sector_lookup: bool,
                               phys_c: u16,
                               phys_s: u8,
                               element_type: Option<TrackElement>|
     -> Color {
        // Draw the outer curve
        add_arc(path_builder, center, inner_radius, start_angle, end_angle);
        // Draw line segment to end angle of inner curve
        path_builder.line_to(
            center.x + outer_radius * end_angle.cos(),
            center.y + outer_radius * end_angle.sin(),
        );
        // Draw inner curve back to start angle
        add_arc(path_builder, center, outer_radius, end_angle, start_angle);
        // Draw line segment back to start angle of outer curve
        path_builder.line_to(
            center.x + inner_radius * start_angle.cos(),
            center.y + inner_radius * start_angle.sin(),
        );
        path_builder.close();

        // Use a predefined color for each sector
        let color;

        if let Some(element_type) = element_type {
            match sector_lookup {
                true => {
                    // If we're drawing a sector lookup bitmap, we encode the physical head,
                    // cylinder, and sector index as r, g, b components.
                    // This is so that we can retrieve a mapping of physical sector from bitmap
                    // x,y coordinates.
                    // Alpha must remain 255 for our values to survive pre-multiplication.
                    color = VizColor::from_rgba8(r.side, phys_c as u8, phys_s, 255);
                }
                false => {
                    let generic_elem = GenericTrackElement::from(element_type);
                    color = rr
                        .palette
                        .as_ref()
                        .and_then(move |palette| palette.get(&generic_elem).copied())
                        .unwrap_or(VizColor::TRANSPARENT);
                }
            }
        }
        else {
            color = VizColor::BLACK;
        }

        let skia_color = Color::from(color);
        paint.set_color(skia_color);
        skia_color
    };

    let (clip_start, clip_end) = t_params.quadrant_clip(r.quadrant.unwrap_or(0));

    for draw_markers in [false, true].iter() {
        for (ti, track_meta) in r_metadata.iter().enumerate() {
            let mut has_elements = false;
            let outer_radius = t_params.total_radius - (ti as f32 * t_params.render_track_width);
            let inner_radius = outer_radius - (t_params.render_track_width * (1.0 - p.track_gap));
            let mut paint = Paint {
                blend_mode: BlendMode::SourceOver,
                anti_alias: !r.draw_sector_lookup,
                ..Default::default()
            };

            // Look for metadata items crossing the index, and draw them first.
            // We limit the maximum index overlap as an 8192 byte sector at the end of a track will
            // wrap the index twice.

            if !r.draw_sector_lookup && !*draw_markers {
                for meta_item in track_meta.items.iter() {
                    if meta_item.end >= r_tracks[ti].len() {
                        let meta_length = meta_item.end - meta_item.start;
                        let overlap_long = meta_length > overlap_max;

                        log::trace!(
                            "render_track_metadata_quadrant(): Overlapping metadata item at {}-{} len: {} max: {} long: {}",
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

                        let start_color = draw_metadata_slice(
                            &mut path_builder,
                            &mut paint,
                            start_angle,
                            end_angle,
                            inner_radius,
                            outer_radius,
                            false,
                            0,
                            0,
                            Some(meta_item.element),
                        );

                        //let overlap_long = false;
                        if overlap_long {
                            // Long elements are gradually faded out across the index to imply they continue.
                            let end_color =
                                Color::from_rgba(start_color.red(), start_color.green(), start_color.blue(), 0.0)
                                    .unwrap();

                            let (start_pt, end_pt) = match p.direction {
                                TurningDirection::CounterClockwise => (
                                    Point::from_xy(center.x, 0.0),
                                    Point::from_xy(center.x, t_params.total_radius / 8.0),
                                ),
                                TurningDirection::Clockwise => (
                                    Point::from_xy(center.x, center.y),
                                    Point::from_xy(center.x, center.y - t_params.total_radius / 8.0),
                                ),
                            };

                            // Set up a vertical gradient (top to bottom)
                            let gradient = LinearGradient::new(
                                start_pt, //Point::from_xy(center.x, 0.0),
                                end_pt,   //Point::from_xy(center.x, total_radius / 8.0),
                                vec![GradientStop::new(0.0, start_color), GradientStop::new(1.0, end_color)],
                                SpreadMode::Pad,
                                Transform::identity(),
                            )
                            .unwrap();

                            paint.shader = gradient;
                        }

                        if let Some(path) = path_builder.finish() {
                            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                        }

                        path_builder = PathBuilder::new(); // Reset the path builder for the next sector
                    }
                }
            }

            let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

            // Draw non-overlapping metadata.
            for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                let generic_elem = GenericTrackElement::from(meta_item.element);
                if let GenericTrackElement::Marker = generic_elem {
                    if !*draw_markers {
                        continue;
                    }
                }
                else if *draw_markers {
                    continue;
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

                (start_angle, end_angle) = p.direction.adjust_angles((start_angle, end_angle));

                // Normalize the angle to the range 0..2π
                // start_angle = (start_angle % TAU).abs();
                // end_angle = (end_angle % TAU).abs();

                // Exchange start and end if reversed
                if start_angle > end_angle {
                    std::mem::swap(&mut start_angle, &mut end_angle);
                }

                // Skip sectors that are outside the current quadrant
                let (hit, (start_angle, end_angle)) = t_params.quadrant_hit_test(quadrant, (start_angle, end_angle));
                if !hit {
                    continue;
                }

                draw_metadata_slice(
                    &mut path_builder,
                    &mut paint,
                    start_angle,
                    end_angle,
                    inner_radius,
                    outer_radius,
                    r.draw_sector_lookup,
                    ti as u16,
                    phys_s,
                    Some(meta_item.element),
                );

                if let Some(path) = path_builder.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                }

                path_builder = PathBuilder::new(); // Reset the path builder for the next sector
            }

            // If a track contained no elements, draw a black ring
            if !has_elements && r.draw_empty_tracks {
                draw_metadata_slice(
                    &mut path_builder,
                    &mut paint,
                    clip_start,
                    clip_end,
                    inner_radius,
                    outer_radius,
                    true,
                    0,
                    0,
                    None,
                );

                if let Some(path) = path_builder.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                }
                path_builder = PathBuilder::new(); // Reset the path builder for the next sector
            }
        }
    }

    Ok(())
}

/// Rasterize a representation of a specific sector to a `tiny_skia::Pixmap`.
/// Unlike other metadata rendering functions, this does not operate per quadrant, but should be
/// given a composited pixmap.
pub fn rasterize_disk_selection(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    p: &CommonVizParams,
    r: &RenderDiskSelectionParams,
) -> Result<(), DiskVisualizationError> {
    let track = stream(r.ch, disk_image);
    let track_len = track.len();
    let r_metadata = metadata(r.ch, disk_image);

    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(disk_image.tracks(r.ch.h()) as usize, track_limit);
    if r.ch.c() >= num_tracks as u16 {
        return Err(DiskVisualizationError::NoTracks);
    }

    let image_size = pixmap.width() as f32;
    let total_radius = image_size / 2.0;
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
    let center = Point::from_xy(image_size / 2.0, image_size / 2.0);

    let draw_sector_slice = |path_builder: &mut PathBuilder,
                             paint: &mut Paint,
                             start_angle: f32,
                             end_angle: f32,
                             inner_radius: f32,
                             outer_radius: f32,
                             color: Color|
     -> Color {
        // Draw the outer curve
        add_arc(path_builder, center, inner_radius, start_angle, end_angle);
        // Draw line segment to end angle of inner curve
        path_builder.line_to(
            center.x + outer_radius * end_angle.cos(),
            center.y + outer_radius * end_angle.sin(),
        );
        // Draw inner curve back to start angle
        add_arc(path_builder, center, outer_radius, end_angle, start_angle);
        // Draw line segment back to start angle of outer curve
        path_builder.line_to(
            center.x + inner_radius * start_angle.cos(),
            center.y + inner_radius * start_angle.sin(),
        );
        path_builder.close();
        paint.set_color(color);
        color
    };

    let (clip_start, clip_end) = match p.direction {
        TurningDirection::CounterClockwise => (0.0, TAU),
        TurningDirection::Clockwise => (TAU, 0.0),
    };

    for draw_markers in [false, true].iter() {
        let ti = r.ch.c() as usize;
        let track_meta = r_metadata;

        let outer_radius = total_radius - (ti as f32 * track_width);
        let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));
        let mut paint = Paint {
            blend_mode: BlendMode::SourceOver,
            anti_alias: true,
            ..Default::default()
        };

        let mut phys_s: u8 = 0; // Physical sector index, 0-indexed from first sector on track

        // Draw non-overlapping metadata.
        for (_mi, meta_item) in track_meta.items.iter().enumerate() {
            let generic_elem = GenericTrackElement::from(meta_item.element);
            if let GenericTrackElement::Marker = generic_elem {
                if !*draw_markers {
                    continue;
                }
            }
            else if *draw_markers {
                continue;
            }

            // Advance physical sector number for each sector header encountered.
            if meta_item.element.is_sector_header() {
                phys_s = phys_s.wrapping_add(1);
            }

            if !meta_item.element.is_sector_data() || ((phys_s as usize) < r.sector_idx) {
                continue;
            }

            let mut path_builder = PathBuilder::new();
            let mut start_angle = ((meta_item.start as f32 / track_len as f32) * TAU) + p.index_angle;
            let mut end_angle = ((meta_item.end as f32 / track_len as f32) * TAU) + p.index_angle;

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

            draw_sector_slice(
                &mut path_builder,
                &mut paint,
                start_angle,
                end_angle,
                inner_radius,
                outer_radius,
                r.color.into(),
            );

            if let Some(path) = path_builder.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            }

            // Rendered one sector, stop.
            break;
        }
    }

    Ok(())
}

fn add_arc(
    path: &mut PathBuilder,
    center: Point,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    //direction: RotationDirection,
) {
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

    path.move_to(x1, y1);
    path.cubic_to(x2, y2, x3, y3, x4, y4);
}

pub fn draw_index_hole(
    pixmap: &mut Pixmap,
    offset_radius: f32,
    angle: f32,
    circle_radius: f32,
    stroke_width: f32,
    color: Color,
    direction: TurningDirection,
) {
    let center_x = pixmap.width() as f32 / 2.0;
    let center_y = pixmap.height() as f32 / 2.0;
    let max_radius = center_x.min(center_y);
    let scaled_radius = offset_radius * max_radius;

    let normalized_angle = match direction {
        TurningDirection::CounterClockwise => angle,
        TurningDirection::Clockwise => TAU - angle,
    };

    let offset_x = center_x + scaled_radius * normalized_angle.cos();
    let offset_y = center_y + scaled_radius * normalized_angle.sin();

    let mut pb = PathBuilder::new();
    pb.push_circle(offset_x, offset_y, circle_radius);
    let path = pb.finish().unwrap();

    let mut paint = Paint::default();
    paint.set_color(color);

    let stroke = Stroke {
        width: stroke_width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}
