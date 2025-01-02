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

    examples/imgviz/src/render.rs

    Rendering functions for imgviz.

*/
use std::time::Instant;

use anyhow::bail;
use fast_image_resize::{images::Image as FirImage, FilterType, PixelType, ResizeAlg, Resizer};
use tiny_skia::{Color, IntSize, Pixmap, PremultipliedColorU8};

use fluxfox::{
    visualization::{
        render_track_data,
        render_track_mask,
        RenderMaskType,
        RenderTrackDataParams,
        ResolutionType,
        TurningDirection,
    },
    DiskImage,
};

pub struct RenderParams {
    pub bg_color: Option<Color>,
    pub track_bg_color: Option<Color>,
    pub render_size: u32,
    pub supersample: u8,
    pub side: u32,
    pub min_radius: f32,
    pub direction: TurningDirection,
    pub angle: f32,
    pub track_limit: usize,
    pub track_gap: f32,
    pub decode: bool,
    pub weak: bool,
    pub errors: bool,
    pub weak_color: Option<Color>,
    pub error_color: Option<Color>,
    pub resolution_type: ResolutionType,
}

#[allow(dead_code)]
pub(crate) fn color_to_premultiplied(color: Color) -> PremultipliedColorU8 {
    PremultipliedColorU8::from_rgba(
        (color.red() * color.alpha() * 255.0) as u8,
        (color.green() * color.alpha() * 255.0) as u8,
        (color.blue() * color.alpha() * 255.0) as u8,
        (color.alpha() * 255.0) as u8,
    )
    .expect("Failed to create PremultipliedColorU8")
}

pub fn render_side(disk: &DiskImage, p: RenderParams) -> Result<Pixmap, anyhow::Error> {
    let direction = match p.side {
        0 => TurningDirection::Clockwise,
        1 => TurningDirection::CounterClockwise,
        _ => {
            bail!("Invalid side: {}", p.side);
        }
    };

    let angle = direction.adjust_angle(p.angle);

    let supersample_size = match p.supersample {
        1 => p.render_size,
        2 => p.render_size * 2,
        4 => p.render_size * 4,
        8 => p.render_size * 8,
        _ => {
            bail!("Invalid supersample factor: {}", p.supersample);
        }
    };

    let mut rendered_image = Pixmap::new(supersample_size, supersample_size).unwrap();
    if let Some(color) = p.track_bg_color {
        rendered_image.fill(color);
    }
    let data_render_start_time = Instant::now();

    let mut render_params = RenderTrackDataParams {
        bg_color: p.bg_color,
        map_color: p.error_color,
        head: p.side as u8,
        image_size: (supersample_size, supersample_size),
        image_pos: (0, 0),
        min_radius_fraction: p.min_radius,
        index_angle: angle,
        track_limit: p.track_limit,
        track_gap: p.track_gap,
        direction,
        decode: p.decode,
        resolution: p.resolution_type,
        pin_last_standard_track: true,
        sector_mask: p.decode,
    };

    println!("Rendering data layer for side {}...", p.side);
    match render_track_data(&disk, &mut rendered_image, &render_params) {
        Ok(_) => {
            println!("Rendered data layer in {:?}", data_render_start_time.elapsed());
        }
        Err(e) => {
            eprintln!("Error rendering tracks: {}", e);
            std::process::exit(1);
        }
    };

    // Render error bits on composited image if requested.
    if p.errors {
        let error_render_start_time = Instant::now();
        println!("Rendering error map layer for side {}...", p.side);
        match render_track_mask(&disk, &mut rendered_image, RenderMaskType::Errors, &render_params) {
            Ok(_) => {
                println!("Rendered error map layer in {:?}", error_render_start_time.elapsed());
            }
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };
    }

    // Render weak bits on composited image if requested.
    if p.weak {
        render_params.map_color = p.weak_color;
        let weak_render_start_time = Instant::now();
        println!("Rendering weak bits layer for side {}...", p.side);
        match render_track_mask(&disk, &mut rendered_image, RenderMaskType::WeakBits, &render_params) {
            Ok(_) => {
                println!("Rendered weak bits layer in {:?}", weak_render_start_time.elapsed());
            }
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };
    }

    let resampled_image = match p.supersample {
        1 => rendered_image,
        _ => {
            let resample_start_time = Instant::now();

            let src_image = match FirImage::from_slice_u8(
                rendered_image.width(),
                rendered_image.height(),
                rendered_image.data_mut(),
                PixelType::U8x4,
            ) {
                Ok(image) => image,
                Err(e) => {
                    eprintln!("Error converting image: {}", e);
                    std::process::exit(1);
                }
            };
            let mut dst_image = FirImage::new(p.render_size, p.render_size, PixelType::U8x4);

            let mut resizer = Resizer::new();
            let resize_opts =
                fast_image_resize::ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));

            println!("Resampling output image for side {}...", p.side);
            match resizer.resize(&src_image, &mut dst_image, &resize_opts) {
                Ok(_) => {
                    println!(
                        "Resampled image to {} in {:?}",
                        p.render_size,
                        resample_start_time.elapsed()
                    );
                    Pixmap::from_vec(
                        dst_image.into_vec(),
                        IntSize::from_wh(p.render_size, p.render_size).unwrap(),
                    )
                    .unwrap()
                }
                Err(e) => {
                    eprintln!("Error resizing image: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    Ok(resampled_image)
}
