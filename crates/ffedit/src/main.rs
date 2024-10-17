use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::Line;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::io;
use std::time::{Duration, Instant};

// Define an enum for the history entries
enum HistoryEntry {
    UserCommand(String),
    CommandResponse(String),
}

const MAX_HISTORY: usize = 1000; // Maximum number of history entries

struct App {
    input: String,
    history: Vec<HistoryEntry>, // Store history as Vec<HistoryEntry>
}

impl App {
    fn new() -> App {
        App {
            input: String::new(),
            history: Vec::new(),
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    let command = self.input.clone();
                    self.add_command_to_history(&command);

                    // Process the command and get the response
                    let response = process_command(&command);
                    self.add_response_to_history(&response);

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
    }

    fn add_command_to_history(&mut self, command: &str) {
        // Add the command as a UserCommand variant to the history
        self.history.push(HistoryEntry::UserCommand(command.to_string()));
    }

    fn add_response_to_history(&mut self, response: &str) {
        // Add the response as a CommandResponse variant to the history
        self.history.push(HistoryEntry::CommandResponse(response.to_string()));
    }
}

// Command processor stub
fn process_command(command: &str) -> String {
    if command == "?" {
        "help requested".to_string()
    } else {
        "command accepted".to_string()
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
                    match key.code {
                        KeyCode::Esc => {
                            break;
                        }
                        _ => app.on_key(key.code),
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

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(1),
                Constraint::Length(3), // for input box
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

    // Input box, with String::from(&app.input)
    let input = Paragraph::new(String::from(&app.input))
        .style(Style::default())
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);
}
