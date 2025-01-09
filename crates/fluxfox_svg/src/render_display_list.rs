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

use crate::{render_elements::*, styles::ElementStyle};

use fluxfox::{
    track_schema::GenericTrackElement,
    visualization::{
        prelude::VizRect,
        types::display_list::{VizDataSliceDisplayList, VizElementDisplayList},
    },
    FoxHashMap,
};
use svg::node::element::Group;

pub fn render_display_list_as_svg(
    viewbox: VizRect<f32>,
    angle: f32,
    display_list: &VizElementDisplayList,
    track_style: &ElementStyle,
    element_styles: &FoxHashMap<GenericTrackElement, ElementStyle>,
) -> Group {
    let center = viewbox.center();
    let angle_degrees = angle.to_degrees();

    // Rotate around the center of the viewbox
    let mut group = Group::new().set(
        "transform",
        format!("rotate({} {:.3} {:.3})", angle_degrees, center.x, center.y),
    );

    for element in display_list.iter() {
        let node = svg_render_element(element, track_style, element_styles);
        match node {
            RenderNode::Path(path) => {
                group = group.add(path);
            }
            RenderNode::Circle(circle) => {
                group = group.add(circle);
            }
        }
    }

    group
}

pub fn render_data_display_list_as_svg(
    viewbox: VizRect<f32>,
    angle: f32,
    display_list: &VizDataSliceDisplayList,
) -> Group {
    let center = viewbox.center();
    let angle_degrees = angle.to_degrees();

    // Rotate around the center of the viewbox
    let mut group = Group::new().set(
        "transform",
        format!("rotate({} {:.3} {:.3})", angle_degrees, center.x, center.y),
    );

    for slice in display_list.iter() {
        let path = svg_render_data_slice(slice, display_list.track_width);
        group = group.add(path);
    }

    // Turn off antialiasing to avoid moiré artifacts
    group.set("shape-rendering", "crispEdges")
}
