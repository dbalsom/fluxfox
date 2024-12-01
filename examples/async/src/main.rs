/*
    FluxFox
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

    examples/async/src/main.rs

    This is a simple example of how to use FluxFox with the Tokio async runtime.
*/
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{task, time::sleep};

use anyhow::{anyhow, Error};
use bpaf::*;

use fluxfox::{
    io::{Cursor, ReadSeek, Write},
    DiskImage,
    LoadingStatus,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Opts {
    debug:    bool,
    filename: PathBuf,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Opts> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let filename = short('t')
        .long("filename")
        .help("Filename of image to read")
        .argument::<PathBuf>("FILE");

    construct!(Opts { debug, filename })
        .to_options()
        .descr("imginfo: display info about disk image")
}

#[tokio::main]
async fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    let file_vec = match std::fs::read(opts.filename.clone()) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    let mut reader = Cursor::new(file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut reader) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            return;
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let disk_opt = match load_disk_image(reader, opts).await {
        Ok(disk) => Some(disk),
        Err(_) => None,
    };

    if let Some(mut disk) = disk_opt {
        println!("Disk image info:");
        println!("{}", "-".repeat(79));
        let _ = disk.dump_info(&mut std::io::stdout());
        println!();
    }
}

// Load a disk image from a stream, displaying a progress spinner.
async fn load_disk_image<RS: ReadSeek + Send + 'static>(mut reader: RS, opts: Opts) -> Result<DiskImage, Error> {
    let progress = Arc::new(Mutex::new(0.0));

    // Define a callback to update the progress percentage as the disk image loads.
    let progress_clone = Arc::clone(&progress);
    let callback: Arc<dyn Fn(LoadingStatus) + Send + Sync> = Arc::new(move |status| match status {
        LoadingStatus::Progress(p) => {
            let mut progress = progress_clone.lock().unwrap();
            *progress = p * 100.0;
        }
        LoadingStatus::Complete => {
            let mut progress = progress_clone.lock().unwrap();
            *progress = 100.0;
        }
        _ => {}
    });

    // Spawn a task for loading the disk image
    let mut load_handle =
        task::spawn(async move { DiskImage::load_async(&mut reader, Some(opts.filename), None, Some(callback)).await });

    // Display spinner with percentage progress
    let spinner_chars = ['|', '/', '-', '\\'];
    let mut spinner_idx = 0;

    loop {
        tokio::select! {
            result = &mut load_handle => {
                // When the loading task completes, handle the result
                let disk = match result {
                    Ok(Ok(disk)) => disk,
                    Ok(Err(e)) => {
                        eprintln!("Error loading disk image: {:?}", e);
                        return Err(anyhow!(e));
                    }
                    Err(e) => {
                        eprintln!("Task failed: {:?}", e);
                        return Err(anyhow!(e));
                    }
                };

                // Break out of the loop with the loaded disk
                break Ok(disk);
            }

            _ = sleep(Duration::from_millis(100)) => {
                // Update the spinner and display progress
                let progress = *progress.lock().unwrap();
                print!("\rLoading disk image... {} {:.2}%", spinner_chars[spinner_idx], progress);
                std::io::stdout().flush().unwrap();
                spinner_idx = (spinner_idx + 1) % spinner_chars.len();
            }
        }
    }
}
