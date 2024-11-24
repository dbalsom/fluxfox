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
use crate::{native::worker, App};
use anyhow::{anyhow, Error};
use fluxfox::{
    structure_parsers::DiskStructureGenericElement,
    tiny_skia,
    tiny_skia::{Color, Pixmap},
    visualization::{
        render_track_data,
        render_track_metadata_quadrant,
        RenderTrackDataParams,
        RenderTrackMetadataParams,
        ResolutionType,
    },
    DiskImage,
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

pub enum RenderMessage {
    DataRenderComplete(u8),
    DataRenderError(String),
}

pub struct VisualizationState {
    pub supersample: u32,
    pub meta_pixmap_pool: Vec<Arc<Mutex<Pixmap>>>,
    pub data_img: [Arc<Mutex<Pixmap>>; 2],
    pub metadata_img: [Pixmap; 2],
    pub composite_img: [Pixmap; 2],
    pub sector_lookup_img: [Pixmap; 2],
    pub meta_palette: HashMap<DiskStructureGenericElement, Color>,
    pub have_render: [bool; 2],
    pub canvas: [Option<PixelCanvas>; 2],
    pub sides: usize,
    pub render_sender: mpsc::SyncSender<RenderMessage>,
    pub render_receiver: mpsc::Receiver<RenderMessage>,
}

impl Default for VisualizationState {
    fn default() -> Self {
        let (render_sender, render_receiver) = mpsc::sync_channel(2);
        Self {
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
                (DiskStructureGenericElement::SectorData, pal_medium_green),
                (DiskStructureGenericElement::SectorBadData, pal_orange),
                (DiskStructureGenericElement::SectorDeletedData, pal_dark_green),
                (DiskStructureGenericElement::SectorBadDeletedData, viz_light_red),
                (DiskStructureGenericElement::SectorHeader, pal_light_blue),
                (DiskStructureGenericElement::SectorBadHeader, pal_medium_blue),
                (DiskStructureGenericElement::Marker, vis_purple),
            ]),
            canvas: [Some(canvas0), Some(canvas1)],
            ..VisualizationState::default()
        }
    }

    pub(crate) fn render_visualization(&mut self, disk_lock: Arc<RwLock<DiskImage>>, side: usize) -> Result<(), Error> {
        if self.meta_pixmap_pool.len() < 4 {
            return Err(anyhow!("Pixmap pool not initialized"));
        }

        let render_lock = disk_lock.clone();
        let render_sender = self.render_sender.clone();
        let render_target = self.data_img[side].clone();

        let disk = &disk_lock.read().unwrap();
        let head = side as u8;
        let quadrant = 0;
        let angle = 0.0;
        let min_radius_fraction = 0.333;
        let render_track_gap = 0.10;
        let direction = match head {
            0 => fluxfox::visualization::RotationDirection::CounterClockwise,
            _ => fluxfox::visualization::RotationDirection::Clockwise,
        };
        let track_ct = disk.get_track_ct(side.into());

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
                image_pos: (0, 0),
                min_radius_fraction,
                index_angle: angle,
                track_limit: track_ct,
                track_gap: render_track_gap,
                direction: direction.opposite(),
                decode: true,
                resolution: ResolutionType::Byte,
                pin_last_standard_track: true,
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
            Err(e) => {
                log::error!("Error spawning rendering worker for data layer: {}", e);
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
            draw_empty_tracks: true,
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

            let paint = tiny_skia::PixmapPaint::default();
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

            let paint = tiny_skia::PixmapPaint::default();
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

    fn composite(&mut self, side: usize) {
        // If we can acquire the data mutex, we can composite the data and metadata layers.
        match self.data_img[side].try_lock() {
            Ok(data) => {
                let mut paint = PixmapPaint::default();
                // Scale the data pixmap down to the composite size with bilinear filtering.
                paint.quality = FilterQuality::Bilinear;
                let scale = 1.0 / self.supersample as f32;
                let transform = tiny_skia::Transform::from_scale(scale, scale);
                self.composite_img[side].fill(Color::TRANSPARENT);
                self.composite_img[side].draw_pixmap(0, 0, data.as_ref(), &paint, transform, None);
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
            Err(_) => {
                log::debug!("Data pixmap locked, deferring compositing...");
                let paint = tiny_skia::PixmapPaint::default();
                self.composite_img[side].fill(Color::TRANSPARENT);
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

    pub(crate) fn is_open(&self) -> bool {
        self.have_render[0]
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        // Receive events
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
                        canvas.show(
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
                                                ui.label("No sector");
                                            }
                                            else {
                                                let head = pixel.red();
                                                let cyl = pixel.green();
                                                let sector = pixel.blue();
                                                ui.label(format!("Sector lookup: {} {} {}", head, cyl, sector));
                                            }
                                        });
                                    },
                                );
                            }),
                        );
                    }
                }
            }
        });
    }
}

impl App {}
