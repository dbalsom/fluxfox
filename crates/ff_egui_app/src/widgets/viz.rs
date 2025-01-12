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

#[cfg(not(target_arch = "wasm32"))]
use crate::native::worker;
#[cfg(target_arch = "wasm32")]
use crate::wasm::worker;
use std::{
    collections::HashMap,
    default::Default,
    f32::consts::TAU,
    sync::{mpsc, Arc, Mutex, RwLock},
};

use crate::{time::*, App};

use fluxfox::{
    prelude::*,
    track_schema::GenericTrackElement,
    visualization::{
        prelude::*,
        rasterize_disk::{rasterize_disk_selection, rasterize_track_metadata_quadrant},
    },
    FoxHashMap,
};

#[cfg(feature = "svg")]
use fluxfox_svg::prelude::*;

use fluxfox_egui::widgets::texture::{PixelCanvas, PixelCanvasDepth};
use fluxfox_tiny_skia::tiny_skia::{BlendMode, Color, FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::{app::Tool, lock::TrackingLock};
use anyhow::{anyhow, Error};
use eframe::emath::{Pos2, Rect, RectTransform};
use egui::{Key::V, Vec2};
use fluxfox_egui::visualization::viz_elements::paint_elements;
use fluxfox_tiny_skia::{
    render_display_list::render_data_display_list,
    render_elements::skia_render_display_list,
    styles::{default_skia_styles, SkiaStyle},
};
use tiny_skia::Paint;

pub const VIZ_DATA_SUPERSAMPLE: u32 = 2;
pub const VIZ_RESOLUTION: u32 = 512;
pub const VIZ_SUPER_RESOLUTION: u32 = VIZ_RESOLUTION * VIZ_DATA_SUPERSAMPLE;

#[allow(dead_code)]
pub enum RenderMessage {
    DataRenderComplete(u8),
    DataRenderError(String),
}

#[derive(Copy, Clone, PartialEq)]
pub enum VizEvent {
    NewSectorSelected { c: u8, h: u8, s_idx: u8 },
    SectorDeselected,
}

struct VisualizationContext<'a> {
    got_hit: [bool; 2],
    context_menu_open: &'a mut bool,
    selection_rect_opt: &'a mut Option<Rect>,
    display_list_opt: &'a mut Option<VizElementDisplayList>,
    events: &'a mut Vec<VizEvent>,
    common_viz_params: &'a mut CommonVizParams,
}

pub struct VisualizationState {
    pub disk: Option<TrackingLock<DiskImage>>,
    pub resolution: u32,
    pub common_viz_params: CommonVizParams,
    pub compatible: bool,
    pub supersample: u32,
    pub data_img: [Arc<Mutex<Pixmap>>; 2],
    pub meta_img: [Arc<RwLock<Pixmap>>; 2],
    pub selection_img: [Arc<Mutex<Pixmap>>; 2],

    pub composite_img: [Pixmap; 2],
    pub sector_lookup_img: [Pixmap; 2],
    pub meta_palette: HashMap<GenericTrackElement, VizColor>,
    pub meta_display_list: [Arc<Mutex<Option<VizElementDisplayList>>>; 2],
    pub have_render: [bool; 2],
    pub canvas: [Option<PixelCanvas>; 2],
    pub sides: usize,
    pub render_sender: mpsc::SyncSender<RenderMessage>,
    pub render_receiver: mpsc::Receiver<RenderMessage>,
    pub decode_data_layer: bool,
    pub show_data_layer: bool,
    pub show_metadata_layer: bool,
    #[allow(dead_code)]
    pub show_error_layer: bool,
    #[allow(dead_code)]
    pub show_weak_layer: bool,
    #[allow(dead_code)]
    pub show_selection_layer: bool,
    pub selection_display_list: [Arc<Mutex<Option<VizElementDisplayList>>>; 2],
    pub selection_display_list2: Option<VizElementDisplayList>,
    last_event: Option<VizEvent>,
    context_menu_open: bool,
    /// A list of events that have occurred since the last frame.
    /// The main app should drain this list and process all events.
    events: Vec<VizEvent>,
    /// A flag indicating whether we received a hit-tested selection.
    got_hit: bool,
    /// The response rect received from the pixel canvas when we got a hit-tested selection.
    selection_rect_opt: Option<Rect>,
    angle: f32,
    zoom_quadrant: [Option<usize>; 2],
}

impl Default for VisualizationState {
    #[rustfmt::skip]
    fn default() -> Self {
        let (render_sender, render_receiver) = mpsc::sync_channel(2);
        Self {
            disk: None,
            resolution: VIZ_RESOLUTION,
            common_viz_params: CommonVizParams::default(),
            compatible: false,
            supersample: VIZ_DATA_SUPERSAMPLE,
            data_img: [
                Arc::new(Mutex::new(Pixmap::new(VIZ_SUPER_RESOLUTION, VIZ_SUPER_RESOLUTION).unwrap())),
                Arc::new(Mutex::new(Pixmap::new(VIZ_SUPER_RESOLUTION, VIZ_SUPER_RESOLUTION).unwrap())),
            ],
            meta_img: [
                Arc::new(RwLock::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
                Arc::new(RwLock::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
            ],
            selection_img: [
                Arc::new(Mutex::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
                Arc::new(Mutex::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
            ],
            composite_img: [
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
            ],
            sector_lookup_img: [
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
            ],
            meta_palette: HashMap::new(),
            meta_display_list: [Arc::new(Mutex::new(None)), Arc::new(Mutex::new(None))],
            have_render: [false; 2],
            canvas: [None, None],
            sides: 1,
            render_sender,
            render_receiver,
            decode_data_layer: true,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
            show_selection_layer: true,
            selection_display_list: [Arc::new(Mutex::new(None)), Arc::new(Mutex::new(None))],
            selection_display_list2: None,
            last_event: None,
            context_menu_open: false,
            events: Vec::new(),
            got_hit: false,
            selection_rect_opt: None,
            angle: 0.0,
            zoom_quadrant: [None, None],
        }
    }
}

impl VisualizationState {
    pub fn new(ctx: egui::Context, resolution: u32) -> Self {
        assert_eq!(resolution % 2, 0);

        let viz_light_red: VizColor = VizColor::from_rgba8(180, 0, 0, 255);

        //let viz_orange: Color = Color::from_rgba8(255, 100, 0, 255);
        let vis_purple: VizColor = VizColor::from_rgba8(180, 0, 180, 255);
        //let viz_cyan: Color = Color::from_rgba8(70, 200, 200, 255);
        //let vis_light_purple: Color = Color::from_rgba8(185, 0, 255, 255);

        let pal_medium_green = VizColor::from_rgba8(0x38, 0xb7, 0x64, 0xff);
        let pal_dark_green = VizColor::from_rgba8(0x25, 0x71, 0x79, 0xff);
        //let pal_dark_blue = Color::from_rgba8(0x29, 0x36, 0x6f, 0xff);
        let pal_medium_blue = VizColor::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
        let pal_light_blue = VizColor::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
        //let pal_dark_purple = Color::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
        let pal_orange = VizColor::from_rgba8(0xef, 0x7d, 0x57, 0xff);
        //let pal_dark_red = Color::from_rgba8(0xb1, 0x3e, 0x53, 0xff);

        // let mut meta_pixmap_pool = Vec::new();
        // for _ in 0..4 {
        //     let pixmap = Arc::new(Mutex::new(Pixmap::new(resolution / 2, resolution / 2).unwrap()));
        //     meta_pixmap_pool.push(pixmap);
        // }

        let mut canvas0 = PixelCanvas::new((resolution, resolution), ctx.clone(), "head0_canvas");
        canvas0.set_bpp(PixelCanvasDepth::Rgba);
        let mut canvas1 = PixelCanvas::new((resolution, resolution), ctx.clone(), "head1_canvas");
        canvas1.set_bpp(PixelCanvasDepth::Rgba);

        Self {
            meta_palette: FoxHashMap::from([
                (GenericTrackElement::SectorData, pal_medium_green),
                (GenericTrackElement::SectorBadData, pal_orange),
                (GenericTrackElement::SectorDeletedData, pal_dark_green),
                (GenericTrackElement::SectorBadDeletedData, viz_light_red),
                (GenericTrackElement::SectorHeader, pal_light_blue),
                (GenericTrackElement::SectorBadHeader, pal_medium_blue),
                (GenericTrackElement::Marker, vis_purple),
            ]),
            canvas: [Some(canvas0), Some(canvas1)],
            ..VisualizationState::default()
        }
    }

    #[allow(dead_code)]
    pub fn compatible(&self) -> bool {
        self.compatible
    }

    pub fn update_disk(&mut self, disk_lock: TrackingLock<DiskImage>) {
        let disk = disk_lock.read(Tool::Visualization).unwrap();
        self.compatible = disk.can_visualize();
        self.sides = disk.heads() as usize;
        self.disk = Some(disk_lock.clone());
    }

    pub fn render_visualization(&mut self, side: usize) -> Result<(), Error> {
        // if self.meta_pixmap_pool.len() < 4 {
        //     return Err(anyhow!("Pixmap pool not initialized"));
        // }

        if self.disk.is_none() {
            // No disk to render.
            return Ok(());
        }

        let render_lock = self.disk.as_ref().unwrap().clone();
        let render_sender = self.render_sender.clone();
        let render_target = self.data_img[side].clone();
        let render_meta_display_list = self.meta_display_list[side].clone();

        let disk = self.disk.as_ref().unwrap().read(Tool::Visualization).unwrap();
        self.compatible = disk.can_visualize();
        if !self.compatible {
            return Err(anyhow!("Incompatible disk resolution"));
        }

        let head = side as u8;
        let min_radius_fraction = 0.30;
        let render_track_gap = 0.0;
        let direction = match head {
            0 => TurningDirection::Clockwise,
            _ => TurningDirection::CounterClockwise,
        };
        let track_ct = disk.track_ct(side.into());

        if side >= disk.heads() as usize {
            // Ignore request for non-existent side.
            return Ok(());
        }

        self.common_viz_params = CommonVizParams {
            radius: Some(VIZ_SUPER_RESOLUTION as f32 / 2.0),
            max_radius_ratio: 1.0,
            min_radius_ratio: min_radius_fraction,
            pos_offset: None,
            index_angle: 0.0,
            track_limit: Some(track_ct),
            pin_last_standard_track: true,
            track_gap: render_track_gap,
            direction,
        };

        let inner_common_params = self.common_viz_params.clone();
        let inner_decode_data = self.decode_data_layer;
        let inner_angle = self.common_viz_params.index_angle;

        // Render the main data layer.
        match worker::spawn_closure_worker(move || {
            let data_params = RenderTrackDataParams {
                side: head,
                decode: inner_decode_data,
                slices: 1440,
                ..Default::default()
            };

            let vector_params = RenderVectorizationParams::default();

            let disk = render_lock.read(Tool::Visualization).unwrap();
            let mut render_pixmap = render_target.lock().unwrap();
            render_pixmap.fill(Color::TRANSPARENT);

            let vectorize_data_timer = Instant::now();
            match vectorize_disk_data(&disk, &inner_common_params, &data_params, &vector_params) {
                Ok(display_list) => {
                    log::debug!(
                        "render worker: Data layer vectorized for side {} in {:.2}ms, created display list of {} elements",
                        head,
                        vectorize_data_timer.elapsed().as_millis(),
                        display_list.len()
                    );

                    // Disable antialiasing to reduce moiré. For antialiasing, use supersampling.
                    let mut paint = Paint {
                        anti_alias: false,
                        ..Default::default()
                    };

                    match render_data_display_list(&mut render_pixmap, &mut paint, inner_angle, &display_list) {
                        Ok(_) => {
                            log::debug!("render worker: Data display list rendered for side {}", head);
                            render_sender.send(RenderMessage::DataRenderComplete(head)).unwrap();
                        }
                        Err(e) => {
                            log::error!("Error rendering display list: {}", e);
                            //std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Error rendering tracks: {}", e);
                    //std::process::exit(1);
                }
            };
        }) {
            Ok(_) => {
                log::debug!("Spawned rendering worker for data layer...");
            }
            Err(_e) => {
                log::error!("Error spawning rendering worker for data layer!");
            }
        };

        // // Render the main data layer (old rasterization method)
        // match worker::spawn_closure_worker(move || {
        //     let render_params = RenderTrackDataParams {
        //         side: head,
        //         decode: inner_decode_data,
        //         sector_mask: false,
        //         resolution: ResolutionType::Byte,
        //         ..Default::default()
        //     };
        //
        //     let rasterize_params = RenderRasterizationParams {
        //         image_size: VizDimensions::from((VIZ_RESOLUTION, VIZ_RESOLUTION)),
        //         supersample: VIZ_DATA_SUPERSAMPLE,
        //         image_bg_color: None,
        //         disk_bg_color: None,
        //         mask_color: None,
        //         palette: None,
        //         pos_offset: None,
        //     };
        //
        //     let disk = render_lock.read().unwrap();
        //     let mut render_pixmap = render_target.lock().unwrap();
        //     render_pixmap.fill(Color::TRANSPARENT);
        //
        //     match rasterize_track_data(
        //         &disk,
        //         &mut render_pixmap,
        //         &inner_common_params,
        //         &render_params,
        //         &rasterize_params,
        //     ) {
        //         Ok(_) => {
        //             log::debug!("render worker: Data layer rendered for side {}", head);
        //             render_sender.send(RenderMessage::DataRenderComplete(head)).unwrap();
        //         }
        //         Err(e) => {
        //             log::error!("Error rendering tracks: {}", e);
        //             std::process::exit(1);
        //         }
        //     };
        // }) {
        //     Ok(_) => {
        //         log::debug!("Spawned rendering worker for data layer...");
        //     }
        //     Err(_e) => {
        //         log::error!("Error spawning rendering worker for data layer!");
        //     }
        // };

        // Clear pixmap before rendering
        self.meta_img[side].write().unwrap().fill(Color::TRANSPARENT);

        let render_params = RenderTrackMetadataParams {
            // Render all quadrants
            quadrant: None,
            side: head,
            geometry: RenderGeometry::Sector,
            winding: Default::default(),
            draw_empty_tracks: false,
            draw_sector_lookup: false,
        };

        // let rasterize_params = RenderRasterizationParams {
        //     image_size: VizDimensions::from((VIZ_RESOLUTION, VIZ_RESOLUTION)),
        //     supersample: VIZ_DATA_SUPERSAMPLE,
        //     image_bg_color: None,
        //     disk_bg_color: None,
        //     mask_color: None,
        //     palette: Some(self.meta_palette.clone()),
        //     pos_offset: None,
        // };

        // Update the common viz params for metadata rendering. Metadata is not super-sampled
        self.common_viz_params.radius = Some(VIZ_RESOLUTION as f32 / 2.0);

        let mut display_list_guard = render_meta_display_list.lock().unwrap();
        let display_list = vectorize_disk_elements_by_quadrants(&disk, &self.common_viz_params, &render_params)?;

        let mut paint = Paint::default();
        let styles = default_skia_styles();
        skia_render_display_list(
            &mut self.meta_img[head as usize].write().unwrap(),
            &mut paint,
            &Transform::identity(),
            &display_list,
            &SkiaStyle::default(),
            &styles,
        );

        *display_list_guard = Some(display_list);
        //
        // // Render metadata quadrants into pixmap pool.
        // for quadrant in 0..4 {
        //     render_params.quadrant = Some(quadrant as u8);
        //     let mut pixmap = self.meta_pixmap_pool[quadrant].lock().unwrap();
        //
        //     match rasterize_track_metadata_quadrant(
        //         disk,
        //         &mut pixmap,
        //         &self.common_viz_params,
        //         &render_params,
        //         &rasterize_params,
        //     ) {
        //         Ok(_) => {
        //             log::debug!("...Rendered quadrant {}", quadrant);
        //         }
        //         Err(e) => {
        //             log::error!("Error rendering quadrant: {}", e);
        //             return Err(anyhow!("Error rendering metadata"));
        //         }
        //     }
        // }
        //
        // // Composite metadata quadrants into one complete metadata pixmap.
        // for quadrant in 0..4 {
        //     log::debug!("Received quadrant {}, compositing...", quadrant);
        //     let (x, y) = match quadrant {
        //         0 => (0, 0),
        //         1 => (VIZ_RESOLUTION / 2, 0),
        //         2 => (0, VIZ_RESOLUTION / 2),
        //         3 => (VIZ_RESOLUTION / 2, VIZ_RESOLUTION / 2),
        //         _ => panic!("Invalid quadrant"),
        //     };
        //
        //     let paint = PixmapPaint::default();
        //     //let mut pixmap = self.meta_pixmap_pool[quadrant].lock()?;
        //
        //     self.metadata_img[side].draw_pixmap(
        //         x as i32,
        //         y as i32,
        //         self.meta_pixmap_pool[quadrant].lock().unwrap().as_ref(),
        //         &paint,
        //         Transform::identity(),
        //         None,
        //     );
        //
        //     // Clear pixmap after compositing
        //     self.meta_pixmap_pool[quadrant]
        //         .lock()
        //         .unwrap()
        //         .as_mut()
        //         .fill(Color::TRANSPARENT);
        // }

        // // Render sector lookup quadrants into pixmap pool.
        // render_params.draw_sector_lookup = true;
        // // Make it easier to select sectors by removing track gap.
        // self.common_viz_params.track_gap = 0.0;
        //
        // for quadrant in 0..4 {
        //     render_params.quadrant = Some(quadrant as u8);
        //     let mut pixmap = self.meta_pixmap_pool[quadrant].lock().unwrap();
        //
        //     match rasterize_track_metadata_quadrant(
        //         disk,
        //         &mut pixmap,
        //         &self.common_viz_params,
        //         &render_params,
        //         &rasterize_params,
        //     ) {
        //         Ok(_) => {
        //             log::debug!("...Rendered quadrant {}", quadrant);
        //         }
        //         Err(e) => {
        //             log::error!("Error rendering quadrant: {}", e);
        //             return Err(anyhow!("Error rendering metadata"));
        //         }
        //     }
        // }
        //
        // // Composite sector lookup quadrants into final pixmap.
        // for quadrant in 0..4 {
        //     log::debug!("Received quadrant {}, compositing...", quadrant);
        //     let (x, y) = match quadrant {
        //         0 => (0, 0),
        //         1 => (VIZ_RESOLUTION / 2, 0),
        //         2 => (0, VIZ_RESOLUTION / 2),
        //         3 => (VIZ_RESOLUTION / 2, VIZ_RESOLUTION / 2),
        //         _ => panic!("Invalid quadrant"),
        //     };
        //
        //     let paint = PixmapPaint::default();
        //     //let mut pixmap = self.meta_pixmap_pool[quadrant].lock()?;
        //
        //     self.sector_lookup_img[side].draw_pixmap(
        //         x as i32,
        //         y as i32,
        //         self.meta_pixmap_pool[quadrant].lock().unwrap().as_ref(),
        //         &paint,
        //         Transform::identity(),
        //         None,
        //     );
        //
        //     // Clear pixmap after compositing
        //     self.meta_pixmap_pool[quadrant]
        //         .lock()
        //         .unwrap()
        //         .as_mut()
        //         .fill(Color::TRANSPARENT);
        // }

        if let Some(canvas) = &mut self.canvas[side] {
            if canvas.has_texture() {
                log::debug!("Updating canvas...");
                canvas.update_data(self.meta_img[side].read().unwrap().data());
                self.have_render[side] = true;
            }
            else {
                log::debug!("Canvas not initialized, deferring update...");
                //self.draw_deferred = true;
            }
        }
        Ok(())
    }

    pub fn set_zoom(&mut self, side: usize, zoom: f32) {
        if let Some(canvas) = &mut self.canvas[side] {
            canvas.set_zoom(zoom);
        }
    }

    pub fn set_quadrant(&mut self, side: usize, quadrant: Option<usize>) {
        if let Some(canvas) = &mut self.canvas[side] {
            if let Some(quadrant) = quadrant {
                let resolution = Vec2::new(self.resolution as f32, self.resolution as f32);
                let view_rect = match quadrant & 0x03 {
                    0 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, 0.0), resolution / 2.0),
                    1 => Rect::from_min_size(Pos2::ZERO, resolution / 2.0),
                    2 => Rect::from_min_size(Pos2::new(0.0, resolution.y / 2.0), resolution / 2.0),
                    3 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, resolution.y / 2.0), resolution / 2.0),
                    _ => unreachable!(),
                };

                log::debug!("Setting virtual rect for quadrant {}: {:?}", quadrant & 0x03, view_rect);
                canvas.set_virtual_rect(view_rect);
            }
            else {
                canvas.set_virtual_rect(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(self.resolution as f32, self.resolution as f32),
                ));
            }
        }

        self.zoom_quadrant[side] = quadrant;
    }

    pub fn clear_selection(&mut self, side: usize) {
        if let Ok(mut pixmap) = self.selection_img[side].try_lock() {
            pixmap.fill(Color::TRANSPARENT);
        }
    }

    pub fn update_selection(&mut self, c: u8, h: u8, s_idx: u8) {
        if self.disk.is_none() {
            // No disk to render.
            return;
        }

        self.clear_selection(h as usize);

        let side = h as usize;
        let mut do_composite = false;
        match self.disk.as_ref().unwrap().read(Tool::Visualization) {
            Ok(disk) => {
                // If we can acquire the selection image mutex, we can composite the data and metadata layers.
                match self.selection_img[side].try_lock() {
                    Ok(mut data) => {
                        let render_selection_params = RenderDiskSelectionParams {
                            selection_type: RenderDiskSelectionType::Sector,
                            ch: DiskCh::new(c as u16, h),
                            sector_idx: s_idx as usize,
                            color: VizColor::from_rgba8(255, 255, 255, 255),
                        };

                        match rasterize_disk_selection(
                            &disk,
                            &mut data,
                            &self.common_viz_params,
                            &render_selection_params,
                        ) {
                            Ok(_) => {
                                log::debug!("Sector selection rendered for side {}", side);
                            }
                            Err(e) => {
                                log::error!("Error rendering sector selection: {}", e);
                            }
                        }

                        do_composite = true;
                    }
                    Err(_) => {
                        log::debug!("Data pixmap locked, deferring selection update...");
                    }
                }
            }
            Err(e) => {
                log::debug!(
                    "Disk image could not be locked for reading! Locked by {:?}, deferring sector selection...",
                    e
                );
                return;
            }
        };

        if do_composite {
            self.composite(side);
        }
    }

    fn composite(&mut self, side: usize) {
        // If we can acquire the data mutex, we can composite the data and metadata layers.
        match self.data_img[side].try_lock() {
            Ok(data) => {
                let mut paint = PixmapPaint::default();
                // Scale the data pixmap down to the composite size with bilinear filtering.
                paint.quality = FilterQuality::Bilinear;

                if self.show_data_layer {
                    log::debug!(">>>> Compositing data layer for side {}", side);
                    let scale = 1.0 / self.supersample as f32;
                    let transform = Transform::from_scale(scale, scale);
                    self.composite_img[side].fill(Color::TRANSPARENT);
                    self.composite_img[side].draw_pixmap(0, 0, data.as_ref(), &paint, transform, None);
                }
                else {
                    self.composite_img[side].fill(Color::TRANSPARENT);
                }
                if self.show_metadata_layer {
                    paint = PixmapPaint {
                        opacity:    1.0,
                        blend_mode: BlendMode::Color,
                        quality:    FilterQuality::Nearest,
                    };
                    self.composite_img[side].draw_pixmap(
                        0,
                        0,
                        self.meta_img[side].read().unwrap().as_ref(),
                        &paint,
                        Transform::identity(),
                        None,
                    );
                }
            }
            Err(_) => {
                log::debug!("Data pixmap locked, deferring compositing...");
                let paint = PixmapPaint::default();
                self.composite_img[side].fill(Color::TRANSPARENT);
                if self.show_metadata_layer {
                    self.composite_img[side].draw_pixmap(
                        0,
                        0,
                        self.meta_img[side].read().unwrap().as_ref(),
                        &paint,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }

        // Finally, composite the sector selection.
        match self.selection_img[side].try_lock() {
            Ok(selection) => {
                let paint = PixmapPaint {
                    opacity:    1.0,
                    blend_mode: BlendMode::Overlay,
                    quality:    FilterQuality::Nearest,
                };

                if self.show_metadata_layer {
                    self.composite_img[side].draw_pixmap(0, 0, selection.as_ref(), &paint, Transform::identity(), None);
                }
            }
            Err(_) => {
                log::debug!("Selection pixmap locked, deferring compositing...");
            }
        }

        if let Some(canvas) = &mut self.canvas[side] {
            if canvas.has_texture() {
                log::debug!("composite(): Updating canvas...");
                canvas.update_data(self.composite_img[side].data());
                self.have_render[side] = true;
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_open(&self) -> bool {
        self.have_render[0]
    }

    pub(crate) fn enable_data_layer(&mut self, state: bool) {
        self.show_data_layer = state;
        for side in 0..self.sides {
            self.composite(side);
        }
    }

    pub(crate) fn enable_metadata_layer(&mut self, state: bool) {
        self.show_metadata_layer = state;
        for side in 0..self.sides {
            self.composite(side);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn enable_error_layer(&mut self, state: bool) {
        self.show_error_layer = state;
        for side in 0..self.sides {
            self.composite(side);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn enable_weak_layer(&mut self, state: bool) {
        self.show_weak_layer = state;
        for side in 0..self.sides {
            self.composite(side);
        }
    }

    pub(crate) fn save_side_as_png(&mut self, filename: &str, side: usize) {
        if let Some(canvas) = &mut self.canvas[side] {
            let png_data = canvas.to_png();
            _ = App::save_file_as(filename, &png_data);
        }
    }

    #[cfg(feature = "svg")]
    pub(crate) fn save_side_as_svg(&self, filename: &str, side: usize) -> Result<(), Error> {
        if !(self.show_data_layer || self.show_metadata_layer) {
            // Nothing to render
            return Err(anyhow!("No layers selected for rendering"));
        }

        let mut renderer = SvgRenderer::new()
            .with_side(side as u8)
            .with_radius_ratios(0.3, 1.0)
            .with_track_gap(0.1)
            .with_data_layer(self.show_data_layer, None)
            .with_metadata_layer(self.show_metadata_layer)
            .with_layer_stack(true)
            .with_initial_turning(TurningDirection::Clockwise)
            .with_blend_mode(fluxfox_svg::prelude::BlendMode::Color)
            .with_side_view_box(VizRect::from((
                0.0,
                0.0,
                self.resolution as f32,
                self.resolution as f32,
            )));

        if let Some(disk) = self.disk.as_ref().and_then(|d| d.read(Tool::Visualization).ok()) {
            renderer = renderer
                .render(&disk)
                .map_err(|e| anyhow!("Error rendering SVG: {}", e))?;
        }
        else {
            return Err(anyhow!("Couldn't lock disk for reading"));
        }

        let documents = renderer
            .create_documents()
            .map_err(|e| anyhow!("Error creating SVG document: {}", e))?;

        if documents.is_empty() {
            return Err(anyhow!("No SVG documents created"));
        }

        let svg_data = documents[0].document.to_string();

        App::save_file_as(filename, svg_data.as_bytes()).map_err(|e| anyhow!("Error saving SVG file: {}", e))
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) -> Option<VizEvent> {
        let mut new_event = None;
        let mut svg_context = None;

        // Receive render events
        while let Ok(msg) = self.render_receiver.try_recv() {
            match msg {
                RenderMessage::DataRenderComplete(head) => {
                    log::debug!("Data render of head {} complete", head);
                    self.composite(head as usize);
                }
                RenderMessage::DataRenderError(e) => {
                    log::error!("Data render error: {}", e);
                }
            }
        }

        // Clear the hover selection display list every frame. This avoids "sticky" ghost selections
        // if the mouse cursor rapidly leaves the window
        self.selection_display_list2 = None;

        let mut context = VisualizationContext {
            got_hit: [false, false],
            context_menu_open: &mut self.context_menu_open,
            selection_rect_opt: &mut self.selection_rect_opt,
            display_list_opt: &mut self.selection_display_list2,
            events: &mut self.events,
            common_viz_params: &mut self.common_viz_params,
        };

        ui.horizontal(|ui| {
            ui.set_min_width(200.0);
            ui.add(
                egui::Slider::new(&mut self.angle, 0.0..=TAU)
                    .text("Index Angle:")
                    .step_by((TAU / 360.0) as f64),
            );
        });

        ui.horizontal(|ui| {
            for side in 0..self.sides {
                context.common_viz_params.direction = TurningDirection::from(side as u8);

                if self.have_render[side] {
                    if let Some(canvas) = &mut self.canvas[side] {
                        // Adjust the angle for the turning direction of this side, then
                        // set the canvas rotation angle.
                        canvas.set_rotation(TurningDirection::from(side as u8).opposite().adjust_angle(self.angle));
                        let layer_id = ui.layer_id();
                        let response = canvas.show_as_mesh2(
                            ui,
                            Some(|response: &egui::Response, screen_pos: Pos2, virtual_pos: Pos2| {
                                if !*context.context_menu_open {
                                    egui::popup::show_tooltip(
                                        &response.ctx,
                                        layer_id,
                                        response.id.with("render_hover_tooltip"),
                                        |ui| {
                                            ui.label(format!(
                                                "Hovering at: ({:.2}, {:.2})",
                                                virtual_pos.x, virtual_pos.y
                                            ));

                                            // Create a rotation transformation for the current side, turning and angle
                                            // This really should be done for us by the canvas, I think.
                                            let rotation = VizRotation::new(
                                                TurningDirection::from(side as u8).adjust_angle(self.angle),
                                                VizPoint2d::new(
                                                    VIZ_RESOLUTION as f32 / 2.0,
                                                    VIZ_RESOLUTION as f32 / 2.0,
                                                ),
                                            );

                                            let hit_test_params = RenderDiskHitTestParams {
                                                side: side as u8,
                                                selection_type: RenderDiskSelectionType::Sector,
                                                geometry: RenderGeometry::Arc,
                                                point: VizPoint2d::new(virtual_pos.x, virtual_pos.y).rotate(&rotation),
                                            };

                                            if let Some(disk) =
                                                self.disk.as_ref().and_then(|d| d.read(Tool::Visualization).ok())
                                            {
                                                Self::perform_hit_test(&disk, ui, side, &hit_test_params, &mut context);
                                            }
                                            else {
                                                log::warn!("Couldn't lock disk for reading.");
                                            }
                                            // match self.disk.as_ref().unwrap().try_read() {
                                            //     Ok(disk) => {
                                            //         self.common_viz_params.radius = Some(VIZ_RESOLUTION as f32 / 2.0);
                                            //         match vectorize_disk_hit_test(
                                            //             &disk,
                                            //             &self.common_viz_params,
                                            //             &hit_test_params,
                                            //             VizElementFlags::HIGHLIGHT,
                                            //         ) {
                                            //             Ok(hit) => {
                                            //                 ui.label(format!("angle: {:.3}", hit.angle));
                                            //                 ui.label(format!("Cylinder: {}", hit.track));
                                            //                 ui.label(format!("bit index: {}", hit.bit_index));
                                            //                 if let Some(display_list) = hit.display_list {
                                            //                     ui.label(format!(
                                            //                         "Got display list of {} elements.",
                                            //                         display_list.iter().count()
                                            //                     ));
                                            //
                                            //                     got_hit = true;
                                            //                     display_list_opt = Some(display_list);
                                            //                 }
                                            //                 else {
                                            //                     new_event = Some(VizEvent::SectorDeselected);
                                            //                     ui.label("No sector");
                                            //                 }
                                            //             }
                                            //             Err(e) => {
                                            //                 log::error!("Error hit testing disk: {}", e);
                                            //             }
                                            //         }
                                            //     }
                                            //     Err(_) => log::warn!("Couldn't lock disk for reading."),
                                            // }
                                        },
                                    );
                                }
                            }),
                        );

                        if let Some(response) = response {
                            if context.got_hit[side] {
                                //log::warn!("got hit! rect: {:?}", response.rect);
                                *context.selection_rect_opt = Some(response.rect);
                            }
                            else {
                                //*context.selection_rect_opt = None;
                            }

                            if response.clicked_elsewhere() {
                                *context.context_menu_open = false; // Reset state
                            }

                            response.context_menu(|ui| {
                                *context.context_menu_open = true;
                                if ui.button("Save as PNG").clicked() {
                                    let png_data = canvas.to_png();
                                    let file_name = format!("fluxfox_viz_side{}.png", side);

                                    _ = App::save_file_as(&file_name, &png_data);
                                    ui.close_menu();
                                }
                                if ui.button("Save as SVG").clicked() {
                                    svg_context = Some((format!("fluxfox_viz_side{}.svg", side), side));
                                    ui.close_menu();
                                }
                            });
                        };
                    }
                }
            }
        });

        if let (Some(display_list), Some(rect)) = (context.display_list_opt, context.selection_rect_opt) {
            // Only draw the hover-selection if the mouse is over the canvas
            if ui.ctx().pointer_hover_pos().is_some() {
                let resolution = Vec2::new(VIZ_RESOLUTION as f32, VIZ_RESOLUTION as f32);
                let viz_rect = if let Some(quadrant) = self.zoom_quadrant[display_list.side as usize] {
                    match quadrant & 0x03 {
                        0 => Rect::from_min_size(Pos2::ZERO, resolution / 2.0),
                        1 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, 0.0), resolution / 2.0),
                        2 => Rect::from_min_size(Pos2::new(0.0, resolution.y / 2.0), resolution / 2.0),
                        3 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, resolution.y / 2.0), resolution / 2.0),
                        _ => unreachable!(),
                    }
                }
                else {
                    Rect::from_min_size(Pos2::ZERO, resolution)
                };

                if let Some(canvas) = &self.canvas[display_list.side as usize] {
                    let canvas_transform = canvas.virtual_transform().inverse();
                    let virtual_rect = canvas.virtual_rect();
                    let to_screen = RectTransform::from_to(
                        viz_rect, // Local space
                        *rect,    // Screen space
                    );

                    let from_virtual = RectTransform::from_to(*virtual_rect, *rect);

                    let combined_transform = Self::combine_transforms(&canvas_transform, &to_screen);

                    let rotation = VizRotation::new(
                        TurningDirection::from(display_list.side)
                            .opposite()
                            .adjust_angle(self.angle),
                        VizPoint2d::new(VIZ_RESOLUTION as f32 / 2.0, VIZ_RESOLUTION as f32 / 2.0),
                    );

                    let selection_painter = ui.painter().with_clip_rect(*rect);

                    paint_elements(
                        &selection_painter,
                        &from_virtual,
                        &rotation,
                        &Default::default(),
                        &display_list.items(0).unwrap(),
                        false,
                    );
                }
            }

            // log::warn!("Drawing debug rect...");
            // selection_painter.rect_filled(
            //     Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 500.0)),
            //     0.0,
            //     egui::Color32::RED,
            // );
        }

        // Deferred SVG rendering from context menu
        if let Some((svg_filename, side)) = svg_context {
            if let Err(e) = self.save_side_as_svg(&svg_filename, side) {
                log::error!("Error saving SVG: {}", e);
            }
        }

        if self.last_event != new_event {
            self.last_event = new_event.clone();
        }
        new_event
    }

    fn combine_transforms(transform_a: &RectTransform, transform_b: &RectTransform) -> RectTransform {
        // Combine transformations by chaining their mappings
        RectTransform::from_to(*transform_a.from(), transform_b.transform_rect(*transform_a.to()))
    }

    fn perform_hit_test(
        disk: &DiskImage,
        ui: &mut egui::Ui,
        side: usize,
        params: &RenderDiskHitTestParams,
        context: &mut VisualizationContext,
    ) {
        context.common_viz_params.direction = TurningDirection::from(side as u8);
        context.common_viz_params.radius = Some(VIZ_RESOLUTION as f32 / 2.0);

        match vectorize_disk_hit_test(&disk, context.common_viz_params, params, VizElementFlags::HIGHLIGHT) {
            Ok(hit) => {
                ui.label(format!("angle: {:.3}", hit.angle));
                ui.label(format!("Cylinder: {}", hit.track));
                ui.label(format!("bit index: {}", hit.bit_index));
                if let Some(display_list) = hit.display_list {
                    ui.label(format!("Got display list of {} elements.", display_list.len()));
                    context.got_hit[side] = true;
                    *context.display_list_opt = Some(display_list);
                }
                else {
                    context.got_hit[side] = false;
                    *context.display_list_opt = None;
                    context.events.push(VizEvent::SectorDeselected);
                    ui.label("No sector");
                }
            }
            Err(e) => log::error!("Error hit testing disk: {}", e),
        }
    }

    // fn show_context_menu(
    //     &self,
    //     ui: &mut egui::Ui,
    //     side: usize,
    //     canvas: &mut PixelCanvas,
    //     context: &mut VisualizationContext,
    // ) {
    //     ui.context_menu(|ui| {
    //         *context.context_menu_open = true;
    //         if ui.button("Save as PNG").clicked() {
    //             let png_data = canvas.to_png();
    //             let file_name = format!("fluxfox_viz_side{}.png", side);
    //             _ = App::save_file_as(&file_name, &png_data);
    //             ui.close_menu();
    //         }
    //     });
    // }
}

impl App {}
