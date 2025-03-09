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

//! This module defines a [DiskLock] trait for locking disk access. This trait is primarily employed
//! to enable support for custom lock strategies, such as fluxfox-egui's `TrackingLock` which is
//! used to track disk access across that application's various tools.
//!
//! The concept of a [DiskLock] is that it can be supplied with a [LockContext] trait implementor
//! which allows the tracking and querying of what has the disk image locked. This is useful for
//! debugging lock contention issues.

use crate::DiskImage;
use std::{
    cell::{Ref, RefCell, RefMut},
    fmt::{Debug, Display},
    hash::Hash,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Trait defining the context of a lock. At a minimum, this trait should implement `Display` and
/// `Debug` so that context-specific information can be logged, and `Eq` and `Hash` so that the
/// context can be used as a key in a hash map to track the lock status.
pub trait LockContext: Display + Debug + Eq + Hash {}

/// Trait defining the locking behavior with tracking capabilities.
///
/// - `T`: The type of the data being protected.
/// - `C`: The type representing the context acquiring the lock.
pub trait DiskLock<T, C: LockContext> {
    type T;
    type C: LockContext;

    /// The guard returned by the `read` method.
    type ReadGuard<'a>: Deref<Target = T> + 'a
    where
        Self: 'a;

    /// The guard returned by the `write` method.
    type WriteGuard<'a>: Deref<Target = T> + DerefMut<Target = T> + 'a
    where
        Self: 'a;

    /// Attempts to acquire a read lock for the given tool. On failure, the method should return
    /// the context that is holding the write lock.
    fn read(&self, context: C) -> Result<Self::ReadGuard<'_>, C>;

    /// Attempts to acquire a write lock for the given tool. On failure, the method should return
    /// a vector of the contexts holding read locks.
    fn write(&self, context: C) -> Result<Self::WriteGuard<'_>, Vec<C>>;

    /// Return the number of strong references to the inner lock.
    fn strong_count(&self) -> usize;
}

#[derive(Clone)]
pub struct RefLock<T> {
    inner: Rc<RefCell<T>>,
}

impl<T> RefLock<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn into_inner(self) -> Result<T, Self> {
        Rc::try_unwrap(self.inner)
            .map(|cell| cell.into_inner())
            .map_err(|rc| Self { inner: rc })
    }
}

impl<T> DiskLock<T, NullContext> for RefLock<T> {
    type T = DiskImage;
    type C = NullContext;
    type ReadGuard<'a>
        = Ref<'a, T>
    where
        T: 'a;
    type WriteGuard<'a>
        = RefMut<'a, T>
    where
        T: 'a;

    fn read(&self, _tool: NullContext) -> Result<Self::ReadGuard<'_>, NullContext> {
        Ok(self.inner.borrow())
    }

    fn write(&self, _tool: NullContext) -> Result<Self::WriteGuard<'_>, Vec<NullContext>> {
        Ok(self.inner.borrow_mut())
    }

    fn strong_count(&self) -> usize {
        0
    }
}

/// A newtype wrapper around Arc<RwLock<T>> without tracking.
#[derive(Clone)]
pub struct NonTrackingDiskLock<T> {
    inner: Arc<RwLock<T>>,
}

impl<T> NonTrackingDiskLock<T> {
    pub fn new(inner: Arc<RwLock<T>>) -> Self {
        Self { inner }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NullContext {
    _private: u8,
}

impl Display for NullContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NullContext")
    }
}

impl LockContext for NullContext {}

/// Allow coercing an Arc<RwLock<T>> into a NonTrackingDiskLock<T>.
impl From<Arc<RwLock<DiskImage>>> for NonTrackingDiskLock<DiskImage> {
    fn from(arc: Arc<RwLock<DiskImage>>) -> Self {
        NonTrackingDiskLock { inner: arc }
    }
}

impl<T> NonTrackingDiskLock<T> {
    /// Returns a cloned Arc<RwLock<T>> pointing to the inner lock.
    pub fn as_inner(&self) -> Arc<RwLock<T>> {
        Arc::clone(&self.inner)
    }
}

impl<T> DiskLock<T, NullContext> for NonTrackingDiskLock<T> {
    type T = DiskImage;
    type C = NullContext;
    type ReadGuard<'a>
        = RwLockReadGuard<'a, T>
    where
        T: 'a;
    type WriteGuard<'a>
        = RwLockWriteGuard<'a, T>
    where
        T: 'a;

    fn read(&self, _tool: NullContext) -> Result<Self::ReadGuard<'_>, NullContext> {
        match self.inner.try_read() {
            Ok(guard) => Ok(guard),
            Err(_) => Err(NullContext::default()),
        }
    }

    fn write(&self, _tool: NullContext) -> Result<Self::WriteGuard<'_>, Vec<NullContext>> {
        match self.inner.try_write() {
            Ok(guard) => Ok(guard),
            Err(_) => Err(Vec::new()),
        }
    }

    fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}
