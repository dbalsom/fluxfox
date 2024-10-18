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
use crate::app::AppContext;
use crate::disk_selection::{DiskSelection, SelectionLevel};
use crate::history::HistoryWidget;
use crate::widget::{FoxWidget, TabSelectableWidget};
use anyhow::{anyhow, Error};
use fluxfox::{DiskCh, DiskImage};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Scrollbar, ScrollbarOrientation, ScrollbarState};
use std::ops::Add;

#[derive(Clone, Debug)]
pub enum DataToken {
    HexAddress(u16),
    DataByte(u8),
    AddressMarker(u8),
}

#[derive(Default, Copy, Clone, Debug)]
pub enum DataBlockType {
    #[default]
    Track,
    Sector,
}

#[derive(Clone)]
pub struct DataBlock {
    pub caption: String,
    pub block_type: DataBlockType,
    pub cylinder: u16,
    pub head: u8,
    pub sector: Option<u8>,
    pub columns: usize,
    pub rows: usize,
    pub data: Vec<u8>,
    pub formatted_lines: Vec<Vec<DataToken>>,
    pub scroll_offset: usize,

    pub vertical_scroll_state: ScrollbarState,
    pub tab_selected: bool,
}

impl Default for DataBlock {
    fn default() -> Self {
        DataBlock {
            caption: String::new(),
            block_type: DataBlockType::Track,
            cylinder: 0,
            head: 0,
            sector: None,
            columns: 16,
            rows: 0,
            data: Vec::new(),
            formatted_lines: Vec::new(),
            scroll_offset: 0,

            vertical_scroll_state: ScrollbarState::default(),
            tab_selected: false,
        }
    }
}

impl DataBlock {
    pub fn load(&mut self, disk: &mut DiskImage, selection: &DiskSelection) -> Result<(), Error> {
        let block_type = match selection.level() {
            SelectionLevel::Cylinder => DataBlockType::Track,
            SelectionLevel::Sector => DataBlockType::Sector,
            _ => return Err(anyhow!("Invalid selection level")),
        };

        match block_type {
            DataBlockType::Track => {
                let ch = selection.into_ch()?;
                let rtr = disk.read_track(ch)?;

                log::debug!("load(): read_track() returned {} bytes", rtr.read_buf.len());
                if rtr.read_buf.is_empty() {
                    return Err(anyhow!("No data read"));
                }

                self.head = ch.h();
                self.cylinder = ch.c();
                self.sector = None;

                self.update_data(rtr.read_buf);
            }
            DataBlockType::Sector => {
                // self.track = ctx.selection.track;
                // self.head = ctx.selection.head;
                // self.sector = ctx.selection.sector;
            }
        }

        Ok(())
    }

    pub fn get_line(&self, index: usize) -> Option<Line> {
        if index >= self.formatted_lines.len() {
            return None;
        }

        let line_vec = self.formatted_lines.get(index)?;

        let mut line = Line::default();

        for token in line_vec {
            match token {
                DataToken::HexAddress(addr) => {
                    // Use blue style
                    line.spans
                        .push(Span::styled(format!("{:04X}", addr), Style::default().fg(Color::Cyan)));
                    line.spans
                        .push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                }
                DataToken::DataByte(byte) => {
                    line.spans
                        .push(Span::styled(format!("{:02X} ", byte), Style::default()));
                }
                DataToken::AddressMarker(byte) => {
                    line.spans.push(Span::styled(
                        format!("{:02X}", byte),
                        Style::default().add_modifier(Modifier::REVERSED),
                    ));
                }
            }
        }

        // 2nd pass to add DataBytes as ascii representation
        line.spans
            .push(Span::styled("| ", Style::default().fg(Color::DarkGray)));
        for token in line_vec {
            match token {
                DataToken::DataByte(byte) => {
                    let ascii_span = if byte.is_ascii_graphic() {
                        Span::styled(format!("{}", *byte as char), Style::default())
                    } else {
                        Span::styled(".", Style::default().fg(Color::Gray))
                    };
                    line.spans.push(ascii_span);
                }
                _ => {}
            }
        }

        Some(line)
    }

    pub fn set_caption(&mut self, caption: &str) {
        self.caption = caption.to_string();
    }

    pub fn update_data(&mut self, data: Vec<u8>) {
        self.data = data;
        self.format(); // Reformat the data when it changes
    }

    /// Formats the data into hex and ASCII lines
    fn format(&mut self) {
        self.formatted_lines.clear();
        let bytes_per_line = 16;

        for (row, chunk) in self.data.chunks(bytes_per_line).enumerate() {
            // Format hex address

            let mut token_vec = Vec::new();
            let addr = DataToken::HexAddress((row * bytes_per_line) as u16);

            token_vec.push(addr);

            for byte in chunk.iter() {
                // Format data byte
                let data_byte = DataToken::DataByte(*byte);
                token_vec.push(data_byte);
            }

            self.formatted_lines.push(token_vec);
        }
    }

    pub fn required_width(&self) -> u16 {
        // 4 hex digits + space + pipe + space
        7 +
            // column * (2 hex digits + 1 space)
        self.columns as u16 * 3 +
            // space + pipe + space
        3 +
            // column * 1 ASCII char
        self.columns as u16
    }
}

impl FoxWidget for DataBlock {}

impl TabSelectableWidget for DataBlock {
    fn select(&mut self) {
        self.tab_selected = true;
    }
    fn deselect(&mut self) {
        self.tab_selected = false;
    }
}

impl Widget for DataBlock {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // Render a border around the widget
        let border_style = if self.tab_selected {
            Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let block = if self.caption.is_empty() {
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled("Data Block", Style::default()))
        } else {
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(format!("Data Block: {}", self.caption), Style::default()))
        };
        let inner = block.inner(area);
        block.render(area, buf);

        // Calculate the inner area for rendering the hex dump
        //log::debug!("inner width: {}", inner.width);
        if inner.width < self.required_width() || inner.height == 0 {
            return; // Not enough space to render
        }

        // Determine the number of visible rows and total rows
        let total_rows = self.formatted_lines.len();
        let visible_rows = inner.height as usize;
        let rows_to_render = visible_rows.min(total_rows.saturating_sub(self.scroll_offset));

        for i in 0..rows_to_render {
            let line_index = self.scroll_offset + i;
            if let Some(line) = self.get_line(line_index) {
                // Render the line at the appropriate position

                buf.set_line(inner.x, inner.y + i as u16, &line, inner.width);
            }
        }

        // Update scrollbar content
        self.vertical_scroll_state = self.vertical_scroll_state.content_length(total_rows);

        // Render a scrollbar if needed
        if total_rows > visible_rows {
            // Calculate scrollbar size and position
            let scrollbar_height = (visible_rows as f64 / total_rows as f64 * inner.height as f64).ceil() as u16;
            let scrollbar_position = ((self.scroll_offset as f64 / total_rows as f64)
                * (inner.height as f64 - scrollbar_height as f64))
                .ceil() as u16;

            // Render a scrollbar using the ScrollBar widget if needed
            if total_rows > visible_rows {
                // Create a ScrollBar widget
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight) // Vertical on the right side
                    .thumb_symbol("░")
                    .track_symbol(Some("|"));

                // Render the scrollbar
                scrollbar.render(inner, buf, &mut self.vertical_scroll_state);
            }
        }
    }
}
