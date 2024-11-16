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

// Worker code adapted from
// https://www.tweag.io/blog/2022-11-24-wasm-threads-and-messages/

use eframe::wasm_bindgen;
use eframe::wasm_bindgen::{JsCast, JsValue};
use eframe::wasm_bindgen::closure::Closure;
use eframe::wasm_bindgen::prelude::wasm_bindgen;

// Spawn a worker and communicate with it.
#[allow (dead_code)]
pub(crate) fn spawn_worker() {
    let worker_opts = web_sys::WorkerOptions::new();
    worker_opts.set_type(web_sys::WorkerType::Module);
    let worker = match web_sys::Worker::new_with_options("./worker.js", &worker_opts) {
        Ok(worker) => worker,
        Err(e) => {
            log::error!("failed to spawn worker: {:?}", e);
            return;
        }
    };

    // let callback = Closure<FnMut(web_sys::MessageEvent)>::new(|msg| {
    //     assert_eq!(msg.data.as_f64(), Some(2.0));
    // });

    let callback = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|msg: web_sys::MessageEvent| {
        log::debug!("Received result from worker: {:?}", msg);
    });

    // Set up a callback to be invoked whenever we receive a message from the worker.
    // .as_ref().unchecked_ref() turns a wasm_bindgen::Closure into a &js_sys::Function
    worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));

    // Send a message to the worker.
    worker.post_message(&JsValue::from(1.0)).expect("failed to post");

    // Did you notice that `set_onmessage` took a borrow? We still own `callback`, and we'd
    // better not free it too soon! See also
    // https://rustwasm.github.io/wasm-bindgen/reference/weak-references.html
    std::mem::forget(callback); // FIXME: memory management is hard
}

// Spawn a worker and communicate with it.
#[allow (dead_code)]
pub(crate) fn spawn_loading_worker(bytes: &[u8]) {
    let worker_opts = web_sys::WorkerOptions::new();
    worker_opts.set_type(web_sys::WorkerType::Module);
    let worker = match web_sys::Worker::new_with_options("./load_worker.js", &worker_opts) {
        Ok(worker) => worker,
        Err(e) => {
            log::error!("failed to spawn worker: {:?}", e);
            return;
        }
    };

    // let callback = Closure<FnMut(web_sys::MessageEvent)>::new(|msg| {
    //     assert_eq!(msg.data.as_f64(), Some(2.0));
    // });

    let callback = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|msg: web_sys::MessageEvent| {
        log::debug!("Worker reports it received {} bytes.", msg.data().as_f64().unwrap_or(0.0));
    });

    // Set up a callback to be invoked whenever we receive a message from the worker.
    // .as_ref().unchecked_ref() turns a wasm_bindgen::Closure into a &js_sys::Function
    worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));

    // Convert the `u8` slice to a `Uint8Array`.
    //let data_array = unsafe { web_sys::js_sys::Uint8Array::view(bytes) };
    log::debug!("Creating Uint8Array from {} bytes.", bytes.len());
    let data_array = web_sys::js_sys::Uint8Array::from(bytes);


    // Send the data to the worker.
    worker.post_message(&data_array).expect("failed to post");

    // Did you notice that `set_onmessage` took a borrow? We still own `callback`, and we'd
    // better not free it too soon! See also
    // https://rustwasm.github.io/wasm-bindgen/reference/weak-references.html
    std::mem::forget(callback); // FIXME: memory management is hard

    log::debug!("spawn_loading_worker(): finished");
}

// Spawn a worker and communicate with it.
pub(crate) fn spawn_closure_worker(f: impl FnOnce() + Send + 'static) -> Result<web_sys::Worker, JsValue> {
    let worker_opts = web_sys::WorkerOptions::new();
    worker_opts.set_type(web_sys::WorkerType::Module);
    let worker = web_sys::Worker::new_with_options("./worker.js", &worker_opts)?;

    // Double-boxing because `dyn FnOnce` is unsized and so `Box<dyn FnOnce()>` is a fat pointer.
    // But `Box<Box<dyn FnOnce()>>` is just a plain pointer, and since wasm has 32-bit pointers,
    // we can cast it to a `u32` and back.
    let ptr = Box::into_raw(Box::new(Box::new(f) as Box<dyn FnOnce()>));
    let msg = web_sys::js_sys::Array::new();

    // Send the worker a reference to our memory chunk, so it can initialize a wasm module
    // using the same memory.
    msg.push(&wasm_bindgen::memory());

    // Also send the worker the address of the closure we want to execute.
    msg.push(&JsValue::from(ptr as u32));

    // Send the data to the worker.
    log::debug!("spawn_closure_worker(): posting message to worker");
    worker.post_message(&msg)?;

    Ok(worker)
}

#[wasm_bindgen]
pub fn closure_worker_entry_point(ptr: u32) {
    // Interpret the address we were given as a pointer to a closure to call.
    log::debug!("In closure worker!");
    let closure = unsafe { Box::from_raw(ptr as *mut Box<dyn FnOnce()>) };
    (*closure)();
}

// An entry point for the JavaScript worker to call back into WASM.
#[wasm_bindgen]
pub fn load_worker_entry_point(data: web_sys::js_sys::Uint8Array) {

    log::debug!("In worker: received {} bytes.", data.length());
    let rust_data: Vec<u8> = data.to_vec();

    web_sys::js_sys::global()
        .dyn_into::<web_sys::DedicatedWorkerGlobalScope>()
        .unwrap()
        .post_message(&JsValue::from(rust_data.len()))
        .unwrap();

    log::debug!("loading worker: completed");
}