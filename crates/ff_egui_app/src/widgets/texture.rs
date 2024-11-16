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
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use egui::{
    Color32,
    ColorImage,
    Context,
    ImageData,
    Rect,
    ScrollArea,
    TextureHandle,
    TextureOptions,
};

use anyhow::Error;

#[repr(u8)]
pub enum PixelCanvasZoom {
    One,
    Two,
    Four,
    Eight,
    Sixteen,
}

pub const GRAYSCALE_RAMP: [Color32; 256] = {
    let mut palette = [Color32::BLACK; 256];
    let mut i = 0;
    while i < 256 {
        let shade = i as u8;
        palette[i] = Color32::from_rgb(shade, shade, shade);
        i += 1;
    }
    palette
};

pub const ZOOM_LUT: [f32; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];

pub const DEFAULT_WIDTH: u32 = 128;
pub const DEFAULT_HEIGHT: u32 = 128;

pub const PALETTE_1BPP: [Color32; 2] = [Color32::from_rgb(0, 0, 0), Color32::from_rgb(255, 255, 255)];
pub const PALETTE_2BPP: [Color32; 4] = [
    Color32::from_rgb(0x00u8, 0x00u8, 0x00u8),
    Color32::from_rgb(0x55u8, 0xFFu8, 0xFFu8),
    Color32::from_rgb(0xFFu8, 0x55u8, 0xFFu8),
    Color32::from_rgb(0xFFu8, 0xFFu8, 0xFFu8),
];
pub const PALETTE_4BPP: [Color32; 16] = [
    Color32::from_rgb(0x00u8, 0x00u8, 0x00u8),
    Color32::from_rgb(0x00u8, 0x00u8, 0xAAu8),
    Color32::from_rgb(0x00u8, 0xAAu8, 0x00u8),
    Color32::from_rgb(0x00u8, 0xAAu8, 0xAAu8),
    Color32::from_rgb(0xAAu8, 0x00u8, 0x00u8),
    Color32::from_rgb(0xAAu8, 0x00u8, 0xAAu8),
    Color32::from_rgb(0xAAu8, 0x55u8, 0x00u8),
    Color32::from_rgb(0xAAu8, 0xAAu8, 0xAAu8),
    Color32::from_rgb(0x55u8, 0x55u8, 0x55u8),
    Color32::from_rgb(0x55u8, 0x55u8, 0xFFu8),
    Color32::from_rgb(0x55u8, 0xFFu8, 0x55u8),
    Color32::from_rgb(0x55u8, 0xFFu8, 0xFFu8),
    Color32::from_rgb(0xFFu8, 0x55u8, 0x55u8),
    Color32::from_rgb(0xFFu8, 0x55u8, 0xFFu8),
    Color32::from_rgb(0xFFu8, 0xFFu8, 0x55u8),
    Color32::from_rgb(0xFFu8, 0xFFu8, 0xFFu8),
];

#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub enum PixelCanvasDepth {
    #[default]
    OneBpp,
    TwoBpp,
    FourBpp,
    EightBpp,
    Rgb,
    Rgba,
}

impl PixelCanvasDepth {
    pub fn bits(&self) -> usize {
        match self {
            PixelCanvasDepth::OneBpp => 1,
            PixelCanvasDepth::TwoBpp => 2,
            PixelCanvasDepth::FourBpp => 4,
            PixelCanvasDepth::EightBpp => 8,
            PixelCanvasDepth::Rgb => 24,
            PixelCanvasDepth::Rgba => 32,
        }
    }
}

pub struct PixelCanvasPalette {
    name:   String,
    depth:  PixelCanvasDepth,
    colors: Vec<Color32>,
}

pub struct PixelCanvas {
    data_buf: Vec<u8>,
    backing_buf: Vec<Color32>,
    view_dimensions: (u32, u32),
    zoom: f32,
    bpp: PixelCanvasDepth,
    device_palette: PixelCanvasPalette,
    use_device_palette: bool,
    current_palette: PixelCanvasPalette,

    palettes: Vec<Vec<PixelCanvasPalette>>,
    texture: Option<TextureHandle>,
    image_data: ImageData,
    texture_opts: TextureOptions,
    default_uv: Rect,
    ctx: Context,
    data_unpacked: bool,
}

impl Default for PixelCanvas {
    fn default() -> Self {
        Self {
            data_buf: Vec::new(),
            backing_buf: Vec::new(),
            view_dimensions: (DEFAULT_WIDTH, DEFAULT_HEIGHT),
            zoom: 1.0,
            bpp: PixelCanvasDepth::OneBpp,
            device_palette: PixelCanvasPalette {
                name:   "Default".to_string(),
                depth:  PixelCanvasDepth::OneBpp,
                colors: vec![Color32::from_rgb(0, 0, 0), Color32::from_rgb(255, 255, 255)],
            },
            use_device_palette: true,
            current_palette: PixelCanvasPalette {
                name:   "Default".to_string(),
                depth:  PixelCanvasDepth::OneBpp,
                colors: vec![Color32::from_rgb(0, 0, 0), Color32::from_rgb(255, 255, 255)],
            },
            palettes: Vec::new(),
            texture: None,
            image_data: PixelCanvas::create_default_imagedata((DEFAULT_WIDTH, DEFAULT_HEIGHT)),
            texture_opts: TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
                mipmap_mode: Some(egui::TextureFilter::Nearest),
            },
            default_uv: Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            ctx: Context::default(),
            data_unpacked: false,
        }
    }
}

#[allow(dead_code)]
impl PixelCanvas {
    pub fn new(dims: (u32, u32), ctx: Context) -> Self {
        let mut pc = PixelCanvas::default();
        pc.view_dimensions = dims;
        pc.data_buf = vec![0; PixelCanvas::calc_slice_size(dims, pc.bpp)];
        pc.backing_buf = vec![Color32::WHITE; (dims.0 * dims.1) as usize];
        pc.ctx = ctx.clone();
        pc.texture = Some(PixelCanvas::create_texture(&mut pc));
        pc
    }

    pub fn create_default_colorimage(dims: (u32, u32)) -> ColorImage {
        ColorImage::new([dims.0 as usize, dims.1 as usize], Color32::BLACK)
    }

    pub fn create_default_imagedata(dims: (u32, u32)) -> ImageData {
        ImageData::Color(Arc::new(PixelCanvas::create_default_colorimage(dims)))
    }

    pub fn update_imagedata(&mut self) {
        if !self.data_unpacked {
            log::warn!("PixelCanvas::update_imagedata(): Data was not unpacked before ColorImage update!");
        }
        let color_image = ColorImage {
            size:   [self.view_dimensions.0 as usize, self.view_dimensions.1 as usize],
            pixels: self.backing_buf.clone(),
        };
        self.image_data = ImageData::Color(Arc::new(color_image));
    }

    pub fn use_device_palette(&mut self, state: bool) {
        self.use_device_palette = state;
    }

    pub fn calc_slice_size(dims: (u32, u32), bpp: PixelCanvasDepth) -> usize {
        let size = match bpp {
            PixelCanvasDepth::OneBpp => (dims.0 * dims.1) as usize / 8 + 1,
            PixelCanvasDepth::TwoBpp => (dims.0 * dims.1) as usize / 4 + 1,
            PixelCanvasDepth::FourBpp => (dims.0 * dims.1) as usize / 1 + 1,
            PixelCanvasDepth::EightBpp => (dims.0 * dims.1) as usize,
            PixelCanvasDepth::Rgb => (dims.0 * dims.1) as usize * 3,
            PixelCanvasDepth::Rgba => (dims.0 * dims.1) as usize * 4,
        };

        size
    }

    pub fn create_texture(&mut self) -> TextureHandle {
        self.image_data = PixelCanvas::create_default_imagedata(self.view_dimensions);
        self.ctx
            .load_texture("pixel_canvas".to_string(), self.image_data.clone(), self.texture_opts)
    }

    pub fn get_width(&self) -> f32 {
        self.view_dimensions.0 as f32 * self.zoom
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        if let Some(texture) = &self.texture {
            ui.vertical(|ui| {
                // Draw background rect
                //ui.painter().rect_filled(ui.max_rect(), egui::Rounding::default(), egui::Color32::BLACK);
                let scroll_area = ScrollArea::vertical().auto_shrink([false; 2]);
                let img_w = self.view_dimensions.0 as f32 * self.zoom;
                let img_h = self.view_dimensions.1 as f32 * self.zoom;
                ui.shrink_width_to_current();
                ui.set_width(img_w);
                //ui.set_height(ui.cursor().top() + img_h);
                ui.set_height(img_h);

                scroll_area.show_viewport(ui, |ui, viewport| {
                    let start_x = viewport.min.x + ui.min_rect().left();
                    let start_y = viewport.min.y + ui.min_rect().top();

                    //log::debug!("Viewport is: {:?} StartX: {} StartY: {}", viewport, start_x, start_y);

                    ui.painter().image(
                        texture.id(),
                        Rect::from_min_max(
                            egui::pos2(start_x, start_y),
                            egui::pos2(start_x + img_w, start_y + img_h),
                        ),
                        self.default_uv,
                        Color32::WHITE,
                    );
                });
            });
        /*            log::debug!(
            "Drawing PixelCanvas texture ({}x{}), id: {:?}",
            texture.size()[0],
            texture.size()[1],
            texture.id()
        );*/
        }
        else {
            log::debug!("No texture to draw.");
        }
    }

    pub fn get_required_data_size(&self) -> usize {
        PixelCanvas::calc_slice_size(self.view_dimensions, self.bpp)
    }

    pub fn update_data(&mut self, data: &[u8]) {
        let slice_size = PixelCanvas::calc_slice_size(self.view_dimensions, self.bpp);
        log::debug!("PixelCanvas::update_data(): Updating data with {} bytes for slice of {}", data.len(), slice_size);
        let shortfall = slice_size.saturating_sub(data.len());
        self.data_buf.clear();
        self.data_buf
            .extend_from_slice(&data[0..std::cmp::min(slice_size, data.len())]);
        if shortfall > 0 {
            self.data_buf.extend_from_slice(&vec![0; shortfall]);
        }

        assert_eq!(self.data_buf.len(), slice_size);

        // if self.texture.is_none() {
        //     log::debug!("PixelCanvas::update_data(): Creating initial texture...");
        //     self.texture = Some(self.create_texture());
        // }
        self.unpack_pixels();
        self.update_texture();
    }

    pub fn update_device_palette(&mut self, palette: Vec<Color32>) {
        let depth = match palette.len() {
            256 => Some(PixelCanvasDepth::EightBpp),
            16 => Some(PixelCanvasDepth::FourBpp),
            4 => Some(PixelCanvasDepth::TwoBpp),
            2 => Some(PixelCanvasDepth::OneBpp),
            _ => None,
        };

        if let Some(depth) = depth {
            self.device_palette.colors = palette;
            self.device_palette.depth = depth;
            self.unpack_pixels();
            self.update_texture();
        }
    }

    pub fn has_texture(&self) -> bool {
        self.texture.is_some()
    }

    pub fn update_texture(&mut self) {
        self.update_imagedata();
        if let Some(texture) = &mut self.texture {
            log::debug!("PixelCanvas::update_texture(): Updating texture with new data...");
            texture.set(self.image_data.clone(), self.texture_opts);
        }
    }

    pub fn set_bpp(&mut self, bpp: PixelCanvasDepth) {
        self.bpp = bpp;
        self.data_unpacked = false;
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    pub fn resize(&mut self, dims: (u32, u32)) {
        self.view_dimensions = dims;
        self.data_buf = vec![0; PixelCanvas::calc_slice_size(dims, self.bpp)];
        self.backing_buf = vec![Color32::BLACK; (dims.0 * dims.1) as usize];

        self.texture = Some(self.create_texture());
        self.data_unpacked = false;
    }

    pub fn save_buffer(&mut self, path: &Path) -> Result<(), Error> {
        let byte_slice: &[u8] = bytemuck::cast_slice(&self.backing_buf);
        image::save_buffer(
            path,
            byte_slice,
            self.view_dimensions.0,
            self.view_dimensions.1,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }

    fn unpack_pixels(&mut self) {
        //let dims = self.view_dimensions.0 * self.view_dimensions.1;
        //let max_index = std::cmp::min(dims as usize, self.data_buf.len());
        match self.bpp {
            PixelCanvasDepth::OneBpp => {
                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let byte = self.data_buf[(i / 8) as usize];
                    let bit = 1 << (i % 8);
                    self.backing_buf[i as usize] = if byte & bit != 0 {
                        Color32::WHITE
                    }
                    else {
                        Color32::BLACK
                    };
                }
            }
            PixelCanvasDepth::TwoBpp => {
                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let byte = self.data_buf[(i / 4) as usize];
                    let shift = (i % 4) * 2;
                    let color = (byte >> (6 - shift)) & 0x03;
                    self.backing_buf[i as usize] = PALETTE_2BPP[color as usize];
                }
            }
            PixelCanvasDepth::FourBpp => {
                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let byte = self.data_buf[(i / 2) as usize];
                    let shift = (i % 2) * 4;
                    let color = (byte >> shift) & 0x0F;
                    self.backing_buf[i as usize] = PALETTE_4BPP[color as usize];
                }
            }
            PixelCanvasDepth::EightBpp => {
                let pal = if self.use_device_palette && self.device_palette.colors.len() == 256 {
                    &self.device_palette.colors[0..]
                }
                else {
                    &GRAYSCALE_RAMP
                };

                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let byte = self.data_buf[i as usize];
                    self.backing_buf[i as usize] = pal[byte as usize];
                }
            }
            PixelCanvasDepth::Rgb => {
                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let idx = (i * 3) as usize;
                    let r = self.data_buf[idx];
                    let g = self.data_buf[idx + 1];
                    let b = self.data_buf[idx + 2];
                    self.backing_buf[i as usize] = Color32::from_rgb(r, g, b);
                }
            }
            PixelCanvasDepth::Rgba => {
                log::debug!("PixelCanvas::unpack_pixels(): Unpacking RGBA data...");
                for i in 0..self.view_dimensions.0 * self.view_dimensions.1 {
                    let idx = (i * 4) as usize;
                    let r = self.data_buf[idx];
                    let g = self.data_buf[idx + 1];
                    let b = self.data_buf[idx + 2];
                    let a = self.data_buf[idx + 3];
                    self.backing_buf[i as usize] = Color32::from_rgba_premultiplied(r, g, b, a);
                }
            }
        }
        self.data_unpacked = true;
    }
}
