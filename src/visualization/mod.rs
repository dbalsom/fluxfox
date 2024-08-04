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

use crate::diskimage::{TrackData, TrackDataStream};
use crate::{DiskImage, DiskImageError};
use image::{ImageBuffer, Pixel, Rgba};

use crate::structure_parsers::system34::{System34Element, System34Marker};
use crate::structure_parsers::{DiskStructureElement, DiskStructureMetadata};
use std::cmp::min;
use std::f32::consts::{PI, TAU};

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

impl From<System34Element> for Rgba<u8> {
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
            System34Element::Data(true) => Rgba([0, 255, 0, 64]),
            System34Element::Data(false) => Rgba([255, 128, 0, 64]),
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
            System34Element::Data(_) => 64,
        }
    }

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
            (1, System34Element::Data(true)) => Rgba([0, 255, 0, 64]),
            (_, System34Element::Data(true)) => Rgba([0, 255, 168, 80]),
            (1, System34Element::Data(false)) => Rgba([255, 128, 0, 128]),
            (_, System34Element::Data(false)) => Rgba([255, 128, 168, 160]),
        }
    }
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
        .filter_map(|track_i| match disk_image.track_pool[*track_i].data {
            TrackData::BitStream { ref data, .. } => Some(data),
            _ => None,
        })
        .collect()
}

fn collect_metadata(head: u8, disk_image: &DiskImage) -> Vec<&DiskStructureMetadata> {
    disk_image.track_map[head as usize]
        .iter()
        .map(|track_i| &disk_image.track_pool[*track_i].metadata)
        .collect()
}

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
