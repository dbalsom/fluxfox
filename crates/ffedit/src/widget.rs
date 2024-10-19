/*
    ffedit
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

use crate::components::metadata_header::MetaDataHeader;
use ratatui::widgets::{ScrollbarState, WidgetRef};

pub trait TabSelectableWidget {
    fn can_select(&self) -> bool;
    fn select(&mut self);
    fn deselect(&mut self);
}

pub trait ScrollableWidget {
    fn scroll_up(&mut self);
    fn scroll_down(&mut self);
    fn page_up(&mut self);
    fn page_down(&mut self);
}

pub trait HasMetaDataHeader {
    fn set_header(&mut self, header: MetaDataHeader);
}

#[derive(Default)]
pub struct WidgetState {
    pub visible_rows: usize,
    pub vertical_scroll_state: ScrollbarState,
}

pub trait FoxWidget: WidgetRef + TabSelectableWidget + ScrollableWidget {}
