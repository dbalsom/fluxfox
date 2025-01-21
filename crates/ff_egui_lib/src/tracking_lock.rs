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

//! The tracking_lock module defines a [TrackingLock] that tracks lock usage by [Tool].

use crate::UiLockContext;
use egui::Ui;
use fluxfox::{
    disk_lock::{DiskLock, LockContext, NonTrackingDiskLock},
    DiskImage,
};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

impl LockContext for UiLockContext {}

/// The [TrackingLock] is a wrapper around `Arc<RwLock<T>>` that tracks lock usage by [Tool].
pub struct TrackingLock<T> {
    inner:    Arc<RwLock<T>>,
    // Protects the tracking data
    tracking: Arc<Mutex<TrackingData>>,
}

struct TrackingData {
    read_locks: HashMap<UiLockContext, usize>, // Tool -> count of read locks
    write_lock: Option<UiLockContext>,         // Currently held write lock
}

impl<T> Clone for TrackingLock<T> {
    fn clone(&self) -> Self {
        TrackingLock {
            inner:    Arc::clone(&self.inner),
            tracking: Arc::clone(&self.tracking),
        }
    }
}

impl DiskLock<DiskImage, UiLockContext> for TrackingLock<DiskImage> {
    type T = DiskImage;
    type C = UiLockContext;
    type ReadGuard<'a> = TrackingReadGuard<'a, DiskImage>;
    type WriteGuard<'a> = TrackingWriteGuard<'a, DiskImage>;

    fn read(&self, context: UiLockContext) -> Result<TrackingReadGuard<'_, DiskImage>, UiLockContext> {
        self.read(context)
    }

    fn write(&self, context: UiLockContext) -> Result<TrackingWriteGuard<'_, DiskImage>, Vec<UiLockContext>> {
        self.write(context)
    }

    fn strong_count(&self) -> usize {
        self.strong_count()
    }
}

impl From<Arc<RwLock<DiskImage>>> for TrackingLock<DiskImage> {
    fn from(lock: Arc<RwLock<DiskImage>>) -> Self {
        TrackingLock::from_arc(lock)
    }
}

impl From<TrackingLock<DiskImage>> for NonTrackingDiskLock<DiskImage> {
    fn from(lock: TrackingLock<DiskImage>) -> Self {
        NonTrackingDiskLock::new(lock.inner)
    }
}

impl<T> TrackingLock<T> {
    /// Creates a new TrackingLock from base T.
    pub fn new(data: T) -> Self {
        TrackingLock {
            inner:    Arc::new(RwLock::new(data)),
            tracking: Arc::new(Mutex::new(TrackingData {
                read_locks: HashMap::new(),
                write_lock: None,
            })),
        }
    }

    /// Creates a new TrackingLock from Arc<RwLock<T>>.
    pub fn from_arc(lock: Arc<RwLock<T>>) -> Self {
        TrackingLock {
            inner:    lock,
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
    pub fn read(&self, context: UiLockContext) -> Result<TrackingReadGuard<'_, T>, UiLockContext> {
        let mut tracking = self.tracking.lock().unwrap();

        if tracking.write_lock.is_some() {
            log::error!(
                "Tool {:?} attempted to acquire a read lock while a write lock is held by {:?}",
                context,
                tracking.write_lock
            );
            return Err(tracking.write_lock.unwrap());
        }

        // Acquire the actual read lock
        match self.inner.try_read() {
            Ok(guard) => {
                // Increment the read lock count for the tool
                *tracking.read_locks.entry(context).or_insert(0) += 1;
                Ok(TrackingReadGuard {
                    guard,
                    tracking: Arc::clone(&self.tracking),
                    context,
                })
            }
            Err(_) => {
                // Failed to acquire the read lock. We should have detected this above, so panic.
                panic!("Failed to detect write lock while acquiring read lock");
            }
        }
    }

    /// Attempts to acquire a write lock for the given tool.
    pub fn write(&self, context: UiLockContext) -> Result<TrackingWriteGuard<'_, T>, Vec<UiLockContext>> {
        let mut tracking = self.tracking.lock().unwrap();

        if !tracking.read_locks.is_empty() {
            log::error!(
                "Tool {:?} attempted to acquire a write lock while read locks are held by {:?}",
                context,
                tracking.read_locks.iter().map(|(t, _)| *t).collect::<Vec<_>>()
            );
            return Err(tracking.read_locks.keys().cloned().collect());
        }

        if tracking.write_lock.is_some() {
            log::error!(
                "Tool {:?} attempted to acquire a write lock while write lock is held by {:?}",
                context,
                tracking.write_lock
            );
            return Err(vec![tracking.write_lock.unwrap()]);
        }

        // Set the write lock
        tracking.write_lock = Some(context);

        // Acquire the actual write lock
        match self.inner.try_write() {
            Ok(guard) => Ok(TrackingWriteGuard {
                guard,
                tracking: Arc::clone(&self.tracking),
                context,
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
    guard:    RwLockReadGuard<'a, T>,
    tracking: Arc<Mutex<TrackingData>>,
    context:  UiLockContext,
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
        if let Some(count) = tracking.read_locks.get_mut(&self.context) {
            *count -= 1;
            if *count == 0 {
                tracking.read_locks.remove(&self.context);
            }
        }
        else {
            log::error!(
                "Context {:?} is dropping a read lock but it was not registered",
                self.context
            );
        }
    }
}

/// Guard that removes the write lock tracking when dropped.
pub struct TrackingWriteGuard<'a, T> {
    guard:    RwLockWriteGuard<'a, T>,
    tracking: Arc<Mutex<TrackingData>>,
    context:  UiLockContext,
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
            if current_tool != self.context {
                log::error!(
                    "Tool {:?} is dropping a write lock but the current write lock is held by {:?}",
                    self.context,
                    current_tool
                );
            }
            else {
                tracking.write_lock = None;
                log::debug!(
                    "Tool {:?} dropped write lock. Strong references left: {}",
                    self.context,
                    Arc::strong_count(&self.tracking)
                );
            }
        }
        else {
            log::error!(
                "Tool {:?} is dropping a write lock but no write lock was registered",
                self.context
            );
        }
    }
}
