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

use std::{
    cmp::min,
    f32::consts::{PI, TAU},
};

use crate::{
    visualization::{RenderTrackDataParams, TurningDirection},
    DiskImage,
    DiskImageError,
    MAX_CYLINDER,
};

use crate::visualization::{CommonVizParams, PixmapToDiskParams};
use tiny_skia::Pixmap;

const MFM_GRAYSCALE_RAMP: [u64; 16] = [
    0x8888888888888888, // popcount: 16
    0x888888888888888A, // popcount: 17
    0x888A88888888888A, // popcount: 18
    0x888A88A88888888A, // popcount: 19
    0x8A8A88A88888888A, // popcount: 20
    0xAA8A88A88888888A, // popcount: 21
    0xAA8A88A88888A88A, // popcount: 22
    0xAA8A8AA88888A88A, // popcount: 23
    0xAA8A8AA88888AA8A, // popcount: 24
    0xAAAA8AA88888AA8A, // popcount: 25
    0xAAAA8AAA8888AA8A, // popcount: 26
    0xAAAA8AAAA888AA8A, // popcount: 27
    0xAAAA8AAAA888AAAA, // popcount: 28
    0xAAAA8AAAAA88AAAA, // popcount: 29
    0xAAAAAAAAAA88AAAA, // popcount: 30
    0xAAAAAAAAAAAAAAAA, // popcount: 32
];

/// We can't collect mutable references to the track streams, so we collect the indices into the
/// track pool instead.
fn collect_stream_indices(head: u8, disk_image: &mut DiskImage) -> Vec<usize> {
    disk_image.track_map[head as usize].iter().copied().collect()
}

/// The reverse of the normal visualization logic, this function takes a pixmap and writes it to
/// the disk image. This is completely useless other than for novelty purposes.
pub fn render_pixmap_to_disk(
    pixmap: &Pixmap,
    disk_image: &mut DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    p2d: &PixmapToDiskParams,
) -> Result<(), DiskImageError> {
    let (sample_width, sample_height) = p2d.sample_size;
    let (img_width, img_height) = p2d.img_dimensions.to_tuple();
    let span = pixmap.width();

    let (x_offset, y_offset) = p2d.img_pos.to_tuple();

    if p2d.mask_resolution < 1 || p2d.mask_resolution > 8 {
        return Err(DiskImageError::ParameterError);
    }
    let index_mask: usize = !((1 << p2d.mask_resolution) - 1);
    log::debug!("render_pixmap_to_disk(): using bit index_mask: {:#08b}", index_mask);

    // We work in sampling coordinates, so we need to adjust the center and radius to match the
    // sampling resolution.
    let center_x = sample_width as f32 / 2.0;
    let center_y = sample_height as f32 / 2.0;
    let total_radius = sample_width.min(sample_height) as f32 / 2.0;
    let mut min_radius = p.min_radius_ratio * total_radius; // Scale min_radius to pixel value

    let track_indices = collect_stream_indices(r.side, disk_image);
    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(track_indices.len(), track_limit);

    log::trace!("collected {} track references.", num_tracks);

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
    let pix_buf = pixmap.pixels();

    let map_sample_uv = |uv: (u32, u32)| -> (u32, u32) {
        let (x, y) = uv;
        let x = x as f32 * (img_width as f32 / sample_width as f32);
        let y = y as f32 * (img_height as f32 / sample_height as f32);
        (x as u32, y as u32)
    };

    // Sample the image and write sampled pixels to the disk image.
    // The sampling resolution needs to be quite high, at least 4096x4096, to get a good result
    // without gaps between pixels on the track which will introduce MFM errors.
    for v in 0..sample_height {
        for u in 0..sample_width {
            let dx = u as f32 - center_x;
            let dy = v as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let angle = (dy.atan2(dx) + PI) % TAU;

            if distance >= min_radius && distance <= total_radius {
                let track_offset = (distance - min_radius) / track_width;
                if track_offset.fract() < p.track_gap {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < p2d.skip_tracks as usize {
                    continue;
                }
                if track_index < num_tracks {
                    // Adjust angle via input angle parameter, for clockwise or counter-clockwise turning
                    let mut normalized_angle = match p.direction {
                        TurningDirection::Clockwise => angle - p.index_angle,
                        TurningDirection::CounterClockwise => TAU - (angle - p.index_angle),
                    };
                    // Normalize the angle to the range 0..2π
                    while normalized_angle < 0.0 {
                        normalized_angle += TAU;
                    }
                    normalized_angle = (normalized_angle + PI) % TAU;

                    if let Some(track) = disk_image.track_pool[track_indices[track_index]].stream_mut() {
                        let bit_index = ((normalized_angle / TAU) * track.len() as f32) as usize;

                        let mut render_enable = true;

                        // Control rendering based on metadata if sector masking is enabled.
                        if r.sector_mask && !track.is_data(bit_index, false) {
                            render_enable = false;
                        }

                        if render_enable {
                            // Mask the bit index to the resolution specified
                            let bit_index = bit_index & index_mask;
                            // Ensure bit_index is within bounds
                            let bit_index = min(bit_index, track.len() - 9);

                            // We ignore resolution here - we can only render bytes.
                            let (img_x, img_y) = map_sample_uv((u, v));
                            let offset = ((img_y + y_offset) * span + (img_x + x_offset)) as usize;
                            if offset < pix_buf.len() {
                                let color = pix_buf[offset];

                                // We work in monochrome so just take the red channel...
                                let color_value = color.red();
                                let alpha_value = color.alpha();

                                // We might want to implement support for grayscale images in the future, but
                                // for now do a simple threshold.
                                let mfm_data: u8 = match color_value {
                                    //0..40 => 0x88,
                                    0..128 => p2d.black_byte,
                                    _ => p2d.white_byte,
                                };

                                // Alpha channel controls whether we write the pixel or not
                                if alpha_value >= 128 {
                                    track.write_raw_u8(bit_index, mfm_data);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Produce a 64-bit MFM pattern representing an 8-bit flux density ramp.
pub fn gen_ramp_64(value: u8) -> u64 {
    let base_pattern = 0x8888_8888_8888_8888u64; // Base MFM pattern mapping to 0 (darkest color)
    let mut result = base_pattern;

    // Slot positions in the 64-bit base pattern (middle of each run of 3 zeros)
    const SLOTS: [u8; 16] = [61, 57, 53, 49, 45, 41, 37, 33, 29, 25, 21, 17, 13, 9, 5, 1];

    // Iterate through the 8 bits of the input value
    for i in 0..8 {
        if value & (1 << i) != 0 {
            // Set a bit in the corresponding slot pair
            result |= 1 << SLOTS[i * 2]; // First slot in the pair
            result |= 1 << SLOTS[i * 2 + 1]; // Second slot in the pair
        }
    }

    result
}

/// Render a grayscale pixmap to disk.
/// Applesauce renders an 8-bit grayscale SVG image using 0.25 degree arcs (1440 slices per track)
/// Therefore we write 64-bit values to the disk image at a time.
pub fn render_pixmap_to_disk_grayscale(
    pixmap: &Pixmap,
    disk_image: &mut DiskImage,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    p2d: &PixmapToDiskParams,
) -> Result<(), DiskImageError> {
    let (sample_width, sample_height) = p2d.sample_size;
    let (img_width, img_height) = p2d.img_dimensions.to_tuple();
    let span = pixmap.width();

    let (x_offset, y_offset) = p2d.img_pos.to_tuple();

    // if p2d.mask_resolution < 1 || p2d.mask_resolution > 8 {
    //     return Err(DiskImageError::ParameterError);
    // }

    let color_ramp_bytes = (0..256)
        .map(|v| MFM_GRAYSCALE_RAMP[v / 16].to_be_bytes())
        .collect::<Vec<[u8; 8]>>();

    // Ignore the mask resolution and use 6 bits for now.
    let index_mask: usize = !((1 << 6) - 1);
    log::debug!(
        "render_pixmap_to_disk_grayscale(): using bit index_mask: {:#08b}",
        index_mask
    );

    // We work in sampling coordinates, so we need to adjust the center and radius to match the
    // sampling resolution.
    let center_x = sample_width as f32 / 2.0;
    let center_y = sample_height as f32 / 2.0;
    let total_radius = sample_width.min(sample_height) as f32 / 2.0;
    let mut min_radius = p.min_radius_ratio * total_radius; // Scale min_radius to pixel value

    let track_indices = collect_stream_indices(r.side, disk_image);
    let track_limit = p.track_limit.unwrap_or(MAX_CYLINDER);
    let num_tracks = min(track_indices.len(), track_limit);

    log::trace!("collected {} track references.", num_tracks);

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
    let pix_buf = pixmap.pixels();

    let map_sample_uv = |uv: (u32, u32)| -> (u32, u32) {
        let (x, y) = uv;
        let x = x as f32 * (img_width as f32 / sample_width as f32);
        let y = y as f32 * (img_height as f32 / sample_height as f32);
        (x as u32, y as u32)
    };

    // Sample the image and write sampled pixels to the disk image.
    // The sampling resolution needs to be quite high, at least 4096x4096, to get a good result
    // without gaps between pixels on the track which will introduce MFM errors.
    for v in 0..sample_height {
        for u in 0..sample_width {
            let dx = u as f32 - center_x;
            let dy = v as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let angle = (dy.atan2(dx) + PI) % TAU;

            if distance >= min_radius && distance <= total_radius {
                let track_offset = (distance - min_radius) / track_width;
                if track_offset.fract() < p.track_gap {
                    continue;
                }

                let track_index = (num_tracks - 1).saturating_sub(track_offset.floor() as usize);

                if track_index < p2d.skip_tracks as usize {
                    continue;
                }
                if track_index < num_tracks {
                    // Adjust angle via input angle parameter, for clockwise or counter-clockwise turning
                    let mut normalized_angle = match p.direction {
                        TurningDirection::Clockwise => angle - p.index_angle,
                        TurningDirection::CounterClockwise => TAU - (angle - p.index_angle),
                    };

                    // Normalize the angle to the range 0..2π
                    while normalized_angle < 0.0 {
                        normalized_angle += TAU;
                    }
                    normalized_angle = (normalized_angle + PI) % TAU;

                    if let Some(track) = disk_image.track_pool[track_indices[track_index]].stream_mut() {
                        let bit_index = ((normalized_angle / TAU) * track.len() as f32) as usize;
                        // Mask the bit index to the resolution specified
                        let bit_index = bit_index & index_mask;
                        // Ensure bit_index is within bounds
                        let bit_index = min(bit_index, track.len() - 64);
                        //let bit_index = (bit_index % track.len()) - 64;

                        let mut render_enable = true;

                        // Control rendering based on metadata if sector masking is enabled.
                        if r.sector_mask && !track.is_data(bit_index, false) {
                            render_enable = false;
                        }

                        if render_enable {
                            //log::debug!("rendering enabled...");
                            let (img_x, img_y) = map_sample_uv((u, v));
                            let offset = ((img_y + y_offset) * span + (img_x + x_offset)) as usize;
                            if offset < pix_buf.len() {
                                let color = pix_buf[offset];

                                // We work in monochrome so just take the green channel...
                                let color_value = color.green();
                                let alpha_value = color.alpha();

                                // Alpha channel controls whether we write the pixel or not
                                if alpha_value >= 128 {
                                    track.write_raw_buf(&color_ramp_bytes[color_value as usize], bit_index);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
