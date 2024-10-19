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

use std::cell::RefCell;
use std::io;
use std::path::{PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub use crate::app_context::AppContext;
use crate::cmd_interpreter::{CommandInterpreter, CommandResult};
use crate::components::data_block::DataBlock;
use crate::components::history::{HistoryWidget};
use crate::disk_selection::DiskSelection;
use crate::logger::{init_logger, LogEntry};
use crate::modal::ModalState;
use crate::widget::{FoxWidget, TabSelectableWidget};
use crate::CmdParams;
use crossbeam_channel::{Receiver};
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use fluxfox::DiskImage;
use ratatui::prelude::*;
use ratatui::widgets::{Gauge, Paragraph};
use ratatui::DefaultTerminal;
use tui_popup::{Popup, SizedWrapper};

// Application state to support different modes
#[derive(Default)]
pub(crate) enum ApplicationState {
    #[default]
    Normal,
    Modal(ModalState),
}

pub(crate) enum AppEvent {
    LoadingStatus(f64),
    DiskImageLoaded(DiskImage, PathBuf),
    DiskImageLoadingFailed(String),
    DiskSelectionChanged,
    Log(LogEntry),
    OpenFileRequest(PathBuf),
}

pub(crate) struct UiContext {
    pub(crate) dragging: bool,
    pub(crate) split_percentage: u16,
}

pub(crate) struct App {
    pub(crate) params: CmdParams,
    pub(crate) input: String,
    pub(crate) ci: CommandInterpreter,
    pub(crate) history: Rc<RefCell<HistoryWidget>>,
    pub(crate) widgets: Vec<Rc<RefCell<dyn FoxWidget>>>,
    pub(crate) receiver: Receiver<AppEvent>,
    pub(crate) ctx: AppContext,
    pub(crate) ui_ctx: UiContext,
    pub(crate) selected_widget: usize,
}

impl App {
    pub fn new(params: CmdParams) -> App {
        let (sender, receiver) = crossbeam_channel::unbounded::<AppEvent>();
        init_logger(sender.clone()).unwrap();

        log::info!("Logger initialized!");

        let db = Rc::new(RefCell::new(DataBlock::default()));
        let history = Rc::new(RefCell::new(HistoryWidget::new(None)));

        let mut widgets = Vec::new();
        widgets.push(history.clone() as Rc<RefCell<dyn FoxWidget>>);
        widgets.push(db.clone() as Rc<RefCell<dyn FoxWidget>>);

        let mut app = App {
            params,
            input: String::new(),
            ci: CommandInterpreter::new(),
            history,
            receiver,
            ctx: AppContext {
                selection: DiskSelection::default(),
                state: ApplicationState::Normal,
                di: None,
                di_name: None,
                sender,
                db,
            },
            ui_ctx: UiContext {
                dragging: false,
                split_percentage: 50,
            },
            widgets,
            selected_widget: 0,
        };

        if let Some(ref in_file) = app.params.in_filename {
            app.ctx.load_disk_image(in_file.clone());
        }

        app
    }

    fn select_next_widget(&mut self) {
        log::debug!("select_next_widget()... Selecting next widget");
        self.widgets[self.selected_widget].borrow_mut().deselect();
        self.selected_widget = (self.selected_widget + 1) % self.widgets.len();
        self.widgets[self.selected_widget].borrow_mut().select();
    }

    fn draw(&mut self, f: &mut Frame) {
        let app_layout = Layout::default()
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

        let horiz_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(self.ui_ctx.split_percentage), // Dynamic split for the history
                    Constraint::Percentage(100 - self.ui_ctx.split_percentage), // Remaining space for the data pane
                ]
                .as_ref(),
            )
            .split(app_layout[1]);

        let history_height = app_layout[0].height as usize;

        let image_name = if let Some(di_name) = &self.ctx.di_name {
            format!("{}", di_name.to_string_lossy())
        } else {
            "No Disk Image".to_string()
        };

        let image_resolution = if let Some(di) = &self.ctx.di {
            format!("{:?}", di.resolution())
        } else {
            "".to_string()
        };

        let mut title_line = Line::from(vec![
            Span::styled("ffedit ", Style::light_blue(Style::default())),
            Span::styled(image_name, Style::default()),
        ]);

        if !image_resolution.is_empty() {
            title_line.push_span(Span::styled(" [", Style::default()));
            title_line.push_span(Span::styled(image_resolution, Style::light_blue(Style::default())));
            title_line.push_span(Span::styled("]", Style::default()));
        }

        f.render_widget(Paragraph::new(title_line), app_layout[0]);

        self.draw_history(f, horiz_split[0]);
        self.draw_data_pane(f, horiz_split[1]);
        //
        // // Account for the 2 lines taken by the border (top and bottom)
        // let visible_height = if history_height > 2 { history_height - 2 } else { 0 };
        //
        // // Calculate the start index to show the last N lines, where N is the visible widget height
        // let start_index = if self.history.len() > visible_height {
        //     self.history.len() - visible_height
        // } else {
        //     0
        // };
        //
        // // Build the visible history to display
        // let visible_history: Vec<Line> = self.history[start_index..]
        //     .iter()
        //     .map(|entry| match entry {
        //         HistoryEntry::UserCommand(cmd) => Line::from(Span::styled(format!("> {}", cmd), Style::default())),
        //         HistoryEntry::CommandResponse(resp) => {
        //             Line::from(Span::styled(resp.clone(), Style::default().fg(Color::Cyan)))
        //         }
        //     })
        //     .collect();
        //
        // let history_paragraph =
        //     Paragraph::new(visible_history).block(Block::default().borders(Borders::ALL).title("History"));
        // f.render_widget(history_paragraph, app_layout[1]);

        // Display prompt and input on a single line below the history
        let prompt_with_input = format!("{}>{}", self.prompt(), self.input);
        let input_line = Line::from(vec![Span::styled(prompt_with_input, Style::default())]);
        f.render_widget(Paragraph::new(input_line), app_layout[2]);

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
            self.handle_app_events();

            // Handle input
            if event::poll(tick_rate.saturating_sub(last_tick.elapsed()))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            // Check for key press event only
                            if let Some(result) = self.on_key(key.code, key.modifiers) {
                                if let CommandResult::UserExit = result {
                                    break Ok(());
                                }
                            }
                        }
                    }
                    Event::Mouse(mouse_event) => {
                        if let Ok(size) = terminal.size() {
                            self.on_mouse(mouse_event, size);
                        }
                    }
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }
    }

    fn on_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<CommandResult> {
        match &self.ctx.state {
            ApplicationState::Normal => self.on_key_normal(code, modifiers),
            ApplicationState::Modal(modal_state) => {
                if modal_state.input_enabled() {
                    self.on_key_normal(code, modifiers)
                } else {
                    None
                }
            }
        }
    }

    fn on_key_normal(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<CommandResult> {
        match code {
            KeyCode::Char(c) if c == 'c' && modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(CommandResult::UserExit);
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    let mut history = self.history.borrow_mut();
                    let command = self.input.clone();
                    history.push_user_cmd(&command);

                    // Process the command and get the result
                    let result = self.ci.process_command(&mut self.ctx, &command);
                    match result {
                        CommandResult::Success(response) => {
                            history.push_cmd_response(&response);
                        }
                        CommandResult::Error(response) => {
                            history.push_cmd_response(&response);
                        }
                        CommandResult::UserExit => {
                            return Some(CommandResult::UserExit);
                        }
                    }

                    // Clear input after processing
                    self.input.clear();
                }
            }
            KeyCode::BackTab => {
                self.select_next_widget();
            }
            KeyCode::PageUp => {
                self.widgets[self.selected_widget].borrow_mut().page_up();
            }
            KeyCode::PageDown => {
                self.widgets[self.selected_widget].borrow_mut().page_down();
            }
            _ => {}
        }
        None
    }

    fn on_mouse(&mut self, event: MouseEvent, size: Size) {
        match event.kind {
            MouseEventKind::Down(_) => {
                log::debug!("mouse down");
                // Start dragging if mouse is near the split
                if event.column >= (self.ui_ctx.split_percentage - 2)
                    && event.column <= (self.ui_ctx.split_percentage + 2)
                {
                    self.ui_ctx.dragging = true;
                }
            }
            MouseEventKind::Drag(_) => {
                if self.ui_ctx.dragging {
                    // Update split based on mouse position
                    self.ui_ctx.split_percentage = (event.column as f64 / size.width as f64 * 100.0) as u16;
                }
            }
            MouseEventKind::Up(_) => {
                self.ui_ctx.dragging = false;
            }
            _ => {}
        }
    }

    // Generate the prompt based on current head, cylinder, and sector selection
    fn prompt(&self) -> String {
        self.ctx.selection.to_string()
    }

    fn draw_history(&self, f: &mut Frame, area: Rect) {
        f.render_widget_ref(&*self.history.borrow(), area);
    }

    fn draw_data_pane(&self, f: &mut Frame, area: Rect) {
        // Display data pane content here
        //let block = Block::default().borders(Borders::ALL).title("Data Pane");
        f.render_widget_ref(&*self.ctx.db.borrow(), area);
    }
}
