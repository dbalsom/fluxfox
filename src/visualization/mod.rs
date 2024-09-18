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

use crate::bitstream::TrackDataStream;
use crate::structure_parsers::system34::System34Element;
use crate::structure_parsers::{DiskStructureElement, DiskStructureGenericElement, DiskStructureMetadata};
use crate::trackdata::TrackData;
use crate::{DiskImage, DiskImageError, FoxHashMap};
use std::cmp::min;
use std::f32::consts::{PI, TAU};
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Point, PremultipliedColorU8, Stroke,
    Transform,
};

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

// Define an enum to choose between bit and byte resolution
#[derive(Copy, Clone, Debug)]
pub enum ResolutionType {
    Bit,
    Byte,
}

/*
impl From<System34Element> for Rgba<u8> {
    #[rustfmt::skip]
    fn from(element: System34Element) -> Self {
        match element {
            System34Element::Gap1 => Rgba([0, 0, 0, 128]),
            System34Element::Gap2 => Rgba([0, 0, 0, 128]),
            System34Element::Gap3 => Rgba([0, 0, 0, 128]),
            System34Element::Gap4a => Rgba([0, 0, 0, 128]),
            System34Element::Gap4b => Rgba([0, 0, 0, 128]),
            System34Element::Sync => Rgba([0, 255, 255, 128]),
            System34Element::Marker(System34Marker::Iam, Some(false)) => Rgba([255, 0, 255, 128]),
            System34Element::Marker(System34Marker::Iam, _) => Rgba([0, 0, 255, 128]),
            System34Element::Marker(System34Marker::Idam, _) => Rgba([0, 128, 255, 200]),
            System34Element::Marker(System34Marker::Dam, _) => Rgba([255, 255, 0, 200]),
            System34Element::Marker(System34Marker::Ddam, _) => Rgba([255, 255, 0, 200]),
            System34Element::Data { address_crc: true, data_crc: true, .. } => Rgba([0, 255, 0, 64]),
            System34Element::Data { address_crc: true, data_crc: false, .. } => Rgba([255, 128, 0, 64]),
            System34Element::Data { address_crc: false, data_crc: true, .. } => Rgba([0, 255, 0, 64]),
            System34Element::Data { address_crc: false, data_crc: false, .. } => Rgba([255, 128, 0, 64]),
        }
    }
}

impl System34Element {
    pub fn alpha(&self) -> u8 {
        match self {
            System34Element::Gap1 => 128,
            System34Element::Gap2 => 128,
            System34Element::Gap3 => 128,
            System34Element::Gap4a => 128,
            System34Element::Gap4b => 128,
            System34Element::Sync => 128,
            System34Element::Marker(System34Marker::Iam, Some(false)) => 200,
            System34Element::Marker(System34Marker::Iam, _) => 200,
            System34Element::Marker(System34Marker::Idam, _) => 200,
            System34Element::Marker(System34Marker::Dam, _) => 200,
            System34Element::Marker(System34Marker::Ddam, _) => 200,
            System34Element::Data { .. } => 64,
        }
    }

    #[rustfmt::skip]
    pub fn to_rgba_nested(&self, nest_lvl: u32) -> Rgba<u8> {
        match (nest_lvl, self) {
            (_, System34Element::Gap1) => Rgba([0, 0, 0, 128]),
            (_, System34Element::Gap2) => Rgba([0, 0, 0, 128]),
            (_, System34Element::Gap3) => Rgba([0, 0, 0, 128]),
            (_, System34Element::Gap4a) => Rgba([0, 0, 0, 128]),
            (_, System34Element::Gap4b) => Rgba([0, 0, 0, 128]),
            (_, System34Element::Sync) => Rgba([0, 255, 255, 128]),
            (_, System34Element::Marker(System34Marker::Iam, Some(false))) => Rgba([255, 0, 255, 128]),
            (_, System34Element::Marker(System34Marker::Iam, _)) => Rgba([0, 0, 255, 128]),
            (_, System34Element::Marker(System34Marker::Idam, _)) => Rgba([0, 128, 255, 200]),
            (_, System34Element::Marker(System34Marker::Dam, _)) => Rgba([255, 255, 0, 200]),
            (_, System34Element::Marker(System34Marker::Ddam, _)) => Rgba([255, 255, 0, 200]),
            (1, System34Element::Data { data_crc: true, deleted: false, .. }) => Rgba([0, 255, 0, 64]),
            (_, System34Element::Data { data_crc: true, deleted: false, .. }) => Rgba([0, 255, 168, 80]),
            (1, System34Element::Data { data_crc: true, deleted: true, .. }) => Rgba([0, 0, 255, 64]),
            (_, System34Element::Data { data_crc: true, deleted: true, .. }) => Rgba([0, 168, 255, 80]),
            (1, System34Element::Data { data_crc: false, .. }) => Rgba([255, 128, 0, 128]),
            (_, System34Element::Data { data_crc: false, .. }) => Rgba([255, 128, 168, 160]),
        }
    }
}*/

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
        .filter_map(|track_i| match disk_image.track_pool[*track_i].data {
            TrackData::BitStream { ref data, .. } => Some(data),
            _ => None,
        })
        .collect()
}

fn collect_metadata(head: u8, disk_image: &DiskImage) -> Vec<&DiskStructureMetadata> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].metadata())
        .collect()
}

/*
/// Render a disk image to an image buffer.
pub fn render_tracks(
    disk_image: &DiskImage,
    imgbuf: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    head: u8,
    image_size: (u32, u32),
    image_pos: (u32, u32),
    min_radius_fraction: f32, // Minimum radius as a fraction (0.0 to 1.0)
    track_gap_weight: f32,
    direction: RotationDirection, // Added parameter for rotation direction
    resolution: ResolutionType,   // Added parameter for resolution type
    colorize: bool,
) -> Result<(), DiskImageError> {
    let (width, height) = image_size;
    let (x_offset, y_offset) = image_pos;

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let min_radius = min_radius_fraction * total_radius; // Scale min_radius to pixel value

    let rtracks = collect_streams(head, disk_image);
    let rmetadata = collect_metadata(head, disk_image);
    let num_tracks = rtracks.len();

    log::trace!("collected {} track references.", num_tracks);
    for (ti, track) in rtracks.iter().enumerate() {
        log::trace!("track {} length: {}", ti, track.len());
    }

    let track_width = (total_radius - min_radius) / num_tracks as f32;

    // Draw the tracks
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let angle = (dy.atan2(dx) + PI) % TAU;

            if distance >= min_radius && distance <= total_radius {
                let track_index =
                    (num_tracks - 1).saturating_sub(((distance - min_radius) / track_width).floor() as usize) as usize;

                if track_index < num_tracks {
                    // Adjust angle for clockwise or counter-clockwise
                    let normalized_angle = if matches!(direction, RotationDirection::Clockwise) {
                        angle
                    } else {
                        TAU - angle
                    };

                    let normalized_angle = (normalized_angle + PI) % TAU;
                    let bit_index = ((normalized_angle / TAU) * rtracks[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    let bit_index = min(bit_index, rtracks[track_index].len() - 1);

                    let color = match resolution {
                        ResolutionType::Bit => {
                            if rtracks[track_index][bit_index] {
                                Rgba([255, 255, 255, 255])
                            } else {
                                Rgba([0, 0, 0, 0])
                            }
                        }
                        ResolutionType::Byte => {
                            // Calculate the byte value
                            let byte_index = bit_index / 8;
                            let _bit_offset = bit_index % 8;
                            let byte_value = if byte_index < rtracks[track_index].len() / 8 - 1 {
                                let mut build_byte: u8 = 0;
                                for bi in 0..8 {
                                    build_byte |= if rtracks[track_index][bit_index + bi] { 1 } else { 0 };
                                    build_byte <<= 1;
                                }
                                build_byte
                            } else {
                                0
                            };

                            let gray_value = POPCOUNT_TABLE[byte_value as usize];

                            let mut data_color = if track_index > 39 {
                                Rgba([255, 0, 0, 255])
                            } else if track_index == 0 {
                                //Rgba([gray_value, gray_value, 255, 255])
                                Rgba([gray_value, gray_value, gray_value, 255])
                            } else {
                                Rgba([gray_value, gray_value, gray_value, 255])
                            };

                            let meta_color: Option<Rgba<u8>> = match rmetadata[track_index].item_at(bit_index << 1) {
                                Some((item, nest_ct)) => {
                                    if let DiskStructureElement::System34(element) = item.elem_type {
                                        Some(element.to_rgba_nested(nest_ct))
                                    } else {
                                        None
                                    }
                                }
                                None => None,
                            };

                            if let Some(meta_color) = meta_color {
                                if colorize {
                                    data_color.blend(&meta_color);
                                }
                                data_color
                            } else {
                                data_color
                            }
                        }
                    };

                    imgbuf.put_pixel(x + x_offset, y + y_offset, color);
                }
            } else {
                imgbuf.put_pixel(x + x_offset, y + y_offset, Rgba([0, 0, 0, 0]));
            }
        }
    }

    // Draw inter-track gaps
    for i in 0..=num_tracks {
        let radius = min_radius + i as f32 * track_width;

        for y in 0..height {
            for x in 0..width {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance >= radius - track_gap_weight / 2.0 && distance <= radius + track_gap_weight / 2.0 {
                    let color = Rgba([0, 0, 0, 255]);
                    imgbuf.put_pixel(x + x_offset, y + y_offset, color);
                }
            }
        }
    }

    Ok(())
}
*/
/// Render a representation of a disk's data to a Pixmap.
/// Used as a base for other visualization functions.
pub fn render_track_data(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    head: u8,
    image_size: (u32, u32),
    image_pos: (u32, u32),
    min_radius_fraction: f32, // Minimum radius as a fraction (0.0 to 1.0)
    index_angle: f32,         // Index angle (0 starts data at 90 degrees cw)
    track_limit: usize,
    track_gap_weight: f32,
    direction: RotationDirection, // Added parameter for rotation direction
    decode: bool,                 // Decode data (true) or render as raw MFM (false)
    resolution: ResolutionType,   // Added parameter for resolution type
) -> Result<(), DiskImageError> {
    let (width, height) = image_size;
    let span = pixmap.width();

    let (x_offset, y_offset) = image_pos;

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let min_radius = min_radius_fraction * total_radius; // Scale min_radius to pixel value
    let _min_radius_sq = min_radius * min_radius;

    let rtracks = collect_streams(head, disk_image);
    //let rmetadata = collect_metadata(head, disk_image);
    let num_tracks = min(rtracks.len(), track_limit);

    log::trace!("collected {} track references.", num_tracks);
    for (ti, track) in rtracks.iter().enumerate() {
        log::trace!("track {} length: {}", ti, track.len());
    }

    //println!("track data type is : {:?}", rtracks[0]);

    let track_width = (total_radius - min_radius) / num_tracks as f32;
    let _track_width_sq = track_width * track_width;
    let _render_track_width = track_width * (1.0 - track_gap_weight);

    let pix_buf = pixmap.pixels_mut();

    let color_black = PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
    let color_white = PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();
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
                if track_offset.fract() < track_gap_weight {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < num_tracks {
                    // Adjust angle for clockwise or counter-clockwise
                    let normalized_angle = match direction {
                        RotationDirection::Clockwise => angle - index_angle,
                        RotationDirection::CounterClockwise => TAU - (angle - index_angle),
                    };

                    let normalized_angle = (normalized_angle + PI) % TAU;
                    let bit_index = ((normalized_angle / TAU) * rtracks[track_index].len() as f32) as usize;

                    // Ensure bit_index is within bounds
                    let bit_index = min(bit_index, rtracks[track_index].len() - 9);

                    let color = match resolution {
                        ResolutionType::Bit => {
                            if rtracks[track_index][bit_index] {
                                color_white
                            } else {
                                color_black
                            }
                        }
                        ResolutionType::Byte => {
                            // Calculate the byte value
                            let byte_value = match decode {
                                false => rtracks[track_index].read_byte(bit_index).unwrap_or_default(),
                                true => {
                                    // Only render bits in 16-bit steps.
                                    let decoded_bit_idx = (bit_index) & !0xF;
                                    rtracks[track_index]
                                        .read_decoded_byte(decoded_bit_idx)
                                        .unwrap_or_default()
                                }
                            };

                            // let byte_value = if byte_index < rtracks[track_index].len() / 8 - 1 {
                            //     let mut build_byte: u8 = 0;
                            //     for bi in 0..8 {
                            //         build_byte |= if rtracks[track_index][bit_index + bi] { 1 } else { 0 };
                            //         build_byte <<= 1;
                            //     }
                            //     build_byte
                            // } else {
                            //     0
                            // };

                            let gray_value = POPCOUNT_TABLE[byte_value as usize];

                            PremultipliedColorU8::from_rgba(gray_value, gray_value, gray_value, 255).unwrap()
                        }
                    };

                    pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = color;
                }
            } else {
                pix_buf[((y + y_offset) * span + (x + x_offset)) as usize] = color_trans;
                //imgbuf.put_pixel(x + x_offset, y + y_offset, Rgba([0, 0, 0, 0]));
            }
        }
    }

    // Draw inter-track gaps
    /*    for i in 0..=num_tracks {
        let radius = min_radius + i as f32 * track_width;

        for y in 0..height {
            for x in 0..width {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance >= radius - track_gap_weight / 2.0 && distance <= radius + track_gap_weight / 2.0 {
                    let color = Rgba([0, 0, 0, 255]);
                    imgbuf.put_pixel(x + x_offset, y + y_offset, color);
                }
            }
        }
    }*/

    Ok(())
}

fn calculate_track_width(total_tracks: usize, image_radius: f64, min_radius_ratio: f64) -> f64 {
    (image_radius - (image_radius * min_radius_ratio)) / total_tracks as f64
}

/// Render a representation of a disk's data to a Pixmap.
/// Used as a base for other visualization functions.
pub fn render_track_metadata_quadrant(
    disk_image: &DiskImage,
    pixmap: &mut Pixmap,
    quadrant: u8,
    head: u8,
    min_radius_ratio: f32, // Minimum radius as a fraction (0.0 to 1.0)
    index_angle: f32,
    track_limit: usize,
    track_gap: f32,
    direction: RotationDirection, // Added parameter for rotation direction
    palette: FoxHashMap<DiskStructureGenericElement, Color>,
) -> Result<(), DiskImageError> {
    let rtracks = collect_streams(head, disk_image);
    let rmetadata = collect_metadata(head, disk_image);
    let total_tracks = min(rtracks.len(), track_limit);

    let image_size = pixmap.width() as f64 * 2.0;
    let image_radius = image_size / 2.0;
    let track_width = calculate_track_width(total_tracks, image_radius, min_radius_ratio as f64);

    let center = match quadrant {
        0 => Point::from_xy(image_radius as f32, image_radius as f32),
        1 => Point::from_xy(0.0, image_radius as f32),
        2 => Point::from_xy(image_radius as f32, 0.0),
        3 => Point::from_xy(0.0, 0.0),
        _ => panic!("Invalid quadrant"),
    };

    let mut path_builder = PathBuilder::new();
    // let quadrant_angles_cw = match quadrant {
    //     0 => (PI, PI / 2.0),
    //     1 => (PI / 2.0, 0.0),
    //     2 => (0.0, 3.0 * PI / 2.0),
    //     3 => (3.0 * PI / 2.0, PI),
    //     _ => panic!("Invalid quadrant"),
    // };
    let quadrant_angles_cc = match quadrant {
        0 => (PI, 3.0 * PI / 2.0),
        1 => (3.0 * PI / 2.0, 2.0 * PI),
        2 => (PI / 2.0, PI),
        3 => (0.0, PI / 2.0),
        _ => panic!("Invalid quadrant"),
    };

    //println!("Rendering side {:?}", direction);

    for draw_markers in [false, true].iter() {
        for (ti, track_meta) in rmetadata.iter().enumerate() {
            for (_mi, meta_item) in track_meta.items.iter().enumerate() {
                if let DiskStructureElement::System34(System34Element::Marker(..)) = meta_item.elem_type {
                    if !*draw_markers {
                        continue;
                    }
                } else if *draw_markers {
                    continue;
                }

                let outer_radius = image_radius as f32 - (ti as f32 * track_width as f32);
                let inner_radius = outer_radius - (track_width as f32 * (1.0 - track_gap));

                let mut start_angle = ((meta_item.start as f32 / rtracks[ti].len() as f32) * TAU) + index_angle;
                let mut end_angle = ((meta_item.end as f32 / rtracks[ti].len() as f32) * TAU) + index_angle;

                if start_angle > end_angle {
                    std::mem::swap(&mut start_angle, &mut end_angle);
                }

                let (clip_start, clip_end) = match direction {
                    RotationDirection::CounterClockwise => (quadrant_angles_cc.0, quadrant_angles_cc.1),
                    RotationDirection::Clockwise => (quadrant_angles_cc.0, quadrant_angles_cc.1),
                };

                (start_angle, end_angle) = match direction {
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

                // Draw the outer curve
                add_arc(
                    &mut path_builder,
                    center,
                    inner_radius,
                    start_angle.max(clip_start),
                    end_angle.min(clip_end),
                );
                // Draw line segment to end angle of inner curve
                path_builder.line_to(
                    center.x + outer_radius * end_angle.cos(),
                    center.y + outer_radius * end_angle.sin(),
                );
                // Draw inner curve back to start angle
                add_arc(
                    &mut path_builder,
                    center,
                    outer_radius,
                    end_angle.min(clip_end),
                    start_angle.max(clip_start),
                );
                // Draw line segment back to start angle of outer curve
                path_builder.line_to(
                    center.x + inner_radius * start_angle.cos(),
                    center.y + inner_radius * start_angle.sin(),
                );
                path_builder.close();

                // Use a predefined color for each sector
                let generic_elem = DiskStructureGenericElement::from(meta_item.elem_type);
                let null_color = Color::from_rgba(0.0, 0.0, 0.0, 0.0).unwrap();
                let color = palette.get(&generic_elem).unwrap_or(&null_color);

                let mut paint = Paint {
                    blend_mode: BlendMode::SourceOver,
                    ..Default::default()
                };
                paint.set_color(*color);

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
