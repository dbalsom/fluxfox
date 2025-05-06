/*
    FluxFox
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

    examples/async/src/main.rs

    This is a simple example of how to use FluxFox with the Tokio async runtime.
*/

use std::path::PathBuf;

use bpaf::*;

use fluxfox::{io::Cursor, DiskImage};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Opts {
    debug: bool,
    deserialize_fn: Option<PathBuf>,
    serialize_fn: Option<PathBuf>,
    output_fn: Option<PathBuf>,
}

/// Set up bpaf argument parsing.
fn opts() -> OptionParser<Opts> {
    let debug = long("debug").help("Print debug messages").switch();

    let deserialize_fn = short('d')
        .long("deserialize")
        .help("Filename of serialized disk image to deserialize")
        .argument::<PathBuf>("FILE_TO_DESERIALIZE")
        .optional();

    let serialize_fn = short('s')
        .long("serialize")
        .help("Filename of disk image to read and serialize")
        .argument::<PathBuf>("FILE_TO_SERIALIZE")
        .optional();

    let output_fn = short('o')
        .long("output")
        .help("Filename of serialized disk image to output")
        .argument::<PathBuf>("FILE_TO_OUTPUT")
        .optional();

    construct!(Opts {
        debug,
        deserialize_fn,
        serialize_fn,
        output_fn,
    })
    .guard(
        |opts| opts.deserialize_fn.is_some() || opts.serialize_fn.is_some(),
        "Must specify either serialize or deserialize",
    )
    .guard(
        |opts| {
            (opts.serialize_fn.is_some() && opts.deserialize_fn.is_some())
                || !(opts.serialize_fn.is_some() && opts.output_fn.is_none())
        },
        "Must specify output filename for serialization",
    )
    .guard(
        |opts| !(opts.serialize_fn.is_some() && opts.deserialize_fn.is_some()),
        "Cannot serialize and deserialize at the same time",
    )
    .to_options()
    .descr("serde_demo: demonstrate serialization and deserialization of DiskImages")
}

fn main() {
    env_logger::init();

    // Get the command line options.
    let opts = opts().run();

    if opts.serialize_fn.is_some() {
        serialize(opts);
    }
    else if opts.deserialize_fn.is_some() {
        deserialize(opts);
    }
}

fn serialize(opts: Opts) {
    let filename = opts.serialize_fn.clone().unwrap();
    let file_vec = match std::fs::read(&filename) {
        Ok(file_vec) => file_vec,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    let mut reader = Cursor::new(file_vec);

    let disk_image_type = match DiskImage::detect_format(&mut reader, Some(&filename.clone())) {
        Ok(disk_image_type) => disk_image_type,
        Err(e) => {
            eprintln!("Error detecting disk image type: {}", e);
            return;
        }
    };

    println!("Detected disk image type: {}", disk_image_type);

    let mut disk = match DiskImage::load(&mut reader, Some(&filename.clone()), None, None) {
        Ok(disk) => disk,
        Err(e) => {
            eprintln!("Error loading disk image: {}", e);
            std::process::exit(1);
        }
    };

    println!("Disk image info:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_info(&mut std::io::stdout());
    println!();

    println!("Serializing disk image to file: {:?}", opts.output_fn.clone().unwrap());

    match bincode::serialize(&disk) {
        Ok(serialized) => match std::fs::write(opts.output_fn.unwrap(), serialized) {
            Ok(_) => println!("Serialization successful"),
            Err(e) => eprintln!("Error writing serialized disk image: {}", e),
        },
        Err(e) => eprintln!("Error serializing disk image: {}", e),
    }
}

fn deserialize(opts: Opts) {
    let serialized = match std::fs::read(opts.deserialize_fn.unwrap()) {
        Ok(serialized) => serialized,
        Err(e) => {
            eprintln!("Error reading serialized disk image: {}", e);
            std::process::exit(1);
        }
    };

    let mut disk: DiskImage = match bincode::deserialize(&serialized) {
        Ok(disk) => {
            println!("Deserialization successful");
            disk
        }
        Err(e) => {
            eprintln!("Error deserializing disk image: {}", e);
            std::process::exit(1);
        }
    };

    println!("Deserialized disk image:");
    println!("{}", "-".repeat(79));
    let _ = disk.dump_info(&mut std::io::stdout());
    println!();
}
