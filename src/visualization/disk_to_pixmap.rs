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

//! Module for rendering disk images to a pixmap, requiring tiny_skia

use crate::{
    visualization::{
        collect_error_maps,
        collect_metadata,
        collect_streams,
        collect_weak_masks,
        RenderMaskType,
        RenderTrackDataParams,
        ResolutionType,
        TurningDirection,
        POPCOUNT_TABLE,
    },
    DiskImage,
    DiskImageError,
};
use std::{
    cmp::min,
    f32::consts::{PI, TAU},
};
use tiny_skia::*;

/// Render a representation of a disk's data to a Pixmap.
/// Used as a base for other visualization functions.
pub fn render_track_data(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    p: &RenderTrackDataParams,
) -> Result<(), DiskImageError> {
    let (width, height) = p.image_size;
    let span = pixmap.width();

    let (x_offset, y_offset) = p.image_pos;

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let mut min_radius = p.min_radius_fraction * total_radius; // Scale min_radius to pixel value

    let rtracks = collect_streams(p.head, disk_image);
    let rmetadata = collect_metadata(p.head, disk_image);
    let num_tracks = min(rtracks.len(), p.track_limit);

    if num_tracks == 0 {
        return Err(DiskImageError::IncompatibleImage("No tracks to visualize!".to_string()));
    }

    log::trace!("collected {} track references.", num_tracks);
    for (ti, track) in rtracks.iter().enumerate() {
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
        p.min_radius_fraction * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let pix_buf = pixmap.pixels_mut();

    let color_black = PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
    let color_white = PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();
    let color_bg: PremultipliedColorU8 = match p.bg_color {
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
                    if rtracks[track_index].is_empty() {
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
                    let bit_index = ((normalized_angle / TAU) * rtracks[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    //let bit_index = min(bit_index, rtracks[track_index].len() - 9);
                    let bit_index = bit_index % rtracks[track_index].len();

                    let color = match p.resolution {
                        ResolutionType::Bit => {
                            if rtracks[track_index][bit_index] {
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
                            let decode_override = p.decode
                                && !rmetadata[track_index].items.is_empty()
                                && rtracks[track_index].is_data(decoded_bit_idx, false);

                            let byte_value = match decode_override {
                                false => rtracks[track_index].read_raw_u8(bit_index).unwrap_or_default(),
                                true => {
                                    // Only render bits in 16-bit steps.
                                    rtracks[track_index]
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
    p: &RenderTrackDataParams,
) -> Result<(), DiskImageError> {
    let (width, height) = p.image_size;
    let span = pixmap.width();

    let (x_offset, y_offset) = p.image_pos;

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let mut min_radius = p.min_radius_fraction * total_radius; // Scale min_radius to pixel value

    let track_refs = match map {
        RenderMaskType::WeakBits => collect_weak_masks(p.head, disk_image),
        RenderMaskType::Errors => collect_error_maps(p.head, disk_image),
    };
    let num_tracks = min(track_refs.len(), p.track_limit);
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
        p.min_radius_fraction * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let _track_width_sq = track_width * track_width;
    let _render_track_width = track_width * (1.0 - p.track_gap);

    let pix_buf = pixmap.pixels_mut();

    let weak_color: PremultipliedColorU8 = match p.map_color {
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
                        pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = weak_color;
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
