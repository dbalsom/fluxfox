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
use crate::cmd_interpreter::{Command, CommandResult};
use crate::disk_selection::SelectionLevel;

pub(crate) struct CylinderCommand;

impl Command for CylinderCommand {
    fn execute(&self, app: &mut AppContext, args: Vec<String>) -> Result<CommandResult, String> {
        if args.len() != 1 {
            return Err(format!("Usage: c {}", self.usage()));
        }
        let new_cylinder: u16 = args[0].parse::<u16>().map_err(|_| "Invalid cylinder number")?;

        if let Some(di) = &app.di {
            if new_cylinder >= di.tracks(app.selection.head.unwrap_or(0)) {
                return Err(format!("Invalid cylinder number: {}", new_cylinder));
            }
        }

        _ = app.sender.send(AppEvent::DiskSelectionChanged);

        if app.selection.level < SelectionLevel::Cylinder {
            app.selection.level = SelectionLevel::Cylinder
        }
        app.selection.cylinder = Some(new_cylinder);
        Ok(CommandResult::Success(format!("Changed cylinder to: {}", new_cylinder)))
    }

    fn usage(&self) -> String {
        "<cylinder #>".into()
    }
}
