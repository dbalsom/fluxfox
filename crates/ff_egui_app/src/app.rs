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

use egui::Layout;
use std::{
    default::Default,
    sync::{mpsc, Arc, RwLock},
};

use fluxfox::{file_system::fat::fat_fs::FatFileSystem, DiskImage, DiskImageError, LoadingStatus};
use fluxfox_egui::{
    widgets::{
        boot_sector::BootSectorWidget,
        disk_info::DiskInfoWidget,
        filesystem::FileSystemWidget,
        header_group::HeaderGroup,
    },
    SectorSelection,
    TrackListSelection,
    UiEvent,
};

#[cfg(not(target_arch = "wasm32"))]
pub const APP_NAME: &str = "fluxfox-egui";
#[cfg(not(target_arch = "wasm32"))]
use crate::native::worker;
#[cfg(target_arch = "wasm32")]
use crate::wasm::worker;
#[cfg(target_arch = "wasm32")]
pub const APP_NAME: &str = "fluxfox-web";

use crate::{
    widgets::{filename::FilenameWidget, hello::HelloWidget},
    windows::{file_viewer::FileViewer, sector_viewer::SectorViewer, viz::VizViewer},
};
use fluxfox_egui::widgets::track_list::TrackListWidget;

pub const DEMO_IMAGE: &[u8] = include_bytes!("../../../resources/demo.imz");

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

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppUserOptions {
    auto_show_viz: bool,
    logo_panel:    bool,
}

impl Default for AppUserOptions {
    fn default() -> Self {
        Self {
            auto_show_viz: true,
            logo_panel:    true,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct PersistentState {
    user_opts: AppUserOptions,
}

#[derive(Default)]
pub struct AppWidgets {
    hello: HelloWidget,
    disk_info: DiskInfoWidget,
    boot_sector: BootSectorWidget,
    track_list: TrackListWidget,
    file_system: FileSystemWidget,
    filename: FilenameWidget,
}

impl AppWidgets {
    pub fn update(&mut self, disk_lock: Arc<RwLock<DiskImage>>, name: Option<String>) {
        log::debug!(
            "AppWidgets::update(): Attempting to lock disk image with {} references",
            Arc::strong_count(&disk_lock)
        );
        let disk = disk_lock.read().unwrap();
        self.filename.set(name);
        self.disk_info.update(&disk, None);
        self.boot_sector.update(&disk);
        self.track_list.update(&disk);

        drop(disk);
        log::debug!(
            "AppWidgets::update(): Disk image lock released. {} references remaining",
            Arc::strong_count(&disk_lock)
        );
    }

    pub fn update_mut(&mut self, disk_lock: Arc<RwLock<DiskImage>>) {
        let mut fs = match FatFileSystem::mount(disk_lock, None) {
            Ok(fs) => {
                log::debug!("FAT filesystem mounted successfully!");
                Some(fs)
            }
            Err(e) => {
                log::error!("Error mounting FAT filesystem: {:?}", e);
                None
            }
        };

        if let Some(fs) = &mut fs {
            self.file_system.update(fs);
        }

        drop(fs);
    }

    pub fn reset(&mut self) {
        self.filename = FilenameWidget::default();
        self.disk_info = DiskInfoWidget::default();
        self.boot_sector = BootSectorWidget::default();
        self.track_list = TrackListWidget::default();
        self.file_system = FileSystemWidget::default();
    }
}

#[derive(Default)]
pub struct AppWindows {
    viz_viewer:    VizViewer,
    sector_viewer: SectorViewer,
    file_viewer:   FileViewer,
}

impl AppWindows {
    pub fn reset(&mut self) {
        self.viz_viewer.reset();
        self.sector_viewer = SectorViewer::default();
        self.file_viewer = FileViewer::default();
    }
}

pub enum AppEvent {
    #[allow(dead_code)]
    Reset,
    ResetDisk,
    ImageLoaded,
    SectorSelected(SectorSelection),
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
    pub(crate) disk_image: Option<Arc<RwLock<DiskImage>>>,
    supported_extensions: Vec<String>,

    widgets: AppWidgets,
    viz_window_open: bool,
    windows: AppWindows,

    events: Vec<AppEvent>,
    deferred_file_ui_event: Option<UiEvent>,
    sector_selection: Option<SectorSelection>,

    error_msg: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let (load_sender, load_receiver) = mpsc::sync_channel(128);
        Self {
            // Example stuff:
            p_state: PersistentState {
                user_opts: AppUserOptions::default(),
            },
            run_mode: RunMode::Reactive,
            ctx_init: false,
            dropped_files: Vec::new(),

            load_status:   ThreadLoadStatus::Inactive,
            load_sender:   Some(load_sender),
            load_receiver: Some(load_receiver),

            disk_image_name: None,
            disk_image: None,

            supported_extensions: Vec::new(),

            widgets: AppWidgets::default(),
            viz_window_open: false,

            windows: AppWindows::default(),

            events: Vec::new(),
            deferred_file_ui_event: None,
            sector_selection: None,

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

        app_state.windows.viz_viewer.init(cc.egui_ctx.clone(), 512);
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

        // Show windows
        if let Some(disk_image) = &self.disk_image {
            self.windows.viz_viewer.show(ctx, disk_image.clone());
        }

        self.windows.sector_viewer.show(ctx);
        self.windows.file_viewer.show(ctx);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            self.handle_menu(ctx, ui);

            // Done with menu bar.
            if self.p_state.user_opts.logo_panel {
                self.widgets.hello.show(ui, APP_NAME, &self.supported_extensions);
                ui.add_space(8.0);
            }

            // Show filename widget
            self.widgets.filename.show(ui);
        });

        egui::SidePanel::left("disk_info_gallery")
            .exact_width(250.0)
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    self.handle_image_info(ui);
                    ui.add_space(6.0);
                    self.handle_bootsector_info(ui);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanels and SidePanels

            self.show_error(ui);

            // Show dropped files (if any):
            self.handle_dropped_files(ctx, None);
            self.handle_loading_progress(ui);

            ui.with_layout(Layout::top_down_justified(egui::Align::Center), |ui| {
                ui.allocate_ui_with_layout(ui.available_size(), Layout::left_to_right(egui::Align::Min), |ui| {
                    self.handle_track_info(ui);
                    self.handle_fs_info(ui);
                });
            });

            self.handle_load_messages(ctx);

            ui.with_layout(Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });

        self.handle_events();
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

    pub fn new_disk(&mut self) {
        log::debug!("Resetting application state for new disk...");
        self.error_msg = None;
        self.viz_window_open = false;
        self.widgets.reset();
        self.windows.reset();
    }

    pub fn reset(&mut self) {
        log::debug!("Resetting application state...");
        self.disk_image = None;
        self.disk_image_name = None;
        self.error_msg = None;
        self.load_status = ThreadLoadStatus::Inactive;
        self.run_mode = RunMode::Reactive;
        self.viz_window_open = false;
        self.widgets.reset();
        self.windows.reset();
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

    fn handle_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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
                //log::debug!("Running on web platform, showing Image menu");
                ui.menu_button("Image", |ui| {
                    if ui.button("Load demo image...").clicked() {
                        let mut cursor = std::io::Cursor::new(DEMO_IMAGE);
                        DiskImage::load(&mut cursor, None, None, None)
                            .map(|disk| {
                                log::debug!("Disk image loaded successfully!");
                                self.disk_image = Some(Arc::new(RwLock::new(disk)));
                                self.disk_image_name = Some("demo.imz".to_string());
                                self.new_disk();
                                ctx.request_repaint();
                                self.events.push(AppEvent::ImageLoaded);
                            })
                            .unwrap_or_else(|e| {
                                log::error!("Error loading disk image: {:?}", e);
                                self.error_msg = Some(e.to_string());
                            });

                        ui.close_menu();
                    }
                });
            }

            ui.menu_button("Windows", |ui| {
                ui.checkbox(self.windows.viz_viewer.open_mut(), "Visualization");
            });

            ui.menu_button("Options", |ui| {
                ui.checkbox(&mut self.p_state.user_opts.auto_show_viz, "Auto-show Visualization");
                ui.checkbox(&mut self.p_state.user_opts.logo_panel, "Show fluxfox logo panel");
            });
        });
    }

    fn handle_events(&mut self) {
        while let Some(event) = self.events.pop() {
            match event {
                AppEvent::Reset => {
                    log::debug!("Got AppEvent::Reset");
                    self.reset();
                }
                AppEvent::ResetDisk => {
                    log::debug!("Got AppEvent::ResetDisk");
                    self.new_disk();
                }
                AppEvent::ImageLoaded => {
                    log::debug!("Got AppEvent::ImageLoaded");
                    // Return to reactive mode
                    self.run_mode = RunMode::Reactive;
                    self.error_msg = None;

                    match self
                        .windows
                        .viz_viewer
                        .render(self.disk_image.as_ref().unwrap().clone())
                    {
                        Ok(_) => {
                            log::info!("Visualization rendered successfully!");
                        }
                        Err(e) => {
                            log::error!("Error rendering visualization: {:?}", e);
                        }
                    }

                    if self.p_state.user_opts.auto_show_viz {
                        self.windows.viz_viewer.set_open(true);
                    }

                    // Update widgets.
                    log::debug!("Updating widgets with new disk image...");
                    self.widgets
                        .update(self.disk_image.as_ref().unwrap().clone(), self.disk_image_name.clone());
                    self.widgets.update_mut(self.disk_image.as_ref().unwrap().clone());

                    log::debug!("Updating sector viewer...");
                    self.windows
                        .sector_viewer
                        .update(self.disk_image.as_ref().unwrap().clone(), SectorSelection::default());
                    self.sector_selection = Some(SectorSelection::default());
                    self.widgets.hello.set_small(true);
                }
                AppEvent::SectorSelected(selection) => {
                    self.windows
                        .sector_viewer
                        .update(self.disk_image.as_ref().unwrap().clone(), selection.clone());
                    self.sector_selection = Some(selection);

                    self.windows.sector_viewer.set_open(true);
                }
            }
        }
    }

    fn handle_image_info(&mut self, ui: &mut egui::Ui) {
        if self.disk_image.is_some() {
            HeaderGroup::new("Disk Info").strong().expand().show(ui, |ui| {
                self.widgets.disk_info.show(ui);
            });
        }
    }

    fn handle_bootsector_info(&mut self, ui: &mut egui::Ui) {
        if self.disk_image.is_some() {
            HeaderGroup::new("Boot Sector").strong().expand().show(ui, |ui| {
                self.widgets.boot_sector.show(ui);
            });
        }
    }

    fn handle_track_info(&mut self, ui: &mut egui::Ui) {
        if self.disk_image.is_some() {
            ui.group(|ui| {
                if let Some(selection) = self.widgets.track_list.show(ui) {
                    log::debug!("TrackList selection: {:?}", selection);
                    match selection {
                        TrackListSelection::Track(_track) => {
                            //self.events.push(AppEvent::SectorSelected(SectorSelection::Track(track)));
                        }
                        TrackListSelection::Sector(sector) => {
                            self.events.push(AppEvent::SectorSelected(sector));
                        }
                    }
                }
            });
        }
    }

    fn handle_fs_info(&mut self, ui: &mut egui::Ui) {
        let mut new_event = None;
        if let Some(disk) = &mut self.disk_image {
            // egui::Window::new("Test Table").resizable(true).show(ui.ctx(), |ui| {
            //     self.widgets.file_list.show(ui);
            // });

            ui.group(|ui| {
                new_event = self.widgets.file_system.show(ui);
            });

            if Arc::strong_count(disk) > 1 {
                log::debug!("handle_fs_info(): Disk image is locked, deferring event...");
                self.deferred_file_ui_event = new_event.take();
            }
            else if new_event.is_none() && self.deferred_file_ui_event.is_some() {
                log::debug!("handle_fs_info(): Disk image is unlocked, processing deferred event...");
                new_event = self.deferred_file_ui_event.take();
            }

            if let Some(event) = new_event {
                match event {
                    UiEvent::ExportFile(path) => {
                        log::debug!("Exporting file: {:?}", path);

                        let mut fs = FatFileSystem::mount(disk.clone(), None).unwrap();
                        let file_data = match fs.read_file(&path) {
                            Ok(data) => data,
                            Err(e) => {
                                log::error!("Error reading file: {:?}", e);
                                return;
                            }
                        };
                        fs.unmount();

                        match App::save_file_as(&path, &file_data) {
                            Ok(_) => {
                                log::info!("File saved successfully!");
                            }
                            Err(e) => {
                                log::error!("Error saving file: {:?}", e);
                            }
                        }
                    }
                    UiEvent::SelectFile(file) => {
                        let selected_file = file.path;
                        log::debug!("Selected file: {:?}", selected_file);
                        match FatFileSystem::mount(disk.clone(), None) {
                            Ok(mut fs) => {
                                log::debug!("FAT filesystem mounted successfully!");
                                self.windows.file_viewer.update(&fs, selected_file);
                                self.windows.file_viewer.set_open(true);

                                fs.unmount();
                            }
                            Err(e) => {
                                log::error!("Error mounting FAT filesystem: {:?}", e);
                            }
                        };
                    }
                    _ => {}
                }
            };
        }
    }

    fn handle_load_messages(&mut self, ctx: &egui::Context) {
        let mut new_disk = false;
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

                                self.widgets = AppWidgets::default();
                                self.viz_window_open = false;
                                ctx.request_repaint();

                                match self.load_status {
                                    ThreadLoadStatus::Inactive => {
                                        //log::debug!("Inactive->Loading. Sending AppEvent::ResetDisk");
                                        self.events.push(AppEvent::ResetDisk);
                                    }
                                    _ => {}
                                };

                                self.load_status = ThreadLoadStatus::Loading(progress);
                            }
                            ThreadLoadStatus::Success(disk) => {
                                log::info!("Disk image loaded successfully!");

                                self.disk_image = Some(Arc::new(RwLock::new(disk)));
                                //log::debug!("ThreadLoadStatus -> Inactive");
                                if let ThreadLoadStatus::Inactive = self.load_status {
                                    new_disk = true;
                                }
                                self.load_status = ThreadLoadStatus::Inactive;
                                ctx.request_repaint();
                                self.events.push(AppEvent::ImageLoaded);
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
        else {
            log::error!("No load receiver available!");
        }

        if new_disk {
            log::debug!("Resetting disk due to new disk without loading notification...");
            self.new_disk();
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

                if let Some(file) = self.dropped_files.first() {
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
        if let Some(file) = self.dropped_files.first() {
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
