/*
    ffedit
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
use crate::{
    logger::LogEntry,
    widget::{FoxWidget, ScrollableWidget, TabSelectableWidget, WidgetState},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, WidgetRef},
};
use std::{cell::RefCell, cmp, collections::VecDeque};

// Define an enum for the history entries
#[derive(Clone)]
pub(crate) enum HistoryEntry {
    UserCommand(String),
    CommandResponse(String),
    Info(String),
    Trace(String),
    Debug(String),
    Warning(String),
    Error(String),
}

pub(crate) const MAX_HISTORY: usize = 1000; // Maximum number of history entries

pub(crate) struct HistoryWidget {
    pub(crate) max_len: usize,                  // Maximum length of the history
    pub(crate) history: VecDeque<HistoryEntry>, // Store history as Vec<HistoryEntry>
    pub scroll_offset: usize,
    pub vertical_scroll_state: ScrollbarState,
    pub tab_selected: bool,
    pub ui_state: RefCell<WidgetState>,
}

impl HistoryWidget {
    pub fn new(max_len: Option<usize>) -> HistoryWidget {
        HistoryWidget {
            max_len: max_len.unwrap_or(MAX_HISTORY),
            history: VecDeque::with_capacity(max_len.unwrap_or(MAX_HISTORY)),
            scroll_offset: 0,
            vertical_scroll_state: ScrollbarState::default(),
            tab_selected: false,
            ui_state: RefCell::new(WidgetState::default()),
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.history.push_back(entry); // Push a new entry onto the history
        if self.history.len() > MAX_HISTORY {
            _ = self.history.pop_front(); // Remove the oldest entry if the history is too long
        }
        self.scroll_to_end();
    }

    pub fn push_user_cmd(&mut self, cmd: &str) {
        self.push(HistoryEntry::UserCommand(cmd.to_string()));
    }

    pub fn push_cmd_response(&mut self, response: &str) {
        let response_lines = response.split("\n");
        for line in response_lines {
            self.push(HistoryEntry::CommandResponse(line.to_string()));
        }
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        match entry {
            LogEntry::Info(msg) => self.push(HistoryEntry::Info(msg)),
            LogEntry::Trace(msg) => self.push(HistoryEntry::Trace(msg)),
            LogEntry::Debug(msg) => self.push(HistoryEntry::Debug(msg)),
            LogEntry::Warning(msg) => self.push(HistoryEntry::Warning(msg)),
            LogEntry::Error(msg) => self.push(HistoryEntry::Error(msg)),
        }
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

        // Update scrollbar content
        let block = Block::default().borders(Borders::ALL).border_style(border_style);

        let inner = block.inner(area);
        // Determine the number of visible rows and total rows
        let total_rows = self.history.len();
        let visible_rows = inner.height as usize;
        state.visible_rows = visible_rows;
        let scroll_pos = cmp::min(self.scroll_offset, total_rows.saturating_sub(visible_rows));
        let title = format!("History [{}/{}]", scroll_pos, self.history.len());
        block.title(title).render(area, buf);

        let mut vertical_scroll_state = self
            .vertical_scroll_state
            .content_length(total_rows.saturating_sub(visible_rows))
            .viewport_content_length(1)
            .position(scroll_pos);

        let visible_history: Vec<Line> = self
            .history
            .iter()
            .skip(scroll_pos)
            .map(|entry| match entry {
                HistoryEntry::UserCommand(cmd) => Line::from(Span::styled(format!("> {}", cmd), Style::default())),
                HistoryEntry::CommandResponse(resp) => {
                    Line::from(Span::styled(resp.clone(), Style::default().fg(Color::Cyan)))
                }
                HistoryEntry::Trace(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::DarkGray))),
                HistoryEntry::Debug(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Green))),
                HistoryEntry::Info(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::White))),
                HistoryEntry::Warning(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Yellow))),
                HistoryEntry::Error(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Red))),
            })
            .collect();

        let history_paragraph = Paragraph::new(visible_history);

        history_paragraph.render(inner, buf);

        // Render a scrollbar if needed
        if total_rows > visible_rows {
            // if true {
            // Calculate scrollbar size and position
            let scrollbar_height = (visible_rows as f64 / total_rows as f64 * inner.height as f64).ceil() as u16;
            let scrollbar_position = ((scroll_pos as f64 / total_rows as f64)
                * (inner.height as f64 - scrollbar_height as f64))
                .ceil() as u16;

            // Render a scrollbar using the ScrollBar widget if needed
            if total_rows > visible_rows {
                // Create a ScrollBar widget
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight) // Vertical on the right side
                    .thumb_symbol("█")
                    .track_style(Style::default().fg(Color::Cyan))
                    .track_symbol(Some("|"));

                // Render the scrollbar
                scrollbar.render(inner, buf, &mut vertical_scroll_state);
            }
        }
    }
}

impl FoxWidget for HistoryWidget {}

impl ScrollableWidget for HistoryWidget {
    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }
    fn scroll_down(&mut self) {
        self.scroll_offset += 1;
        if self.scroll_offset >= self.history.len() {
            self.scroll_offset = self.history.len() - 1;
        }
    }
    fn page_up(&mut self) {
        let state = self.ui_state.borrow();
        self.scroll_offset = self.scroll_offset.saturating_sub(state.visible_rows);
    }
    fn page_down(&mut self) {
        let state = self.ui_state.borrow();
        self.scroll_offset = (self.scroll_offset + state.visible_rows).min(self.history.len() - 1);
    }
    fn scroll_to_start(&mut self) {
        self.scroll_offset = 0;
    }
    fn scroll_to_end(&mut self) {
        self.scroll_offset = self.history.len();
    }
}

impl TabSelectableWidget for HistoryWidget {
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

impl WidgetRef for &HistoryWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.render_ref_internal(area, buf);
    }
}

impl WidgetRef for HistoryWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.render_ref_internal(area, buf);
    }
}
