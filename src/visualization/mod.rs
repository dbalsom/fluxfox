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
use image::{ImageBuffer, Rgb, Rgba, RgbaImage};

use std::cmp::min;
use std::f32::consts::PI;

pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

// Define an enum to choose between bit and byte resolution
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
    disk_image.tracks[head as usize]
        .iter()
        .filter_map(|track| match track.data {
            TrackData::BitStream { ref data, .. } => Some(data),
            _ => None,
        })
        .collect()
}

/// Render a disk image to an image buffer.
pub fn render_tracks(
    disk_image: &DiskImage,
    head: u8,
    //tracks: Vec<BitVec>,
    image_size: (u32, u32),
    min_radius_fraction: f32, // Minimum radius as a fraction (0.0 to 1.0)
    track_gap_weight: f32,
    direction: RotationDirection, // Added parameter for rotation direction
    resolution: ResolutionType,   // Added parameter for resolution type
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, DiskImageError> {
    let (width, height) = image_size;
    let mut imgbuf = RgbaImage::new(width, height);

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let total_radius = width.min(height) as f32 / 2.0;
    let min_radius = min_radius_fraction * total_radius; // Scale min_radius to pixel value

    let rtracks = collect_streams(head, disk_image);
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
            let angle = (dy.atan2(dx) + PI) % (2.0 * PI);

            if distance >= min_radius && distance <= total_radius {
                let track_index =
                    (num_tracks - 1).saturating_sub(((distance - min_radius) / track_width).floor() as usize) as usize;

                if track_index < num_tracks {
                    // Adjust angle for clockwise or counter-clockwise
                    let normalized_angle = if matches!(direction, RotationDirection::Clockwise) {
                        angle
                    } else {
                        2.0 * PI - angle
                    };

                    let normalized_angle = (normalized_angle + PI) % (2.0 * PI);
                    let bit_index = ((normalized_angle / (2.0 * PI)) * rtracks[track_index].len() as f32) as usize;

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

                            if track_index > 39 {
                                Rgba([255, 0, 0, 255])
                            } else if track_index == 0 {
                                Rgba([gray_value, gray_value, 255, 255])
                            } else {
                                Rgba([gray_value, gray_value, gray_value, 255])
                            }
                        }
                    };

                    imgbuf.put_pixel(x, y, color);
                }
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
                    imgbuf.put_pixel(x, y, color);
                }
            }
        }
    }

    Ok(imgbuf)
}
