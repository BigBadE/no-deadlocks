use std::sync::{LockResult, TryLockResult, TryLockError, PoisonError};
use std::cell::UnsafeCell;

/// An instrumented version of `std::sync::RwLock`
pub struct RwLock<T: ?Sized>{
    key: usize,
    poisoned: bool,
    manager: std::sync::Arc<crate::lock_manager::LockManager>,
    inner: UnsafeCell<T>,
}

impl<T> RwLock<T> {
    pub fn new(inner: T) -> Self {
        let manager = crate::lock_manager::LockManager::get_global_manager();
        let key = manager.create_lock();
        RwLock {
            inner: UnsafeCell::new(inner),
            poisoned: false,
            manager,
            key,
        }
    }

    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    pub fn get_mut(&mut self) -> &mut T {
        unsafe {&mut *self.inner.get()}
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn try_read(&self) -> TryLockResult<RwLockReadGuard<T>> {
        let mut guard = self.manager.write_lock();
        let representation = guard.locks.get_mut(&self.key).unwrap();
        if representation.try_read_lock() {
            let returned_guard = RwLockReadGuard {
                inner: &self,
            };
            if self.is_poisoned() {
                Err(TryLockError::Poisoned(
                    PoisonError::new(returned_guard),
                ))
            } else {
                Ok(returned_guard)
            }
        } else {
            Err(TryLockError::WouldBlock)
        }
    }

    pub fn try_write(&self) -> TryLockResult<RwLockWriteGuard<T>> {
        let mut guard = self.manager.write_lock();
        let representation = guard.locks.get_mut(&self.key).unwrap();
        if representation.try_write_lock() {
            let returned_guard = RwLockWriteGuard {
                inner: unsafe { &mut *(self as *const _ as *mut _) },
            };
            if self.is_poisoned() {
                Err(TryLockError::Poisoned(
                    PoisonError::new(returned_guard),
                ))
            } else {
                Ok(returned_guard)
            }
        } else {
            Err(TryLockError::WouldBlock)
        }
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<T>> {
        loop {
            let mut guard = self.manager.write_lock();
            let representation = guard.locks.get_mut(&self.key).unwrap();
            if representation.try_read_lock() {
                let returned_guard = RwLockReadGuard {
                    inner: &self,
                };
                if self.is_poisoned() {
                    return Err(PoisonError::new(returned_guard))
                } else {
                    return Ok(returned_guard)
                }
            } else {
                representation.subscribe_read();
                guard.analyse();
                std::thread::yield_now();
            }
        }
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<T>> {
        loop {
            let mut guard = self.manager.write_lock();
            let representation = guard.locks.get_mut(&self.key).unwrap();
            if representation.try_write_lock() {
                let returned_guard = RwLockWriteGuard {
                    inner: unsafe { &mut *(self as *const _ as *mut _) },
                };
                if self.is_poisoned() {
                    return Err(PoisonError::new(returned_guard))
                } else {
                    return Ok(returned_guard)
                }
            } else {
                representation.subscribe_write();
                guard.analyse();
                std::thread::yield_now();
            }
        }
    }
}

pub struct RwLockReadGuard<'l, T: ?Sized> {
    inner: &'l RwLock<T>
}
impl<'l, T: ?Sized> std::ops::Deref for RwLockReadGuard<'l, T> {
    type Target = T;
    fn deref(&self) -> &<Self as std::ops::Deref>::Target {
        unsafe{ &(*self.inner.inner.get()) }
    }
}
impl<'l, T: ?Sized> Drop for RwLockReadGuard<'l, T> {
    fn drop(&mut self) {
        let mut guard = self.inner.manager.write_lock();
        guard.locks.get_mut(&self.inner.key).unwrap().unlock();
        if std::thread::panicking() {
            unsafe {(*(self.inner as *const _ as *mut RwLock<T>)).poisoned = true};
        }
    }
}
pub struct RwLockWriteGuard<'l, T: ?Sized> {
    inner: &'l mut RwLock<T>
}

impl<'l, T: ?Sized> std::ops::Deref for RwLockWriteGuard<'l, T> {
    type Target = T;
    fn deref(&self) -> &<Self as std::ops::Deref>::Target {
        unsafe{ &(*self.inner.inner.get()) }
    }
}
impl<'l, T: ?Sized> std::ops::DerefMut for RwLockWriteGuard<'l, T> {
    fn deref_mut(&mut self) -> &mut <Self as std::ops::Deref>::Target {
        unsafe {&mut *self.inner.inner.get()}
    }
}
impl<'l, T: ?Sized> Drop for RwLockWriteGuard<'l, T> {
    fn drop(&mut self) {
        let mut guard = self.inner.manager.write_lock();
        guard.locks.get_mut(&self.inner.key).unwrap().unlock();
        if std::thread::panicking() {
            self.inner.poisoned = true;
        }
    }
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for RwLock<T> {}
