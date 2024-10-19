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
use indexmap::IndexMap;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::widgets::{Block, WidgetRef};

#[derive(Default)]
pub enum MetaDataType {
    #[default]
    Track,
    Sector,
}

pub enum MetaDataItem {
    Null,
    Good(String),
    Bad(String),
}

pub struct MetaDataHeader {
    pub map: IndexMap<String, Vec<MetaDataItem>>,
}

impl MetaDataHeader {
    pub fn new(dh_type: MetaDataType) -> MetaDataHeader {
        
        match dh_type {
            MetaDataType::Sector => {
                let mut map = IndexMap::new();
                map.insert("Sector ID".to_string(), Vec::new());

                MetaDataHeader { map }
            }
            MetaDataType::Track => {
                let mut map = IndexMap::new();
                map.insert("Encoding".to_string(), Vec::new());
                map.insert("Bit Length".to_string(), Vec::new());
                map.insert("Bitrate".to_string(), Vec::new());
                MetaDataHeader { map }
            }
        }
    }

    pub fn set_key_good(&mut self, key: &str, value: String) {
        self.map.insert(key.to_string(), vec![MetaDataItem::Good(value)]);
    }

    pub fn set_key(&mut self, key: &str, value: MetaDataItem) {
        self.map.insert(key.to_string(), vec![value]);
    }

    pub fn insert_key(&mut self, key: &str, value: MetaDataItem) {
        self.map.entry(key.to_string()).or_insert_with(Vec::new).push(value);
    }

    pub fn rows(&self) -> u16 {
        self.map.len() as u16
    }

    /// Returns the maximum length of a key name in the metadata header.
    /// 1 character is included for the colon.
    pub fn max_key_len(&self) -> usize {
        self.map.keys().map(|k| k.len() + 1).max().unwrap_or(0)
    }
}

impl WidgetRef for MetaDataHeader {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let mut y = area.top();

        let style = Style::from((Color::White, Color::DarkGray));
        let block = Block::default().style(style);
        block.render(area, buf);

        for (key, value) in &self.map {
            // Set key style to white text on dark blue background
            let key_style = style;

            let key_pad = self.max_key_len() - key.len();
            let key_str = format!("{}:{:width$}", key, "", width = key_pad);
            let mut x = area.left() + key_str.len() as u16;
            buf.set_string(area.left(), y, key_str, key_style);
            for item in value {
                match item {
                    MetaDataItem::Null => {}
                    MetaDataItem::Good(s) => {
                        buf.set_string(x, y, s, key_style);
                        x += s.len() as u16;
                    }
                    MetaDataItem::Bad(s) => {
                        buf.set_string(x, y, s, key_style);
                        x += s.len() as u16;
                    }
                }
            }
            y += 1;
        }
    }
}
