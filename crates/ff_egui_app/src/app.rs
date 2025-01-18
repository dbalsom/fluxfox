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

use egui::Layout;
use fluxfox::{
    file_system::{fat::fat_fs::FatFileSystem, FileSystemArchive},
    DiskImage,
    DiskImageError,
    LoadingStatus,
};
use fluxfox_egui::{
    controls::{
        boot_sector::BootSectorWidget,
        disk_info::DiskInfoWidget,
        error_banner::ErrorBanner,
        filesystem::FileSystemWidget,
        header_group::{HeaderFn, HeaderGroup},
    },
    SectorSelection,
    TrackListSelection,
    TrackSelection,
    TrackSelectionScope,
    UiEvent,
};
use std::{
    collections::VecDeque,
    default::Default,
    fmt,
    fmt::{Display, Formatter},
    path::PathBuf,
    sync::{mpsc, Arc},
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
    lock::TrackingLock,
    widgets::{filename::FilenameWidget, hello::HelloWidget},
    windows::{
        element_map::ElementMapViewer,
        file_viewer::FileViewer,
        new_viz::NewVizViewer,
        sector_viewer::SectorViewer,
        source_map::SourceMapViewer,
        track_timing_viewer::TrackTimingViewer,
        track_viewer::TrackViewer,
        viz::VizViewer,
    },
};
use fluxfox_egui::controls::track_list::TrackListWidget;

pub const DEMO_IMAGE: &[u8] = include_bytes!("../../../resources/demo.imz");
/// The number of selection slots available for disk images.
/// These slots will be enumerating in the UI as `A:`, `B:`, and so on. The UI design does not
/// anticipate more than 2 slots, but could be adapted for more if you wish.
pub const DISK_SLOTS: usize = 2;

/// fluxfox-egui comprises several conceptual Tools.
///
/// Each tool has a unique identifier that can be used to track disk image locks for debugging.
/// Each tool may correspond to one or more widgets or windows, but provides a shared pool of
/// resources and communication channels.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    /// App is not a tool per se, but represents locks made in the main application logic.
    /// Some Tools do not need to keep a persistent disk image lock as they display static
    /// data that cannot change for the life of the loaded image (for example, the SourceMap)
    /// The main application logic locks the disk image for the duration of the tool's update
    /// cycle.
    App,
    /// The visualization tool renders a graphical depiction of the disk and allows track element
    /// selection. It must own a DiskLock to support hit-testing user selections and rendering
    /// vector display lists of the current selection.
    Visualization,
    NewViz,
    SectorViewer,
    TrackViewer,
    TrackListViewer,
    /// The filesystem viewer is currently the only tool that requires a write lock, due to
    /// the use of a StandardSectorView, which requires a mutable reference to the disk image.
    /// StandardSectorView is used as an interface for reading and writing sectors in a standard
    /// raw-sector based order, such as what is expected by rust-fatfs.
    FileSystemViewer,
    /// A file system operation not necessarily tied to the filesystem viewer.
    FileSystemOperation,
    SourceMap,
    TrackElementMap,
    TrackTimingViewer,
}

impl Display for Tool {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Default)]
pub enum ThreadLoadStatus {
    #[default]
    Inactive,
    Loading(f64),
    Success(DiskImage, usize),
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
    logo_panel: bool,
    archive_format: FileSystemArchive,
}

impl Default for AppUserOptions {
    fn default() -> Self {
        Self {
            auto_show_viz: true,
            logo_panel: true,
            archive_format: FileSystemArchive::Zip,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct PersistentState {
    user_opts: AppUserOptions,
}

pub struct AppWidgets {
    hello: HelloWidget,
    disk_info: DiskInfoWidget,
    boot_sector: BootSectorWidget,
    track_list: TrackListWidget,
    file_system: FileSystemWidget,
    filename: FilenameWidget,
}

impl AppWidgets {
    pub fn new(_ui_sender: mpsc::SyncSender<UiEvent>) -> Self {
        Self {
            hello: HelloWidget::default(),
            disk_info: DiskInfoWidget::default(),
            boot_sector: BootSectorWidget::default(),
            track_list: TrackListWidget::default(),
            file_system: FileSystemWidget::default(),
            filename: FilenameWidget::default(),
        }
    }

    pub fn update_disk(&mut self, disk_lock: TrackingLock<DiskImage>, name: Option<String>) {
        let disk = match disk_lock.read(Tool::App) {
            Ok(disk) => disk,
            Err(_) => {
                log::error!("Failed to lock disk image for reading. Cannot update widgets.");
                return;
            }
        };
        self.filename.set(name);
        self.disk_info.update(&disk, None);
        self.boot_sector.update(&disk);
        self.track_list.update(&disk);
    }

    pub fn update_mut(&mut self, disk_lock: TrackingLock<DiskImage>) {
        let mut fs = match FatFileSystem::mount(disk_lock, Tool::FileSystemViewer, None) {
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

pub struct AppWindows {
    viz_viewer: VizViewer,
    new_viz_viewer: NewVizViewer,
    sector_viewer: SectorViewer,
    track_viewer: TrackViewer,
    file_viewer: FileViewer,
    source_map: SourceMapViewer,
    element_map: ElementMapViewer,
    track_timing_viewer: TrackTimingViewer,
}

impl AppWindows {
    pub fn new(_ui_sender: mpsc::SyncSender<UiEvent>) -> Self {
        Self {
            viz_viewer: VizViewer::new(),
            new_viz_viewer: NewVizViewer::default(),
            sector_viewer: SectorViewer::default(),
            track_viewer: TrackViewer::default(),
            file_viewer: FileViewer::default(),
            source_map: SourceMapViewer::default(),
            element_map: ElementMapViewer::default(),
            track_timing_viewer: TrackTimingViewer::default(),
        }
    }

    pub fn reset(&mut self) {
        self.viz_viewer.reset();
        self.new_viz_viewer.reset();
        self.sector_viewer = SectorViewer::default();
        self.track_viewer = TrackViewer::default();
        self.file_viewer = FileViewer::default();
        self.source_map = SourceMapViewer::default();
        self.element_map = ElementMapViewer::default();
        self.track_timing_viewer = TrackTimingViewer::default();
    }

    /// Update windows that hold a disk image lock with a new lock.
    pub fn update_disk(&mut self, disk_lock: TrackingLock<DiskImage>, _name: Option<String>) {
        // The visualization viewer can hold a read lock in the background for rendering, so it
        // should be updated last.
        match disk_lock.read(Tool::App) {
            Ok(disk) => self.source_map.update(&disk),
            Err(_) => {
                log::error!("Failed to lock disk image for reading. Cannot update windows.");
                return;
            }
        };

        log::debug!("Updating track data viewer...");
        self.track_viewer.update_disk(disk_lock.clone());

        log::debug!("Updating sector viewer...");
        self.sector_viewer.update(disk_lock.clone(), SectorSelection::default());

        log::debug!("Updating visualization...");
        self.viz_viewer.update_disk(disk_lock.clone());
    }
}

/// App events are sent from Tools to the main application state to request changes in the UI.
pub enum AppEvent {
    #[allow(dead_code)]
    Reset,
    ResetDisk,
    /// A DiskImage has been successfully loaded into the specified slot index.
    ImageLoaded(usize),
    SectorSelected(SectorSelection),
    TrackSelected(TrackSelection),
    TrackElementsSelected(TrackSelection),
    TrackTimingsSelected(TrackSelection),
}

/// A [DiskSlot] represents data about a specific disk image slot.
#[derive(Default)]
pub struct DiskSlot {
    pub image: Option<TrackingLock<DiskImage>>,
    pub image_name: Option<String>,
    pub source_path: Option<PathBuf>,
}

impl DiskSlot {
    /// Create a new [DiskSlot] with the given [DiskImage], name, and source path.
    /// Note: Do not use `into_arc` with [DiskImage], as the [TrackingLock] will do this for you.
    pub fn new(image: DiskImage, name: Option<String>, path: Option<PathBuf>) -> Self {
        Self {
            image: Some(TrackingLock::new(image)),
            image_name: name,
            source_path: path,
        }
    }

    pub fn attach_image(&mut self, image: DiskImage) {
        self.image = Some(TrackingLock::new(image));
    }

    pub fn set_name(&mut self, name: Option<String>) {
        self.image_name = name;
    }

    pub fn set_source_path(&mut self, path: Option<PathBuf>) {
        self.source_path = path;
    }
}

/// The main fluxfox-gui application state.
pub struct App {
    /// State that should be serialized and deserialized on restart should be stored here.
    /// Everything else will start as default.
    p_state: PersistentState,
    run_mode: RunMode,
    ctx_init: bool,
    pub(crate) dropped_files: Vec<egui::DroppedFile>,
    load_status: ThreadLoadStatus,
    load_sender: Option<mpsc::SyncSender<ThreadLoadStatus>>,
    load_receiver: Option<mpsc::Receiver<ThreadLoadStatus>>,

    tool_sender:   mpsc::SyncSender<UiEvent>,
    tool_receiver: mpsc::Receiver<UiEvent>,

    /// The selected disk slot. This is used to track which disk image is currently selected for
    /// viewing and manipulation.
    pub(crate) selected_slot: usize,
    pub(crate) disk_slots: [DiskSlot; DISK_SLOTS],
    old_locks: Vec<TrackingLock<DiskImage>>,

    supported_extensions: Vec<String>,

    widgets: AppWidgets,
    windows: AppWindows,

    events: VecDeque<AppEvent>,
    deferred_file_ui_event: Option<UiEvent>,
    sector_selection: Option<SectorSelection>,
    track_selection: Option<TrackSelection>,

    error_msg: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let (load_sender, load_receiver) = mpsc::sync_channel(128);
        let (tool_sender, tool_receiver) = mpsc::sync_channel(16);
        Self {
            // Example stuff:
            p_state: PersistentState {
                user_opts: AppUserOptions::default(),
            },
            run_mode: RunMode::Reactive,
            ctx_init: false,
            dropped_files: Vec::new(),

            load_status: ThreadLoadStatus::Inactive,
            load_sender: Some(load_sender),
            load_receiver: Some(load_receiver),

            widgets: AppWidgets::new(tool_sender.clone()),
            windows: AppWindows::new(tool_sender.clone()),
            tool_sender,
            tool_receiver,

            selected_slot: 0,
            disk_slots: Default::default(),
            old_locks: Vec::new(),

            supported_extensions: Vec::new(),

            events: VecDeque::new(),
            deferred_file_ui_event: None,
            sector_selection: None,
            track_selection: None,

            error_msg: None,
        }
    }
}

impl App {
    pub fn selected_disk(&self) -> Option<TrackingLock<DiskImage>> {
        self.disk_slots[self.selected_slot].image.clone()
    }

    pub fn have_disk_in_slot(&self, slot: usize) -> bool {
        if slot >= DISK_SLOTS {
            return false;
        }
        self.disk_slots[slot].image.is_some()
    }

    pub fn have_disk_in_selected_slot(&self) -> bool {
        self.have_disk_in_slot(self.selected_slot)
    }

    pub fn slot(&self, slot: usize) -> &DiskSlot {
        &self.disk_slots[slot % DISK_SLOTS]
    }

    pub fn slot_mut(&mut self, slot: usize) -> &mut DiskSlot {
        &mut self.disk_slots[slot % DISK_SLOTS]
    }

    pub fn selected_slot(&self) -> &DiskSlot {
        &self.disk_slots[self.selected_slot]
    }

    pub fn selected_slot_mut(&mut self) -> &mut DiskSlot {
        &mut self.disk_slots[self.selected_slot]
    }

    pub fn set_slot(&mut self, slot: usize, new_slot: DiskSlot) {
        if slot >= DISK_SLOTS {
            return;
        }

        if self.disk_slots[slot].image.is_some() {
            log::debug!(
                "load_slot(): Ejecting disk {:?} from slot {}",
                self.disk_slots[slot].image_name,
                slot
            );
            self.eject_slot(slot);
        }
        log::debug!("load_slot(): Loading disk into slot {}", slot);
        self.disk_slots[slot] = new_slot;
    }

    /// Eject the DiskSlot from the given slot index.
    /// The corresponding DiskLock is moved to the old_locks list so that memory leaks can be
    /// detected and reported.
    pub fn eject_slot(&mut self, slot: usize) {
        if slot >= DISK_SLOTS {
            return;
        }
        if let Some(disk_slot) = self.disk_slots.get_mut(slot) {
            if let Some(disk) = disk_slot.image.take() {
                log::debug!("eject_slot(): Ejecting disk from slot {}", slot);
                self.old_locks.push(disk);
            }
            else {
                log::trace!("eject_slot(): No disk in slot {}", slot);
            }
        }
        else {
            log::error!("eject_slot(): Invalid slot index {}", slot);
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

        // Initialize the visualization viewer
        app_state
            .windows
            .viz_viewer
            .init(cc.egui_ctx.clone(), 512, app_state.tool_sender.clone());

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

    pub fn collect_garbage(&mut self) {
        let lock_ct = self.old_locks.len();
        self.old_locks.retain(|lock| lock.strong_count() > 0);
        if lock_ct != self.old_locks.len() {
            log::debug!(
                "collect_garbage(): Collected {} locks, {} remaining",
                lock_ct - self.old_locks.len(),
                self.old_locks.len()
            );
        }
    }
}

impl eframe::App for App {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        self.collect_garbage();

        if !self.ctx_init {
            self.ctx_init(ctx);
        }

        if matches!(self.run_mode, RunMode::Continuous) {
            ctx.request_repaint();
        }

        // Show windows
        if self.have_disk_in_selected_slot() {
            self.windows.source_map.show(ctx);
        }

        #[cfg(feature = "devmode")]
        {
            self.windows.new_viz_viewer.show(ctx);
        }
        self.windows.viz_viewer.show(ctx);
        self.windows.sector_viewer.show(ctx);
        self.windows.track_viewer.show(ctx);
        self.windows.file_viewer.show(ctx);
        self.windows.element_map.show(ctx);
        self.windows.track_timing_viewer.show(ctx);

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

        self.handle_ui_events();
        self.handle_app_events();
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
        //self.disk_image_name = None;
        self.error_msg = None;
        self.widgets.reset();
        self.windows.reset();
    }

    pub fn reset(&mut self) {
        log::debug!("Resetting application state...");

        for i in 0..DISK_SLOTS {
            self.eject_slot(i);
        }
        self.error_msg = None;
        self.load_status = ThreadLoadStatus::Inactive;
        self.run_mode = RunMode::Reactive;
        self.widgets.reset();
        self.windows.reset();
    }

    // Optional: clear dropped files when done
    fn clear_dropped_files(&mut self) {
        self.dropped_files.clear();
    }

    fn show_error(&mut self, ui: &mut egui::Ui) {
        if let Some(msg) = &self.error_msg {
            ErrorBanner::new(msg).large().show(ui);

            // egui::Frame::none()
            //     .fill(egui::Color32::DARK_RED)
            //     .rounding(8.0)
            //     .inner_margin(8.0)
            //     .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
            //     .show(ui, |ui| {
            //         ui.horizontal(|ui| {
            //             ui.label(egui::RichText::new("🗙").color(egui::Color32::WHITE).size(32.0));
            //             ui.add(egui::Label::new(
            //                 egui::RichText::new(msg).color(egui::Color32::WHITE).size(24.0),
            //             ));
            //         });
            //     });
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

                                let disk = DiskSlot {
                                    image: Some(TrackingLock::new(disk)),
                                    image_name: Some("demo.imz".to_string()),
                                    source_path: None,
                                };
                                self.set_slot(0, disk);
                                self.new_disk();
                                ctx.request_repaint();
                                self.events.push_back(AppEvent::ImageLoaded(self.selected_slot));
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
                #[cfg(feature = "devmode")]
                {
                    ui.checkbox(self.windows.new_viz_viewer.open_mut(), "Visualization (New)");
                }
                ui.checkbox(self.windows.source_map.open_mut(), "Image Source Map");
            });

            ui.menu_button("Options", |ui| {
                ui.checkbox(&mut self.p_state.user_opts.auto_show_viz, "Auto-show Visualization");
                ui.checkbox(&mut self.p_state.user_opts.logo_panel, "Show fluxfox logo panel");

                ui.menu_button("Archive format", |ui| {
                    ui.radio_value(
                        &mut self.p_state.user_opts.archive_format,
                        FileSystemArchive::Zip,
                        "ZIP",
                    );
                    ui.radio_value(
                        &mut self.p_state.user_opts.archive_format,
                        FileSystemArchive::Tar,
                        "TAR",
                    );
                });
            });
        });
    }

    fn handle_app_events(&mut self) {
        while let Some(event) = self.events.pop_front() {
            match event {
                AppEvent::Reset => {
                    log::debug!("Got AppEvent::Reset");
                    self.reset();
                }
                AppEvent::ResetDisk => {
                    log::debug!("Got AppEvent::ResetDisk");
                    self.new_disk();
                }
                AppEvent::ImageLoaded(slot_idx) => {
                    log::debug!("Got AppEvent::ImageLoaded");
                    // Return to reactive mode
                    self.run_mode = RunMode::Reactive;
                    self.error_msg = None;

                    if self.p_state.user_opts.auto_show_viz {
                        self.windows.viz_viewer.set_open(true);
                        #[cfg(feature = "devmode")]
                        {
                            self.windows.new_viz_viewer.set_open(true);
                        }
                    }

                    if let (Some(disk_image), image_name) = (
                        self.slot(slot_idx).image.clone(),
                        self.slot(slot_idx).image_name.clone(),
                    ) {
                        // Update widgets. Update widgets that use a mutable reference first.
                        log::debug!("Updating widgets with new disk image...");
                        self.widgets.update_mut(disk_image.clone());
                        self.widgets.update_disk(disk_image.clone(), image_name.clone());

                        self.windows.update_disk(disk_image.clone(), image_name.clone());

                        self.sector_selection = Some(SectorSelection::default());
                        self.widgets.hello.set_small(true);
                    }
                }
                AppEvent::SectorSelected(selection) => {
                    if let Some(disk) = self.selected_disk() {
                        self.windows.sector_viewer.update(disk.clone(), selection.clone());
                        self.sector_selection = Some(selection);

                        self.windows.sector_viewer.set_open(true);
                    }
                }
                AppEvent::TrackSelected(selection) => {
                    if let Some(_disk) = self.selected_disk() {
                        self.windows.track_viewer.update_selection(selection.clone());
                        self.track_selection = Some(selection);
                        self.windows.track_viewer.set_open(true);
                    }
                }
                AppEvent::TrackElementsSelected(selection) => {
                    if let Some(disk) = self.selected_disk() {
                        self.windows.element_map.update(disk.clone(), selection.clone());
                        self.windows.element_map.set_open(true);
                    }
                }
                AppEvent::TrackTimingsSelected(selection) => {
                    if let Some(disk) = self.selected_disk() {
                        match disk.read(Tool::App) {
                            Ok(disk) => {
                                if let Some(track) = disk.track(selection.phys_ch) {
                                    if let Some(track) = track.as_fluxstream_track() {
                                        self.windows.track_timing_viewer.update(
                                            selection.phys_ch,
                                            track.flux_deltas(),
                                            Some(track.pll_markers()),
                                        );
                                        self.windows.track_timing_viewer.set_open(true);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to lock disk image for reading. Locked by {:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_image_info(&mut self, ui: &mut egui::Ui) {
        if self.have_disk_in_selected_slot() {
            HeaderGroup::new("Disk Info").strong().expand().show(
                ui,
                |ui| {
                    self.widgets.disk_info.show(ui);
                },
                None::<HeaderFn>,
            );
        }
    }

    fn handle_bootsector_info(&mut self, ui: &mut egui::Ui) {
        if self.have_disk_in_selected_slot() {
            HeaderGroup::new("Boot Sector").strong().expand().show(
                ui,
                |ui| {
                    self.widgets.boot_sector.show(ui);
                },
                None::<HeaderFn>,
            );
        }
    }

    /// Handle UI events - events sent from tools to the application.
    fn handle_ui_events(&mut self) {
        let mut keep_polling = true;
        while keep_polling {
            match self.tool_receiver.try_recv() {
                Ok(event) => match event {
                    UiEvent::SelectionChange(selection) => match selection {
                        TrackListSelection::Track(track) => {
                            self.events.push_back(AppEvent::TrackSelected(track));
                        }
                        TrackListSelection::Sector(sector) => {
                            log::warn!("handle_ui_events(): Sector selected: {:?}", sector);
                            self.events.push_back(AppEvent::SectorSelected(sector));
                        }
                    },
                    _ => {
                        log::warn!("Unhandled UiEvent: {:?}", event);
                    }
                },
                Err(_) => {
                    keep_polling = false;
                }
            }
        }
    }

    fn handle_track_info(&mut self, ui: &mut egui::Ui) {
        if self.have_disk_in_selected_slot() {
            ui.group(|ui| {
                if let Some(selection) = self.widgets.track_list.show(ui) {
                    log::debug!("TrackList selection: {:?}", selection);
                    match selection {
                        TrackListSelection::Track(track) => match track.sel_scope {
                            TrackSelectionScope::DecodedDataStream => {
                                self.events.push_back(AppEvent::TrackSelected(track));
                            }
                            TrackSelectionScope::Elements => {
                                self.events.push_back(AppEvent::TrackElementsSelected(track));
                            }
                            TrackSelectionScope::Timings => {
                                self.events.push_back(AppEvent::TrackTimingsSelected(track));
                            }
                            _ => log::warn!("Unsupported TrackSelectionScope: {:?}", track.sel_scope),
                        },
                        TrackListSelection::Sector(sector) => {
                            self.events.push_back(AppEvent::SectorSelected(sector));
                        }
                    }
                }
            });
        }
    }

    fn handle_fs_info(&mut self, ui: &mut egui::Ui) {
        let mut new_event = None;
        if let Some(disk) = &mut self.selected_disk() {
            // egui::Window::new("Test Table").resizable(true).show(ui.ctx(), |ui| {
            //     self.widgets.file_list.show(ui);
            // });

            ui.group(|ui| {
                new_event = self.widgets.file_system.show(ui);
            });

            // if Arc::strong_count(disk) > 1 {
            //     log::debug!("handle_fs_info(): Disk image is locked, deferring event...");
            //     self.deferred_file_ui_event = new_event.take();
            // }
            // else if new_event.is_none() && self.deferred_file_ui_event.is_some() {
            //     log::debug!("handle_fs_info(): Disk image is unlocked, processing deferred event...");
            //     new_event = self.deferred_file_ui_event.take();
            // }

            if new_event.is_none() && self.deferred_file_ui_event.is_some() {
                new_event = self.deferred_file_ui_event.take();
            }

            if let Some(event) = new_event {
                match event {
                    UiEvent::ExportFile(path) => {
                        log::debug!("Exporting file: {:?}", path);

                        let mut fs = FatFileSystem::mount(disk.clone(), Tool::FileSystemOperation, None).unwrap();
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
                        let selected_file = file.path().to_string();
                        log::debug!("Selected file: {:?}", selected_file);
                        match FatFileSystem::mount(disk.clone(), Tool::FileSystemOperation, None) {
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
                    #[cfg(feature = "archives")]
                    UiEvent::ExportDirAsArchive(path) => {
                        log::debug!("Exporting directory as archive: {:?}", path);
                        match FatFileSystem::mount(disk.clone(), Tool::FileSystemOperation, None) {
                            Ok(mut fs) => {
                                let archive_data = match fs.root_as_archive(self.p_state.user_opts.archive_format) {
                                    Ok(data) => data,
                                    Err(e) => {
                                        log::error!("Error exporting directory as archive: {:?}", e);
                                        return;
                                    }
                                };
                                fs.unmount();

                                let slot = self.selected_slot();
                                let mut zip_name = slot.image_name.clone().unwrap_or("disk".to_string());
                                zip_name.push_str(self.p_state.user_opts.archive_format.ext());

                                match App::save_file_as(&zip_name, &archive_data) {
                                    Ok(_) => {
                                        log::info!("Archive {} saved successfully!", zip_name);
                                    }
                                    Err(e) => {
                                        log::error!("Error saving archive: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Error mounting FAT filesystem: {:?}", e);
                            }
                        }
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

                                self.widgets = AppWidgets::new(self.tool_sender.clone());
                                *self.windows.viz_viewer.open_mut() = false;
                                ctx.request_repaint();

                                match self.load_status {
                                    ThreadLoadStatus::Inactive => {
                                        log::debug!("ThreadLoadStatus::Inactive->Loading. Sending AppEvent::ResetDisk");
                                        self.events.push_back(AppEvent::ResetDisk);
                                    }
                                    _ => {}
                                };

                                self.load_status = ThreadLoadStatus::Loading(progress);
                            }
                            ThreadLoadStatus::Success(disk, slot_idx) => {
                                log::info!("Disk image loaded successfully!");
                                self.disk_slots[slot_idx].attach_image(disk);
                                //log::debug!("ThreadLoadStatus -> Inactive");
                                if let ThreadLoadStatus::Inactive = self.load_status {
                                    new_disk = true;
                                }
                                self.load_status = ThreadLoadStatus::Inactive;
                                ctx.request_repaint();
                                self.events.push_back(AppEvent::ImageLoaded(slot_idx));
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

        fn dropped_filename(file: &egui::DroppedFile) -> String {
            if let Some(path) = &file.path {
                path.display().to_string()
            }
            else if !file.name.is_empty() {
                file.name.clone()
            }
            else {
                "Unknown".to_owned()
            }
        }

        // Check for new dropped files or file completion status
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| dropped_filename(f))
                    .for_each(|name| {
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
            let file = file.clone();
            if let Some(bytes) = &file.bytes {
                // Only process if bytes are now available
                log::info!("Processing file: {} ({} bytes)", file.name, bytes.len());

                let bytes = bytes.clone();
                let bytes_vec = bytes.to_vec();
                let mut cursor = std::io::Cursor::new(bytes_vec);

                let sender1 = self.load_sender.as_mut().unwrap().clone();
                let sender2 = self.load_sender.as_mut().unwrap().clone();

                // Remove the old disk image
                self.eject_slot(self.selected_slot);
                // Set the name of the new disk image
                self.selected_slot_mut().image_name = Some(dropped_filename(&file));

                log::debug!("Spawning thread to load disk image");
                let loading_slot = self.selected_slot;
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
                            sender1.send(ThreadLoadStatus::Success(disk, loading_slot)).unwrap();
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
