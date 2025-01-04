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
use std::{collections::HashMap, time::Instant};

use anyhow::bail;
use fast_image_resize::{images::Image as FirImage, FilterType, PixelType, ResizeAlg, Resizer};
use tiny_skia::{BlendMode, Color, IntSize, Paint, Pixmap, PremultipliedColorU8, Transform};

use crate::{
    args::VizArgs,
    style::{style_map_to_skia, Style},
};
use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::skia_render_element,
        rasterize_track_data,
        render_track_mask,
        types::{VizColor, VizDimensions},
        CommonVizParams,
        RenderMaskType,
        RenderRasterizationParams,
        RenderTrackDataParams,
        ResolutionType,
        TurningDirection,
        VizElementDisplayList,
    },
    DiskImage,
};

pub struct RenderParams {
    pub bg_color: Option<VizColor>,
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
    pub weak_color: Option<VizColor>,
    pub error_color: Option<VizColor>,
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

pub fn render_side(
    disk: &DiskImage,
    opts: &VizArgs,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    rr: &RenderRasterizationParams,
    weak_color: VizColor,
    error_color: VizColor,
) -> Result<Pixmap, anyhow::Error> {
    let direction = match r.side {
        0 => TurningDirection::Clockwise,
        1 => TurningDirection::CounterClockwise,
        _ => {
            bail!("Invalid side: {}", r.side);
        }
    };

    let mut rr = rr.clone();

    let angle = direction.adjust_angle(p.index_angle);

    let supersample_size = match rr.supersample {
        1 => rr.image_size,
        2 => rr.image_size.scale(2),
        4 => rr.image_size.scale(4),
        8 => rr.image_size.scale(8),
        _ => {
            bail!("Invalid supersample factor: {}", rr.supersample);
        }
    };

    let mut rendered_image = Pixmap::new(supersample_size.x, supersample_size.y).unwrap();

    // To implement the disk background color, we first fill the entire image with it.
    // The areas outside the disk circumference will be set to the img_bg_color during rendering.
    if let Some(color) = rr.disk_bg_color {
        rendered_image.fill(Color::from(color));
    }
    let data_render_start_time = Instant::now();

    println!("Rendering data layer for side {}...", r.side);
    match rasterize_track_data(disk, &mut rendered_image, p, r, &rr) {
        Ok(_) => {
            println!("Rendered data layer in {:?}", data_render_start_time.elapsed());
        }
        Err(e) => {
            eprintln!("Error rendering tracks: {}", e);
            std::process::exit(1);
        }
    };

    // Render error bits on composited image if requested.
    if opts.errors {
        let error_render_start_time = Instant::now();
        println!("Rendering error map layer for side {}...", r.side);
        match render_track_mask(disk, &mut rendered_image, RenderMaskType::Errors, p, r, &rr) {
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
    if opts.weak {
        rr.mask_color = Some(weak_color);
        let weak_render_start_time = Instant::now();
        println!("Rendering weak bits layer for side {}...", r.side);
        match render_track_mask(disk, &mut rendered_image, RenderMaskType::WeakBits, p, r, &rr) {
            Ok(_) => {
                println!("Rendered weak bits layer in {:?}", weak_render_start_time.elapsed());
            }
            Err(e) => {
                eprintln!("Error rendering tracks: {}", e);
                std::process::exit(1);
            }
        };
    }

    let resampled_image = match rr.supersample {
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
            let mut dst_image = FirImage::new(rr.image_size.x, rr.image_size.y, PixelType::U8x4);

            let mut resizer = Resizer::new();
            let resize_opts =
                fast_image_resize::ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));

            println!("Resampling output image for side {}...", r.side);
            match resizer.resize(&src_image, &mut dst_image, &resize_opts) {
                Ok(_) => {
                    println!(
                        "Resampled image to {} in {:?}",
                        rr.image_size,
                        resample_start_time.elapsed()
                    );
                    Pixmap::from_vec(
                        dst_image.into_vec(),
                        IntSize::from_wh(rr.image_size.x, rr.image_size.y).unwrap(),
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

pub fn rasterize_display_list(
    pixmap: &mut Pixmap,
    angle: f32,
    display_list: &VizElementDisplayList,
    palette: &HashMap<GenericTrackElement, Style>,
) {
    let mut paint = Paint {
        blend_mode: BlendMode::SourceOver,
        anti_alias: true,
        ..Default::default()
    };

    let mut transform = Transform::identity();

    if angle != 0.0 {
        log::warn!("Rotating tiny_skia Transform by {}", angle);
        transform = Transform::from_rotate_at(
            angle.to_degrees(),
            pixmap.width() as f32 / 2.0,
            pixmap.height() as f32 / 2.0,
        );
    }

    let skia_styles = style_map_to_skia(palette);

    for element in display_list.iter() {
        skia_render_element(pixmap, &mut paint, element, &transform, &skia_styles);
    }
}
