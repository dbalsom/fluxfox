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
mod open;

use crate::AppContext;
use std::collections::HashMap;

use open::OpenCommand;

// Trait for commands
trait Command {
    fn execute(&self, app: &mut AppContext, args: Vec<String>) -> Result<CommandResult, String>;
    fn usage(&self) -> String;
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
        let parts: Vec<String> = input.split_whitespace().map(String::from).collect();
        if parts.is_empty() {
            return Err("No command provided".into());
        }

        let command_name = &parts[0];
        let args = parts[1..].to_vec();

        if let Some(command) = self.commands.get(command_name) {
            command.execute(app, args)
        } else {
            Err(format!("Unknown command: {}\nType ? for help", command_name))
        }
    }

    fn get_usage(&self) -> String {
        if self.commands.is_empty() {
            return "No commands have been registered.".to_string();
        }

        let str = self
            .commands
            .iter()
            .map(|(name, command)| format!("{} - {}", name, command.usage()))
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
        self.registry.register_command("open", Box::new(OpenCommand));
    }

    // Command processor
    pub(crate) fn process_command(&self, app: &mut AppContext, command: &str) -> CommandResult {
        if command == "q" {
            CommandResult::UserExit
        } else if command == "?" {
            // Return help information by calling get_usage on the registry
            let help_message = self.registry.get_usage();
            CommandResult::Success(help_message)
        } else {
            self.registry
                .dispatch(app, command)
                .unwrap_or_else(|e| CommandResult::Error(format!("Error: {}", e)))
        }
    }
}
