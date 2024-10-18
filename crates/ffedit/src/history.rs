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
use crate::logger::LogEntry;
use crate::widget::{FoxWidget, TabSelectableWidget};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use std::cmp;
use std::collections::VecDeque;

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

#[derive(Clone)]
pub(crate) struct HistoryWidget {
    pub(crate) max_len: usize,                  // Maximum length of the history
    pub(crate) history: VecDeque<HistoryEntry>, // Store history as Vec<HistoryEntry>

    pub vertical_scroll_state: ScrollbarState,
    pub scroll_pos: usize,
    pub tab_selected: bool,
}

impl HistoryWidget {
    pub fn new(max_len: Option<usize>) -> HistoryWidget {
        HistoryWidget {
            max_len: max_len.unwrap_or(MAX_HISTORY),
            history: VecDeque::with_capacity(max_len.unwrap_or(MAX_HISTORY)),
            vertical_scroll_state: ScrollbarState::default(),
            scroll_pos: 0,
            tab_selected: false,
        }
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll_pos = self.history.len();
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
        self.push(HistoryEntry::CommandResponse(response.to_string()));
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
}

impl FoxWidget for HistoryWidget {}

impl TabSelectableWidget for HistoryWidget {
    fn select(&mut self) {
        self.tab_selected = true;
    }
    fn deselect(&mut self) {
        self.tab_selected = false;
    }
}

impl Widget for HistoryWidget {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // Render a border around the widget
        let border_style = if self.tab_selected {
            Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let block = Block::default().borders(Borders::ALL).border_style(border_style);
        // Determine the number of visible rows and total rows
        let inner = block.inner(area);
        let total_rows = self.history.len();
        let visible_rows = inner.height as usize;

        // Update scrollbar content
        let scroll_pos = cmp::min(self.scroll_pos, total_rows.saturating_sub(visible_rows));
        let title = format!("History [{}/{}]", scroll_pos, self.history.len());

        block.title(title).render(area, buf);

        self.vertical_scroll_state = self
            .vertical_scroll_state
            .content_length(total_rows)
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
                    .thumb_symbol("░")
                    .track_symbol(Some("|"));

                // Render the scrollbar
                scrollbar.render(inner, buf, &mut self.vertical_scroll_state);
            }
        }
    }
}
