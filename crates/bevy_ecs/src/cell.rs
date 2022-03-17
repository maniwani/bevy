use std::cell::UnsafeCell;

/// A "reference holder" similar to [`UnsafeCell`] except:
///
/// - it can be constructed from a shared or mutable reference
/// - references to it can be shared between threads
///
/// `bevy_ecs` uses this to have a single argument type in several core `unsafe` [`World`](crate::world::World)
/// access methods. In those contexts, the multi-threaded executor ensures that the active references
/// of systems do not alias in unsound ways.
pub enum SemiSafeCell<'a, T> {
    Ref(&'a T),
    Mut(&'a UnsafeCell<T>),
}

impl<'a, T> SemiSafeCell<'a, T> {
    /// Constructs a cell from a shared reference.
    pub fn from_ref(value: &'a T) -> Self {
        Self::Ref(value)
    }

    /// Constructs a cell from a mutable reference.
    pub fn from_mut(value: &'a mut T) -> Self {
        // SAFETY: `&mut` ensures unique access.
        unsafe { Self::Mut(&*(value as *mut T as *const UnsafeCell<T>)) }
    }

    /// Returns a shared reference to the underlying data.
    ///
    /// # Safety
    ///
    /// Caller must ensure there are no active mutable references to the underlying data.
    pub unsafe fn as_ref(self) -> &'a T {
        match self {
            Self::Ref(borrow) => borrow,
            Self::Mut(cell) => &*cell.get(),
        }
    }

    /// Returns mutable reference to the underlying data.
    ///
    /// # Safety
    ///
    /// Caller must ensure access to the underlying data is unique (no active references, mutable or not).
    ///
    /// # Panics
    ///
    /// Panics if the cell was constructed from a shared reference.
    pub unsafe fn as_mut(self) -> &'a mut T {
        match self {
            Self::Ref(_) => {
                panic!("cannot return mutable reference from SemiSafeCell::Ref");
            }
            Self::Mut(cell) => &mut *cell.get(),
        }
    }
}

impl<T> Copy for SemiSafeCell<'_, T> {}
impl<T> Clone for SemiSafeCell<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

// SAFETY: Multi-threaded executor does not run systems with conflicting access at the same time.
// `World` is `Send` + `Sync`
unsafe impl<T: Send> Send for SemiSafeCell<'_, T> {}
unsafe impl<T: Sync> Sync for SemiSafeCell<'_, T> {}
