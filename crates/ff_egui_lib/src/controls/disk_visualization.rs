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
    collections::HashMap,
    default::Default,
    f32::consts::TAU,
    ops::Range,
    sync::{mpsc, Arc, Mutex, RwLock},
};

use crate::{
    controls::{
        canvas::{PixelCanvas, PixelCanvasDepth},
        header_group::{HeaderFn, HeaderGroup},
    },
    visualization::viz_elements::paint_elements,
    widgets::chs::ChsWidget,
    SaveFileCallbackFn,
    SectorSelection,
    TrackListSelection,
    UiError,
    UiEvent,
    UiLockContext,
};

use fluxfox::{
    prelude::*,
    track_schema::GenericTrackElement,
    visualization::{prelude::*, rasterize_disk::rasterize_disk_selection},
    FoxHashMap,
};

#[cfg(feature = "svg")]
use fluxfox_svg::prelude::*;
use fluxfox_tiny_skia::{
    render_display_list::render_data_display_list,
    render_elements::skia_render_display_list,
    styles::{default_skia_styles, SkiaStyle},
    tiny_skia::{BlendMode, Color, FilterQuality, Paint, Pixmap, PixmapPaint, Transform},
};

use egui::{emath::RectTransform, Align, Layout, Pos2, Rect, Vec2};

// Conditional thread stuff
use crate::tracking_lock::TrackingLock;
#[cfg(target_arch = "wasm32")]
use rayon::spawn;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::spawn;

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
    side_rects: [Rect; 2],
    got_hit: [bool; 2],
    context_menu_open: &'a mut bool,
    hover_rect_opt: &'a mut Option<Rect>,
    display_list_opt: &'a mut Option<VizElementDisplayList>,
    hover_display_list_opt: &'a mut Option<VizElementDisplayList>,
    ui_sender: &'a mut Option<mpsc::SyncSender<UiEvent>>,
    events: &'a mut Vec<VizEvent>,
    common_viz_params: &'a mut CommonVizParams,
    selection: &'a mut Option<SelectionContext>,
    hover_selection: Option<SelectionContext>,
}

#[derive(Clone)]
struct SelectionContext {
    mouse_pos: Pos2,
    side: u8,
    c: u16,
    bitcell_idx: usize,
    angle: f32,
    element_type: GenericTrackElement,
    element_range: Range<usize>,
    element_idx: usize,
    element_chsn: Option<DiskChsn>,
}

pub struct DiskVisualization {
    pub disk: Option<TrackingLock<DiskImage>>,
    pub ui_sender: Option<mpsc::SyncSender<UiEvent>>,
    pub resolution: u32,
    pub common_viz_params: CommonVizParams,
    pub compatible: bool,
    pub supersample: u32,
    pub data_img: [Arc<Mutex<Pixmap>>; 2],
    pub meta_img: [Arc<RwLock<Pixmap>>; 2],
    pub selection_img: [Arc<Mutex<Pixmap>>; 2],

    pub composite_img: [Pixmap; 2],
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
    pub selection_display_list: Option<VizElementDisplayList>,
    pub hover_display_list: Option<VizElementDisplayList>,
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
    selection: Option<SelectionContext>,
    save_file_callback: Option<SaveFileCallbackFn>,
}

impl Default for DiskVisualization {
    #[rustfmt::skip]
    fn default() -> Self {
        let (render_sender, render_receiver) = mpsc::sync_channel(2);
        Self {
            disk: None,
            ui_sender: None,
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
            selection_display_list: None,
            hover_display_list: None,
            last_event: None,
            context_menu_open: false,
            events: Vec::new(),
            got_hit: false,
            selection_rect_opt: None,
            angle: 0.0,
            zoom_quadrant: [None, None],
            selection: None,
            save_file_callback: None,
        }
    }
}

impl DiskVisualization {
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

        log::warn!("Creating visualization state...");
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
            ..DiskVisualization::default()
        }
    }

    pub fn set_save_file_callback(&mut self, callback: SaveFileCallbackFn) {
        self.save_file_callback = Some(callback);
    }

    pub fn set_event_sender(&mut self, sender: mpsc::SyncSender<UiEvent>) {
        log::warn!("Setting event sender...");
        self.ui_sender = Some(sender);
    }

    #[allow(dead_code)]
    pub fn compatible(&self) -> bool {
        self.compatible
    }

    pub fn update_disk(&mut self, disk_lock: impl Into<TrackingLock<DiskImage>>) {
        let disk_lock = disk_lock.into();
        self.disk = Some(disk_lock.clone());

        let disk = disk_lock.read(UiLockContext::DiskVisualization).unwrap();
        self.compatible = disk.can_visualize();
        log::debug!("update_disk(): setting compatible flag to {}", self.compatible);
        self.sides = disk.heads() as usize;

        // Reset selections
        self.selection = None;
        self.selection_display_list = None;
        self.hover_display_list = None;
    }

    pub fn render_visualization(&mut self, side: usize) -> Result<(), UiError> {
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

        let disk = self
            .disk
            .as_ref()
            .unwrap()
            .read(UiLockContext::DiskVisualization)
            .unwrap();
        self.compatible = disk.can_visualize();
        if !self.compatible {
            return Err(UiError::VisualizationError("Incompatible disk resolution".to_string()));
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
            ..CommonVizParams::default()
        };

        let inner_common_params = self.common_viz_params.clone();
        let inner_decode_data = self.decode_data_layer;
        let inner_angle = self.common_viz_params.index_angle;

        log::debug!("Spawning rendering thread...");
        // Render the main data layer.
        spawn(move || {
            let data_params = RenderTrackDataParams {
                side: head,
                decode: inner_decode_data,
                slices: 1440,
                ..Default::default()
            };

            let vector_params = RenderVectorizationParams::default();

            let disk = render_lock.read(UiLockContext::DiskVisualization).unwrap();
            let mut render_pixmap = render_target.lock().unwrap();
            render_pixmap.fill(Color::TRANSPARENT);

            match vectorize_disk_data(&disk, &inner_common_params, &data_params, &vector_params) {
                Ok(display_list) => {
                    log::debug!(
                        "render worker: Data layer vectorized for side {}, created display list of {} elements",
                        head,
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
                        }
                    }
                }
                Err(e) => {
                    log::error!("Error rendering tracks: {}", e);
                }
            };
        });

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
        let display_list = vectorize_disk_elements_by_quadrants(&disk, &self.common_viz_params, &render_params)
            .map_err(|e| {
                log::error!("Error vectorizing disk elements: {}", e);
                UiError::VisualizationError(format!("Error vectorizing disk elements: {}", e))
            })?;

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
        match self.disk.as_ref().unwrap().read(UiLockContext::DiskVisualization) {
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
                let mut paint = PixmapPaint {
                    quality: FilterQuality::Bilinear,
                    ..Default::default()
                };

                // Scale the data pixmap down to the composite size with bilinear filtering.
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

    pub fn enable_data_layer(&mut self, state: bool) {
        self.show_data_layer = state;
        for side in 0..self.sides {
            self.composite(side);
        }
    }

    pub fn enable_metadata_layer(&mut self, state: bool) {
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

    pub fn save_side_as_png(&mut self, filename: &str, side: usize) {
        if let (Some(canvas), Some(callback)) = (&mut self.canvas[side], self.save_file_callback.as_ref()) {
            let png_data = canvas.to_png();
            _ = callback(filename, &png_data);
        }
    }

    #[cfg(feature = "svg")]
    pub fn save_side_as_svg(&self, filename: &str, side: usize) -> Result<(), UiError> {
        if !(self.show_data_layer || self.show_metadata_layer) {
            // Nothing to render
            return Err(UiError::VisualizationError("No layers enabled".to_string()));
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

        if let Some(disk) = self
            .disk
            .as_ref()
            .and_then(|d| d.read(UiLockContext::DiskVisualization).ok())
        {
            renderer = renderer
                .render(&disk)
                .map_err(|e| UiError::VisualizationError(format!("Error rendering SVG: {}", e)))?;
        }
        else {
            return Err(UiError::VisualizationError(
                "Couldn't lock disk for reading".to_string(),
            ));
        }

        let documents = renderer
            .create_documents()
            .map_err(|e| UiError::VisualizationError(format!("Error creating SVG document: {}", e)))?;

        if documents.is_empty() {
            return Err(UiError::VisualizationError("No SVG documents created".to_string()));
        }

        let svg_data = documents[0].document.to_string();

        if let Some(callback) = self.save_file_callback.as_ref() {
            _ = callback(filename, svg_data.as_bytes());
        }

        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<VizEvent> {
        let mut new_event = None;
        #[cfg(feature = "svg")]
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
        self.hover_display_list = None;

        let mut context = VisualizationContext {
            side_rects: [Rect::ZERO, Rect::ZERO],
            got_hit: [false, false],
            context_menu_open: &mut self.context_menu_open,
            hover_rect_opt: &mut self.selection_rect_opt,
            display_list_opt: &mut self.selection_display_list,
            hover_display_list_opt: &mut self.hover_display_list,
            ui_sender: &mut self.ui_sender,
            events: &mut self.events,
            common_viz_params: &mut self.common_viz_params,
            selection: &mut self.selection,
            hover_selection: None,
        };

        ui.horizontal(|ui| {
            ui.set_min_width(400.0);
            ui.label("Index Angle:");
            ui.add(egui::Slider::new(&mut self.angle, 0.0..=TAU).step_by((TAU / 360.0) as f64));
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
                            Some(|response: &egui::Response, _screen_pos: Pos2, virtual_pos: Pos2| {
                                // Create a rotation transformation for the current side, turning and angle
                                // This really should be done for us by the canvas, I think.
                                let rotation = VizRotation::new(
                                    TurningDirection::from(side as u8).adjust_angle(self.angle),
                                    VizPoint2d::new(VIZ_RESOLUTION as f32 / 2.0, VIZ_RESOLUTION as f32 / 2.0),
                                );

                                let hit_test_params = RenderDiskHitTestParams {
                                    side: side as u8,
                                    selection_type: RenderDiskSelectionType::Sector,
                                    geometry: RenderGeometry::Arc,
                                    point: VizPoint2d::new(virtual_pos.x, virtual_pos.y).rotate(&rotation),
                                };

                                if let Some(disk) = self
                                    .disk
                                    .as_ref()
                                    .and_then(|d| d.read(UiLockContext::DiskVisualization).ok())
                                {
                                    Self::perform_hit_test(&disk, side, &hit_test_params, &mut context);
                                }
                                else {
                                    log::warn!("Couldn't lock disk for reading.");
                                }

                                if !*context.context_menu_open {
                                    if let Some(selection) = &context.hover_selection {
                                        egui::popup::show_tooltip(
                                            &response.ctx,
                                            layer_id,
                                            response.id.with("render_hover_tooltip"),
                                            |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(format!("{}", selection.element_type));
                                                });
                                            },
                                        );
                                    }
                                }
                            }),
                        );

                        if let Some(response) = response {
                            //log::debug!("setting side {} rect: {:?}", side, response.rect);
                            context.side_rects[side] = response.rect;

                            if response.clicked() {
                                if let Some(selection) = &context.hover_selection {
                                    // Send the selection event to the main app
                                    if let Some(sender) = context.ui_sender {
                                        if let Some(chsn) = selection.element_chsn {
                                            let event =
                                                UiEvent::SelectionChange(TrackListSelection::Sector(SectorSelection {
                                                    phys_ch:    DiskCh::new(selection.c, selection.side),
                                                    sector_id:  chsn,
                                                    bit_offset: Some(selection.bitcell_idx),
                                                }));

                                            _ = sender.send(event);
                                        }
                                    }
                                    else {
                                        log::warn!("No UI sender available!");
                                    }

                                    *context.selection = Some(selection.clone());
                                    *context.display_list_opt = context.hover_display_list_opt.clone();
                                }
                                else {
                                    // Clicked off of disk, clear selection
                                    *context.selection = None;
                                    *context.display_list_opt = None;
                                }
                            }

                            if context.got_hit[side] {
                                //log::warn!("got hit! rect: {:?}", response.rect);
                                *context.hover_rect_opt = Some(response.rect);
                            }

                            if response.clicked_elsewhere() {
                                *context.context_menu_open = false; // Reset state
                            }

                            response.context_menu(|ui| {
                                *context.context_menu_open = true;
                                if ui.button("Save as PNG").clicked() {
                                    let png_data = canvas.to_png();
                                    let file_name = format!("fluxfox_viz_side{}.png", side);

                                    if let Some(callback) = self.save_file_callback.as_ref() {
                                        _ = callback(&file_name, &png_data);
                                    };
                                    ui.close_menu();
                                }

                                #[cfg(feature = "svg")]
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

        // Draw sticky selection
        if let Some(display_list) = &context.display_list_opt {
            if let Some(canvas) = &self.canvas[display_list.side as usize] {
                let virtual_rect = canvas.virtual_rect();
                let rect = context.side_rects[display_list.side as usize];
                let from_virtual = RectTransform::from_to(*virtual_rect, rect);

                //log::debug!("have sticky display list: side {} rect: {:?}", display_list.side, rect);
                let rotation = VizRotation::new(
                    TurningDirection::from(display_list.side)
                        .opposite()
                        .adjust_angle(self.angle),
                    VizPoint2d::new(VIZ_RESOLUTION as f32 / 2.0, VIZ_RESOLUTION as f32 / 2.0),
                );

                let selection_painter = ui.painter().with_clip_rect(rect);

                paint_elements(
                    &selection_painter,
                    &from_virtual,
                    &rotation,
                    &Default::default(),
                    display_list.items(0).unwrap(),
                    false,
                );
            }
        }

        // Draw hover selection
        if let (Some(hover_display_list), Some(rect)) =
            (context.hover_display_list_opt.take(), context.hover_rect_opt.clone())
        {
            //let resolution = Vec2::new(VIZ_RESOLUTION as f32, VIZ_RESOLUTION as f32);
            // let viz_rect = if let Some(quadrant) = self.zoom_quadrant[hover_display_list.side as usize] {
            //     match quadrant & 0x03 {
            //         0 => Rect::from_min_size(Pos2::ZERO, resolution / 2.0),
            //         1 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, 0.0), resolution / 2.0),
            //         2 => Rect::from_min_size(Pos2::new(0.0, resolution.y / 2.0), resolution / 2.0),
            //         3 => Rect::from_min_size(Pos2::new(resolution.x / 2.0, resolution.y / 2.0), resolution / 2.0),
            //         _ => unreachable!(),
            //     }
            // }
            // else {
            //     Rect::from_min_size(Pos2::ZERO, resolution)
            // };

            if let Some(canvas) = &self.canvas[hover_display_list.side as usize] {
                //let canvas_transform = canvas.virtual_transform().inverse();
                let virtual_rect = canvas.virtual_rect();
                // let to_screen = RectTransform::from_to(
                //     viz_rect, // Local space
                //     *rect,    // Screen space
                // );

                let from_virtual = RectTransform::from_to(*virtual_rect, rect);

                let rotation = VizRotation::new(
                    TurningDirection::from(hover_display_list.side)
                        .opposite()
                        .adjust_angle(self.angle),
                    VizPoint2d::new(VIZ_RESOLUTION as f32 / 2.0, VIZ_RESOLUTION as f32 / 2.0),
                );

                let selection_painter = ui.painter().with_clip_rect(rect);

                // Draw the hover-selection if the mouse is over the canvas
                if ui.ctx().pointer_hover_pos().is_some() {
                    paint_elements(
                        &selection_painter,
                        &from_virtual,
                        &rotation,
                        &Default::default(),
                        hover_display_list.items(0).unwrap(),
                        false,
                    );
                }
            }
        }

        Self::show_info_pane(ui, &context);

        // Deferred SVG rendering from context menu
        #[cfg(feature = "svg")]
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

    fn show_info_pane(ui: &mut egui::Ui, context: &VisualizationContext<'_>) {
        ui.allocate_ui_with_layout(ui.available_size(), Layout::top_down(Align::Min), |ui| {
            HeaderGroup::new("Element Info").expand().show(
                ui,
                |ui| {
                    ui.allocate_ui_with_layout(ui.available_size(), Layout::left_to_right(Align::Min), |ui| {
                        if let Some(selection) = &context.selection {
                            HeaderGroup::new("Selected").show(
                                ui,
                                |ui| {
                                    Self::show_selection(ui, selection, false);
                                    //ui.set_min_height(200.0);
                                },
                                None::<HeaderFn>,
                            );
                        }
                        if let Some(selection) = &context.hover_selection {
                            HeaderGroup::new("Hovered").show(
                                ui,
                                |ui| {
                                    Self::show_selection(ui, selection, true);
                                    //ui.set_min_height(200.0);
                                },
                                None::<HeaderFn>,
                            );
                        }
                    });
                    ui.set_min_height(200.0);
                },
                None::<HeaderFn>,
            );
        });
    }

    fn show_selection(ui: &mut egui::Ui, selection: &SelectionContext, hover: bool) {
        egui::Grid::new(format!("viz_selection_info_grid_h:{}", hover))
            .striped(true)
            .show(ui, |ui| {
                ui.label("Side:");
                ui.label(format!("{}", selection.side));
                if hover {
                    ui.label("x, y:");
                    ui.label(format!("{:?}", selection.mouse_pos));
                }
                ui.end_row();
                ui.label("Cylinder:");
                ui.label(format!("{}", selection.c));

                if hover {
                    ui.label("Bitcell:");
                    ui.label(format!("{}", selection.bitcell_idx));
                }
                ui.end_row();

                ui.label("Element:");
                ui.label(format!("{}", selection.element_type));
                if hover {
                    ui.label("Angle:");
                    ui.label(format!("{:.3} ({:.2}°)", selection.angle, selection.angle.to_degrees()));
                }
                ui.end_row();

                ui.label("Element Range:");
                ui.label(format!("{:?}", selection.element_range));
                ui.end_row();

                if let Some(chsn) = selection.element_chsn {
                    ui.label("Sector ID:");
                    ui.add(ChsWidget::from_chs(chsn.into()));
                    ui.end_row();

                    ui.label("Sector size:");
                    ui.label(format!("{}", chsn.n_size()));
                    ui.end_row();
                }
            });
    }

    fn combine_transforms(transform_a: &RectTransform, transform_b: &RectTransform) -> RectTransform {
        // Combine transformations by chaining their mappings
        RectTransform::from_to(*transform_a.from(), transform_b.transform_rect(*transform_a.to()))
    }

    fn perform_hit_test(
        disk: &DiskImage,
        side: usize,
        params: &RenderDiskHitTestParams,
        context: &mut VisualizationContext<'_>,
    ) {
        context.common_viz_params.direction = TurningDirection::from(side as u8);
        context.common_viz_params.radius = Some(VIZ_RESOLUTION as f32 / 2.0);

        match vectorize_disk_hit_test(disk, context.common_viz_params, params, VizElementFlags::HIGHLIGHT) {
            Ok(hit) => {
                // ui.label(format!("angle: {:.3}", hit.angle));
                // ui.label(format!("Cylinder: {}", hit.track));
                // ui.label(format!("bit index: {}", hit.bit_index));
                if let Some(display_list) = hit.display_list {
                    //ui.label(format!("Got display list of {} elements.", display_list.len()));
                    context.got_hit[side] = true;

                    // The first item of the display list should have our element metadata.
                    // The selection will always be at track index 0.
                    if let Some(item) = display_list.items(0).and_then(|items| items.first()) {
                        context.hover_selection = Some(SelectionContext {
                            mouse_pos: params.point.to_tuple().into(),
                            side: side as u8,
                            c: hit.track,
                            bitcell_idx: hit.bit_index,
                            angle: hit.angle,
                            element_type: item.info.element_type,
                            element_range: item.info.bit_range.clone().unwrap_or(Range {
                                start: 0usize,
                                end:   0usize,
                            }),
                            element_idx: item.info.element_idx.unwrap_or(0),
                            element_chsn: item.info.chsn,
                        });
                    }

                    *context.hover_display_list_opt = Some(display_list);
                }
                else {
                    context.got_hit[side] = false;
                    *context.hover_display_list_opt = None;
                    context.events.push(VizEvent::SectorDeselected);
                }
            }
            Err(e) => log::error!("Error hit testing disk: {}", e),
        }
    }
}
