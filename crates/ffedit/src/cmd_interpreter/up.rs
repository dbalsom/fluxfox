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
use crate::app::{AppContext, AppEvent};
use crate::cmd_interpreter::{Command, CommandArgs, CommandResult};
use crate::disk_selection::SelectionLevel;
use fluxfox::DiskCh;

pub(crate) struct UpCommand;

impl Command for UpCommand {
    fn execute(&self, app: &mut AppContext, _args: CommandArgs) -> Result<CommandResult, String> {
        let old_selection_level = app.selection.level;
        app.selection.level = match app.selection.level {
            SelectionLevel::Sector => {
                app.selection.sector = None;
                SelectionLevel::Cylinder
            }
            SelectionLevel::Cylinder => {
                app.selection.cylinder = None;
                SelectionLevel::Disk
            }
            _ => {
                // Can't go up from disk level
                app.selection.level
            }
        };

        if old_selection_level != app.selection.level {
            _ = app.sender.send(AppEvent::DiskSelectionChanged);
        }

        Ok(CommandResult::Success(format!(
            "Moved up selection level. New level: {:?}",
            app.selection.level
        )))
    }

    fn usage(&self) -> String {
        "No arguments".into()
    }
}
