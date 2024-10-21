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
mod c;
mod h;
mod list;
mod open;
mod s;
mod up;

use crate::app::AppContext;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static COMMAND_ALIASES: Lazy<HashMap<String, String>> = Lazy::new(|| {
    HashMap::from([
        ("?".to_string(), "help".to_string()),
        ("q".to_string(), "quit".to_string()),
        ("o".to_string(), "open".to_string()),
        ("..".to_string(), "up".to_string()),
        ("ls".to_string(), "list".to_string()),
        ("dir".to_string(), "list".to_string()),
    ])
});

pub struct CommandArgs {
    pub command: String,
    pub argv: Option<Vec<String>>,
    pub raw_args: Option<String>,
}

// Trait for commands
trait Command {
    fn execute(&self, app: &mut AppContext, args: CommandArgs) -> Result<CommandResult, String>;
    fn usage(&self) -> String;
    fn desc(&self) -> String;
}

// Command registry for managing and dispatching commands
#[derive(Default)]
struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        CommandRegistry {
            commands: HashMap::new(),
        }
    }

    fn register_command(&mut self, name: &str, command: Box<dyn Command>) {
        self.commands.insert(name.to_string(), command);
    }

    fn dispatch(&self, app: &mut AppContext, input: &str) -> Result<CommandResult, String> {
        let cmd_args = parse_input(input);

        if let Some(command) = self.commands.get(&cmd_args.command) {
            command.execute(app, cmd_args)
        } else {
            Err(format!("Unknown command: {} [Type ? for help]", &cmd_args.command))
        }
    }

    fn get_usage(&self) -> String {
        if self.commands.is_empty() {
            return "No commands have been registered.".to_string();
        }

        let str = self
            .commands
            .iter()
            .map(|(name, command)| format!("{} - {} - {}", name, command.usage(), command.desc()))
            .collect::<Vec<_>>()
            .join("\n");

        str
    }
}

// Command result for processing commands
pub enum CommandResult {
    Success(String),
    Error(String),
    UserExit, // Used to indicate that the user wants to quit the application
}

pub struct CommandInterpreter {
    registry: CommandRegistry,
}

impl Default for CommandInterpreter {
    fn default() -> Self {
        let mut i = CommandInterpreter {
            registry: CommandRegistry::new(),
        };
        i.register_default_commands();
        i
    }
}

impl CommandInterpreter {
    pub fn new() -> CommandInterpreter {
        Default::default()
    }

    // Registering commands with the registry
    fn register_default_commands(&mut self) {
        self.registry.register_command("open", Box::new(open::OpenCommand));
        self.registry.register_command("h", Box::new(h::HeadCommand));
        self.registry.register_command("c", Box::new(c::CylinderCommand));
        self.registry.register_command("s", Box::new(s::SectorCommand));
        self.registry.register_command("up", Box::new(up::UpCommand));
        self.registry.register_command("list", Box::new(list::ListCommand));
    }

    // Command processor
    pub(crate) fn process_command(&self, app: &mut AppContext, command: &str) -> CommandResult {
        // Resolve command aliases
        let command_string = command.to_string();
        let resolved_command = COMMAND_ALIASES.get(command).unwrap_or(&command_string);

        if resolved_command == "q" {
            CommandResult::UserExit
        } else if resolved_command == "help" {
            // Return help information by calling get_usage on the registry
            let help_message = self.registry.get_usage();
            CommandResult::Success(help_message)
        } else {
            self.registry
                .dispatch(app, resolved_command)
                .unwrap_or_else(|e| CommandResult::Error(format!("Error: {}", e)))
        }
    }
}

fn parse_input(input: &str) -> CommandArgs {
    let parts = split_quoted(input);
    let command = parts[0].clone();
    let argv = if parts.len() > 1 {
        Some(parts[1..].to_vec())
    } else {
        None
    };
    let raw_args = split_once(input).get(1).map(|s| s.clone());

    CommandArgs {
        command,
        argv,
        raw_args,
    }
}

fn split(input: &str) -> Vec<String> {
    input.split_whitespace().map(String::from).collect()
}

fn split_once(input: &str) -> Vec<String> {
    let parts = input
        .splitn(2, char::is_whitespace)
        .map(String::from)
        .collect::<Vec<String>>();

    parts
}

fn split_quoted(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_quotes = false;
    let mut current = String::new();

    for c in input.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' | '\t' | '\n' if !in_quotes => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}
