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
mod cmd_interpreter;
mod layout;
mod modal;

use core::fmt;
use std::fmt::Display;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bpaf::{construct, short, OptionParser, Parser};
use crossbeam_channel::{Receiver, Sender};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::widgets::Gauge;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use tui_popup::{Popup, SizedWrapper};

use crate::cmd_interpreter::CommandInterpreter;
use crate::modal::ModalState;
use cmd_interpreter::CommandResult;
use fluxfox::DiskImage;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct CmdParams {
    in_filename: PathBuf,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<CmdParams> {
    let in_filename = short('i')
        .long("in_filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("IN_FILE");

    construct!(CmdParams { in_filename }).to_options()
}

// Application state to support different modes
#[derive(Default)]
enum ApplicationState {
    #[default]
    Normal,
    Modal(ModalState),
}

// Define an enum for the history entries
enum HistoryEntry {
    UserCommand(String),
    CommandResponse(String),
}

#[derive(Default)]
pub struct DiskSelection {
    pub level: SelectionLevel,
    pub head: Option<u8>,
    pub cylinder: Option<u16>,
    pub sector: Option<u8>,
}

impl Display for DiskSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> fmt::Result {
        match self.level {
            SelectionLevel::Disk => write!(f, ""),
            SelectionLevel::Head => write!(f, "[h:{}]", self.head.unwrap_or(0)),
            SelectionLevel::Track => write!(f, "[h:{} c:{}]", self.head.unwrap_or(0), self.cylinder.unwrap_or(0)),
            SelectionLevel::Sector => write!(
                f,
                "[h:{} c:{} s:{}]",
                self.head.unwrap_or(0),
                self.cylinder.unwrap_or(0),
                self.sector.unwrap_or(0)
            ),
        }
    }
}

/// Track the selection level
#[derive(Default)]
pub enum SelectionLevel {
    #[default]
    Disk,
    Head,
    Track,
    Sector,
}

const MAX_HISTORY: usize = 1000; // Maximum number of history entries

pub enum AppThreadMessage {
    LoadingStatus(f32),
    DiskImageLoaded(DiskImage),
}

// Contain mutable data for App
// This avoids borrowing issues when passing the mutable context to the command processor
struct AppContext {
    selection: DiskSelection,
    state: ApplicationState,
    di: Option<DiskImage>,
    sender: Sender<AppThreadMessage>,
}

struct App {
    input: String,
    ci: CommandInterpreter,
    history: Vec<HistoryEntry>, // Store history as Vec<HistoryEntry>

    receiver: Receiver<AppThreadMessage>,
    ctx: AppContext,
}

impl App {
    fn new() -> App {
        let (sender, receiver) = crossbeam_channel::unbounded::<AppThreadMessage>();

        App {
            input: String::new(),
            ci: CommandInterpreter::new(),
            history: Vec::new(),
            receiver,
            ctx: AppContext {
                selection: DiskSelection::default(),
                state: ApplicationState::Normal,
                di: None,
                sender,
            },
        }
    }

    fn on_key(&mut self, code: KeyCode) -> Option<CommandResult> {
        match &self.ctx.state {
            ApplicationState::Normal => self.on_key_normal(code),
            ApplicationState::Modal(modal_state) => {
                if modal_state.input_enabled() {
                    self.on_key_normal(code)
                } else {
                    None
                }
            }
        }
    }

    fn on_key_normal(&mut self, code: KeyCode) -> Option<CommandResult> {
        match code {
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    let command = self.input.clone();
                    self.add_command_to_history(&command);

                    // Process the command and get the result
                    let result = self.ci.process_command(&mut self.ctx, &command);
                    match result {
                        CommandResult::Success(response) => {
                            self.add_response_to_history(&response);
                        }
                        CommandResult::Error(response) => {
                            self.add_response_to_history(&response);
                        }
                        CommandResult::UserExit => {
                            return Some(CommandResult::UserExit);
                        }
                    }

                    // Clear input after processing
                    self.input.clear();
                }

                // Keep only the last MAX_HISTORY entries
                if self.history.len() > MAX_HISTORY {
                    self.history.drain(0..self.history.len() - MAX_HISTORY);
                }
            }
            _ => {}
        }
        None
    }

    fn add_command_to_history(&mut self, command: &str) {
        // Add the command as a UserCommand variant to the history
        self.history.push(HistoryEntry::UserCommand(command.to_string()));
    }

    fn add_response_to_history(&mut self, response: &str) {
        // Add the response as a CommandResponse variant to the history
        for line in response.lines() {
            self.history.push(HistoryEntry::CommandResponse(line.to_string()));
        }
    }

    // Generate the prompt based on current head, cylinder, and sector selection
    fn prompt(&self) -> String {
        self.ctx.selection.to_string()
    }
}

fn main() -> Result<(), io::Error> {
    // Initialize terminal
    let mut stdout = io::stdout();
    enable_raw_mode()?; // Enable raw mode
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = &mut App::new();
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        // Draw the UI
        terminal.draw(|f| ui(f, app))?;

        // Handle input
        if crossterm::event::poll(tick_rate.saturating_sub(last_tick.elapsed()))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Check for key press event only
                    if let Some(result) = app.on_key(key.code) {
                        if let CommandResult::UserExit = result {
                            break;
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Restore the terminal
    disable_raw_mode()?; // Disable raw mode
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(1),
                Constraint::Length(1), // for input box
            ]
            .as_ref(),
        )
        .split(f.area());

    let widget_height = chunks[0].height as usize;

    // Account for the 2 lines taken by the border (top and bottom)
    let visible_height = if widget_height > 2 { widget_height - 2 } else { 0 };

    // Calculate the start index to show the last N lines, where N is the visible widget height
    let start_index = if app.history.len() > visible_height {
        app.history.len() - visible_height
    } else {
        0
    };

    // Build the visible history to display
    let visible_history: Vec<Line> = app.history[start_index..]
        .iter()
        .map(|entry| match entry {
            HistoryEntry::UserCommand(cmd) => Line::from(Span::styled(format!("> {}", cmd), Style::default())),
            HistoryEntry::CommandResponse(resp) => {
                Line::from(Span::styled(resp.clone(), Style::default().fg(Color::Cyan)))
            }
        })
        .collect();

    let history_paragraph =
        Paragraph::new(visible_history).block(Block::default().borders(Borders::ALL).title("History"));
    f.render_widget(history_paragraph, chunks[0]);

    // Display prompt and input on a single line below the history
    let prompt_with_input = format!("{}>{}", app.prompt(), app.input);
    let input_line = Line::from(vec![Span::styled(prompt_with_input, Style::default())]);
    f.render_widget(Paragraph::new(input_line), chunks[1]);

    match &app.ctx.state {
        ApplicationState::Normal => {}
        ApplicationState::Modal(modal_state) => {
            match modal_state {
                ModalState::ProgressBar(title, progress) => {
                    // Display a progress bar
                    let gauge = Gauge::default().ratio(*progress);
                    let sized = SizedWrapper {
                        inner: gauge,
                        width: (f.area().width / 2) as usize,
                        height: 1,
                    };

                    let popup = Popup::new(sized)
                        .title(title.clone())
                        .style(Style::new().white().on_black());
                    f.render_widget(&popup, f.area());
                }
            }
        }
    }
}
