use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Default)]
pub struct Cell<T>(Arc<Mutex<T>>);

impl<T: Copy> Cell<T> {
    pub fn new(v: T) -> Self {
        Cell(Arc::new(Mutex::new(v)))
    }

    pub fn get(&self) -> T {
        *self.0.lock().unwrap()
    }

    pub fn set(&self, v: T) {
        *self.0.lock().unwrap() = v;
    }
}

#[derive(Clone, Default)]
pub struct RefCell<T>(Arc<Mutex<T>>);

impl<T> RefCell<T> {
    pub fn new(v: T) -> Self {
        RefCell(Arc::new(Mutex::new(v)))
    }

    pub fn borrow(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }

    pub fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}
