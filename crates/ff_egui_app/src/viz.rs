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
use std::default::Default;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Error};
use fluxfox::{tiny_skia, DiskImage};
use fluxfox::structure_parsers::DiskStructureGenericElement;
use fluxfox::tiny_skia::{Color, Pixmap};
use fluxfox::visualization::RenderTrackMetadataParams;
use fluxfox::visualization::render_track_metadata_quadrant;
use crate::App;
use crate::widgets::texture::{PixelCanvas, PixelCanvasDepth};

pub const VIZ_RESOLUTION: u32 = 512;

pub struct VisualizationState {
    pub meta_pixmap_pool: Vec<Arc<Mutex<Pixmap>>>,
    pub metadata_img: [Pixmap; 2],
    pub meta_palette: HashMap<DiskStructureGenericElement, Color>,
    pub have_render: bool,
    pub canvas: Option<PixelCanvas>,
}

impl Default for VisualizationState {
    fn default() -> Self {
        Self {
            meta_pixmap_pool: Vec::new(),
            metadata_img: [Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap(), Pixmap::new(VIZ_RESOLUTION, VIZ_RESOLUTION).unwrap()],
            meta_palette: HashMap::new(),
            have_render: false,
            canvas: None,
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

        let mut canvas = PixelCanvas::new((resolution, resolution), ctx.clone());
        canvas.set_bpp(PixelCanvasDepth::Rgba);

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
            canvas: Some(canvas),
            ..VisualizationState::default()
        }
    }

    pub(crate) fn render_visualization(&mut self, disk_image: Option<&mut DiskImage>, side: usize) -> Result<(), Error> {

        if let Some(disk) = disk_image {
            let head = side as u8;
            let quadrant = 0;
            let angle = 0.0;
            let min_radius_fraction = 0.333;
            let render_track_gap = 0.10;
            let direction = fluxfox::visualization::RotationDirection::CounterClockwise;

            let track_ct = disk.get_track_ct(side.into());
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
            };

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
                self.meta_pixmap_pool[quadrant].lock().unwrap().as_mut().fill(Color::TRANSPARENT);
            }

            if let Some(canvas) = &mut self.canvas {
                if canvas.has_texture() {
                    log::debug!("Updating canvas...");
                    log::debug!("pixmap data slice: {:0X?}", &self.metadata_img[side].data()[0..16]);
                    canvas.update_data(self.metadata_img[side].data());
                    self.have_render = true;
                }
                else {
                    log::debug!("Canvas not initialized, deferring update...");
                    //self.draw_deferred = true;
                }
            }

        }
        Ok(())
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        if self.have_render {
            if let Some(canvas) = &mut self.canvas {
                canvas.draw(ui);
            }
        }
    }
}

impl App {

}

