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
use crate::disk_selection::{DiskSelection, SelectionLevel};

use crate::{
    components::metadata_header::{MetaDataHeader, MetaDataType},
    widget::{FoxWidget, ScrollableWidget, TabSelectableWidget, WidgetState},
};
use anyhow::{anyhow, Error};
use fluxfox::prelude::*;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Scrollbar, ScrollbarOrientation, ScrollbarState, WidgetRef},
};
use std::cell::RefCell;

#[derive(Clone, Debug)]
pub enum DataToken {
    Padding(u16),
    HexAddress(u16),
    DataByte { byte: u8, last: bool, wrapping: bool },
    AddressMarker(u8),
}

#[derive(Default, Copy, Clone, Debug)]
pub enum DataBlockType {
    #[default]
    Track,
    Sector,
}

pub struct DataBlock {
    pub caption: String,
    pub block_type: DataBlockType,
    pub cylinder: u16,
    pub head: u8,
    pub sector: Option<u8>,
    pub columns: usize,
    pub rows: usize,
    pub data: Vec<u8>,
    pub data_context_len: usize,
    pub formatted_lines: Vec<Vec<DataToken>>,
    pub visible_rows: usize,
    pub scroll_offset: usize,
    pub vertical_scroll_state: ScrollbarState,
    pub tab_selected: bool,
    pub data_header: MetaDataHeader,
    pub ui_state: RefCell<WidgetState>,
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
            data_context_len: 0,
            formatted_lines: Vec::new(),
            visible_rows: 0,
            scroll_offset: 0,
            vertical_scroll_state: ScrollbarState::default(),
            tab_selected: false,
            data_header: MetaDataHeader::new(MetaDataType::Track),
            ui_state: RefCell::new(WidgetState::default()),
        }
    }
}

impl DataBlock {
    pub fn load(&mut self, disk: &mut DiskImage, selection: &DiskSelection) -> Result<(), Error> {
        self.block_type = match selection.level() {
            SelectionLevel::Cylinder => DataBlockType::Track,
            SelectionLevel::Sector => DataBlockType::Sector,
            _ => return Err(anyhow!("Invalid selection level")),
        };

        match self.block_type {
            DataBlockType::Track => {
                let ch = selection.into_ch()?;
                let rtr = disk.read_track(ch, None)?;
                let ti = disk.track(ch).ok_or(anyhow!("Track not found"))?.info();

                log::debug!("load(): read_track() returned {} bytes", rtr.read_buf.len());
                if rtr.read_buf.is_empty() {
                    return Err(anyhow!("No data read"));
                }

                self.head = ch.h();
                self.cylinder = ch.c();
                self.sector = None;

                self.data_header = MetaDataHeader::new(MetaDataType::Track);
                self.data_header.set_key_good("Encoding", ti.encoding.to_string());
                self.data_header.set_key_good("Bit Length", ti.bit_length.to_string());
                self.data_header.set_key_good("Bitrate", ti.data_rate.to_string());

                self.set_caption(&format!("Track: {}", ch));

                self.scroll_offset = 0;

                log::debug!("first byte of track is {:02X}", rtr.read_buf[0]);
                self.update_data(rtr.read_buf, rtr.read_len_bytes);
            }
            DataBlockType::Sector => {
                let ch = selection.into_ch()?;
                let chs = selection.into_chs()?;

                let rsr = disk.read_sector(
                    ch,
                    DiskChsnQuery::new(chs.c(), chs.h(), chs.s(), None),
                    None,
                    None,
                    RwScope::DataOnly,
                    true,
                )?;

                self.head = ch.h();
                self.cylinder = ch.c();
                self.sector = selection.sector;

                self.data_header = MetaDataHeader::new(MetaDataType::Sector);
                self.data_header
                    .set_key_good("Sector ID", rsr.id_chsn.unwrap_or_default().to_string());
                self.data_header
                    .set_key_good("Address CRC Valid", (!rsr.address_crc_error).to_string());
                self.data_header
                    .set_key_good("Data: CRC Valid", (!rsr.data_crc_error).to_string());

                self.set_caption(&format!("Sector: {}", chs));

                self.scroll_offset = 0;
                let read_buf_len = rsr.read_buf.len();
                self.update_data(rsr.read_buf, read_buf_len);
            }
        }

        Ok(())
    }

    pub fn set_metadata(&mut self, metadata: MetaDataHeader) {
        self.data_header = metadata;
    }

    //noinspection RsExternalLinter
    pub fn get_line(&self, index: usize) -> Option<Line> {
        if index >= self.formatted_lines.len() {
            return None;
        }

        let wrap = match self.block_type {
            DataBlockType::Track => true,
            DataBlockType::Sector => false,
        };

        let line_vec = self.formatted_lines.get(index)?;

        let mut line = Line::default();

        let mut byte_count = 0;
        let mut last_token_wrapped = false;
        for token in line_vec {
            match token {
                DataToken::HexAddress(addr) => {
                    // Use blue style
                    line.spans
                        .push(Span::styled(format!("{:04X}", addr), Style::default().fg(Color::Cyan)));
                    line.spans
                        .push(Span::styled(" |", Style::default().fg(Color::DarkGray)));
                }
                DataToken::DataByte { byte, last, wrapping } => {
                    let mut style = Style::default();

                    style = if *last { style.underlined() } else { style };
                    style = if *wrapping { style.fg(Color::DarkGray) } else { style };

                    let mut pad_style = if byte_count == 0 { Style::default() } else { style };

                    let pad_char = if byte_count > 0 && *wrapping && !last_token_wrapped {
                        pad_style = Style::default();
                        "|"
                    }
                    else {
                        " "
                    };

                    line.spans.push(Span::styled(pad_char, pad_style));

                    line.spans.push(Span::styled(format!("{:02X}", byte), style));
                    last_token_wrapped = *wrapping;
                    byte_count += 1;
                }
                DataToken::Padding(size) => {
                    line.spans
                        .push(Span::styled(" ".repeat((*size + 1) as usize), Style::default()));
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
                DataToken::DataByte { byte, .. } => {
                    let ascii_span = if byte.is_ascii_graphic() {
                        Span::styled(format!("{}", *byte as char), Style::default())
                    }
                    else {
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

    pub fn update_data(&mut self, data: Vec<u8>, data_context_len: usize) {
        self.data = data;
        self.data_context_len = data_context_len;
        self.format(); // Reformat the data when it changes
    }

    /// Formats the data into vectors of tokens
    fn format(&mut self) {
        let wrap = match self.block_type {
            DataBlockType::Track => true,
            DataBlockType::Sector => false,
        };

        self.formatted_lines.clear();
        let bytes_per_line = 16;

        let data_partial_row_len = self.data.len() % bytes_per_line;

        let dump_partial_row_len = self.data_context_len % bytes_per_line;
        let data_full_rows = self.data_context_len / bytes_per_line;
        let data_rows = data_full_rows + if data_partial_row_len > 0 { 1 } else { 0 };
        let dump_rows = self.data.len() / bytes_per_line + if dump_partial_row_len > 0 { 1 } else { 0 };

        log::debug!(
            "partial row len: {} last full data row {:04X}",
            data_partial_row_len,
            data_full_rows * bytes_per_line
        );

        let mut previous_row = 0;

        let mut line_iterator = self.data.chunks(bytes_per_line).peekable();

        // Format whole lines of data context
        for (row, chunk) in line_iterator.by_ref().take(data_full_rows).enumerate() {
            // Format hex address

            let mut token_vec = Vec::new();
            let addr = DataToken::HexAddress((row * bytes_per_line) as u16);

            token_vec.push(addr);

            let second_to_last_row = data_partial_row_len > 0 && row == data_full_rows - 1;
            //let last_row = row == data_even_rows - 1;

            for (bi, byte) in chunk.iter().enumerate() {
                // Format data byte
                let mut mark_last_row = false;
                if second_to_last_row && bi >= data_partial_row_len {
                    mark_last_row = true;
                }

                let data_byte = DataToken::DataByte {
                    byte: *byte,
                    last: mark_last_row,
                    wrapping: false,
                };
                token_vec.push(data_byte);
            }

            // if chunk.len() < bytes_per_line {
            //     // Pad the line with spaces
            //     let pad_len = bytes_per_line - chunk.len();
            //     for _ in 0..pad_len {
            //         token_vec.push(DataToken::Padding(2));
            //     }
            // }

            previous_row = row;
            self.formatted_lines.push(token_vec);
        }

        // Format any remaining partial line of data context
        if let Some(incomplete_line) = line_iterator.next() {
            if data_partial_row_len > 0 {
                let mut token_vec = Vec::new();

                let addr = DataToken::HexAddress(((previous_row + 1) * bytes_per_line) as u16);
                token_vec.push(addr);

                // Add data bytes
                for di in 0..data_partial_row_len {
                    token_vec.push(DataToken::DataByte {
                        byte: incomplete_line[di],
                        last: true,
                        wrapping: false,
                    });
                }

                if incomplete_line.len() == bytes_per_line {
                    // Add wrapping bytes to end of line
                    for di in data_partial_row_len..bytes_per_line {
                        if wrap {
                            token_vec.push(DataToken::DataByte {
                                byte: incomplete_line[di],
                                last: false,
                                wrapping: true,
                            });
                        }
                        else {
                            token_vec.push(DataToken::Padding(2));
                        }
                    }
                }

                self.formatted_lines.push(token_vec);
            }
        }

        // if wrap {
        //     // Draw wrapped full rows
        //     for (wrap_row, chunk) in self.data[wrapping_idx..].chunks(bytes_per_line).enumerate() {
        //         let mut token_vec = Vec::new();
        //         let addr = DataToken::HexAddress(((last_row + wrap_row + 1) * bytes_per_line) as u16);
        //
        //         token_vec.push(addr);
        //         for (bi, byte) in chunk.iter().enumerate() {
        //             // Format data byte
        //             let data_byte = DataToken::DataByte {
        //                 byte: *byte,
        //                 last: false,
        //                 wrapping: true,
        //             };
        //             token_vec.push(data_byte);
        //         }
        //         self.formatted_lines.push(token_vec);
        //
        //         if wrap_row > 4 {
        //             break;
        //         }
        //     }
        // }
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

    fn render_ref_internal(&self, area: Rect, buf: &mut Buffer) {
        let mut state = self.ui_state.borrow_mut();

        // Render a border around the widget
        let border_style = if self.tab_selected {
            Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)
        }
        else {
            Style::default()
        };

        let mut title_line = Line::from(vec![Span::styled("Data Block", Style::default())]);

        if !self.caption.is_empty() {
            title_line.push_span(Span::styled(format!(": {}", self.caption), Style::default()));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title_line);

        let inner = block.inner(area);
        block.render(area, buf);

        let panel_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(self.data_header.rows()), // for metadata header
                    Constraint::Min(1),                          // for hex display
                ]
                .as_ref(),
            )
            .split(inner);

        // Render the metadata header.
        self.data_header.render_ref(panel_layout[0], buf);

        // Calculate the inner area for rendering the hex dump
        if panel_layout[1].width < self.required_width() || panel_layout[1].height == 0 {
            return; // Not enough space to render
        }

        // Determine the number of visible rows and total rows
        let total_rows = self.formatted_lines.len();
        let visible_rows = panel_layout[1].height as usize;
        state.visible_rows = visible_rows;

        // Render the hex dump.
        let rows_to_render = visible_rows.min(total_rows.saturating_sub(self.scroll_offset));
        for i in 0..rows_to_render {
            let line_index = self.scroll_offset + i;
            if let Some(line) = self.get_line(line_index) {
                // Render the line at the appropriate position
                buf.set_line(panel_layout[1].x, panel_layout[1].y + i as u16, &line, inner.width);
            }
        }

        // let scrollbar_height = (visible_rows as f64 / total_rows as f64 * inner.height as f64).ceil() as u16;
        // let scrollbar_position = ((self.scroll_offset as f64 / total_rows as f64)
        //     * (inner.height as f64 - scrollbar_height as f64))
        //     .ceil() as u16;

        // Update scrollbar content
        let mut vertical_scroll_state = self
            .vertical_scroll_state
            .content_length(total_rows)
            .position(self.scroll_offset);

        // Render a scrollbar if needed
        if total_rows > visible_rows {
            // Calculate scrollbar size and position

            // Render a scrollbar using the ScrollBar widget if needed
            if total_rows > visible_rows {
                // Create a ScrollBar widget
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight) // Vertical on the right side
                    .thumb_symbol("█")
                    .track_style(Style::default().fg(Color::Cyan))
                    .track_symbol(Some("|"));

                // Render the scrollbar
                scrollbar.render(panel_layout[1], buf, &mut vertical_scroll_state);
            }
        }
    }
}

impl FoxWidget for DataBlock {}

impl TabSelectableWidget for DataBlock {
    fn can_select(&self) -> bool {
        true
    }
    fn select(&mut self) {
        self.tab_selected = true;
    }
    fn deselect(&mut self) {
        self.tab_selected = false;
    }
}

impl ScrollableWidget for DataBlock {
    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }
    fn scroll_down(&mut self) {
        self.scroll_offset += 1;
        if self.scroll_offset >= self.formatted_lines.len() {
            self.scroll_offset = self.formatted_lines.len() - 1;
        }
    }
    fn page_up(&mut self) {
        let state = self.ui_state.borrow();
        self.scroll_offset = self.scroll_offset.saturating_sub(state.visible_rows);
    }
    fn page_down(&mut self) {
        let state = self.ui_state.borrow();
        //self.scroll_offset = (self.scroll_offset).min(self.formatted_lines.len() - 1);
        self.scroll_offset = (self.scroll_offset + state.visible_rows).min(self.formatted_lines.len() - 1);
        // log::debug!(
        //     "page_down(): scroll_offset: {} visible: {}",
        //     self.scroll_offset,
        //     state.visible_rows
        // );
    }
    fn scroll_to_start(&mut self) {
        self.scroll_offset = 0;
    }
    fn scroll_to_end(&mut self) {
        self.scroll_offset = self.formatted_lines.len() - 1;
    }
}

impl WidgetRef for &DataBlock {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.render_ref_internal(area, buf);
    }
}

impl WidgetRef for DataBlock {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.render_ref_internal(area, buf);
    }
}
