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
    app::{AppContext, AppEvent},
    cmd_interpreter::{Command, CommandArgs, CommandResult},
};
use std::path::PathBuf;

pub(crate) struct OpenCommand;

impl Command for OpenCommand {
    fn execute(&self, app: &mut AppContext, args: CommandArgs) -> Result<CommandResult, String> {
        if let Some(argv) = args.argv {
            if argv.len() != 1 {
                return Err(format!("Usage: open {}", self.usage()));
            }
            let filename = &argv[0];
            //app.file_opened = Some(filename.clone());

            if let Err(e) = app
                .sender
                .send(AppEvent::OpenFileRequest(PathBuf::from(filename.clone())))
            {
                return Err(format!("Internal error: {}", e));
            }

            Ok(CommandResult::Success(format!("Opening file: {}...", filename)))
        }
        else {
            Err(format!("Usage: open {}", self.usage()))
        }
    }

    fn usage(&self) -> String {
        "<filename>".into()
    }

    fn desc(&self) -> String {
        "Open a disk image file".into()
    }
}
