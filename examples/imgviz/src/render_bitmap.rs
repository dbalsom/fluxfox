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

use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::*,
        rasterize_track_data,
        render_track_mask,
        CommonVizParams,
        RenderMaskType,
        RenderRasterizationParams,
        RenderTrackDataParams,
        ResolutionType,
        TurningDirection,
    },
    DiskImage,
    DiskImageError,
    MAX_CYLINDER,
};
use std::{
    cmp::min,
    collections::HashMap,
    f32::consts::{PI, TAU},
    io::Write,
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use fluxfox_tiny_skia::tiny_skia::{
    BlendMode,
    Color,
    FilterQuality,
    IntSize,
    Paint,
    Pixmap,
    PixmapPaint,
    PremultipliedColorU8,
    Transform,
};

use crate::{
    args::VizArgs,
    config::StyleConfig,
    legend::VizLegend,
    palette::{default_error_bit_color, default_weak_bit_color},
    style::Style,
    text::{render_text, Justification},
};

use crate::style::style_map_to_skia;
use anyhow::{anyhow, bail, Error};
use crossbeam::channel;
use fast_image_resize::{images::Image as FirImage, FilterType, PixelType, ResizeAlg, Resizer};
use fluxfox_tiny_skia::{
    render_display_list::render_data_display_list,
    render_elements::{skia_render_data_slice, skia_render_element},
};
use tiny_skia::Stroke;

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

pub struct RasterizationData {}

pub fn render_bitmap(
    disk: DiskImage,
    starting_head: u32,
    sides_to_render: u32,
    opts: &VizArgs,
    style: &StyleConfig,
    legend: &mut VizLegend,
) -> Result<(), Error> {
    let image_size = opts.resolution;

    // New pool for metadata rendering. Don't bother with quadrants - just render full sides
    let meta_pixmap_pool = [
        Arc::new(Mutex::new(Pixmap::new(opts.resolution, opts.resolution).unwrap())),
        Arc::new(Mutex::new(Pixmap::new(opts.resolution, opts.resolution).unwrap())),
    ];

    // Vec to receive rendered pixmaps for compositing as they are generated.
    let mut rendered_pixmaps = Vec::new();

    let track_cts = [disk.track_ct(0), disk.track_ct(1)];

    let a_disk = disk.into_arc();

    for si in 0..sides_to_render {
        let side = si + starting_head;
        log::debug!(" >>> Rendering side {} of {}...", side, sides_to_render);
        let disk = Arc::clone(&a_disk);

        // Default to clockwise turning, unless --cc flag is passed.
        let mut direction = if opts.cc {
            TurningDirection::CounterClockwise
        }
        else {
            TurningDirection::Clockwise
        };

        // Reverse side 1 as long as --dont_reverse flag is not present.
        if side > 0 && !opts.dont_reverse {
            direction = direction.opposite();
        }

        log::debug!("Rendering display list at resolution: {}", opts.resolution);
        let common_params = CommonVizParams {
            radius: Some(image_size as f32 / 2.0),
            max_radius_ratio: 1.0,
            min_radius_ratio: opts.hole_ratio,
            pos_offset: None,
            index_angle: direction.adjust_angle(opts.angle),
            track_limit: Some(track_cts[side as usize]),
            pin_last_standard_track: true,
            track_gap: opts.track_gap.unwrap_or(0.0),
            direction,
        };

        let render_params = RenderTrackDataParams {
            side: si as u8,
            decode: opts.decode,
            sector_mask: true,
            resolution: Default::default(),
            slices: opts.data_slices,
            ..RenderTrackDataParams::default()
        };

        let rasterization_params = RenderRasterizationParams {
            image_size: VizDimensions::from((image_size, image_size)),
            supersample: opts.supersample,
            image_bg_color: opts.img_bg_color,
            disk_bg_color: opts.track_bg_color,
            mask_color: None,
            palette: None,
            pos_offset: None,
        };

        let weak_color = default_weak_bit_color();
        let error_color = default_error_bit_color();

        // Render data if data flag was passed.
        let mut pixmap = if opts.data {
            // If the rasterize_data flag was set, directly rasterize the data layer, otherwise
            // we'll vectorize it and rasterize that.
            match opts.rasterize_data {
                true => {
                    match rasterize_data_layer(
                        &disk.read().unwrap(),
                        opts,
                        &common_params,
                        &render_params,
                        &rasterization_params,
                    ) {
                        Ok(pixmap) => pixmap,
                        Err(e) => {
                            eprintln!("Error rendering side: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                false => {
                    let mut data_pixmap = Pixmap::new(image_size, image_size).unwrap();

                    // Vectorize the data layer, then render it
                    let vector_params = RenderVectorizationParams {
                        view_box: VizRect::from((0.0, 0.0, image_size as f32, image_size as f32)),
                        image_bg_color: None,
                        disk_bg_color: None,
                        mask_color: None,
                        pos_offset: None,
                    };

                    let display_list = match vectorize_disk_data(
                        &disk.read().unwrap(),
                        &common_params,
                        &render_params,
                        &vector_params,
                    ) {
                        Ok(display_list) => display_list,
                        Err(e) => {
                            eprintln!("Error vectorizing disk elements: {}", e);
                            std::process::exit(1);
                        }
                    };

                    // Disable antialiasing to reduce moiré. For antialiasing, use supersampling.
                    let mut paint = Paint {
                        anti_alias: false,
                        ..Default::default()
                    };

                    render_data_display_list(&mut data_pixmap, &mut paint, common_params.index_angle, &display_list)
                        .map_err(|s| anyhow!("Error rendering data display list: {}", s))?;

                    data_pixmap
                }
            }
        }
        else {
            Pixmap::new(image_size, image_size).unwrap()
        };

        drop(disk);

        let (sender, receiver) = channel::unbounded::<u8>();

        if opts.metadata {
            log::debug!("Rendering metadata for side {}...", side);
            let inner_disk = Arc::clone(&a_disk);
            let inner_pixmap = Arc::clone(&meta_pixmap_pool[side as usize]);
            let palette = style.element_styles.clone();

            let render_debug = opts.debug;
            let inner_params = common_params.clone();

            thread::spawn(move || {
                let mut metadata_pixmap = inner_pixmap.lock().unwrap();

                // We set the angle to 0.0 here, because tiny_skia can rotate the resulting
                // display list for us.
                let mut common_params = inner_params.clone();
                common_params.index_angle = 0.0;

                let metadata_params = RenderTrackMetadataParams {
                    quadrant: None,
                    side: side as u8,
                    geometry: Default::default(),
                    winding: Default::default(),
                    draw_empty_tracks: false,
                    draw_sector_lookup: false,
                };

                let metadata_disk = inner_disk.read().unwrap();

                let list_start_time = Instant::now();
                let display_list = match vectorize_disk_elements(&metadata_disk, &common_params, &metadata_params) {
                    Ok(display_list) => display_list,
                    Err(e) => {
                        eprintln!("Error rendering metadata: {}", e);
                        std::process::exit(1);
                    }
                };

                println!(
                    "visualize_disk_elements() returned a display list of length {} in {:.3}ms",
                    display_list.len(),
                    list_start_time.elapsed().as_secs_f64() * 1000.0
                );

                // Set the index angle for rasterization.
                let angle = inner_params.index_angle;
                rasterize_display_list(&mut metadata_pixmap, angle, &display_list, &palette);

                if render_debug {
                    metadata_pixmap.save_png(format!("new_metadata{}.png", side)).unwrap();
                }

                println!("Sending rendered metadata pixmap for side: {} over channel...", side);
                if let Err(e) = sender.send(side as u8) {
                    eprintln!("Error sending metadata pixmap: {}", e);
                    std::process::exit(1);
                }
            });

            println!("Waiting for metadata pixmap for side {}...", side);
            std::io::stdout().flush().unwrap();
            for (p, recv_side) in receiver.iter().enumerate() {
                // let (x, y) = match recv_side {
                //     0 => (0, 0u32),
                //     1 => (image_size, 0u32),
                //     _ => panic!("Invalid side"),
                // };
                println!("Received metadata pixmap for side {}...", recv_side);
                std::io::stdout().flush().unwrap();

                let paint = match opts.data {
                    true => PixmapPaint {
                        opacity:    1.0,
                        blend_mode: BlendMode::HardLight,
                        quality:    FilterQuality::Nearest,
                    },
                    false => PixmapPaint::default(),
                };

                pixmap.draw_pixmap(
                    0,
                    0,
                    meta_pixmap_pool[recv_side as usize].lock().unwrap().as_ref(), // Convert &Pixmap to PixmapRef
                    &paint,
                    Transform::identity(),
                    None,
                );

                if p == sides_to_render.saturating_sub(1) as usize {
                    break;
                }
            }
        }

        log::debug!("Adding rendered side {} to vector...", side);
        rendered_pixmaps.push(pixmap);
    }

    if rendered_pixmaps.is_empty() {
        eprintln!("No sides rendered!");
        std::process::exit(1);
    }
    //println!("Finished data layer in {:?}", data_render_start_time.elapsed());

    let horiz_gap = 0;

    // Combine both sides into a single image, if we have two sides.
    let (mut composited_image, composited_width) = if (rendered_pixmaps.len() > 1) || (sides_to_render == 2) {
        let final_size = (
            image_size * sides_to_render + horiz_gap * (sides_to_render - 1),
            image_size + legend.height().unwrap_or(0) as u32,
        );

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();
        if let Some(color) = opts.img_bg_color {
            final_image.fill(Color::from(color));
        }

        println!("Compositing sides...");
        for (i, pixmap) in rendered_pixmaps.iter().enumerate() {
            //println!("Compositing pixmap #{}", i);
            final_image.draw_pixmap(
                (image_size + horiz_gap) as i32 * i as i32,
                0,
                pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }

        println!("Saving final image as {}", opts.out_filename.display());
        (final_image, final_size.0)
    }
    else if let Some(height) = legend.height() {
        // Just one side, but we have a legend.
        let final_size = (image_size, image_size + height as u32);

        let mut final_image = Pixmap::new(final_size.0, final_size.1).unwrap();
        if let Some(color) = opts.img_bg_color {
            final_image.fill(Color::from(color));
        }

        println!("Compositing side...");
        final_image.draw_pixmap(
            0,
            0,
            rendered_pixmaps.pop().unwrap().as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );

        println!("Saving final image as {}", opts.out_filename.display());
        (final_image, image_size)
    }
    else {
        // Just one side, and no legend. Nothing to composite.
        println!("Saving final image as {}", opts.out_filename.display());
        (rendered_pixmaps.pop().unwrap(), image_size)
    };

    // // Render index hole if requested.
    // if opts.index_hole {
    //     draw_index_hole(
    //         &mut composited_image,
    //         0.39,
    //         2.88,
    //         10.0,
    //         1.0,
    //         Color::from_rgba8(255, 255, 255, 255),
    //         TurningDirection::CounterClockwise,
    //     );
    // }

    // Render legend.
    legend.render(&mut composited_image);

    // Save image to disk.
    composited_image.save_png(opts.out_filename.clone())?;

    Ok(())
}

pub fn rasterize_data_layer(
    disk: &DiskImage,
    opts: &VizArgs,
    p: &CommonVizParams,
    r: &RenderTrackDataParams,
    rr: &RenderRasterizationParams,
) -> Result<Pixmap, Error> {
    let mut rr = rr.clone();

    let supersample_size = match rr.supersample {
        1 => rr.image_size,
        2 => rr.image_size.scale(2),
        4 => rr.image_size.scale(4),
        8 => rr.image_size.scale(8),
        _ => {
            bail!("Invalid supersample factor: {}", rr.supersample);
        }
    };

    let mut data_layer_pixmap = Pixmap::new(supersample_size.x, supersample_size.y).unwrap();

    // To implement the disk background color, we first fill the entire image with it.
    // The areas outside the disk circumference will be set to the img_bg_color during rendering.
    if let Some(color) = rr.disk_bg_color {
        data_layer_pixmap.fill(Color::from(color));
    }

    println!("Rendering data layer for side {}...", r.side);
    let data_render_start_time = Instant::now();

    match rasterize_track_data(disk, &mut data_layer_pixmap, p, r, &rr) {
        Ok(_) => {
            println!("Rendered data layer in {:?}", data_render_start_time.elapsed());
        }
        Err(e) => {
            eprintln!("Error rendering tracks: {}", e);
            std::process::exit(1);
        }
    };

    // // Render error bits on composited image if requested.
    // if opts.errors {
    //     let error_render_start_time = Instant::now();
    //     println!("Rendering error map layer for side {}...", r.side);
    //     match render_track_mask(disk, &mut rendered_image, RenderMaskType::Errors, p, r, &rr) {
    //         Ok(_) => {
    //             println!("Rendered error map layer in {:?}", error_render_start_time.elapsed());
    //         }
    //         Err(e) => {
    //             eprintln!("Error rendering tracks: {}", e);
    //             std::process::exit(1);
    //         }
    //     };
    // }
    //
    // // Render weak bits on composited image if requested.
    // if opts.weak {
    //     rr.mask_color = Some(weak_color);
    //     let weak_render_start_time = Instant::now();
    //     println!("Rendering weak bits layer for side {}...", r.side);
    //     match render_track_mask(disk, &mut rendered_image, RenderMaskType::WeakBits, p, r, &rr) {
    //         Ok(_) => {
    //             println!("Rendered weak bits layer in {:?}", weak_render_start_time.elapsed());
    //         }
    //         Err(e) => {
    //             eprintln!("Error rendering tracks: {}", e);
    //             std::process::exit(1);
    //         }
    //     };
    // }

    let resampled_image = match rr.supersample {
        1 => data_layer_pixmap,
        _ => {
            let resample_start_time = Instant::now();

            let src_image = match FirImage::from_slice_u8(
                data_layer_pixmap.width(),
                data_layer_pixmap.height(),
                data_layer_pixmap.data_mut(),
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
    styles: &HashMap<GenericTrackElement, Style>,
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

    let skia_styles = style_map_to_skia(styles);

    for element in display_list.iter() {
        skia_render_element(pixmap, &mut paint, &transform, &skia_styles, element);
    }
}
