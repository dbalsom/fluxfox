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
use crate::cmd_interpreter::{CommandInterpreter, CommandResult};
use crate::modal::ModalState;
use crate::{CmdParams, DiskSelection, HistoryEntry, MAX_HISTORY};
use crossbeam_channel::{Receiver, Sender};
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use fluxfox::DiskImage;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::DefaultTerminal;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tui_popup::{Popup, SizedWrapper};

// Application state to support different modes
#[derive(Default)]
enum ApplicationState {
    #[default]
    Normal,
    Modal(ModalState),
}

pub enum AppEvent {
    LoadingStatus(f64),
    DiskImageLoaded(DiskImage, PathBuf),
    DiskImageLoadingFailed(String),
}

// Contain mutable data for App
// This avoids borrowing issues when passing the mutable context to the command processor
pub(crate) struct AppContext {
    selection: DiskSelection,
    state: ApplicationState,
    di: Option<DiskImage>,
    di_name: Option<PathBuf>,
    sender: Sender<AppEvent>,
}

pub(crate) struct App {
    params: CmdParams,
    input: String,
    ci: CommandInterpreter,
    history: Vec<HistoryEntry>, // Store history as Vec<HistoryEntry>

    receiver: Receiver<AppEvent>,
    ctx: AppContext,
}

impl AppContext {
    fn load_disk_image(&mut self, filename: PathBuf) {
        let outer_sender = self.sender.clone();
        let inner_filename = filename.clone();
        std::thread::spawn(move || {
            let inner_sender = outer_sender.clone();

            match DiskImage::load_from_file(
                inner_filename,
                Some(Box::new(move |status| match status {
                    fluxfox::LoadingStatus::Progress(progress) => {
                        inner_sender.send(AppEvent::LoadingStatus(progress)).unwrap();
                    }
                    fluxfox::LoadingStatus::Error => {
                        inner_sender
                            .send(AppEvent::DiskImageLoadingFailed("Unknown error".to_string()))
                            .unwrap();
                    }
                    _ => {}
                })),
            ) {
                Ok(di) => {
                    outer_sender
                        .send(AppEvent::DiskImageLoaded(di, filename.clone()))
                        .unwrap();
                }
                Err(e) => {
                    outer_sender
                        .send(AppEvent::DiskImageLoadingFailed(format!("Error: {}", e)))
                        .unwrap();
                }
            }
        });
    }
}

impl App {
    pub fn new(params: CmdParams) -> App {
        let (sender, receiver) = crossbeam_channel::unbounded::<AppEvent>();

        let mut app = App {
            params,
            input: String::new(),
            ci: CommandInterpreter::new(),
            history: Vec::new(),
            receiver,
            ctx: AppContext {
                selection: DiskSelection::default(),
                state: ApplicationState::Normal,
                di: None,
                di_name: None,
                sender,
            },
        };

        if let Some(ref in_file) = app.params.in_filename {
            app.ctx.load_disk_image(in_file.clone());
        }

        app
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1), // for title
                    Constraint::Min(1),
                    Constraint::Length(1), // for input box
                ]
                .as_ref(),
            )
            .split(f.area());

        let history_height = chunks[0].height as usize;

        let title_bar = format!(
            "{}",
            if let Some(di_name) = &self.ctx.di_name {
                di_name.to_string_lossy()
            } else {
                std::borrow::Cow::Borrowed("No Disk Image")
            }
        );
        let title_line = Line::from(vec![
            Span::styled("ffedit ", Style::light_blue(Style::default())),
            Span::styled(title_bar, Style::default()),
        ]);
        f.render_widget(Paragraph::new(title_line), chunks[0]);

        // Account for the 2 lines taken by the border (top and bottom)
        let visible_height = if history_height > 2 { history_height - 2 } else { 0 };

        // Calculate the start index to show the last N lines, where N is the visible widget height
        let start_index = if self.history.len() > visible_height {
            self.history.len() - visible_height
        } else {
            0
        };

        // Build the visible history to display
        let visible_history: Vec<Line> = self.history[start_index..]
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
        f.render_widget(history_paragraph, chunks[1]);

        // Display prompt and input on a single line below the history
        let prompt_with_input = format!("{}>{}", self.prompt(), self.input);
        let input_line = Line::from(vec![Span::styled(prompt_with_input, Style::default())]);
        f.render_widget(Paragraph::new(input_line), chunks[2]);

        match &self.ctx.state {
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

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), io::Error> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        loop {
            // Draw the UI
            terminal.draw(|f| self.draw(f))?;

            // Receive AppEvents
            while let Ok(msg) = self.receiver.try_recv() {
                match msg {
                    AppEvent::LoadingStatus(progress) => {
                        self.ctx.state = ApplicationState::Modal(ModalState::ProgressBar(
                            "Loading Disk Image".to_string(),
                            progress,
                        ));
                    }
                    AppEvent::DiskImageLoaded(di, di_name) => {
                        self.ctx.di = Some(di);
                        self.ctx.di_name = Some(di_name.file_name().unwrap().into());
                        self.ctx.state = ApplicationState::Normal;
                    }
                    AppEvent::DiskImageLoadingFailed(msg) => {
                        self.ctx.state = ApplicationState::Normal;
                        self.history.push(HistoryEntry::CommandResponse(msg));
                    }
                }
            }

            // Handle input
            if crossterm::event::poll(tick_rate.saturating_sub(last_tick.elapsed()))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        // Check for key press event only
                        if let Some(result) = self.on_key(key.code) {
                            if let CommandResult::UserExit = result {
                                break Ok(());
                            }
                        }
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
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
