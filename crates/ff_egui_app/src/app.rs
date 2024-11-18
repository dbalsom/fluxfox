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

use crate::viz::VisualizationState;
use fluxfox::{DiskImage, DiskImageError, LoadingStatus};
use fluxfox_egui::widgets::disk_info::DiskInfoWidget;
use std::{
    default::Default,
    sync::{mpsc, Arc},
};

#[cfg(not(target_arch = "wasm32"))]
use crate::native::{util, worker};
#[cfg(not(target_arch = "wasm32"))]
pub const APP_NAME: &str = "fluxfox-egui";

#[cfg(target_arch = "wasm32")]
use crate::wasm::{util, worker};
#[cfg(target_arch = "wasm32")]
pub const APP_NAME: &str = "fluxfox-web";

use fluxfox_egui::widgets::track_list::TrackListWidget;

#[derive(Default)]
pub enum ThreadLoadStatus {
    #[default]
    Inactive,
    Loading(f64),
    Success(DiskImage),
    Error(DiskImageError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunMode {
    Reactive,
    Continuous,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
#[derive(Default)]
pub struct PersistentState {
    label: String,
}

#[derive(Default)]
pub struct AppWidgets {
    disk_info:  DiskInfoWidget,
    track_list: TrackListWidget,
}

impl AppWidgets {
    pub fn update(&mut self, disk: &DiskImage, name: Option<String>) {
        self.disk_info.update(disk, name);
        self.track_list.update(disk);
    }
}

pub struct App {
    p_state: PersistentState,
    run_mode: RunMode,
    ctx_init: bool,
    pub(crate) dropped_files: Vec<egui::DroppedFile>,
    load_status: ThreadLoadStatus,
    load_sender: Option<mpsc::SyncSender<ThreadLoadStatus>>,
    load_receiver: Option<mpsc::Receiver<ThreadLoadStatus>>,
    disk_image_name: Option<String>,
    pub(crate) disk_image: Option<DiskImage>,

    pub(crate) viz_state: VisualizationState,
    supported_extensions: Vec<String>,

    widgets: AppWidgets,
    viz_window_open: bool,

    error_msg: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let (load_sender, load_receiver) = mpsc::sync_channel(128);
        Self {
            // Example stuff:
            p_state: PersistentState {
                label: "Hello World!".to_owned(),
            },
            run_mode: RunMode::Reactive,
            ctx_init: false,
            dropped_files: Vec::new(),

            load_status:   ThreadLoadStatus::Inactive,
            load_sender:   Some(load_sender),
            load_receiver: Some(load_receiver),

            disk_image_name: None,
            disk_image: None,

            viz_state: VisualizationState::default(),
            supported_extensions: Vec::new(),

            widgets: AppWidgets::default(),
            viz_window_open: false,

            error_msg: None,
        }
    }
}

impl App {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        let mut app_state = App::default();

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            app_state.p_state = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        app_state.viz_state = VisualizationState::new(cc.egui_ctx.clone(), 512);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        // Set dark mode. This doesn't seem to work for some reason.
        // So we'll use a flag in state and do it on the first update().
        //cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // Get and store the list of supported extensions
        fluxfox::supported_extensions()
            .iter()
            .filter(|ext| **ext != "raw")
            .for_each(|ext| {
                app_state.supported_extensions.push(ext.to_string().to_uppercase());
            });

        app_state.supported_extensions.sort();

        app_state
    }
}

impl eframe::App for App {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        if !self.ctx_init {
            self.ctx_init(ctx);
        }

        if matches!(self.run_mode, RunMode::Continuous) {
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }
                else {
                    ui.menu_button("Image", |ui| {
                        if ui.button("Upload...").clicked() {
                            println!("TODO: upload image");
                        }
                    });
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanels and SidePanels
            ui.add(util::get_logo_image().fit_to_original_size(1.0));

            ui.heading(egui::RichText::new(format!("Welcome to {}!", APP_NAME)).color(ui.visuals().strong_text_color()));

            ui.vertical(|ui| {
                ui.label("Drag disk image files to this window to load. Kryoflux sets should be in single-disk ZIP archives.");
                ui.label(format!(
                    "Image types supported: {}",
                    self.supported_extensions.join(", ")
                ));
            });

            ui.separator();

            self.show_error(ui);

            // Show dropped files (if any):
            self.handle_dropped_files(ctx, None);
            self.handle_loading_progress(ui);
            self.handle_image_info(ui);
            self.handle_track_info(ui);
            self.handle_load_messages(ctx);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });

        // Show windows
        self.viz_window_open = self.viz_state.is_open();

        if self.viz_window_open {
            egui::Window::new("Visualization").fade_in(true).show(ctx, |ui| {
                self.viz_state.show(ui);
            });
        }
    }

    /// Called by the framework to save persistent state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.p_state);
    }
}

impl App {
    /// Initialize the egui context, for visuals, etc.
    /// Tried doing this in new() but it didn't take effect.
    pub fn ctx_init(&mut self, ctx: &egui::Context) {
        ctx.set_visuals(egui::Visuals::dark());
        self.ctx_init = true;
    }

    pub fn reset(&mut self) {
        self.disk_image = None;
        self.disk_image_name = None;
        self.error_msg = None;
        self.load_status = ThreadLoadStatus::Inactive;
        self.run_mode = RunMode::Reactive;
        self.viz_window_open = false;
        self.viz_state = VisualizationState::default();
    }

    // Optional: clear dropped files when done
    fn clear_dropped_files(&mut self) {
        self.dropped_files.clear();
    }

    fn show_error(&mut self, ui: &mut egui::Ui) {
        if let Some(msg) = &self.error_msg {
            egui::Frame::none()
                .fill(egui::Color32::DARK_RED)
                .rounding(8.0)
                .inner_margin(8.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🗙").color(egui::Color32::WHITE).size(32.0));
                        ui.add(egui::Label::new(
                            egui::RichText::new(msg).color(egui::Color32::WHITE).size(24.0),
                        ));
                    });
                });
        }
    }

    fn handle_image_info(&mut self, ui: &mut egui::Ui) {
        if self.disk_image.is_some() {
            ui.group(|ui| {
                self.widgets.disk_info.show(ui);
            });
        }
    }

    fn handle_track_info(&mut self, ui: &mut egui::Ui) {
        if self.disk_image.is_some() {
            ui.group(|ui| {
                self.widgets.track_list.show(ui);
            });
        }
    }

    fn handle_load_messages(&mut self, ctx: &egui::Context) {
        // Read messages from the load thread
        if let Some(receiver) = &self.load_receiver {
            // We should keep draining the receiver until it's empty, otherwise messages arriving
            // faster than once per update() will clog the channel.
            let mut keep_polling = true;
            while keep_polling {
                match receiver.try_recv() {
                    Ok(status) => {
                        match status {
                            ThreadLoadStatus::Loading(progress) => {
                                log::debug!("Loading progress: {:.1}%", progress * 100.0);
                                self.load_status = ThreadLoadStatus::Loading(progress);
                                self.widgets = AppWidgets::default();
                                self.viz_window_open = false;
                                ctx.request_repaint();
                            }
                            ThreadLoadStatus::Success(disk) => {
                                log::info!("Disk image loaded successfully!");

                                let heads = disk.geometry().h();
                                self.disk_image = Some(disk);
                                self.load_status = ThreadLoadStatus::Inactive;
                                ctx.request_repaint();
                                // Return to reactive mode
                                self.run_mode = RunMode::Reactive;

                                match self.viz_state.render_visualization(self.disk_image.as_mut(), 0) {
                                    Ok(_) => {
                                        log::info!("Visualization rendered successfully!");
                                    }
                                    Err(e) => {
                                        log::error!("Error rendering visualization: {:?}", e);
                                    }
                                }

                                if heads > 1 {
                                    match self.viz_state.render_visualization(self.disk_image.as_mut(), 1) {
                                        Ok(_) => {
                                            log::info!("Visualization rendered successfully!");
                                        }
                                        Err(e) => {
                                            log::error!("Error rendering visualization: {:?}", e);
                                        }
                                    }
                                }

                                self.viz_state.set_sides(heads as usize);

                                // Update widgets.
                                self.widgets
                                    .update(self.disk_image.as_ref().unwrap(), self.disk_image_name.clone());
                            }
                            ThreadLoadStatus::Error(e) => {
                                log::error!("Error loading disk image: {:?}", e);
                                self.load_status = ThreadLoadStatus::Error(e.clone());
                                self.error_msg = Some(e.to_string());
                                ctx.request_repaint();
                                // Return to reactive mode
                                self.run_mode = RunMode::Reactive;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        keep_polling = false;
                    }
                }
            }
        }
    }

    fn handle_loading_progress(&mut self, ui: &mut egui::Ui) {
        if let ThreadLoadStatus::Loading(progress) = &self.load_status {
            ui.add(egui::ProgressBar::new(*progress as f32).text(format!("{:.1}%", *progress * 100.0)));
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context, ui: Option<&mut egui::Ui>) {
        if let Some(ui) = ui {
            ui.group(|ui| {
                ui.label("Dropped files:");

                if let Some(file) = self.dropped_files.get(0) {
                    let mut info = if let Some(path) = &file.path {
                        path.display().to_string()
                    }
                    else if !file.name.is_empty() {
                        file.name.clone()
                    }
                    else {
                        "???".to_owned()
                    };

                    let mut additional_info = vec![];
                    if !file.mime.is_empty() {
                        additional_info.push(format!("type: {}", file.mime));
                    }
                    if let Some(bytes) = &file.bytes {
                        additional_info.push(format!("{} bytes", bytes.len()));
                    }
                    else {
                        additional_info.push("loading...".to_string());
                    }

                    if !additional_info.is_empty() {
                        info += &format!(" ({})", additional_info.join(", "));
                    }

                    ui.label(info);
                }
                else {
                    ui.label("No file currently dropped.");
                }
            });
        }

        // Check for new dropped files or file completion status
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                i.raw.dropped_files.iter().map(|f| f.name.clone()).for_each(|name| {
                    log::debug!("Dropped file: {:?}", name);
                });
                let new_dropped_file = &i.raw.dropped_files[0]; // Only take the first file

                // Only process a new file if there's no file already in `self.dropped_files`
                if self.dropped_files.is_empty() {
                    // Add the new file to `self.dropped_files` to track it
                    self.dropped_files = vec![new_dropped_file.clone()];
                }
            }
        });

        self.load_dropped_files();

        // Wait for bytes to be available, then process
        if let Some(file) = self.dropped_files.get(0) {
            if let Some(bytes) = &file.bytes {
                // Only process if bytes are now available
                log::info!("Processing file: {} ({} bytes)", file.name, bytes.len());

                let bytes = bytes.clone();
                let bytes_vec = bytes.to_vec();
                let mut cursor = std::io::Cursor::new(bytes_vec);

                let sender1 = self.load_sender.as_mut().unwrap().clone();
                let sender2 = self.load_sender.as_mut().unwrap().clone();

                // Remove the old disk image
                self.disk_image = None;
                // Set the name of the new disk image
                self.disk_image_name = Some(file.name.clone());

                log::debug!("Spawning thread to load disk image");
                match worker::spawn_closure_worker(move || {
                    log::debug!("Hello from worker thread!");

                    // callback is of type Arc<dyn Fn(LoadingStatus) + Send + Sync>
                    let callback = Arc::new(move |status: LoadingStatus| match status {
                        LoadingStatus::Progress(progress) => {
                            log::debug!("Sending Loading progress: {:.1}%", progress * 100.0);
                            sender2.send(ThreadLoadStatus::Loading(progress)).unwrap();
                        }
                        _ => {}
                    });

                    DiskImage::load(&mut cursor, None, None, Some(callback))
                        .map(|disk| {
                            log::debug!("Disk image loaded successfully!");
                            sender1.send(ThreadLoadStatus::Success(disk)).unwrap();
                        })
                        .unwrap_or_else(|e| {
                            log::error!("Error loading disk image: {:?}", e);
                            sender1.send(ThreadLoadStatus::Error(e)).unwrap();
                        });
                }) {
                    Ok(_) => {
                        log::debug!("Worker thread spawned successfully");
                        // Enter continuous mode.
                        self.run_mode = RunMode::Continuous;
                        ctx.request_repaint();
                    }
                    Err(e) => {
                        log::error!("Error spawning worker thread: {:?}", e);
                    }
                }

                // Clear the dropped file after processing
                self.clear_dropped_files();
            }
            else {
                // Request a repaint until the file's bytes are loaded
                ctx.request_repaint();
            }
        }
    }
}
