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
use fluxfox::{prelude::*, track_schema::GenericTrackElement, visualization::prelude::*, FoxHashMap};

use crate::{
    styles::{default_skia_styles, BlendMode, ElementStyle, SkiaStyle},
    DEFAULT_VIEW_BOX,
};

#[derive(Clone, Default)]
pub struct TinySkiaRenderer {
    // The view box for a single head. This should be square.
    side_view_box: VizRect<f32>,
    // The view box for the entire visualization. This can be rectangular.
    global_view_box: VizRect<f32>,
    // Margins as top, right, bottom, left.
    global_margins: (f32, f32, f32, f32),
    // Whether to render the data layer.
    render_data: bool,
    // Maximum outer radius (should not exceed side_view_box.width / 2).
    // Will be overridden if outer_radius_ratio is set.
    outer_radius: f32,
    // The direct inner radius (should not exceed max_outer_radius). Will be overridden if
    // inner_radius_ratio is set.
    inner_radius: f32,
    // Whether to decode the data layer.
    decode_data: bool,
    // Whether to disable antialiasing for data groups (recommended: true, avoids moiré patterns).
    data_crisp: bool,
    // The number of segments to render the data layer with. Default is 1440.
    data_slices: Option<usize>,
    // Whether to render the metadata layer.
    render_metadata: bool,
    // Whether to render data and metadata layers to separate files,
    // or to render them together in a single file.
    render_layered: bool,
    // Whether to render the sides side-by-side. If false, the sides will be rendered
    // in separate documents.
    render_side_by_side: bool,
    // The spacing between sides when rendering side-by-side. Default is 0.
    side_spacing: f32,
    // The CSS blend mode to apply to the metadata layer if `render_layered` is enabled.
    layer_blend_mode: BlendMode,
    // The total number of sides to render. Default is 1. Will be overridden if `side_to_render`
    // is set.
    total_sides_to_render: u8,
    // The side to render. Default is 0. If set, this will override `total_sides_to_render`
    // to 1.
    side_to_render: Option<u8>,
    // Specify a single track to render. If isolate_track is set, the track will be rendered with
    // width between outer and inner radii.
    track_to_render: Option<u16>,
    // Specify a single sector to render. If isolate_track is set, the sector will be rendered in
    // position on its enclosing track, with the track width between the specified inner and outer
    // radii. Otherwise, it will be rendered at its normal position on the disk.
    // The second parameter in the tuple is an optional bitcell address, to allow selection of
    // sectors when duplicate sector IDs are present. If None, the first matching sector will be
    // rendered.
    sector_to_render: (Option<DiskChsn>, Option<usize>),

    // Flag to control whether turning direction is reversed for the second side.
    reverse_turning: bool,
    // Style mappings for generic elements. If not set, a default set of styles will be used.
    skia_styles: FoxHashMap<GenericTrackElement, SkiaStyle>,
    // Default style for track elements - a solid ring that is the background of each track.
    // Default is transparent fill and 0 stroke.
    track_style: ElementStyle,
    // Internal state
    common_params: CommonVizParams,

    overlay_style: ElementStyle,
    export_path:   Option<String>,

    build_error:   bool,
    error_message: Option<String>,
}

impl TinySkiaRenderer {
    pub fn new() -> Self {
        Self {
            side_view_box: VizRect::from_tuple((0.0, 0.0), (DEFAULT_VIEW_BOX, DEFAULT_VIEW_BOX)),
            global_view_box: VizRect::from_tuple((0.0, 0.0), (DEFAULT_VIEW_BOX, DEFAULT_VIEW_BOX)),
            skia_styles: default_skia_styles(),
            data_crisp: true,
            reverse_turning: true,
            // Start at 2 and take the minimum of image heads and 2.
            // This can also get set to 1 if a specific side is set.
            total_sides_to_render: 2,
            ..Default::default()
        }
    }

    /// Set the view box for a single side. This effectively controls the default "resolution"
    /// of the rendered image. The view box should be square, unless you really want a distorted
    /// image. If radius is not set, radius will be set to half the height of the view box.
    pub fn with_side_view_box(mut self, view_box: VizRect<f32>) -> Self {
        if self.common_params.radius.is_none() {
            self.common_params.radius = Some(view_box.height() / 2.0);
        }
        self.side_view_box = view_box;
        self
    }

    /// Set the value of the track gap - ie, the inverse factor if the track width to render.
    /// 0.5 will render tracks half as wide as the calculated track width.
    /// 0.0 will render tracks at the calculated track width.
    /// Value is clamped to the range [0.0, 0.9].
    pub fn with_track_gap(mut self, gap: f32) -> Self {
        // clamp the value
        self.common_params.track_gap = gap.clamp(0.0, 0.9);
        self
    }

    /// Set the global view box. You could use this to control margins, but it's probably better
    /// to use the `with_margins` method instead.
    pub fn with_global_view_box(mut self, view_box: VizRect<f32>) -> Self {
        self.global_view_box = view_box;
        self
    }

    /// Set a flag to render the data layer representation. In vector format, this will render
    /// the data layer as a series of segments, stroked with a color representing the average flux
    /// density for that segment.
    /// # Arguments
    /// * `state` - A boolean flag to enable or disable rendering of the data layer.
    /// * `segments` - An optional parameter to specify the number of segments to render. If None,
    ///     the default number of segments will be used. If a value is provided, it will be clamped
    ///     to the range [360, 2880].
    pub fn with_data_layer(mut self, state: bool, segments: Option<usize>) -> Self {
        self.render_data = state;
        self.data_slices = segments;

        self
    }

    /// The angle in radians at which the index position will be rendered, from the perspective of
    /// the specified turning direction. The default is 0.0, which will render the index position
    /// at the 3 o'clock position. The angle is specified in radians.
    ///
    /// Note that this value is ignored when generating metadata display lists - in general,
    /// rotation should be handled by the renderer. For SVG output we emit a transformation to
    /// rotate the entire group.
    pub fn with_index_angle(mut self, angle: f32) -> Self {
        self.common_params.index_angle = angle;
        self
    }

    /// Specify a specific side to be rendered instead of the entire disk. The value must be
    /// 0 or 1. If the value is 0, the bottom side will be rendered. If the value is 1, the top
    /// side will be rendered.
    pub fn with_side(mut self, side: u8) -> Self {
        if side > 1 {
            self.build_error = true;
            self.error_message = Some("Invalid side to render.".to_string());
        }
        self.side_to_render = Some(side);
        self.total_sides_to_render = 1;
        self
    }

    /// Set the initial data turning direction. The default is Clockwise. This value represents
    /// how the data wraps on the visualization from the viewer's perspective. It is the reverse
    /// of the physical rotation direction of the disk.
    pub fn with_initial_turning(mut self, turning: TurningDirection) -> Self {
        self.common_params.direction = turning;
        self
    }

    /// Specify whether the turning direction should be reversed for the second side. The default
    /// is true. This will apply to the second side even if only the second side is rendered, for
    /// consistency.
    pub fn with_turning_side_flip(mut self, state: bool) -> Self {
        self.reverse_turning = state;
        self
    }

    /// Specify a single track to be rendered instead of the entire disk.
    /// This will render the same track number on both sides unless a specific side is specified.
    pub fn with_rendered_track(mut self, track: u16) -> Self {
        self.track_to_render = Some(track);
        self
    }

    /// Set the total number of sides to render. This value can be either 1 or 2, but this method
    /// is typically used to set the rendering to 2 sides as 1 side is assumed when the side is
    /// specified with `render_side`.
    fn render_side(mut self, sides: u8) -> Self {
        if sides < 1 || sides > 2 {
            self.build_error = true;
            self.error_message = Some("Invalid number of sides to render.".to_string());
        }
        else {
            self.total_sides_to_render = sides;

            if sides == 2 {
                // If we're rendering both sides, clear the side to render.
                // We shouldn't have really set this in the first place, but whatever.
                self.side_to_render = None;
            }
        }
        self
    }

    /// Flag to decode the data layer when rendering. Currently only supported for MFM tracks.
    /// It will be ignored for GCR tracks.
    pub fn decode_data(mut self, state: bool) -> Self {
        self.decode_data = state;
        self
    }

    /// Render the metadata layer.
    pub fn with_metadata_layer(mut self, state: bool) -> Self {
        self.render_metadata = state;
        self
    }

    /// Render metadata on top of data layers, using the specified blend mode.
    /// The default is true. If false, separate documents will be created for data and metadata
    /// layers.
    pub fn with_layer_stack(mut self, state: bool) -> Self {
        self.render_layered = state;
        self
    }

    /// Set a flag to render both sides of the disk in a single document, with the sides rendered
    /// side-by-side, using the specified value of `spacing` between the two sides.
    /// The default is true. If false, separate documents will be created for each side, and
    /// potentially up to four documents may be created if `render_layered` is false and metadata
    /// layer rendering is enabled.
    pub fn side_by_side(mut self, state: bool, spacing: f32) -> Self {
        self.render_side_by_side = state;
        self.side_spacing = spacing;
        self
    }

    /// Expand the global view box by the specified margins.
    pub fn with_margins(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.global_margins = (top, right, bottom, left);
        self
    }

    /// Specify the blend mode to use when rendering both data and metadata layers as a stack.
    /// The default is `BlendMode::Normal`, which will hide the data layer when metadata is
    /// present - be sure to set it to your desired mode if you want to see both layers.
    pub fn with_blend_mode(mut self, mode: BlendMode) -> Self {
        log::trace!("Setting blend mode to {:?}", mode);
        self.layer_blend_mode = mode;
        self
    }

    /// Specify the inner and outer radius ratios, as a fraction of the side view box width.
    pub fn with_radius_ratios(mut self, inner: f32, outer: f32) -> Self {
        self.common_params.min_radius_ratio = inner;
        self.common_params.max_radius_ratio = outer;
        self
    }

    /// Override the default styles with a custom set of styles. This must be a hash map of
    /// `GenericTrackElement` to `ElementStyle`.
    #[allow(unused_mut)]
    pub fn with_styles(mut self, _styles: FoxHashMap<GenericTrackElement, ElementStyle>) -> Self {
        // TODO: Implement this.
        self
    }

    /// Override the default track style. Useful if you want to render metadata on its own,
    /// with some sort of background.
    pub fn with_track_style(mut self, style: ElementStyle) -> Self {
        self.track_style = style;
        self
    }

    pub fn render(mut self, disk: &DiskImage) -> Result<Self, String> {
        if self.build_error {
            return Err(self.error_message.unwrap_or("Unknown error.".to_string()));
        }

        self.total_sides_to_render = std::cmp::min(self.total_sides_to_render, disk.heads());

        // Render each side
        let starting_side = self.side_to_render.unwrap_or(0);

        log::trace!(
            "render(): starting side: {} sides to render: {}",
            starting_side,
            self.total_sides_to_render
        );

        Ok(self)
    }
}
