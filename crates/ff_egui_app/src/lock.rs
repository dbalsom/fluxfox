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
*/

//! The lock module defines a [TrackingLock] that tracks lock usage by [Tool].

use fluxfox::{
    disk_lock::{DiskLock, LockContext, NonTrackingDiskLock},
    DiskImage,
};
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
// To lock a TrackingLock we must provide a defined [Tool].
use crate::app::Tool;

impl LockContext for Tool {}

/// The [TrackingLock] is a wrapper around `Arc<RwLock<T>>` that tracks lock usage by [Tool].
pub struct TrackingLock<T> {
    inner:    Arc<RwLock<T>>,
    // Protects the tracking data
    tracking: Arc<Mutex<TrackingData>>,
}

struct TrackingData {
    read_locks: HashMap<Tool, usize>, // Tool -> count of read locks
    write_lock: Option<Tool>,         // Currently held write lock
}

impl<T> Clone for TrackingLock<T> {
    fn clone(&self) -> Self {
        TrackingLock {
            inner:    Arc::clone(&self.inner),
            tracking: Arc::clone(&self.tracking),
        }
    }
}

impl DiskLock<DiskImage, Tool> for TrackingLock<DiskImage> {
    type T = DiskImage;
    type C = Tool;
    type ReadGuard<'a> = TrackingReadGuard<'a, DiskImage>;
    type WriteGuard<'a> = TrackingWriteGuard<'a, DiskImage>;

    fn read(&self, tool: Tool) -> Result<TrackingReadGuard<DiskImage>, Tool> {
        self.read(tool)
    }

    fn write(&self, tool: Tool) -> Result<TrackingWriteGuard<DiskImage>, Vec<Tool>> {
        self.write(tool)
    }

    fn strong_count(&self) -> usize {
        self.strong_count()
    }
}

impl From<TrackingLock<DiskImage>> for NonTrackingDiskLock<DiskImage> {
    fn from(lock: TrackingLock<DiskImage>) -> Self {
        NonTrackingDiskLock::new(lock.inner)
    }
}

impl<T> TrackingLock<T> {
    /// Creates a new TrackingLock.
    pub fn new(data: T) -> Self {
        TrackingLock {
            inner:    Arc::new(RwLock::new(data)),
            tracking: Arc::new(Mutex::new(TrackingData {
                read_locks: HashMap::new(),
                write_lock: None,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Attempts to acquire a read lock for the given tool.
    pub fn read(&self, tool: Tool) -> Result<TrackingReadGuard<T>, Tool> {
        let mut tracking = self.tracking.lock().unwrap();

        if tracking.write_lock.is_some() {
            log::error!(
                "Tool {:?} attempted to acquire a read lock while a write lock is held by {:?}",
                tool,
                tracking.write_lock
            );
            return Err(tracking.write_lock.unwrap());
        }

        // Acquire the actual read lock
        match self.inner.try_read() {
            Ok(guard) => {
                // Increment the read lock count for the tool
                *tracking.read_locks.entry(tool).or_insert(0) += 1;
                Ok(TrackingReadGuard {
                    guard,
                    tracking: Arc::clone(&self.tracking),
                    tool,
                })
            }
            Err(_) => {
                // Failed to acquire the read lock. We should have detected this above, so panic.
                panic!("Failed to detect write lock while acquiring read lock");
            }
        }
    }

    /// Attempts to acquire a write lock for the given tool.
    pub fn write(&self, tool: Tool) -> Result<TrackingWriteGuard<T>, Vec<Tool>> {
        let mut tracking = self.tracking.lock().unwrap();

        if !tracking.read_locks.is_empty() {
            log::error!(
                "Tool {:?} attempted to acquire a write lock while read locks are held by {:?}",
                tool,
                tracking.read_locks.iter().map(|(t, _)| *t).collect::<Vec<_>>()
            );
            return Err(tracking.read_locks.keys().cloned().collect());
        }

        if tracking.write_lock.is_some() {
            log::error!(
                "Tool {:?} attempted to acquire a write lock while write lock is held by {:?}",
                tool,
                tracking.write_lock
            );
            return Err(vec![tracking.write_lock.unwrap()]);
        }

        // Set the write lock
        tracking.write_lock = Some(tool);

        // Acquire the actual write lock
        match self.inner.try_write() {
            Ok(guard) => Ok(TrackingWriteGuard {
                guard,
                tracking: Arc::clone(&self.tracking),
                tool,
            }),
            Err(_) => {
                // Failed to acquire the write lock. We should have detected this above, so panic.
                panic!("Failed to detect existing lock while acquiring write lock");
            }
        }
    }

    /// Clones the Arc to allow sharing the TrackingLock.
    pub fn clone_arc(&self) -> Self {
        TrackingLock {
            inner:    Arc::clone(&self.inner),
            tracking: Arc::clone(&self.tracking),
        }
    }
}

/// Guard that removes the read lock tracking when dropped.
pub struct TrackingReadGuard<'a, T> {
    guard: RwLockReadGuard<'a, T>,
    tracking: Arc<Mutex<TrackingData>>,
    tool: Tool,
}

impl<'a, T> Deref for TrackingReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

impl<'a, T> Drop for TrackingReadGuard<'a, T> {
    fn drop(&mut self) {
        let mut tracking = self.tracking.lock().unwrap();
        if let Some(count) = tracking.read_locks.get_mut(&self.tool) {
            *count -= 1;
            if *count == 0 {
                tracking.read_locks.remove(&self.tool);
            }
        }
        else {
            log::error!("Tool {:?} is dropping a read lock but it was not registered", self.tool);
        }
    }
}

/// Guard that removes the write lock tracking when dropped.
pub struct TrackingWriteGuard<'a, T> {
    guard: RwLockWriteGuard<'a, T>,
    tracking: Arc<Mutex<TrackingData>>,
    tool: Tool,
}

impl<'a, T> Deref for TrackingWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

impl<'a, T> DerefMut for TrackingWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard
    }
}

impl<'a, T> Drop for TrackingWriteGuard<'a, T> {
    fn drop(&mut self) {
        let mut tracking = self.tracking.lock().unwrap();
        if let Some(current_tool) = tracking.write_lock {
            if current_tool != self.tool {
                log::error!(
                    "Tool {:?} is dropping a write lock but the current write lock is held by {:?}",
                    self.tool,
                    current_tool
                );
            }
            else {
                tracking.write_lock = None;
                log::debug!(
                    "Tool {:?} dropped write lock. Strong references left: {}",
                    self.tool,
                    Arc::strong_count(&self.tracking)
                );
            }
        }
        else {
            log::error!(
                "Tool {:?} is dropping a write lock but no write lock was registered",
                self.tool
            );
        }
    }
}
