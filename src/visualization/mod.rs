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

    src/visualization/mod.rs

    Visualization module for fluxfox. Contains code for rendering disk images
    to images. Requires the 'vis' feature to be enabled.
*/

//! The `visualization` module provides rendering functions for disk images.
//! This module requires the `viz` feature to be enabled. Graphics support is provided by the
//! `tiny-skia` crate, which will be re-exported.
//!
//! The `imgviz` example in the repository demonstrates how to use the visualization functions.

use crate::{
    bitstream::TrackDataStream,
    structure_parsers::{
        system34::System34Element,
        DiskStructureElement,
        DiskStructureGenericElement,
        DiskStructureMetadata,
    },
};

use crate::{DiskImage, DiskImageError, DiskVisualizationError, FoxHashMap};
use bit_vec::BitVec;
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

/// A map type selector for visualization functions.
#[derive(Copy, Clone, Debug)]
pub enum RenderMapType {
    /// Choose to render the weak bit mask
    WeakBits,
    /// Choose to render the bitstream error mask
    Errors,
}

/// Parameter struct for use with disk surface rendering functions
pub struct RenderTrackDataParams {
    /// Background color to use for area outside of disk ring. If None, the image will be transparent.
    pub bg_color: Option<Color>,
    /// Color to use when rendering a track bit map.
    pub map_color: Option<Color>,
    /// Which side of disk to render
    pub head: u8,
    /// Destination Pixmap size in pixels
    pub image_size: (u32, u32),
    /// Render position in pixels. Image must fit - no clipping is performed.
    pub image_pos: (u32, u32),
    /// Minimum inner radius as a fraction (0.0 to 1.0)
    pub min_radius_fraction: f32,
    /// Angle of index position / start of track
    pub index_angle: f32,
    /// Maximum number of tracks to render
    pub track_limit: usize,
    /// Width of the gap between tracks as a fraction (0.0 to 1.0)
    pub track_gap: f32,
    /// Rotational direction for rendering (Clockwise or CounterClockwise)
    pub direction: RotationDirection,
    /// Decode data in sectors for more visual contrast
    pub decode: bool,
    /// Resolution to render data at (Bit or Byte)
    pub resolution: ResolutionType,
    /// Set the inner radius to the last standard track instead of last track
    /// This keeps proportions consistent between disks with different track counts
    pub pin_last_standard_track: bool,
}

/// Parameter struct for use with disk metadata rendering functions
pub struct RenderTrackMetadataParams {
    /// Which quadrant to render (0-3)
    pub quadrant: u8,
    /// Which side of disk to render
    pub head: u8,
    /// Minimum inner radius as a fraction (0.0 to 1.0)
    pub min_radius_fraction: f32,
    /// Angle of index position / start of track
    pub index_angle: f32,
    /// Maximum number of tracks to render
    pub track_limit: usize,
    /// Width of the gap between tracks as a fraction (0.0 to 1.0)
    pub track_gap: f32,
    /// Rotational direction for rendering (Clockwise or CounterClockwise)
    pub direction: RotationDirection,
    /// Palette to use for rendering metadata elements
    pub palette: FoxHashMap<DiskStructureGenericElement, Color>,
    /// Whether to draw empty tracks as black rings
    pub draw_empty_tracks: bool,
    /// Set the inner radius to the last standard track instead of last track
    /// This keeps proportions consistent between disks with different track counts
    pub pin_last_standard_track: bool,
}

/// Determines the direction of disk surface rotation for visualization functions.
/// Typically, Side 0, the bottom-facing side of a disk, rotates counter-clockwise when viewed
/// from the bottom, and Side 1, the top-facing side, rotates clockwise.
#[derive(Copy, Clone, Debug)]
pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

impl RotationDirection {
    pub fn opposite(&self) -> Self {
        match self {
            RotationDirection::Clockwise => RotationDirection::CounterClockwise,
            RotationDirection::CounterClockwise => RotationDirection::Clockwise,
        }
    }
}

/// Determines the visualization resolution - either byte resolution or bit resolution.
/// Bit resolution requires extremely high resolution output to be legible.
#[derive(Copy, Clone, Debug)]
pub enum ResolutionType {
    Bit,
    Byte,
}

/// Create a lookup table to map a u8 value to a grayscale gradient value based on the number of
/// bits set in the u8 value (popcount)
const POPCOUNT_TABLE: [u8; 256] = {
    let values: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = values[i.count_ones() as usize];
        i += 1;
    }
    table
};

fn collect_streams(head: u8, disk_image: &DiskImage) -> Vec<&TrackDataStream> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].get_track_stream())
        .collect()
}

fn collect_weak_masks(head: u8, disk_image: &DiskImage) -> Vec<&BitVec> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| {
            disk_image.track_pool[*track_i]
                .get_track_stream()
                .map(|track| track.weak_mask())
        })
        .collect()
}

fn collect_error_maps(head: u8, disk_image: &DiskImage) -> Vec<&BitVec> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| {
            disk_image.track_pool[*track_i]
                .get_track_stream()
                .map(|track| track.error_map())
        })
        .collect()
}

fn collect_metadata(head: u8, disk_image: &DiskImage) -> Vec<&DiskStructureMetadata> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].metadata())
        .collect()
}

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
        let overdump = num_tracks - normalized_track_ct;
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

            if distance >= min_radius && distance <= total_radius {
                let track_offset = (distance - min_radius) / track_width;
                if track_offset.fract() < p.track_gap {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < num_tracks {
                    // Adjust angle for clockwise or counter-clockwise
                    let normalized_angle = match p.direction {
                        RotationDirection::Clockwise => angle - p.index_angle,
                        RotationDirection::CounterClockwise => TAU - (angle - p.index_angle),
                    };

                    let normalized_angle = (normalized_angle + PI) % TAU;
                    let bit_index = ((normalized_angle / TAU) * rtracks[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    let bit_index = min(bit_index, rtracks[track_index].len() - 9);

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
                                false => rtracks[track_index].read_raw_byte(bit_index).unwrap_or_default(),
                                true => {
                                    // Only render bits in 16-bit steps.
                                    rtracks[track_index]
                                        .read_decoded_byte(decoded_bit_idx)
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
pub fn render_track_map(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    map: RenderMapType,
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
        RenderMapType::WeakBits => collect_weak_masks(p.head, disk_image),
        RenderMapType::Errors => collect_error_maps(p.head, disk_image),
    };
    let num_tracks = min(track_refs.len(), p.track_limit);

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
                        RotationDirection::Clockwise => angle - p.index_angle,
                        RotationDirection::CounterClockwise => TAU - (angle - p.index_angle),
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

/// Render a representation of a disk's data to a `tiny_skia::Pixmap`, for a specific quadrant of the unit circle.
/// Rendering is performed in quadrants to allow for multithreaded rendering of each quadrant.
pub fn render_track_metadata_quadrant(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    p: &RenderTrackMetadataParams,
) -> Result<(), DiskVisualizationError> {
    let rtracks = collect_streams(p.head, disk_image);
    let rmetadata = collect_metadata(p.head, disk_image);
    let num_tracks = min(rtracks.len(), p.track_limit);

    if num_tracks == 0 {
        return Err(DiskVisualizationError::NoTracks);
    }

    let overlap_max = (1024 + 6) * 16;
    let image_size = pixmap.width() as f32 * 2.0;
    let total_radius = image_size / 2.0;
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
        let overdump = num_tracks - normalized_track_ct;
        p.min_radius_fraction * total_radius - (overdump as f32 * track_width)
    }
    else {
        min_radius
    };

    let track_width = (total_radius - min_radius) / num_tracks as f32;

    let center = match p.quadrant {
        0 => Point::from_xy(total_radius, total_radius),
        1 => Point::from_xy(0.0, total_radius),
        2 => Point::from_xy(total_radius, 0.0),
        3 => Point::from_xy(0.0, 0.0),
        _ => return Err(DiskVisualizationError::InvalidParameter),
    };

    let mut path_builder = PathBuilder::new();
    // let quadrant_angles_cw = match quadrant {
    //     0 => (PI, PI / 2.0),
    //     1 => (PI / 2.0, 0.0),
    //     2 => (0.0, 3.0 * PI / 2.0),
    //     3 => (3.0 * PI / 2.0, PI),
    //     _ => panic!("Invalid quadrant"),
    // };
    let quadrant_angles_cc = match p.quadrant {
        0 => (PI, 3.0 * PI / 2.0),
        1 => (3.0 * PI / 2.0, 2.0 * PI),
        2 => (PI / 2.0, PI),
        3 => (0.0, PI / 2.0),
        _ => return Err(DiskVisualizationError::InvalidParameter),
    };

    //println!("Rendering side {:?}", direction);
    let null_color = Color::from_rgba(0.0, 0.0, 0.0, 0.0).unwrap();

    let draw_metadata_slice = |path_builder: &mut PathBuilder,
                               paint: &mut Paint,
                               start_angle: f32,
                               end_angle: f32,
                               inner_radius: f32,
                               outer_radius: f32,
                               element_type: Option<DiskStructureElement>|
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
            let generic_elem = DiskStructureGenericElement::from(element_type);
            color = p.palette.get(&generic_elem).unwrap_or(&null_color);
        }
        else {
            color = &Color::BLACK;
        }

        paint.set_color(*color);
        *color
    };

    let (clip_start, clip_end) = match p.direction {
        RotationDirection::CounterClockwise => (quadrant_angles_cc.0, quadrant_angles_cc.1),
        RotationDirection::Clockwise => (quadrant_angles_cc.0, quadrant_angles_cc.1),
    };

    for draw_markers in [false, true].iter() {
        for (ti, track_meta) in rmetadata.iter().enumerate() {
            let mut has_elements = false;
            let outer_radius = total_radius - (ti as f32 * track_width);
            let inner_radius = outer_radius - (track_width * (1.0 - p.track_gap));
            let mut paint = Paint {
                blend_mode: BlendMode::SourceOver,
                ..Default::default()
            };

            // Look for metadata items crossing the index, and draw them first.
            // We limit the maximum index overlap as an 8192 byte sector at the end of a track will
            // wrap the index twice.
            if !*draw_markers {
                for meta_item in track_meta.items.iter() {
                    if meta_item.end >= rtracks[ti].len() {
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
                            RotationDirection::CounterClockwise => (start_angle, end_angle),
                            RotationDirection::Clockwise => (TAU - start_angle, TAU - end_angle),
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
                            Some(meta_item.elem_type),
                        );

                        //let overlap_long = false;
                        if overlap_long {
                            // Long elements are gradually faded out across the index to imply they continue.
                            let end_color =
                                Color::from_rgba(start_color.red(), start_color.green(), start_color.blue(), 0.0)
                                    .unwrap();

                            let (start_pt, end_pt) = match p.direction {
                                RotationDirection::CounterClockwise => (
                                    Point::from_xy(center.x, 0.0),
                                    Point::from_xy(center.x, total_radius / 8.0),
                                ),
                                RotationDirection::Clockwise => (
                                    Point::from_xy(center.x, center.y),
                                    Point::from_xy(center.x, center.y - total_radius / 8.0),
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

            // Draw non-overlapping metadata.
            for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                if let DiskStructureElement::System34(System34Element::Marker(..)) = meta_item.elem_type {
                    if !*draw_markers {
                        continue;
                    }
                }
                else if *draw_markers {
                    continue;
                }

                has_elements = true;

                let mut start_angle = ((meta_item.start as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;
                let mut end_angle = ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU) + p.index_angle;

                if start_angle > end_angle {
                    std::mem::swap(&mut start_angle, &mut end_angle);
                }

                (start_angle, end_angle) = match p.direction {
                    RotationDirection::CounterClockwise => (start_angle, end_angle),
                    RotationDirection::Clockwise => (TAU - start_angle, TAU - end_angle),
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

                draw_metadata_slice(
                    &mut path_builder,
                    &mut paint,
                    start_angle,
                    end_angle,
                    inner_radius,
                    outer_radius,
                    Some(meta_item.elem_type),
                );

                if let Some(path) = path_builder.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                }

                path_builder = PathBuilder::new(); // Reset the path builder for the next sector
            }

            // If a track contained no elements, draw a black ring
            if !has_elements && p.draw_empty_tracks {
                draw_metadata_slice(
                    &mut path_builder,
                    &mut paint,
                    clip_start,
                    clip_end,
                    inner_radius,
                    outer_radius,
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
    direction: RotationDirection,
) {
    let center_x = pixmap.width() as f32 / 2.0;
    let center_y = pixmap.height() as f32 / 2.0;
    let max_radius = center_x.min(center_y);
    let scaled_radius = offset_radius * max_radius;

    let normalized_angle = match direction {
        RotationDirection::CounterClockwise => angle,
        RotationDirection::Clockwise => TAU - angle,
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
