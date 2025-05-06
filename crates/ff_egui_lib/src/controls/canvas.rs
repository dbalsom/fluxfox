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

//! The PixelCanvas is a bit of an experiment in creating a pixel-based canvas
//! that supports multiple bit depths and palettes. It's a bit of a work in
//! progress, and perhaps it should be split out into different widgets.
//!
//! Not everything requires multi-bit depth support, and the things that do
//! almost certainly don't need rotation support...

#![allow(dead_code)]

use egui::{
    emath::RectTransform,
    epaint::Vertex,
    Color32,
    ColorImage,
    Context,
    ImageData,
    Mesh,
    Pos2,
    Rect,
    Response,
    ScrollArea,
    Sense,
    Shape,
    TextureHandle,
    TextureOptions,
    Vec2,
};
use std::{io::Cursor, sync::Arc};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[repr(u8)]
pub enum PixelCanvasZoom {
    One,
    Two,
    Four,
    Eight,
    Sixteen,
}

pub type HoverCallback = fn(&Response, f32, f32);

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

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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
    id: String,
    data_buf: Vec<u8>,
    backing_buf: Vec<Color32>,
    view_dimensions: (u32, u32),
    virtual_rect: Rect,
    virtual_transform: RectTransform,
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
    rotation: f32,
}

impl Default for PixelCanvas {
    fn default() -> Self {
        Self {
            id: String::from("pixel_canvas"),
            data_buf: Vec::new(),
            backing_buf: Vec::new(),
            view_dimensions: (DEFAULT_WIDTH, DEFAULT_HEIGHT),
            virtual_rect: Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(DEFAULT_WIDTH as f32, DEFAULT_HEIGHT as f32),
            ),
            virtual_transform: RectTransform::identity(Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(DEFAULT_WIDTH as f32, DEFAULT_HEIGHT as f32),
            )),
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
            rotation: 0.0,
        }
    }
}

#[allow(dead_code)]
impl PixelCanvas {
    pub fn new(dims: (u32, u32), ctx: Context, id: &str) -> Self {
        let mut pc = PixelCanvas::default();
        pc.id = id.to_string();
        pc.view_dimensions = dims;
        pc.virtual_rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(dims.0 as f32, dims.1 as f32));
        pc.data_buf = vec![0; PixelCanvas::calc_slice_size(dims, pc.bpp)];
        pc.backing_buf = vec![Color32::WHITE; (dims.0 * dims.1) as usize];
        pc.ctx = ctx.clone();
        pc.texture = Some(PixelCanvas::create_texture(&mut pc));
        pc
    }

    pub fn clear(&mut self) {
        self.backing_buf = vec![Color32::BLACK; (self.view_dimensions.0 * self.view_dimensions.1) as usize];
        self.update_texture();
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

    pub fn width(&self) -> f32 {
        self.view_dimensions.0 as f32 * self.zoom
    }

    pub fn rotation_mut(&mut self) -> &mut f32 {
        &mut self.rotation
    }

    pub fn set_rotation(&mut self, angle: f32) {
        self.rotation = angle;
    }

    pub fn set_virtual_rect(&mut self, rect: Rect) {
        self.virtual_rect = rect;

        self.virtual_transform = RectTransform::from_to(
            Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(self.view_dimensions.0 as f32, self.view_dimensions.1 as f32),
            ),
            rect,
        );
    }

    pub fn virtual_rect(&self) -> &Rect {
        &self.virtual_rect
    }

    pub fn virtual_transform(&self) -> &RectTransform {
        &self.virtual_transform
    }

    pub fn show(&mut self, ui: &mut egui::Ui, on_hover: Option<impl FnOnce(&Response, f32, f32)>) -> Option<Response> {
        let mut inner_response = None;
        if let Some(texture) = &self.texture {
            ui.vertical(|ui| {
                // Draw background rect
                //ui.painter().rect_filled(ui.max_rect(), egui::Rounding::default(), egui::Color32::BLACK);
                let scroll_area = ScrollArea::vertical().id_salt(&self.id).auto_shrink([false; 2]);
                let img_w = self.view_dimensions.0 as f32 * self.zoom;
                let img_h = self.view_dimensions.1 as f32 * self.zoom;
                ui.shrink_width_to_current();
                ui.set_width(img_w);
                //ui.set_height(ui.cursor().top() + img_h);
                ui.set_height(img_h);

                scroll_area.show_viewport(ui, |ui, viewport| {
                    let start_x = viewport.min.x + ui.min_rect().left();
                    let start_y = viewport.min.y + ui.min_rect().top();

                    let (rect, response) = ui.allocate_exact_size(Vec2::new(img_w, img_h), Sense::click_and_drag());
                    //log::debug!("Viewport is: {:?} StartX: {} StartY: {}", viewport, start_x, start_y);

                    if ui.is_rect_visible(rect) {
                        ui.painter().image(
                            texture.id(),
                            Rect::from_min_max(
                                egui::pos2(start_x, start_y),
                                egui::pos2(start_x + img_w, start_y + img_h),
                            ),
                            self.default_uv,
                            Color32::WHITE,
                        );
                    }

                    if response.secondary_clicked() {
                        log::debug!("Secondary click detected!");
                    }

                    if let Some(mouse_pos) = response.hover_pos() {
                        let x = mouse_pos.x - start_x;
                        let y = mouse_pos.y - start_y;
                        if x >= 0.0 && x < img_w && y >= 0.0 && y < img_h {
                            if let Some(on_hover) = on_hover {
                                on_hover(&response, x, y);
                            }
                        }
                    }
                    inner_response = Some(response);
                });
                inner_response
            })
            .inner
        }
        else {
            log::debug!("No texture to draw.");
            None
        }
    }

    pub fn show_as_mesh2(
        &mut self,
        ui: &mut egui::Ui,
        on_hover: Option<impl FnOnce(&egui::Response, Pos2, Pos2)>,
    ) -> Option<egui::Response> {
        let mut inner_response = None;

        if let Some(texture) = &self.texture {
            ui.vertical(|ui| {
                // The final UI region to fill, e.g. 512×512
                let view_w = self.view_dimensions.0 as f32;
                let view_h = self.view_dimensions.1 as f32;
                ui.set_width(view_w);
                ui.set_height(view_h);

                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(view_w, view_h), egui::Sense::click_and_drag());

                let top_left = Pos2::new(ui.min_rect().left(), ui.min_rect().top());

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter().with_clip_rect(rect);

                    // Full image corners in "image space"
                    let image_corners = [
                        egui::pos2(0.0, 0.0),
                        egui::pos2(view_w, 0.0),
                        egui::pos2(view_w, view_h),
                        egui::pos2(0.0, view_h),
                    ];

                    let pivot = egui::pos2(view_w / 2.0, view_h / 2.0);

                    let sub_rect = self.virtual_rect;
                    let sub_w = sub_rect.width();
                    let sub_h = sub_rect.height();
                    let scale_x = view_w / sub_w;
                    let scale_y = view_h / sub_h;

                    let sub_min_local_x = sub_rect.min.x - pivot.x; // e.g. 256-256=0 if quadrant
                    let sub_min_local_y = sub_rect.min.y - pivot.y; // e.g. 0-256=-256 if quadrant

                    // Scale that local corner (without rotation, i.e. rotation=0 path)
                    let scaled_sub_min_x = sub_min_local_x * scale_x;
                    let scaled_sub_min_y = sub_min_local_y * scale_y;

                    // So offset is:
                    let offset_x = rect.min.x - scaled_sub_min_x;
                    let offset_y = rect.min.y - scaled_sub_min_y;

                    let cos_a = self.rotation.cos();
                    let sin_a = self.rotation.sin();

                    // Create mesh
                    let mut mesh = Mesh::default();
                    for corner in image_corners {
                        // local coords: corner minus pivot
                        let lx = corner.x - pivot.x;
                        let ly = corner.y - pivot.y;

                        // rotate
                        let rx = lx * cos_a - ly * sin_a;
                        let ry = lx * sin_a + ly * cos_a;

                        // scale
                        let sx = rx * scale_x;
                        let sy = ry * scale_y;

                        // offset
                        let final_x = offset_x + sx;
                        let final_y = offset_y + sy;

                        mesh.vertices.push(Vertex {
                            pos:   egui::pos2(final_x, final_y),
                            uv:    egui::pos2(corner.x / 512.0, corner.y / 512.0),
                            color: Color32::WHITE,
                        });
                    }
                    mesh.indices.extend(&[0, 1, 2, 2, 3, 0]);
                    mesh.texture_id = texture.id();

                    painter.add(Shape::Mesh(mesh));
                }

                if let Some(mouse_pos) = response.hover_pos() {
                    let pos = mouse_pos - top_left;

                    if pos.x >= 0.0 && pos.x < view_w && pos.y >= 0.0 && pos.y < view_h {
                        if let Some(on_hover) = on_hover {
                            on_hover(
                                &response,
                                pos.to_pos2(),
                                self.virtual_transform.transform_pos(pos.to_pos2()),
                            );
                        }
                    }
                }

                inner_response = Some(response);
            });

            inner_response
        }
        else {
            None
        }
    }

    pub fn show_as_mesh(
        &mut self,
        ui: &mut egui::Ui,
        on_hover: Option<impl FnOnce(&Response, f32, f32)>,
    ) -> Option<Response> {
        let mut inner_response = None;
        if let Some(texture) = &self.texture {
            ui.vertical(|ui| {
                let scroll_area = ScrollArea::vertical().id_salt(&self.id).auto_shrink([false; 2]);

                let img_w = self.view_dimensions.0 as f32 * self.zoom;
                let img_h = self.view_dimensions.1 as f32 * self.zoom;

                ui.shrink_width_to_current();
                ui.set_width(img_w);
                ui.set_height(img_h);

                // Debug resolution and view dimensions
                log::debug!(
                    "Virtual: {} x {}, View Dimensions: {} x {}",
                    self.virtual_rect.width(),
                    self.virtual_rect.height(),
                    self.view_dimensions.0,
                    self.view_dimensions.1
                );

                scroll_area.show_viewport(ui, |ui, _viewport| {
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(img_w, img_h), Sense::click_and_drag());

                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();

                        // Map the full texture to the mesh (full UV range)
                        let uv = [
                            Pos2::new(0.0, 0.0),
                            Pos2::new(1.0, 0.0),
                            Pos2::new(1.0, 1.0),
                            Pos2::new(0.0, 1.0),
                        ];

                        // Define the transformation from virtual_rect (zoomed-in space) to UI space
                        // Scale and translate based on the virtual rect
                        let scale_x = img_w / self.virtual_rect.width();
                        let scale_y = img_h / self.virtual_rect.height();

                        // Calculate the offset in logical space
                        let offset_x =
                            (img_w / 2.0) - (self.virtual_rect.min.x + self.virtual_rect.width() / 2.0) * scale_x;
                        let offset_y =
                            (img_h / 2.0) - (self.virtual_rect.min.y + self.virtual_rect.height() / 2.0) * scale_y;

                        log::debug!(
                            "Scale X: {}, Scale Y: {}, Offset X: {}, Offset Y: {}",
                            scale_x,
                            scale_y,
                            offset_x,
                            offset_y
                        );

                        // Calculate the center of the image for rotation
                        let mesh_center = rect.center();
                        // let center =
                        //     Pos2::new(rect.min.x + img_w / 2.0 + offset_x, rect.min.y + img_h / 2.0 + offset_y);

                        // Define corners of the image relative to its center
                        let half_size = Vec2::new(img_w / 2.0, img_h / 2.0);
                        let corners = [
                            Vec2::new(-half_size.x, -half_size.y),
                            Vec2::new(half_size.x, -half_size.y),
                            Vec2::new(half_size.x, half_size.y),
                            Vec2::new(-half_size.x, half_size.y),
                        ];

                        // let corners = [
                        //     Vec2::new(0.0, 0.0),
                        //     Vec2::new(img_w , 0.0),
                        //     Vec2::new(img_w, img_h),
                        //     Vec2::new(0.0, img_h),
                        // ];

                        // Apply scaling, translation, and rotation to the corners
                        let transformed_corners: Vec<Pos2> = corners
                            .iter()
                            .map(|corner| {
                                // Scale the corner
                                let scaled = Vec2::new(corner.x * scale_x, corner.y * scale_y);

                                // Rotate the corner around the mesh center
                                let cos_angle = self.rotation.cos();
                                let sin_angle = self.rotation.sin();
                                let rotated = Vec2::new(
                                    scaled.x * cos_angle - scaled.y * sin_angle,
                                    scaled.x * sin_angle + scaled.y * cos_angle,
                                );

                                Pos2::new(
                                    mesh_center.x + rotated.x + offset_x,
                                    mesh_center.y + rotated.y + offset_y,
                                )
                            })
                            .collect();

                        // Rotate corners
                        // let transformed_corners: Vec<Pos2> = corners
                        //     .iter()
                        //     .map(|corner| {
                        //         let cos_angle = self.rotation.cos();
                        //         let sin_angle = self.rotation.sin();
                        //         let rotated = Vec2::new(
                        //             corner.x * cos_angle - corner.y * sin_angle,
                        //             corner.x * sin_angle + corner.y * cos_angle,
                        //         );
                        //         Pos2::new(center.x + rotated.x, center.y + rotated.y)
                        //     })
                        //     .collect();

                        // Create a mesh for the rotated image
                        let mut mesh = Mesh::default();
                        let vertices = transformed_corners
                            .iter()
                            .zip(&uv)
                            .map(|(&pos, &uv)| egui::epaint::Vertex {
                                pos,
                                uv,
                                color: Color32::WHITE,
                            })
                            .collect::<Vec<_>>();
                        mesh.vertices.extend(vertices);

                        // Indices for the two triangles forming the rectangle
                        mesh.indices.extend(&[0, 1, 2, 2, 3, 0]);

                        // Assign the texture ID
                        mesh.texture_id = texture.id();

                        // Add the mesh to the painter
                        painter.add(Shape::Mesh(mesh));
                    }

                    if response.secondary_clicked() {
                        log::debug!("Secondary click detected!");
                    }

                    if let Some(mouse_pos) = response.hover_pos() {
                        let x = mouse_pos.x - rect.min.x;
                        let y = mouse_pos.y - rect.min.y;
                        if x >= 0.0 && x < img_w && y >= 0.0 && y < img_h {
                            if let Some(on_hover) = on_hover {
                                on_hover(&response, x, y);
                            }
                        }
                    }
                    inner_response = Some(response);
                });

                inner_response
            })
            .inner
        }
        else {
            log::debug!("No texture to draw.");
            None
        }
    }

    pub fn get_required_data_size(&self) -> usize {
        PixelCanvas::calc_slice_size(self.view_dimensions, self.bpp)
    }

    pub fn update_data(&mut self, data: &[u8]) {
        let slice_size = PixelCanvas::calc_slice_size(self.view_dimensions, self.bpp);
        log::trace!(
            "PixelCanvas::update_data(): Updating data with {} bytes for slice of {}",
            data.len(),
            slice_size
        );
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
            //log::trace!("PixelCanvas::update_texture(): Updating texture with new data...");
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

    // pub fn save_buffer(&mut self, path: &Path) -> Result<(), Error> {
    //     let byte_slice: &[u8] = bytemuck::cast_slice(&self.backing_buf);
    //     image::save_buffer(
    //         path,
    //         byte_slice,
    //         self.view_dimensions.0,
    //         self.view_dimensions.1,
    //         image::ColorType::Rgba8,
    //     )?;
    //     Ok(())
    // }

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

    pub fn to_png(&self) -> Vec<u8> {
        use ::image::{ImageBuffer, ImageFormat, Rgba};

        let mut img = ImageBuffer::new(self.view_dimensions.0, self.view_dimensions.1);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let idx = (x + y * self.view_dimensions.0) as usize;
            *pixel = Rgba([
                self.backing_buf[idx].r(),
                self.backing_buf[idx].g(),
                self.backing_buf[idx].b(),
                self.backing_buf[idx].a(),
            ]);
        }
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }
}
