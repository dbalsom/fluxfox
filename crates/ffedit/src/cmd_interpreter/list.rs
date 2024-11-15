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
use crate::app::AppContext;
use crate::cmd_interpreter::{Command, CommandArgs, CommandResult};
use crate::disk_selection::SelectionLevel;
use fluxfox::DiskCh;

pub(crate) struct ListCommand;

impl Command for ListCommand {
    fn execute(&self, app: &mut AppContext, _args: CommandArgs) -> Result<CommandResult, String> {
        let mut result_string = String::new();

        if let Some(di) = &app.di {
            match app.selection.level {
                SelectionLevel::Sector => Ok(CommandResult::Success("List sectors here".into())),
                SelectionLevel::Cylinder => {
                    let ch = app
                        .selection
                        .into_ch()
                        .map_err(|_| "Invalid selection level".to_string())?;

                    app.selection.cylinder = None;
                    Ok(CommandResult::Success("List sectors here".into()))
                }
                SelectionLevel::Head => {
                    let h = app
                        .selection
                        .head
                        .ok_or_else(|| "Invalid selection level".to_string())?;

                    let track_ct = di.tracks(h);

                    result_string.push_str(&format!("Head {}, {} tracks:\n", h, track_ct));

                    for i in 0..track_ct {
                        let track = di.track(DiskCh::new(i, h)).unwrap();
                        result_string.push_str(&format!("{:02} | ", i));

                        let ti = track.info();

                        result_string.push_str(&format!(
                            "{:?} encoding, {:6} bits, {:2} sectors, {:?}\n",
                            ti.encoding, ti.bit_length, ti.sector_ct, ti.data_rate
                        ));
                    }

                    Ok(CommandResult::Success(result_string))
                }
                SelectionLevel::Disk => Ok(CommandResult::Success("List tracks for both heads here".into())),
            }
        } else {
            Err("No disk image loaded".into())
        }
    }

    fn usage(&self) -> String {
        "No arguments".into()
    }

    fn desc(&self) -> String {
        "List items depending on current selection level".into()
    }
}
