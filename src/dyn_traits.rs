use std::any::Any;

/// Object-safe equality comparison for trait objects via [`Any`] downcasting.
pub trait DynEq: Any {
    /// Returns the underlying value as `&dyn Any`.
    fn as_any(&self) -> &dyn Any;

    /// Returns `true` if `self` and `other` share the same concrete type and are equal.
    fn dyn_eq(&self, other: &dyn DynEq) -> bool;
}

impl<T: Any + PartialEq> DynEq for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn dyn_eq(&self, other: &dyn DynEq) -> bool {
        if let Some(other_concrete) = other.as_any().downcast_ref::<Self>() {
            self == other_concrete
        } else {
            false
        }
    }
}
