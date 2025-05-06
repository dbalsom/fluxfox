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

pub mod character_encoding;
pub mod controls;
mod range_check;
pub mod tracking_lock;
pub mod traits;
pub mod visualization;
pub mod widgets;

use fluxfox::{file_system::FileEntry, prelude::*};
use std::{
    fmt,
    fmt::{Debug, Display, Formatter, Result},
    sync::Arc,
};

use thiserror::Error;

pub use traits::render_callback::RenderCallback;

#[derive(Debug, Copy, Clone, Default)]
pub enum WidgetSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl WidgetSize {
    pub fn rounding(&self) -> f32 {
        match self {
            WidgetSize::Small => 4.0,
            WidgetSize::Normal => 6.0,
            WidgetSize::Large => 8.0,
        }
    }

    pub fn padding(&self) -> f32 {
        match self {
            WidgetSize::Small => 2.0,
            WidgetSize::Normal => 4.0,
            WidgetSize::Large => 6.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SectorSelection {
    pub phys_ch:    DiskCh,
    pub sector_id:  SectorId,
    pub bit_offset: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub enum TrackSelectionScope {
    RawDataStream,
    #[default]
    DecodedDataStream,
    Elements,
    Timings,
}

#[derive(Debug, Clone, Default)]
pub struct TrackSelection {
    pub sel_scope: TrackSelectionScope,
    pub phys_ch:   DiskCh,
}

#[derive(Debug, Clone)]
pub enum TrackListSelection {
    Track(TrackSelection),
    Sector(SectorSelection),
}

#[derive(Clone)]
pub enum UiEvent {
    SelectionChange(TrackListSelection),
    ExportFile(String),
    SelectPath(String),
    SelectFile(FileEntry),
    ExportDir(String),
    ExportDirAsArchive(String),
}

impl Debug for UiEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // Match on the enum to display only the variant name
        let variant_name = match self {
            UiEvent::SelectionChange(_) => "SelectionChange",
            UiEvent::ExportFile(_) => "ExportFile",
            UiEvent::SelectPath(_) => "SelectPath",
            UiEvent::SelectFile(_) => "SelectFile",
            UiEvent::ExportDir(_) => "ExportDir",
            UiEvent::ExportDirAsArchive(_) => "ExportDirAsArchive",
        };
        write!(f, "{}", variant_name)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum UiLockContext {
    /// App is not a tool per se, but represents locks made in the main application logic.
    /// Some Tools do not need to keep a persistent disk image lock as they display static
    /// data that cannot change for the life of the loaded image (for example, the SourceMap)
    /// The main application logic locks the disk image for the duration of the tool's update
    /// cycle.
    App,
    /// This context represents a lock held by an emulator consuming the fluxfox_egui library.
    /// Ideally an emulator releases its lock before calling into the library, but in cases where
    /// the core runs in a separate thread, this may be unavoidable.
    Emulator,
    /// The visualization tool renders a graphical depiction of the disk and allows track element
    /// selection. It must own a DiskLock to support hit-testing user selections and rendering
    /// vector display lists of the current selection.
    DiskVisualization,
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

impl Display for UiLockContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Error)]
pub enum UiError {
    #[error("An error occurred rendering the disk visualization: {0}")]
    VisualizationError(String),
}

type SaveFileCallbackFn = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;
