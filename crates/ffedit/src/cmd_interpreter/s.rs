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

pub(crate) struct SectorCommand;

impl Command for SectorCommand {
    fn execute(&self, app: &mut AppContext, args: CommandArgs) -> Result<CommandResult, String> {
        if let Some(argv) = args.argv {
            if argv.len() != 1 {
                return Err(format!("Usage: {}", self.usage()));
            }

            let new_sector: u8 = argv[0].parse::<u8>().map_err(|_| "Invalid sector number")?;

            if let Some(di) = &app.di {
                let track = di
                    .track(DiskCh::new(
                        app.selection.cylinder.unwrap_or(0),
                        app.selection.head.unwrap_or(0),
                    ))
                    .ok_or("Invalid track")?;

                if !track.has_sector_id(new_sector) {
                    return Err(format!("Invalid sector number: {}", new_sector));
                }
            }

            _ = app.sender.send(AppEvent::DiskSelectionChanged);

            if app.selection.level < SelectionLevel::Sector {
                app.selection.level = SelectionLevel::Sector
            }
            app.selection.sector = Some(new_sector);
            Ok(CommandResult::Success(format!("Changed sector to: {}", new_sector)))
        } else {
            Err(format!("Usage: c {}", self.usage()))
        }
    }

    fn usage(&self) -> String {
        "<sector #>".into()
    }
}
