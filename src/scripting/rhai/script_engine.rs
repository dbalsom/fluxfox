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
*/
#![allow(dead_code)]
use crate::{
    scripting::{rhai::interface::RhaiInterface, ScriptEngineError},
    DiskImage,
};

use rhai::Dynamic;
use std::sync::{Arc, RwLock};

pub struct RhaiEngine {
    engine:  rhai::Engine,
    context: Arc<RhaiContext>,
}

#[derive(Clone)]
pub struct RhaiContext {
    disk: Arc<RwLock<DiskImage>>,
}

impl RhaiEngine {
    fn init(disk: Arc<RwLock<DiskImage>>) -> Self {
        let mut engine = rhai::Engine::new();
        // Wrap context in Arc so it can be cloned
        let context = Arc::new(RhaiContext { disk });

        // Clone context for the closure
        let context_clone = context.clone();
        engine
            .register_type::<RhaiContext>()
            .register_fn("list_tracks", move || context_clone.list_tracks());

        RhaiEngine { engine, context }
    }

    fn run(&mut self, script: &str) -> Result<(), ScriptEngineError> {
        match self.engine.eval::<()>(script) {
            Ok(_) => Ok(()),
            Err(e) => Err(ScriptEngineError::SyntaxError(e.to_string())),
        }
    }

    fn engine(&mut self) -> &mut rhai::Engine {
        &mut self.engine
    }
}

impl RhaiInterface for RhaiContext {
    fn list_tracks(&self) -> Dynamic {
        // Use the disk image geometry iterator to get the track list
        let disk = self.disk.read().unwrap();
        let geometry = disk.geometry();

        let tracks = geometry.iter().map(|track| Dynamic::from(track)).collect();

        tracks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, types::DiskCh, ImageBuilder, StandardFormat};

    #[test]
    fn create_blank_disk_and_list_tracks() {
        // Create a blank disk using ImageBuilder
        let disk = ImageBuilder::new()
            .with_resolution(DiskDataResolution::MetaSector)
            .with_standard_format(StandardFormat::PcFloppy360)
            .build()
            .expect("Failed to create disk image");

        let disk = disk.into_arc();
        let mut rhai = RhaiEngine::init(disk.clone());

        // Script to call list_tracks
        let script = r#"
            print("Hello from Rhai");
            let tracks = list_tracks();
            tracks
        "#;

        // Execute the script and capture the output
        let result: Result<Dynamic, _> = rhai.engine().eval(script);
        //let result = engine.run(script);
        assert!(result.is_ok());

        // Verify the tracks
        let tracks = result.unwrap();
        assert!(tracks.is_array());

        // Extract the array directly
        let tracks_array = tracks.cast::<rhai::Array>();
        assert!(!tracks_array.is_empty());

        // Iterate through the tracks and print them
        for item in tracks_array {
            let track: DiskCh = item.cast::<DiskCh>();
            println!("{:?}", track);
        }
    }
}
