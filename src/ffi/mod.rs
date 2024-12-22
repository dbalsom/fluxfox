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

//! Define the FFI interface for the FluxFox library.
//! This interface is also used by [ScriptEngine] implementations to interact
//! with the library. The FFI tries to use the subset of capabilities that
//! are available to both C and Rhai.

//! This module is a work in progress. There is no usable FFI interface yet.

use crate::DiskImage;
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    path::PathBuf,
    sync::{Arc, RwLock},
};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SectorIDQueryFfi {
    cylinder: u16,
    has_cylinder: bool,
    head: u8,
    has_head: bool,
    sector: u8,
    has_sector: bool,
}

pub struct DiskImageFfi {
    inner: Arc<RwLock<DiskImage>>, // Raw pointer to the thread-safe wrapper
}

#[no_mangle]
extern "C" fn load_image(path: *const c_char) -> *mut DiskImageFfi {
    // Validate the input pointer
    if path.is_null() {
        return std::ptr::null_mut();
    }

    // Convert C string (UTF-8) to Rust string
    let c_str = unsafe { CStr::from_ptr(path) };
    let rust_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(), // Invalid UTF-8
    };

    // Handle platform-specific path conversion (Windows requires UTF-16)
    let path = PathBuf::from(rust_str);

    // Try to load the disk image
    match DiskImage::load_from_file(&path, None, None) {
        Ok(image) => {
            let ffi_image = DiskImageFfi {
                inner: Arc::new(RwLock::new(image)),
            };
            Box::into_raw(Box::new(ffi_image))
        }
        Err(_) => std::ptr::null_mut(), // Failed to load
    }
}
