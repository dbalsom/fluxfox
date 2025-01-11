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

//! # FluxFox Visualization
//! The `visualization` module provides rendering functions for disk images.
//! This module requires the `viz` feature to be enabled.
//!
//! The general principle is that a disk image is rendered on the unit circle, where angle
//! 0 in radians corresponds to the index position of the disk. The angle increases either
//! clockwise or counter-clockwise from 0 to 2π, depending on the [TurningDirection] specified.
//!
//! [TurningDirection] specifies the way the data is mapped to the unit circle, and is the inverse
//! of the actual physical rotation of the disk. The default turning is clockwise for side 0,
//! and counter-clockwise for side 1. This gives an accurate depiction of how the data would appear
//! looking top-down at each side of a standard PC floppy disk.
//!
//! ## Visualization layers
//! Visualizations can be constructed of several different layers.
//!
//! - The `data layer` is a visualization of the data on the disk, and can be optionally decoded
//!   for MFM tracks in rasterization mode.
//! - The `metadata layer` is a visualization of track elements such as markers, sector headers,
//!   and sector data.
//! - `Mask layers` are visualizations of bit masks, including weak bit masks and error maps.
//!   The mask type may be specified by [RenderMaskType].
//!
//! Layers are typically rendered in order with some sort of blend operation, with mask layers on
//! top.
//!
//! ## Visualization types
//!
//! Two primary rendering modes are supported, `rasterization` and `vectorization`.
//!
//! - `Rasterization` is much slower but can resolve much higher levels of detail for the data
//!    layer. It is subject to moiré patterns and aliasing artifacts, but good results can be
//!    achieved by supersampling.
//!
//! - `Vectorization` can be much faster, especially for rendering the data layer of a disk image.
//!    Vectorization methods return display lists, which may either be rasterized directly, or
//!    converted to SVG, or triangulated and displayed by a GPU. The main advantage of vectorization
//!    is that rasterizing a high resolution vector image is not subject to the same quadratic time
//!    complexity as rasterizing a high resolution raster image, and produces crisper results at
//!    lower resolutions without supersampling.
//!
//! ## Helper crates
//!
//! - The `fluxfox_tiny_skia` crate provides a `tiny_skia` backend for fluxfox's rasterization
//!   functions.
//! - The `fluxfox_svg` crate provides a backend for fluxfox's vectorization functions capable of
//!   saving display lists to SVG files with many configurable options.
//!
//! ## Examples
//! See the `imgviz` example for a demonstration of how to use the visualization functions,
//! `fluxfox_tiny_skia`, and `fluxfox_svg`.
//!

pub mod data_segmenter;
pub mod prelude;
pub mod rasterize_disk;
pub mod types;
pub mod vectorize_disk;

use crate::{
    bitstream_codec::TrackDataStream,
    track_schema::{GenericTrackElement, TrackMetadata},
    visualization::types::{
        color::VizColor,
        shapes::{VizDimensions, VizPoint2d, VizRect, VizRotation},
    },
    DiskCh,
    DiskImage,
    DiskVisualizationError,
    FoxHashMap,
    MAX_CYLINDER,
};
use std::{
    cmp::min,
    f32::consts::{PI, TAU},
};

use bit_vec::BitVec;

/// A vector data visualization is broken up into 1440 slices, representing four slices for
/// each degree angle. This is designed to roughly fit the popcnt of each slice into a u8, even
/// for ED disks (400_000/1440 = 277.78, but popcnt cannot reach that maximum value).
/// This is a compromise to allow for a simple lookup table to map popcnt to a grayscale value.
/// Changing this value would require adjusting the table.
pub(crate) const VIZ_SLICES: usize = 1440;

pub(crate) const DEFAULT_INNER_RADIUS_RATIO: f32 = 0.30; // Matches HxC default for 'dummy' disk viz

/// Create a lookup table to map a u8 value to a grayscale gradient value based on the number of
/// bits set in the u8 value (popcount)
const POPCOUNT_TABLE: [u8; 256] = {
    let values: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = values[i.count_ones() as usize];
        i += 1;
    }
    table
};

/// A simple trait to allow for rotation of visualization elements
pub trait VizRotate {
    /// Produce a rotated copy of the element
    fn rotate(self, rotation: &VizRotation) -> Self;
}

use crate::visualization::prelude::VizElementDisplayList;
#[cfg(feature = "tiny_skia")]
pub use rasterize_disk::rasterize_track_data;
#[cfg(feature = "tiny_skia")]
pub use rasterize_disk::render_track_mask;

/// A map type selector for visualization functions.
#[derive(Copy, Clone, Debug)]
pub enum RenderMaskType {
    /// Select the weak bit mask for rendering
    WeakBits,
    /// Select the bitstream codec error mask for rendering
    Errors,
}

#[derive(Copy, Clone, Default)]
pub enum RenderWinding {
    #[default]
    Clockwise,
    CounterClockwise,
}

/// A [RenderGeometry] enum specifies what geometry to generate for metadata element sectors.
/// This is useful if your rasterizer cannot fill concave paths - you can use an arc and stroke
/// it at the track width instead.
#[derive(Copy, Clone, Default)]
pub enum RenderGeometry {
    #[default]
    Sector,
    Arc,
}

/// Parameter struct for use with display list rasterization functions
#[derive(Clone, Default)]
pub struct RenderVectorizationParams {
    /// View box dimensions to use for the visualization.
    pub view_box: VizRect<f32>,
    /// Image background color to use for the visualization. If None, background will be transparent.
    pub image_bg_color: Option<VizColor>,
    /// Background color to use for the disk surface, in absence of any rendered elements.
    /// If None, the disk surface will be transparent between tracks (determined by track_gap).
    pub disk_bg_color: Option<VizColor>,
    /// Color to use when rendering a track bit mask.
    pub mask_color: Option<VizColor>,
    /// Offset for the output of the rasterization within the destination pixmap, in pixels. If
    /// None, the offset will be set to (0, 0) (no offset).
    pub pos_offset: Option<VizPoint2d<f32>>,
}

/// Parameter struct for use with display list rasterization functions
#[derive(Clone)]
pub struct RenderRasterizationParams {
    /// Dimensions of the image to be rendered.
    pub image_size: VizDimensions,
    /// Supersampling factor to use.
    pub supersample: u32,
    /// Background color to use for area outside of disk ring. If None, the image will have a
    /// transparent background outside the disk surfaces.
    pub image_bg_color: Option<VizColor>,
    /// Background color to use for the disk surface, in absence of any rendered elements.
    /// If None, the disk surface will be transparent where elements are not rendered.
    pub disk_bg_color: Option<VizColor>,
    /// Color to use when rendering a track bit mask.
    pub mask_color: Option<VizColor>,
    /// Palette to use for rasterizing metadata elements. Can be set to None if not rendering
    /// metadata.
    pub palette: Option<FoxHashMap<GenericTrackElement, VizColor>>,
    /// Offset for the output of the rasterization within the destination pixmap, in pixels. If
    /// None, the offset will be set to (0, 0) (no offset).
    pub pos_offset: Option<VizPoint2d<u32>>,
}

impl RenderRasterizationParams {
    /// Return the full resolution of the image to be rendered, taking int account the supersampling
    /// factor.
    pub fn render_size(&self) -> VizDimensions {
        self.image_size.scale(self.supersample)
    }
}

/// Common parameters for all rendering functions
#[derive(Clone)]
pub struct CommonVizParams {
    /// Outer radius of the visualization in pixels. This should equal the width of a square
    /// destination pixmap, divided by two. Pixmap dimensions must be square, and ideally a power
    /// of two. If `None`, the radius will be set to 0.5, to create a rendering with normalized
    /// coordinates from (0.0, 0.0) to (1.0, 1.0). You can then translate the image yourself using
    /// a transformation matrix before rendering.
    pub radius: Option<f32>,
    /// Maximum outer radius as a fraction ot total radius
    /// The outside of the first track will be rendered at this radius.
    pub max_radius_ratio: f32,
    /// Minimum inner radius as a fraction of total radius (0.333) == 1/3 of total radius
    /// If `pin_last_track` is false, the inside of the last track will be rendered at this radius.
    /// If `pin_last_track` is true, the inside of the last standard track will be rendered at this
    ///  radius, but non-standard or over-dumped tracks will be rendered at a smaller radius within.
    /// This is useful for keeping proportions consistent between disks with different track counts,
    /// if for example, you are rendering a slideshow of various disk images.
    pub min_radius_ratio: f32,
    /// Offset for points produced by the rendering function. This is useful for rendering a
    /// visualization off-center. If `None`, the offset will be set to (0.0, 0.0) (no offset).
    /// Note: If you are intending to rasterize the resulting display list and wish to say, place
    /// two visualizations side by side, you should set this value to None and use the `pos_offset`
    /// field of the [RenderRasterizationParams] struct instead.
    pub pos_offset: Option<VizPoint2d<f32>>,
    /// Angle of index position / start of track, in radians. The default value is 0 which will
    /// render the disk with the index position at the 3 o'clock position.
    pub index_angle: f32,
    /// Maximum number of tracks to render. If None, no limit will be enforced.
    pub track_limit: Option<usize>,
    /// Set the inner radius to the last standard track instead of last track
    /// This keeps proportions consistent between disks with different track counts
    pub pin_last_standard_track: bool,
    /// Width of the gap between tracks as a fraction of total track width (0.0 to 1.0)
    /// Track width itself is determined by the track count and the inner and outer radii.
    pub track_gap: f32,
    /// How the data should visually turn on the disk surface, starting from the index position.
    /// Note: this is the logical reverse of the physical rotation of the disk.
    pub direction: TurningDirection,
}

impl Default for CommonVizParams {
    fn default() -> Self {
        Self {
            radius: Some(0.5),
            max_radius_ratio: 1.0,
            min_radius_ratio: DEFAULT_INNER_RADIUS_RATIO,
            pos_offset: Some(VizPoint2d::new(0.0, 0.0)),
            index_angle: 0.0,
            track_limit: None,
            pin_last_standard_track: true,
            track_gap: 0.1,
            direction: TurningDirection::CounterClockwise,
        }
    }
}

impl CommonVizParams {
    pub(crate) fn track_params(&self, num_tracks: usize) -> Result<InternalTrackParams, DiskVisualizationError> {
        let mut tp = InternalTrackParams::default();
        let track_limit = self.track_limit.unwrap_or(MAX_CYLINDER);

        tp.num_tracks = min(num_tracks, track_limit);
        if tp.num_tracks == 0 {
            return Err(DiskVisualizationError::NoTracks);
        }
        tp.total_radius = self.radius.unwrap_or(0.5);
        tp.center = VizPoint2d::new(tp.total_radius, tp.total_radius);
        tp.min_radius = self.min_radius_ratio * tp.total_radius;
        tp.max_radius = self.max_radius_ratio * tp.total_radius;

        // If pinning has been specified, adjust the minimum radius.
        // We subtract any over-dumped tracks from the radius, so that the minimum radius fraction
        // is consistent with the last standard track.
        tp.min_radius = if self.pin_last_standard_track {
            let normalized_track_ct = match num_tracks {
                0..50 => 40,
                50.. => 80,
            };
            let track_width = (tp.total_radius - tp.min_radius) / normalized_track_ct as f32;
            let overdump = num_tracks.saturating_sub(normalized_track_ct);
            self.min_radius_ratio * tp.total_radius - (overdump as f32 * track_width)
        }
        else {
            tp.min_radius
        };

        if tp.max_radius <= tp.min_radius {
            return Err(DiskVisualizationError::InvalidParameter(
                "max_radius must be greater than min_radius".to_string(),
            ));
        }

        // Calculate the rendered width of each track, excluding the track gap.
        tp.render_track_width = (tp.max_radius - tp.min_radius) / num_tracks as f32;
        if self.track_gap == 0.0 {
            // slightly increase the track width to avoid rendering sparkles between tracks if there's
            // 0 gap specified, due to floating point errors
            tp.vector_track_width = tp.render_track_width * 1.01;
        }
        else {
            tp.vector_track_width = tp.render_track_width;
        }

        if tp.vector_track_width == 0.0 {
            // Nothing to render!
            return Err(DiskVisualizationError::NotVisible);
        }

        Ok(tp)
    }
}

#[derive(Clone, Default)]
pub(crate) struct InternalTrackParams {
    pub(crate) total_radius: f32,
    pub(crate) min_radius: f32,
    pub(crate) max_radius: f32,
    pub(crate) num_tracks: usize,
    pub(crate) center: VizPoint2d<f32>,
    pub(crate) total_track_width: f32,
    pub(crate) render_track_width: f32,
    pub(crate) vector_track_width: f32,
    pub(crate) track_overlap: f32,
}

impl InternalTrackParams {
    const QUADRANT_ANGLES: [(f32, f32); 4] = [
        (0.0, PI / 2.0),
        (PI / 2.0, PI),
        (PI, 3.0 * PI / 2.0),
        (3.0 * PI / 2.0, TAU),
    ];

    // Coordinate offset factors for each quadrant, when rendering quadrant pixmaps.
    // These offsets are scaled by the radius of the disk.
    // For example, and offset of (1.0, 1.0) and a radius of 512 will push all coordinates
    // to the right and down by 512 pixels (rendering the lower-right quadrant).
    const QUADRANT_OFFSETS: [(f32, f32); 4] = [
        // Lower-right quadrant
        (1.0, 1.0),
        // Lower-left quadrant
        (0.0, 1.0),
        // Upper-right quadrant
        (1.0, 0.0),
        // Upper-left quadrant
        (0.0, 0.0),
    ];

    /// Return the center point in pixels for a given quadrant, for rendering a quadrant pixmap.
    #[inline]
    pub(crate) fn quadrant_center(&self, quadrant: u8) -> VizPoint2d<f32> {
        let (x, y) = Self::QUADRANT_OFFSETS[quadrant as usize & 0x03];
        VizPoint2d::new(x * self.total_radius, y * self.total_radius)
    }

    /// Return the start and end angles in radians for a given quadrant
    #[inline]
    pub(crate) fn quadrant_clip(&self, quadrant: u8) -> (f32, f32) {
        Self::QUADRANT_ANGLES[quadrant as usize & 0x03]
    }

    // Return a tuple of (bool, (f32, f32)) where the bool is true if the start and end angles
    // overlap the quadrant, and the tuple parameter contains the clipped start and end angles.
    // The angles are not clipped if false is returned.
    #[inline]
    pub(crate) fn quadrant_hit_test(&self, quadrant: u8, angles: (f32, f32)) -> (bool, (f32, f32)) {
        let (clip_start, clip_end) = self.quadrant_clip(quadrant);
        if angles.1 <= clip_start || angles.0 >= clip_end {
            // No overlap
            (false, (angles.0, angles.1))
        }
        else {
            // Overlap - clip the angles to the quadrant
            (true, (angles.0.max(clip_start), angles.1.min(clip_end)))
        }
    }
}

/// Parameter struct for use with disk surface rendering functions
pub struct RenderTrackDataParams {
    /// Which side of disk to render. This may seem superfluous as we render one head at a time,
    /// but the data is stored within the [VizElement] of the resulting display list.
    pub side: u8,
    /// Attempt to decode data on a track for more visual contrast. This will only work if the
    /// encoding and track schema supports random-access decoding. GCR encoding is not supported.
    /// A request to decode an incompatible track will be ignored.
    pub decode: bool,
    /// Mask decoding or encoding operations to sector data regions. This will only work if the
    /// track defines sector data elements. A request to mask an incompatible track will be ignored.
    /// The main advantage of using this flag with `decode` is to avoid visualizing write splices
    /// outside of sector data regions that cause ugly flips in contrast.
    pub sector_mask: bool,
    /// Resolution to render data at (Bit or Byte). Bit resolution requires extremely high
    /// resolution rasterized output to be legible - it's fun but not really practical.
    pub resolution: ResolutionType,
    /// Number of slices to use to segment the data. This is only used during vector-based rendering.
    pub slices: usize,
    /// Factor to overlap slices by. This can avoid rendering artifacts at full opacity, but can
    /// cause artifacts if fractional opacity is used.
    pub overlap: f32,
}

impl Default for RenderTrackDataParams {
    fn default() -> Self {
        Self {
            side: 0,
            decode: false,
            sector_mask: false,
            resolution: ResolutionType::Byte,
            slices: VIZ_SLICES,
            // Default 10% overlap
            overlap: 0.1,
        }
    }
}

/// Parameter struct for use with disk metadata rendering functions
pub struct RenderTrackMetadataParams {
    /// Which quadrant to render (0-3) if Some. If None, all quadrants will be rendered.
    pub quadrant: Option<u8>,
    /// Which side of disk to render
    pub side: u8,
    /// The type of geometry to generate for metadata elements
    pub geometry: RenderGeometry,
    /// Which point winding to use when creating sectors and other closed paths
    pub winding: RenderWinding,
    /// Whether to draw empty tracks as black rings
    pub draw_empty_tracks: bool,
    /// Draw a sector lookup bitmap instead of color information
    pub draw_sector_lookup: bool,
}

impl Default for RenderTrackMetadataParams {
    fn default() -> Self {
        Self {
            quadrant: None,
            side: 0,
            geometry: RenderGeometry::default(),
            winding: RenderWinding::default(),
            draw_empty_tracks: false,
            draw_sector_lookup: false,
        }
    }
}

#[derive(Default, Copy, Clone)]
pub enum RenderDiskSelectionType {
    #[default]
    Sector,
    Track,
}

/// Parameter struct for use with disk selection rendering functions
/// This is useful for rendering a single sector or track on a disk image.
/// Note: more than one VizElement may be emitted for a single sector, depending on the size
/// of the sector. Arcs are split at quadrant boundaries to avoid rendering artifacts.
pub struct RenderDiskSelectionParams {
    /// The selection type (Sector or Track)
    pub selection_type: RenderDiskSelectionType,
    /// The physical cylinder and head to render
    pub ch: DiskCh,
    /// The physical sector index to render, 1-offset
    pub sector_idx: usize,
    /// Color to use to draw sector arc
    pub color: VizColor,
}

impl Default for RenderDiskSelectionParams {
    fn default() -> Self {
        Self {
            ch: DiskCh::new(0, 0),
            selection_type: RenderDiskSelectionType::default(),
            sector_idx: 1,
            color: VizColor::WHITE,
        }
    }
}

/// Parameter struct for use with disk hit test rendering functions
pub struct RenderDiskHitTestParams {
    pub side: u8,
    /// The hit test selection type (Sector or Track)
    pub selection_type: RenderDiskSelectionType,
    /// The coordinate to hit test
    pub point: VizPoint2d<f32>,
}

impl Default for RenderDiskHitTestParams {
    fn default() -> Self {
        Self {
            side: 0,
            selection_type: RenderDiskSelectionType::default(),
            point: VizPoint2d::new(0.0, 0.0),
        }
    }
}

#[derive(Default)]
pub struct DiskHitTestResult {
    pub display_list: Option<VizElementDisplayList>,
    pub angle: f32,
    pub bit_index: usize,
    pub track: u16,
}

/// Determines the direction that the linear track data is mapped to the disk surface during
/// rendering, starting from the index position, either clockwise or counter-clockwise.
/// This is not the physical rotation of the disk, as they are essentially opposites.
///
/// Typically, Side 0, the bottom-facing side of a disk, rotates counter-clockwise when viewed
/// from the bottom, and Side 1, the top-facing side, rotates clockwise, and the turning will be
/// the opposite of the physical rotation.
#[derive(Copy, Clone, Debug, Default)]
pub enum TurningDirection {
    Clockwise,
    #[default]
    CounterClockwise,
}

impl TurningDirection {
    pub fn opposite(&self) -> Self {
        match self {
            TurningDirection::Clockwise => TurningDirection::CounterClockwise,
            TurningDirection::CounterClockwise => TurningDirection::Clockwise,
        }
    }

    pub fn adjust_angle(&self, angle: f32) -> f32 {
        match self {
            TurningDirection::Clockwise => angle,
            TurningDirection::CounterClockwise => TAU - angle,
        }
    }

    pub fn adjust_angles(&self, angles: (f32, f32)) -> (f32, f32) {
        match self {
            TurningDirection::Clockwise => (angles.0, angles.1),
            TurningDirection::CounterClockwise => (TAU - angles.0, TAU - angles.1),
        }
    }
}

impl From<u8> for TurningDirection {
    fn from(val: u8) -> Self {
        match val {
            0 => TurningDirection::Clockwise,
            _ => TurningDirection::CounterClockwise,
        }
    }
}

/// Determines the visualization resolution - either byte resolution or bit resolution.
/// Bit resolution requires extremely high resolution output to be legible.
pub struct PixmapToDiskParams {
    pub img_dimensions: VizDimensions,
    pub img_pos: VizPoint2d<u32>,
    pub sample_size: (u32, u32),
    pub skip_tracks: u16,
    pub black_byte: u8,
    pub white_byte: u8,
    pub mask_resolution: u8,
}

impl Default for PixmapToDiskParams {
    fn default() -> Self {
        Self {
            img_dimensions: VizDimensions::default(),
            img_pos: VizPoint2d::default(),
            sample_size: (4096, 4096),
            skip_tracks: 0,
            black_byte: 0x88,   // Represents a valid MFM pattern with low flux density (0b1000_1000)
            white_byte: 0x66,   // Represents a valid MFM pattern with high flux density (0b1010_1010)
            mask_resolution: 3, // 3 bits = 0b0111 or 8 bit mask
        }
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub enum ResolutionType {
    Bit,
    #[default]
    Byte,
}

fn stream(ch: DiskCh, disk_image: &DiskImage) -> &TrackDataStream {
    disk_image.track_map[ch.h() as usize]
        .get(ch.c() as usize)
        .map(|track_i| disk_image.track_pool[*track_i].stream().unwrap())
        .unwrap()
}

fn metadata(ch: DiskCh, disk_image: &DiskImage) -> &TrackMetadata {
    disk_image.track_map[ch.h() as usize]
        .get(ch.c() as usize)
        .map(|track_i| disk_image.track_pool[*track_i].metadata().unwrap())
        .unwrap()
}

fn collect_streams(head: u8, disk_image: &DiskImage) -> Vec<&TrackDataStream> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].stream())
        .collect()
}

fn collect_weak_masks(head: u8, disk_image: &DiskImage) -> Vec<&BitVec> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].stream().map(|track| track.weak_mask()))
        .collect()
}

fn collect_error_maps(head: u8, disk_image: &DiskImage) -> Vec<&BitVec> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].stream().map(|track| track.error_map()))
        .collect()
}

fn collect_metadata(head: u8, disk_image: &DiskImage) -> Vec<&TrackMetadata> {
    disk_image.track_map[head as usize]
        .iter()
        .filter_map(|track_i| disk_image.track_pool[*track_i].metadata())
        .collect()
}
