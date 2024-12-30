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
*/

#[cfg(not(target_arch = "wasm32"))]
use crate::native::worker;
#[cfg(target_arch = "wasm32")]
use crate::wasm::worker;

use crate::App;
use anyhow::{anyhow, Error};
use fluxfox::{
    prelude::*,
    tiny_skia,
    tiny_skia::{Color, Pixmap},
    track_schema::GenericTrackElement,
    visualization::{
        render_disk_selection,
        render_track_data,
        render_track_metadata_quadrant,
        RenderDiskSelectionParams,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        RotationDirection,
    },
};
use fluxfox_egui::widgets::texture::{PixelCanvas, PixelCanvasDepth};
use std::{
    collections::HashMap,
    default::Default,
    sync::{mpsc, Arc, Mutex, RwLock},
};
use tiny_skia::{BlendMode, FilterQuality, PixmapPaint};

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

pub struct VisualizationState {
    pub compatible: bool,
    pub supersample: u32,
    pub meta_pixmap_pool: Vec<Arc<Mutex<Pixmap>>>,
    pub data_img: [Arc<Mutex<Pixmap>>; 2],
    pub selection_img: [Arc<Mutex<Pixmap>>; 2],
    pub metadata_img: [Pixmap; 2],
    pub composite_img: [Pixmap; 2],
    pub sector_lookup_img: [Pixmap; 2],
    pub meta_palette: HashMap<GenericTrackElement, Color>,
    pub have_render: [bool; 2],
    pub canvas: [Option<PixelCanvas>; 2],
    pub sides: usize,
    pub render_sender: mpsc::SyncSender<RenderMessage>,
    pub render_receiver: mpsc::Receiver<RenderMessage>,
    pub show_data_layer: bool,
    pub show_metadata_layer: bool,
    #[allow(dead_code)]
    pub show_error_layer: bool,
    #[allow(dead_code)]
    pub show_weak_layer: bool,
    #[allow(dead_code)]
    pub show_selection_layer: bool,
    pub last_event: Option<VizEvent>,
}

impl Default for VisualizationState {
    fn default() -> Self {
        let (render_sender, render_receiver) = mpsc::sync_channel(2);
        Self {
            compatible: false,
            supersample: VIZ_DATA_SUPERSAMPLE,
            meta_pixmap_pool: Vec::new(),
            data_img: [
                Arc::new(Mutex::new(
                    Pixmap::new(VIZ_SUPER_RESOLUTION, VIZ_SUPER_RESOLUTION).unwrap(),
                )),
                Arc::new(Mutex::new(
                    Pixmap::new(VIZ_SUPER_RESOLUTION, VIZ_SUPER_RESOLUTION).unwrap(),
                )),
            ],
            selection_img: [
                Arc::new(Mutex::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
                Arc::new(Mutex::new(Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap())),
            ],
            metadata_img: [
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
                Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(),
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
            have_render: [false; 2],
            canvas: [None, None],
            sides: 1,
            render_sender,
            render_receiver,
            show_data_layer: true,
            show_metadata_layer: true,
            show_error_layer: false,
            show_weak_layer: false,
            show_selection_layer: true,
            last_event: None,
        }
    }
}

impl VisualizationState {
    pub fn new(ctx: egui::Context, resolution: u32) -> Self {
        assert_eq!(resolution % 2, 0);

        let viz_light_red: Color = Color::from_rgba8(180, 0, 0, 255);

        //let viz_orange: Color = Color::from_rgba8(255, 100, 0, 255);
        let vis_purple: Color = Color::from_rgba8(180, 0, 180, 255);
        //let viz_cyan: Color = Color::from_rgba8(70, 200, 200, 255);
        //let vis_light_purple: Color = Color::from_rgba8(185, 0, 255, 255);

        let pal_medium_green = Color::from_rgba8(0x38, 0xb7, 0x64, 0xff);
        let pal_dark_green = Color::from_rgba8(0x25, 0x71, 0x79, 0xff);
        //let pal_dark_blue = Color::from_rgba8(0x29, 0x36, 0x6f, 0xff);
        let pal_medium_blue = Color::from_rgba8(0x3b, 0x5d, 0xc9, 0xff);
        let pal_light_blue = Color::from_rgba8(0x41, 0xa6, 0xf6, 0xff);
        //let pal_dark_purple = Color::from_rgba8(0x5d, 0x27, 0x5d, 0xff);
        let pal_orange = Color::from_rgba8(0xef, 0x7d, 0x57, 0xff);
        //let pal_dark_red = Color::from_rgba8(0xb1, 0x3e, 0x53, 0xff);

        let mut meta_pixmap_pool = Vec::new();
        for _ in 0..4 {
            let pixmap = Arc::new(Mutex::new(Pixmap::new(resolution / 2, resolution / 2).unwrap()));
            meta_pixmap_pool.push(pixmap);
        }

        let mut canvas0 = PixelCanvas::new((resolution, resolution), ctx.clone(), "head0_canvas");
        canvas0.set_bpp(PixelCanvasDepth::Rgba);
        let mut canvas1 = PixelCanvas::new((resolution, resolution), ctx.clone(), "head1_canvas");
        canvas1.set_bpp(PixelCanvasDepth::Rgba);

        Self {
            meta_pixmap_pool,
            meta_palette: HashMap::from([
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

    pub fn render_visualization(&mut self, disk_lock: Arc<RwLock<DiskImage>>, side: usize) -> Result<(), Error> {
        if self.meta_pixmap_pool.len() < 4 {
            return Err(anyhow!("Pixmap pool not initialized"));
        }

        let render_lock = disk_lock.clone();
        let render_sender = self.render_sender.clone();
        let render_target = self.data_img[side].clone();

        let disk = &disk_lock.read().unwrap();

        self.compatible = disk.can_visualize();

        if !self.compatible {
            return Err(anyhow!("Incompatible disk resolution"));
        }

        let head = side as u8;
        let quadrant = 0;
        let angle = 0.0;
        let min_radius_fraction = 0.333;
        let render_track_gap = 0.10;
        let direction = match head {
            0 => RotationDirection::CounterClockwise,
            _ => RotationDirection::Clockwise,
        };
        let track_ct = disk.track_ct(side.into());

        if side >= disk.heads() as usize {
            // Ignore request for non-existent side.
            return Ok(());
        }

        // Render the main data layer.
        match worker::spawn_closure_worker(move || {
            let render_params = RenderTrackDataParams {
                bg_color: None,
                map_color: None,
                head,
                image_size: (VIZ_SUPER_RESOLUTION, VIZ_SUPER_RESOLUTION),
                min_radius_fraction,
                index_angle: angle,
                track_limit: track_ct,
                track_gap: render_track_gap,
                direction: direction.opposite(),
                decode: true,
                sector_mask: true,
                pin_last_standard_track: true,
                ..Default::default()
            };

            let disk = render_lock.read().unwrap();
            let mut render_pixmap = render_target.lock().unwrap();
            render_pixmap.fill(Color::TRANSPARENT);

            match render_track_data(&disk, &mut render_pixmap, &render_params) {
                Ok(_) => {
                    log::debug!("render worker: Data layer rendered for side {}", head);
                    render_sender.send(RenderMessage::DataRenderComplete(head)).unwrap();
                }
                Err(e) => {
                    log::error!("Error rendering tracks: {}", e);
                    std::process::exit(1);
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

        // Clear pixmap before rendering
        self.metadata_img[side].fill(Color::TRANSPARENT);

        let mut render_params = RenderTrackMetadataParams {
            quadrant,
            head,
            min_radius_fraction,
            index_angle: angle,
            track_limit: track_ct,
            track_gap: render_track_gap,
            direction,
            palette: self.meta_palette.clone(),
            draw_empty_tracks: false,
            pin_last_standard_track: true,
            draw_sector_lookup: false,
        };

        // Render metadata quadrants into pixmap pool.
        for quadrant in 0..4 {
            render_params.quadrant = quadrant as u8;
            let mut pixmap = self.meta_pixmap_pool[quadrant].lock().unwrap();

            match render_track_metadata_quadrant(disk, &mut pixmap, &render_params) {
                Ok(_) => {
                    log::debug!("...Rendered quadrant {}", quadrant);
                }
                Err(e) => {
                    log::error!("Error rendering quadrant: {}", e);
                    return Err(anyhow!("Error rendering metadata"));
                }
            }
        }

        // Composite metadata quadrants into one complete metadata pixmap.
        for quadrant in 0..4 {
            log::debug!("Received quadrant {}, compositing...", quadrant);
            let (x, y) = match quadrant {
                0 => (0, 0),
                1 => (VIZ_RESOLUTION / 2, 0),
                2 => (0, VIZ_RESOLUTION / 2),
                3 => (VIZ_RESOLUTION / 2, VIZ_RESOLUTION / 2),
                _ => panic!("Invalid quadrant"),
            };

            let paint = PixmapPaint::default();
            //let mut pixmap = self.meta_pixmap_pool[quadrant].lock()?;

            self.metadata_img[side].draw_pixmap(
                x as i32,
                y as i32,
                self.meta_pixmap_pool[quadrant].lock().unwrap().as_ref(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );

            // Clear pixmap after compositing
            self.meta_pixmap_pool[quadrant]
                .lock()
                .unwrap()
                .as_mut()
                .fill(Color::TRANSPARENT);
        }

        // Render sector lookup quadrants into pixmap pool.
        render_params.draw_sector_lookup = true;
        // Make it easier to select sectors by removing track gap.
        render_params.track_gap = 0.0;
        for quadrant in 0..4 {
            render_params.quadrant = quadrant as u8;
            let mut pixmap = self.meta_pixmap_pool[quadrant].lock().unwrap();

            match render_track_metadata_quadrant(disk, &mut pixmap, &render_params) {
                Ok(_) => {
                    log::debug!("...Rendered quadrant {}", quadrant);
                }
                Err(e) => {
                    log::error!("Error rendering quadrant: {}", e);
                    return Err(anyhow!("Error rendering metadata"));
                }
            }
        }

        // Composite sector lookup quadrants into final pixmap.
        for quadrant in 0..4 {
            log::debug!("Received quadrant {}, compositing...", quadrant);
            let (x, y) = match quadrant {
                0 => (0, 0),
                1 => (VIZ_RESOLUTION / 2, 0),
                2 => (0, VIZ_RESOLUTION / 2),
                3 => (VIZ_RESOLUTION / 2, VIZ_RESOLUTION / 2),
                _ => panic!("Invalid quadrant"),
            };

            let paint = PixmapPaint::default();
            //let mut pixmap = self.meta_pixmap_pool[quadrant].lock()?;

            self.sector_lookup_img[side].draw_pixmap(
                x as i32,
                y as i32,
                self.meta_pixmap_pool[quadrant].lock().unwrap().as_ref(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );

            // Clear pixmap after compositing
            self.meta_pixmap_pool[quadrant]
                .lock()
                .unwrap()
                .as_mut()
                .fill(Color::TRANSPARENT);
        }

        if let Some(canvas) = &mut self.canvas[side] {
            if canvas.has_texture() {
                log::debug!("Updating canvas...");
                log::debug!("pixmap data slice: {:0X?}", &self.metadata_img[side].data()[0..16]);
                canvas.update_data(self.metadata_img[side].data());
                self.have_render[side] = true;
            }
            else {
                log::debug!("Canvas not initialized, deferring update...");
                //self.draw_deferred = true;
            }
        }
        Ok(())
    }

    pub fn clear_selection(&mut self, side: usize) {
        if let Ok(mut pixmap) = self.selection_img[side].try_lock() {
            pixmap.fill(Color::TRANSPARENT);
        }
    }

    pub fn update_selection(&mut self, disk_lock: Arc<RwLock<DiskImage>>, c: u8, h: u8, s_idx: u8) {
        let disk = match disk_lock.try_read() {
            Ok(disk) => disk,
            Err(_) => {
                log::debug!("Disk image could not be locked for reading, deferring sector selection...");
                return;
            }
        };

        let side = h as usize;
        let mut do_composite = false;

        self.clear_selection(side);

        // If we can acquire the selection image mutex, we can composite the data and metadata layers.
        match self.selection_img[side].try_lock() {
            Ok(mut data) => {
                let params = RenderDiskSelectionParams {
                    ch: DiskCh::new(c as u16, h),
                    sector_idx: s_idx as usize,
                    track_limit: disk.tracks(h) as usize,
                    direction: RotationDirection::from(h),
                    color: Color::WHITE,
                    pin_last_standard_track: true,
                    ..Default::default()
                };

                match render_disk_selection(&disk, &mut data, &params) {
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
                    let scale = 1.0 / self.supersample as f32;
                    let transform = tiny_skia::Transform::from_scale(scale, scale);
                    self.composite_img[side].fill(Color::TRANSPARENT);
                    self.composite_img[side].draw_pixmap(0, 0, data.as_ref(), &paint, transform, None);
                }
                else {
                    self.composite_img[side].fill(Color::TRANSPARENT);
                }
                if self.show_metadata_layer {
                    paint = PixmapPaint {
                        opacity:    1.0,
                        blend_mode: BlendMode::HardLight,
                        quality:    FilterQuality::Nearest,
                    };
                    self.composite_img[side].draw_pixmap(
                        0,
                        0,
                        self.metadata_img[side].as_ref(),
                        &paint,
                        tiny_skia::Transform::identity(),
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
                        self.metadata_img[side].as_ref(),
                        &paint,
                        tiny_skia::Transform::identity(),
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
                    self.composite_img[side].draw_pixmap(
                        0,
                        0,
                        selection.as_ref(),
                        &paint,
                        tiny_skia::Transform::identity(),
                        None,
                    );
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

    pub(crate) fn set_sides(&mut self, sides: usize) {
        self.sides = sides;
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

    pub(crate) fn save_side_as(&mut self, filename: &str, side: usize) {
        if let Some(canvas) = &mut self.canvas[side] {
            let png_data = canvas.to_png();
            _ = App::save_file_as(filename, &png_data);
        }
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) -> Option<VizEvent> {
        let mut new_event = None;

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

        ui.horizontal(|ui| {
            for side in 0..self.sides {
                if self.have_render[side] {
                    if let Some(canvas) = &mut self.canvas[side] {
                        let layer_id = ui.layer_id();
                        let response = canvas.show(
                            ui,
                            Some(|response: &egui::Response, x: f32, y: f32| {
                                egui::popup::show_tooltip(
                                    &response.ctx,
                                    layer_id,
                                    response.id.with("render_hover_tooltip"),
                                    |ui| {
                                        ui.label(format!("Hovering at: ({:.2}, {:.2})", x, y));

                                        self.sector_lookup_img[side].pixel(x as u32, y as u32).map(|pixel| {
                                            if pixel.alpha() == 0 {
                                                new_event = Some(VizEvent::SectorDeselected);
                                                ui.label("No sector");
                                            }
                                            else {
                                                let head = pixel.red();
                                                let cyl = pixel.green();
                                                let sector = pixel.blue();
                                                ui.label(format!("Sector lookup: {} {} {}", head, cyl, sector));
                                                new_event = Some(VizEvent::NewSectorSelected {
                                                    c: cyl,
                                                    h: head,
                                                    s_idx: sector,
                                                });
                                            }
                                        });
                                    },
                                );
                            }),
                        );

                        if let Some(response) = response {
                            response.context_menu(|ui| {
                                if ui.button("Save as PNG").clicked() {
                                    let png_data = canvas.to_png();
                                    let file_name = format!("fluxfox_viz_side{}.png", side);

                                    _ = App::save_file_as(&file_name, &png_data);
                                    ui.close_menu();
                                }
                            });
                        };
                    }
                }
            }
        });

        if self.last_event != new_event {
            self.last_event = new_event.clone();
        }
        new_event
    }
}

impl App {}
