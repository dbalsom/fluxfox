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
use svg::Document;
use web_time::Instant;

use crate::{
    render_display_list::{render_data_display_list_as_svg, render_display_list_as_svg},
    styles::{default_element_styles, BlendMode, ElementStyle},
    DEFAULT_DATA_SLICES,
    DEFAULT_VIEW_BOX,
};

use crate::{
    document::DocumentLayer,
    overlays::Overlay,
    prelude::{DocumentSide, RenderedDocument},
    render_elements::viz_color_to_value,
    render_overlays::svg_to_group,
};
use svg::node::element::Group;

#[derive(Clone, Default)]
pub struct SvgRenderer {
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
    element_styles: FoxHashMap<GenericTrackElement, ElementStyle>,
    // Default style for track elements - a solid ring that is the background of each track.
    // Default is transparent fill and 0 stroke.
    track_style: ElementStyle,
    // Internal state
    common_params: CommonVizParams,

    overlay: Option<Overlay>,
    overlay_style: ElementStyle,

    data_groups: [Option<Group>; 2],
    metadata_groups: [Option<Group>; 2],
    overlay_groups: [Option<Group>; 2],

    composited_group: Option<Group>,
    export_path: Option<String>,

    build_error:   bool,
    error_message: Option<String>,
}

impl SvgRenderer {
    pub fn new() -> Self {
        Self {
            side_view_box: VizRect::from_tuple((0.0, 0.0), (DEFAULT_VIEW_BOX, DEFAULT_VIEW_BOX)),
            global_view_box: VizRect::from_tuple((0.0, 0.0), (DEFAULT_VIEW_BOX, DEFAULT_VIEW_BOX)),
            element_styles: default_element_styles(),
            data_crisp: true,
            reverse_turning: true,
            // Start at 2 and take the minimum of image heads and 2.
            // This can also get set to 1 if a specific side is set.
            total_sides_to_render: 2,
            overlay_style: Overlay::default_style(),
            ..Default::default()
        }
    }

    /// Set the view box for a single side. This effectively controls the default "resolution"
    /// of the rendered image. The view box should be square, unless you really want a distorted
    /// image. If radius is not set, radius will be set to half the height of the view box.
    pub fn with_side_view_box(mut self, view_box: VizRect<f32>) -> Self {
        self.common_params.radius = Some(view_box.height() / 2.0);
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

    /// Specify an SVG overlay to apply to the rendered visualization. This overlay will be
    /// added to the final document as the last group.
    pub fn with_overlay(mut self, overlay: Overlay) -> Self {
        self.overlay = Some(overlay);
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
    pub fn with_styles(mut self, _styles: FoxHashMap<GenericTrackElement, ElementStyle>) -> Self {
        self
    }

    /// Override the default track style. Useful if you want to render metadata on its own,
    /// with some sort of background.
    pub fn with_track_style(mut self, style: ElementStyle) -> Self {
        self.track_style = style;
        self
    }

    fn render_data_group(&mut self, disk: &DiskImage, side: u8) -> Result<Group, String> {
        log::trace!("Vectorizing data group for side {}...", side);
        let data_params = RenderTrackDataParams {
            side,
            decode: self.decode_data,
            sector_mask: false,
            slices: self.data_slices.unwrap_or(DEFAULT_DATA_SLICES),
            ..Default::default()
        };

        let vector_params = RenderVectorizationParams {
            view_box: self.side_view_box.clone(),
            image_bg_color: None,
            disk_bg_color: None,
            mask_color: None,
            pos_offset: None,
        };

        // Reverse the turning direction for side 1, if reverse turning is enabled (default true)
        if side > 0 && self.reverse_turning {
            self.common_params.direction = self.common_params.direction.opposite();
        }

        let display_list = vectorize_disk_data(disk, &self.common_params, &data_params, &vector_params)
            .map_err(|e| format!("Failed to vectorize data for side {}: {}", side, e))?;

        log::trace!(
            "vectorize_disk_data() returned a display list of length {} for side {}",
            display_list.len(),
            side,
        );

        let mut group = render_data_display_list_as_svg(
            self.side_view_box.clone(),
            self.common_params.index_angle,
            &display_list,
        );

        // Move this side's group over if we're rendering side-by-side, this the second side, and
        // we are rendering two sides.
        if (side > 0) && (self.total_sides_to_render > 1) && self.render_side_by_side {
            group = group.set(
                "transform",
                format!("translate({:.3}, 0)", self.side_spacing + self.side_view_box.width()),
            );
        }

        Ok(group)
    }

    fn render_metadata_group(&mut self, disk: &DiskImage, side: u8) -> Result<Group, String> {
        log::debug!("Vectorizing metadata group for side {}...", side);

        let metadata_params = RenderTrackMetadataParams {
            quadrant: None,
            side,
            geometry: RenderGeometry::Sector,
            winding: Default::default(),
            draw_empty_tracks: false,
            draw_sector_lookup: false,
        };

        let display_list = match vectorize_disk_elements_by_quadrants(disk, &self.common_params, &metadata_params) {
            Ok(display_list) => display_list,
            Err(e) => {
                eprintln!("Error rendering metadata: {}", e);
                std::process::exit(1);
            }
        };

        let mut group = render_display_list_as_svg(
            self.side_view_box.clone(),
            0.0,
            &display_list,
            &self.track_style,
            &self.element_styles,
        );

        // Directly apply our blend mode to this group - blend modes cannot be inherited!
        // Only apply the blend mode if we have a data layer to blend with, and layered rendering
        // is enabled.
        if self.render_data && self.render_layered {
            let mode = self.layer_blend_mode.to_string();
            group = group.set("style", format!("mix-blend-mode: {};", mode));
        }

        // Move this side's group over if we're rendering side-by-side, this the second side, and
        // we are rendering two sides.
        if (side > 0) && (self.total_sides_to_render > 1) && self.render_side_by_side {
            group = group.set(
                "transform",
                format!("translate({:.3}, 0)", self.side_spacing + self.side_view_box.width()),
            );
        }

        Ok(group)
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

        for si in 0..self.total_sides_to_render {
            let side = si + starting_side;
            assert!(side < 2, "Invalid side!");

            // Render data group
            if self.render_data {
                log::trace!("render(): Rendering data layer group for side {}", side);
                let data_timer = Instant::now();
                self.data_groups[side as usize] = Some(self.render_data_group(disk, side)?);
                log::trace!(
                    "render(): Rendering data for side {} took {:.3}ms",
                    side,
                    data_timer.elapsed().as_secs_f64() * 1000.0
                );
            }

            // Render metadata group
            if self.render_metadata {
                log::trace!("render(): Rendering metadata layer group for side {}", side);
                let metadata_timer = Instant::now();
                self.metadata_groups[side as usize] = Some(self.render_metadata_group(disk, side)?);
                log::trace!(
                    "render(): Rendering metadata for side {} took {:.3}ms",
                    side,
                    metadata_timer.elapsed().as_secs_f64() * 1000.0
                );
            }

            // Render overlay group
            if let Some(overlay) = &self.overlay {
                log::trace!("render(): Rendering overlay layer group for side {}", side);
                self.overlay_groups[side as usize] = Some(svg_to_group(
                    overlay.svg(side),
                    &self.side_view_box,
                    &self.overlay_style,
                )?);
            }
        }

        Ok(self)
    }

    /// Create a vector of SVG documents after rendering using the specified parameters.
    /// Up to four documents may be created, depending on the rendering parameters.
    /// For example if layered rendering and side by side rendering are both disabled, four
    /// documents will be created for a dual-sided disk, one for each side and layer.
    pub fn create_documents(mut self) -> Result<Vec<RenderedDocument>, String> {
        let mut output_documents: Vec<RenderedDocument> = Vec::with_capacity(4);

        // We won't be splitting documents in half by side, as we either only have one side or
        // we're rendering both sides in a single document.
        if self.total_sides_to_render == 1 || self.render_side_by_side {
            log::debug!("create_documents(): Rendering all sides together.");
            let data_group = {
                // Put both sides in a single document
                // First, check if we even rendered two sides:
                let mut data_sides = Vec::with_capacity(2);
                for (i, group) in self.data_groups.iter().enumerate() {
                    if group.is_some() {
                        data_sides.push(i);
                    }
                }

                if data_sides.len() > 1 {
                    let mut group = Group::new();
                    for idx in data_sides {
                        group = group.add(self.data_groups[idx].take().unwrap());
                    }
                    Some(group)
                }
                else if !data_sides.is_empty() {
                    // Only one side was rendered, so just return that side's group.
                    self.data_groups.into_iter().flatten().next()
                }
                else {
                    // No sides were rendered, so return None
                    None
                }
            };

            let metadata_group = {
                // Put both sides in a single document
                // First, check if we even rendered two sides:
                let mut metadata_sides = Vec::with_capacity(2);
                for (i, group) in self.metadata_groups.iter().enumerate() {
                    if group.is_some() {
                        metadata_sides.push(i);
                    }
                }

                if metadata_sides.len() > 1 {
                    let mut group = Group::new();
                    for idx in metadata_sides {
                        group = group.add(self.metadata_groups[idx].take().unwrap());
                    }
                    group = group
                        .set("fill", viz_color_to_value(self.overlay_style.fill))
                        .set("stroke", viz_color_to_value(self.overlay_style.stroke))
                        .set("stroke-width", self.overlay_style.stroke_width);
                    Some(group)
                }
                else if !metadata_sides.is_empty() {
                    // Only one side was rendered, so just return that side's group.
                    self.metadata_groups.into_iter().flatten().next()
                }
                else {
                    // No sides were rendered, so return None
                    None
                }
            };

            let overlay_group = {
                // Put both sides in a single document
                // First, check if we even rendered two sides:
                let mut overlay_sides = Vec::with_capacity(2);
                for (i, group) in self.overlay_groups.iter().enumerate() {
                    if group.is_some() {
                        overlay_sides.push(i);
                    }
                }

                if overlay_sides.len() > 1 {
                    let mut group = Group::new();
                    for idx in overlay_sides {
                        group = group.add(self.overlay_groups[idx].take().unwrap());
                    }
                    Some(group)
                }
                else if !overlay_sides.is_empty() {
                    // Only one side was rendered, so just return that side's group.
                    Some(self.overlay_groups[0].take().unwrap())
                }
                else {
                    // No sides were rendered, so return None
                    None
                }
            };

            log::trace!(
                "create_documents(): Got data layer?: {} Got metadata layer? {}.",
                data_group.is_some(),
                metadata_group.is_some()
            );

            if self.total_sides_to_render == 2 {
                // Expand the global view box to accommodate both sides, plus spacing and margin.

                // Calculate new width
                let new_box_width = self.side_view_box.width() * 2.0
                    + self.side_spacing
                    + self.global_margins.1
                    + self.global_margins.3;

                // Calculate new height
                let new_box_height = self.side_view_box.height() + self.global_margins.0 + self.global_margins.2;

                // Set global view box if isn't big enough
                if self.global_view_box.width() < new_box_width || self.global_view_box.height() < new_box_height {
                    self.global_view_box = VizRect::from_tuple((0.0, 0.0), (new_box_width, new_box_height));
                }
            }
            else {
                // Set the global view box to the side view box, plus margins.
                self.global_view_box = VizRect::from_tuple(
                    (0.0, 0.0),
                    (
                        self.side_view_box.width() + self.global_margins.1 + self.global_margins.3,
                        self.side_view_box.height() + self.global_margins.0 + self.global_margins.2,
                    ),
                );
            }

            // We may now have a data group and a metadata group. If layered rendering is enabled,
            // combine them into the same document.
            if self.render_layered {
                log::trace!(
                    "Rendering metadata layer on top of data layer with blend mode: {:?}",
                    self.layer_blend_mode
                );
                let mut document = Document::new().set("viewBox", self.global_view_box.to_tuple());

                if let Some(group) = data_group {
                    document = document.add(group);
                }
                if let Some(group) = metadata_group {
                    document = document.add(group);
                }
                if let Some(group) = overlay_group {
                    document = document.add(group);
                }

                output_documents.push(RenderedDocument {
                    side: DocumentSide::Both,
                    layer: DocumentLayer::Composite,
                    document,
                });
            }
            else {
                log::trace!("Rendering data and metadata layers in separate documents.");
                // If layered rendering is disabled, we need to create separate documents for each
                // layer. We'll start with the data layer.
                if let Some(group) = data_group {
                    let mut document = Document::new().set("viewBox", self.global_view_box.to_tuple());
                    document = document.add(group);
                    output_documents.push(RenderedDocument {
                        side: DocumentSide::Both,
                        layer: DocumentLayer::Data,
                        document,
                    });
                }

                // Now we'll create a document for the metadata layer.
                if let Some(group) = metadata_group {
                    let mut document = Document::new().set("viewBox", self.global_view_box.to_tuple());
                    document = document.add(group);
                    output_documents.push(RenderedDocument {
                        side: DocumentSide::Both,
                        layer: DocumentLayer::Metadata,
                        document,
                    });
                }
            }
        }
        else {
            // We're rendering two sides in separate documents.
            log::warn!("Rendering two sides in separate documents is not yet supported.");
            println!("Rendering two sides in separate documents is not yet supported.");
        }

        log::debug!("create_documents(): Created {} documents.", output_documents.len());
        Ok(output_documents)
    }
}
